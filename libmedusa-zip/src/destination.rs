/*
 * Description: ???
 *
 * Copyright (C) 2023 Danny McClanahan <dmcC2@hypnicjerk.ai>
 * SPDX-License-Identifier: Apache-2.0
 *
 * Licensed under the Apache License, Version 2.0 (see LICENSE).
 */

//! ???

use clap::ValueEnum;
use displaydoc::Display;
use thiserror::Error;
use tokio::{
  fs,
  io::{self, AsyncSeekExt},
  task,
};
use zip::{result::ZipError, ZipWriter};

use std::path::Path;

#[derive(Debug, Display, Error)]
pub enum DestinationError {
  /// i/o error accessing destination file: {0}
  Io(#[from] io::Error),
  /// error setting up zip format in destination file: {0}
  Zip(#[from] ZipError),
  /// error joining zip setup task: {0}
  Join(#[from] task::JoinError),
}

#[derive(Copy, Clone, Default, Debug, ValueEnum)]
pub enum DestinationBehavior {
  /// Create the file if new, or truncate it if it exists.
  #[default]
  AlwaysTruncate,
  /// Initialize an existing zip file.
  AppendOrFail,
  /// Append if the file already exists, otherwise create it.
  OptimisticallyAppend,
  /// Open the file in append mode, but don't try to read any zip info from it.
  ///
  /// This is useful for creating e.g. PEX files or other self-executing zips
  /// with a shebang line.
  AppendToNonZip,
}

/* Make it always go to a tempfile, then atomically move at the end! Unless
 * it already exists! */
impl DestinationBehavior {
  pub async fn initialize(self, path: &Path) -> Result<ZipWriter<std::fs::File>, DestinationError> {
    let (file, with_append) = match self {
      Self::AlwaysTruncate => {
        let f = fs::OpenOptions::new()
          .write(true)
          .create(true)
          .truncate(true)
          .open(path)
          .await?;
        (f, false)
      },
      Self::AppendOrFail => {
        let f = fs::OpenOptions::new()
          .write(true)
          .read(true)
          .open(path)
          .await?;
        (f, true)
      },
      Self::OptimisticallyAppend => {
        match fs::OpenOptions::new()
          .write(true)
          .create_new(true)
          .open(path)
          .await
        {
          Ok(f) => (f, false),
          Err(e) => match e.kind() {
            io::ErrorKind::AlreadyExists => {
              let f = fs::OpenOptions::new()
                .write(true)
                .read(true)
                .open(path)
                .await?;
              (f, true)
            },
            _ => {
              return Err(e.into());
            },
          },
        }
      },
      Self::AppendToNonZip => {
        let mut f = fs::OpenOptions::new()
          .write(true)
          .read(true)
          .open(path)
          .await?;
        /* NB: do NOT!!! open the file for append!!! It will only BREAK EVERYTHING IN
         * MYSTERIOUS WAYS by constantly moving the seek cursor! Opening with
         * ::new_append() will seek to the end for us, but in this case we
         * want to write to a file that *doesn't* already have zip
         * data, so we need to tell the file handle to go to the end before giving it
         * to the zip library. */
        f.seek(io::SeekFrom::End(0)).await?;
        (f, false)
      },
    };
    /* FIXME: remove all the unnecessary tokio::fs::File usage that requires
     * calling .into_std() in case it's slow! */
    let file = file.into_std().await;

    let writer = task::spawn_blocking(move || {
      if with_append {
        Ok::<_, DestinationError>(ZipWriter::new_append(file)?)
      } else {
        Ok(ZipWriter::new(file))
      }
    })
    .await??;

    Ok(writer)
  }
}
