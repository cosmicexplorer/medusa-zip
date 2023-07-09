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
use tokio::{fs, io, task};
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
}

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
          .append(true)
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
                .append(true)
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
    };
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
