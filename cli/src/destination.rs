/*
 * Description: ???
 *
 * Copyright (C) 2023 Danny McClanahan <dmcC2@hypnicjerk.ai>
 * SPDX-License-Identifier: Apache-2.0
 *
 * Licensed under the Apache License, Version 2.0 (see LICENSE).
 */

//! ???

use libmedusa_zip::destination as lib_destination;

use clap::ValueEnum;

#[derive(Copy, Clone, Debug, ValueEnum)]
pub enum DestinationBehavior {
  /// Create the file if new, or truncate it if it exists.
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

impl From<lib_destination::DestinationBehavior> for DestinationBehavior {
  fn from(x: lib_destination::DestinationBehavior) -> Self {
    match x {
      lib_destination::DestinationBehavior::AlwaysTruncate => Self::AlwaysTruncate,
      lib_destination::DestinationBehavior::AppendOrFail => Self::AppendOrFail,
      lib_destination::DestinationBehavior::OptimisticallyAppend => Self::OptimisticallyAppend,
      lib_destination::DestinationBehavior::AppendToNonZip => Self::AppendToNonZip,
    }
  }
}

impl From<DestinationBehavior> for lib_destination::DestinationBehavior {
  fn from(x: DestinationBehavior) -> Self {
    match x {
      DestinationBehavior::AlwaysTruncate => Self::AlwaysTruncate,
      DestinationBehavior::AppendOrFail => Self::AppendOrFail,
      DestinationBehavior::OptimisticallyAppend => Self::OptimisticallyAppend,
      DestinationBehavior::AppendToNonZip => Self::AppendToNonZip,
    }
  }
}

impl Default for DestinationBehavior {
  fn default() -> Self { lib_destination::DestinationBehavior::default().into() }
}
