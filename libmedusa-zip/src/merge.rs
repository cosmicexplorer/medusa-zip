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
  zip::{calculate_new_rightmost_components, DefaultInitializeZipOptions},
  EntryName, MedusaNameFormatError, ModifiedTimeBehavior,
};

use displaydoc::Display;
use futures::stream::StreamExt;
use parking_lot::Mutex;
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
  path::PathBuf,
  sync::Arc,
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
  pub prefix: EntryName,
  pub sources: Vec<PathBuf>,
}

#[derive(Debug, Display, Error)]
pub enum MergeArgParseError {
  /// name formate error in entry: {0}
  NameFormat(#[from] MedusaNameFormatError),
}

/* TODO: make this parse from clap CLI options, not json! */
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
    let mut current_prefix: Option<EntryName> = None;
    let mut current_sources: Vec<PathBuf> = Vec::new();
    for arg in args {
      let arg: &str = arg.as_ref();
      /* If we are starting a new prefix: */
      if arg.starts_with('+') && arg.ends_with('/') {
        let new_prefix = &arg[1..arg.len() - 1];
        let new_prefix = if new_prefix.is_empty() {
          EntryName::empty()
        } else {
          EntryName::validate(new_prefix.to_string())?
        };
        /* Only None on the very first iteration of the loop. */
        if let Some(prefix) = current_prefix.take() {
          let group = MergeGroup {
            prefix,
            sources: current_sources.drain(..).collect(),
          };
          ret.push(group);
        } else {
          assert!(current_sources.is_empty());
        }
        current_prefix = Some(new_prefix);
      } else {
        /* If no prefixes have been declared, assume they begin with an empty prefix. */
        current_prefix.get_or_insert_with(EntryName::empty);
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
    output_zip: ZipWriter<Output>,
  ) -> Result<Output, MedusaMergeError>
  where
    Output: Write+Seek+Send+'static,
  {
    let Self { groups } = self;
    let zip_options = mtime_behavior.set_zip_options_static(ZipLibraryFileOptions::default());

    let (handle_tx, handle_rx) = mpsc::channel::<IntermediateMergeEntry>(PARALLEL_MERGE_ENTRIES);
    let mut handle_jobs = ReceiverStream::new(handle_rx);
    let handle_stream_task = task::spawn(async move {
      let mut previous_directory_components: Vec<String> = Vec::new();
      for MergeGroup { prefix, sources } in groups.into_iter() {
        let current_directory_components: Vec<String> = prefix
          .all_components()
          .map(|s| s.to_string())
          .collect::<Vec<_>>();
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

    let output_zip = Arc::new(Mutex::new(output_zip));
    while let Some(intermediate_entry) = handle_jobs.next().await {
      let output_zip = output_zip.clone();
      match intermediate_entry {
        IntermediateMergeEntry::AddDirectory(name) => {
          task::spawn_blocking(move || {
            let mut output_zip = output_zip.lock();
            output_zip.add_directory(name.into_string(), zip_options)?;
            Ok::<(), ZipError>(())
          })
          .await??;
        },
        IntermediateMergeEntry::MergeZip(source_archive) => {
          task::spawn_blocking(move || {
            let mut output_zip = output_zip.lock();
            output_zip.merge_archive(source_archive)?;
            Ok::<(), ZipError>(())
          })
          .await??;
        },
      }
    }
    handle_stream_task.await??;

    let output_handle = task::spawn_blocking(move || {
      let mut output_zip = Arc::into_inner(output_zip)
        .expect("no other references should exist to output_zip")
        .into_inner();
      let output_handle = output_zip.finish()?;
      Ok::<Output, ZipError>(output_handle)
    })
    .await??;

    Ok(output_handle)
  }
}
