/*
 * Description: ???
 *
 * Copyright (C) 2023 Danny McClanahan <dmcC2@hypnicjerk.ai>
 * SPDX-License-Identifier: Apache-2.0
 *
 * Licensed under the Apache License, Version 2.0 (see LICENSE).
 */

//! ???

use crate::EntryName;

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
  write::{FileOptions as ZipLibFileOptions, ZipWriter},
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
    output_zip: ZipWriter<Output>,
  ) -> Result<Output, MedusaMergeError>
  where
    Output: Write+Seek+Send+'static,
  {
    let Self { groups } = self;
    /* NB: we only add directories in between merging zips here, and it doesn't
     * make sense to also accept a zip_output parameter that won't be used at
     * all. */
    let zip_options = ZipLibFileOptions::default();

    let (handle_tx, handle_rx) = mpsc::channel::<IntermediateMergeEntry>(PARALLEL_MERGE_ENTRIES);
    let mut handle_jobs = ReceiverStream::new(handle_rx);
    let handle_stream_task = task::spawn(async move {
      for MergeGroup { prefix, sources } in groups.into_iter() {
        if let Some(name) = prefix {
          handle_tx
            .send(IntermediateMergeEntry::AddDirectory(name))
            .await?;
        }
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
            /* FIXME: enable merging archives without having to read in the whole thing! */
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
