/*
 * Description: ???
 *
 * Copyright (C) 2023 Danny McClanahan <dmcC2@hypnicjerk.ai>
 * SPDX-License-Identifier: Apache-2.0
 *
 * Licensed under the Apache License, Version 2.0 (see LICENSE).
 */

//! ???

use crate::{
  destination::OutputWrapper,
  zip::{calculate_new_rightmost_components, DefaultInitializeZipOptions, ModifiedTimeBehavior},
  EntryName, MedusaNameFormatError,
};

use displaydoc::Display;
use futures::stream::StreamExt;
use thiserror::Error;
use tokio::{io, sync::mpsc, task};
use tokio_stream::wrappers::ReceiverStream;
use zip::{
  read::ZipArchive,
  result::ZipError,
  write::{FileOptions as ZipLibraryFileOptions, ZipWriter},
};

use std::{
  io::{Seek, Write},
  mem,
  path::PathBuf,
};

#[derive(Debug, Display, Error)]
pub enum MedusaMergeError {
  /// internal zip impl error: {0}
  Zip(#[from] ZipError),
  /// i/o error: {0}
  Io(#[from] io::Error),
  /// error joining threads: {0}
  Join(#[from] task::JoinError),
  /// error sending value: {0}
  Send(#[from] mpsc::error::SendError<IntermediateMergeEntry>),
}

#[derive(Debug, Clone)]
pub struct MergeGroup {
  pub prefix: Option<EntryName>,
  pub sources: Vec<PathBuf>,
}

#[derive(Debug, Display, Error)]
pub enum MergeArgParseError {
  /// name formate error in entry: {0}
  NameFormat(#[from] MedusaNameFormatError),
}

#[derive(Default, Debug, Clone)]
pub struct MedusaMerge {
  pub groups: Vec<MergeGroup>,
}

pub enum IntermediateMergeEntry {
  AddDirectory(EntryName),
  MergeZip(ZipArchive<std::fs::File>),
}

const PARALLEL_MERGE_ENTRIES: usize = 10;

impl MedusaMerge {
  pub fn parse_from_args<R: AsRef<str>>(
    args: impl Iterator<Item=R>,
  ) -> Result<Self, MergeArgParseError> {
    let mut ret: Vec<MergeGroup> = Vec::new();
    /* Each prefix is itself legitimately an Option (to avoid EntryName being
     * empty), so we wrap it again. */
    let mut current_prefix: Option<Option<EntryName>> = None;
    let mut current_sources: Vec<PathBuf> = Vec::new();
    for arg in args {
      let arg: &str = arg.as_ref();
      /* If we are starting a new prefix: */
      if arg.starts_with('+') && arg.ends_with('/') {
        let new_prefix = &arg[1..arg.len() - 1];
        let new_prefix: Option<EntryName> = if new_prefix.is_empty() {
          None
        } else {
          Some(EntryName::validate(new_prefix.to_string())?)
        };
        if let Some(prefix) = current_prefix.take() {
          let group = MergeGroup {
            prefix,
            sources: mem::take(&mut current_sources),
          };
          ret.push(group);
        } else {
          /* Only None on the very first iteration of the loop. */
          assert!(current_sources.is_empty());
        }
        current_prefix = Some(new_prefix);
      } else {
        /* If no prefixes have been declared, assume they begin with an empty prefix. */
        current_prefix.get_or_insert(None);
        current_sources.push(PathBuf::from(arg));
      }
    }
    if let Some(prefix) = current_prefix {
      let group = MergeGroup {
        prefix,
        sources: current_sources,
      };
      ret.push(group);
    }
    Ok(Self { groups: ret })
  }

  pub async fn merge<Output>(
    self,
    mtime_behavior: ModifiedTimeBehavior,
    /* FIXME: make a trait that's like BorrowMut, but doesn't require needing a mutable
     * reference (?) */
    output_zip: OutputWrapper<ZipWriter<Output>>,
  ) -> Result<OutputWrapper<ZipWriter<Output>>, MedusaMergeError>
  where
    Output: Write+Seek+Send+'static,
  {
    let Self { groups } = self;
    let zip_options = mtime_behavior.set_zip_options_static(ZipLibraryFileOptions::default());

    /* This shouldn't really need to be bounded at all, since the task is
     * entirely synchronous. */
    let (handle_tx, handle_rx) = mpsc::channel::<IntermediateMergeEntry>(PARALLEL_MERGE_ENTRIES);
    let mut handle_jobs = ReceiverStream::new(handle_rx);
    let handle_stream_task = task::spawn(async move {
      let mut previous_directory_components: Vec<String> = Vec::new();
      for MergeGroup { prefix, sources } in groups.into_iter() {
        let current_directory_components: Vec<String> = prefix
          .map(|p| {
            p.all_components()
              .map(|s| s.to_string())
              .collect::<Vec<_>>()
          })
          .unwrap_or_default();
        for new_rightmost_components in calculate_new_rightmost_components(
          &previous_directory_components,
          &current_directory_components,
        ) {
          let cur_intermediate_directory: String = new_rightmost_components.join("/");
          let intermediate_dir = EntryName::validate(cur_intermediate_directory)
            .expect("constructed virtual directory should be fine");
          handle_tx
            .send(IntermediateMergeEntry::AddDirectory(intermediate_dir))
            .await?;
        }
        previous_directory_components = current_directory_components;

        for src in sources.into_iter() {
          let archive = task::spawn_blocking(move || {
            let handle = std::fs::OpenOptions::new().read(true).open(src)?;
            ZipArchive::new(handle)
          })
          .await??;
          handle_tx
            .send(IntermediateMergeEntry::MergeZip(archive))
            .await?;
        }
      }
      Ok::<(), MedusaMergeError>(())
    });

    while let Some(intermediate_entry) = handle_jobs.next().await {
      let output_zip = output_zip.clone();
      match intermediate_entry {
        IntermediateMergeEntry::AddDirectory(name) => {
          task::spawn_blocking(move || {
            let mut output_zip = output_zip.lease();
            output_zip.add_directory(name.into_string(), zip_options)?;
            Ok::<(), ZipError>(())
          })
          .await??;
        },
        IntermediateMergeEntry::MergeZip(source_archive) => {
          task::spawn_blocking(move || {
            let mut output_zip = output_zip.lease();
            output_zip.merge_archive(source_archive)?;
            Ok::<(), ZipError>(())
          })
          .await??;
        },
      }
    }
    handle_stream_task.await??;

    Ok(output_zip)
  }
}
