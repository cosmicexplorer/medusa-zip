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
  EntryName, ModifiedTimeBehavior,
};

use displaydoc::Display;
use futures::stream::StreamExt;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::{fs, io, sync::mpsc, task};
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeGroup {
  pub prefix: Option<EntryName>,
  pub sources: Vec<PathBuf>,
}

/* FIXME: make this parse from clap CLI options, not json! */
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct MedusaMerge {
  pub groups: Vec<MergeGroup>,
}

pub enum IntermediateMergeEntry {
  AddDirectory(EntryName),
  MergeZip(ZipArchive<std::fs::File>),
}

const PARALLEL_MERGE_ENTRIES: usize = 10;

impl MedusaMerge {
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
          .map(|e| {
            e.directory_components()
              .into_iter()
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
          let handle = fs::OpenOptions::new().read(true).open(&src).await?;
          let handle = handle.into_std().await;
          let archive = task::spawn_blocking(move || ZipArchive::new(handle)).await??;
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
