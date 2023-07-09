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
use zip::{result::ZipError, ZipWriter};

use std::{fs, io, path::Path};

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
  pub fn initialize(self, path: &Path) -> Result<ZipWriter<fs::File>, ZipError> {
    let (file, with_append) = match self {
      Self::AlwaysTruncate => {
        let f = fs::OpenOptions::new()
          .write(true)
          .create(true)
          .truncate(true)
          .open(path)?;
        (f, false)
      },
      Self::AppendOrFail => {
        let f = fs::OpenOptions::new()
          .write(true)
          .append(true)
          .read(true)
          .open(path)?;
        (f, true)
      },
      Self::OptimisticallyAppend => {
        let exclusive_attempt = fs::OpenOptions::new()
          .write(true)
          .create_new(true)
          .open(path);
        match exclusive_attempt {
          Ok(f) => (f, false),
          Err(e) => match e.kind() {
            io::ErrorKind::AlreadyExists => {
              let f = fs::OpenOptions::new()
                .write(true)
                .append(true)
                .read(true)
                .open(path)?;
              (f, true)
            },
            _ => {
              return Err(e.into());
            },
          },
        }
      },
    };
    if with_append {
      ZipWriter::new_append(file)
    } else {
      Ok(ZipWriter::new(file))
    }
  }
}
