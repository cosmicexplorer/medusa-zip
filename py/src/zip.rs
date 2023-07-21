/*
 * Description: ???
 *
 * Copyright (C) 2023 Danny McClanahan <dmcC2@hypnicjerk.ai>
 * SPDX-License-Identifier: Apache-2.0
 *
 * Licensed under the Apache License, Version 2.0 (see LICENSE).
 */

//! ???

use crate::FileSource;

use libmedusa_zip::zip as lib_zip;

use pyo3::prelude::*;
use zip::DateTime as ZipDateTime;

use std::convert::TryFrom;


#[pyclass]
#[derive(Copy, Clone, PartialEq, Eq)]
pub enum AutomaticModifiedTimeStrategy {
  Reproducible,
  CurrentTime,
  PreserveSourceTime,
}

impl From<lib_zip::AutomaticModifiedTimeStrategy> for AutomaticModifiedTimeStrategy {
  fn from(x: lib_zip::AutomaticModifiedTimeStrategy) -> Self {
    match x {
      lib_zip::AutomaticModifiedTimeStrategy::Reproducible => Self::Reproducible,
      lib_zip::AutomaticModifiedTimeStrategy::CurrentTime => Self::CurrentTime,
      lib_zip::AutomaticModifiedTimeStrategy::PreserveSourceTime => Self::PreserveSourceTime,
    }
  }
}

impl From<AutomaticModifiedTimeStrategy> for lib_zip::AutomaticModifiedTimeStrategy {
  fn from(x: AutomaticModifiedTimeStrategy) -> Self {
    match x {
      AutomaticModifiedTimeStrategy::Reproducible => Self::Reproducible,
      AutomaticModifiedTimeStrategy::CurrentTime => Self::CurrentTime,
      AutomaticModifiedTimeStrategy::PreserveSourceTime => Self::PreserveSourceTime,
    }
  }
}


#[pyclass]
#[derive(Copy, Clone, Default)]
pub struct ModifiedTimeBehavior {
  pub automatic_mtime_strategy: lib_zip::AutomaticModifiedTimeStrategy,
  pub explicit_mtime_timestamp: Option<ZipDateTime>,
}

impl ModifiedTimeBehavior {
  fn automatic(automatic_mtime_strategy: lib_zip::AutomaticModifiedTimeStrategy) -> Self {
    Self {
      automatic_mtime_strategy,
      ..Default::default()
    }
  }

  fn explicit(timestamp: ZipDateTime) -> Self {
    Self {
      explicit_mtime_timestamp: Some(timestamp),
      ..Default::default()
    }
  }
}

impl From<lib_zip::ModifiedTimeBehavior> for ModifiedTimeBehavior {
  fn from(x: lib_zip::ModifiedTimeBehavior) -> Self {
    match x {
      lib_zip::ModifiedTimeBehavior::Reproducible => {
        ModifiedTimeBehavior::automatic(lib_zip::AutomaticModifiedTimeStrategy::Reproducible)
      },
      lib_zip::ModifiedTimeBehavior::CurrentTime => {
        ModifiedTimeBehavior::automatic(lib_zip::AutomaticModifiedTimeStrategy::CurrentTime)
      },
      lib_zip::ModifiedTimeBehavior::PreserveSourceTime => {
        ModifiedTimeBehavior::automatic(lib_zip::AutomaticModifiedTimeStrategy::PreserveSourceTime)
      },
      lib_zip::ModifiedTimeBehavior::Explicit(timestamp) => {
        ModifiedTimeBehavior::explicit(timestamp)
      },
    }
  }
}

impl From<ModifiedTimeBehavior> for lib_zip::ModifiedTimeBehavior {
  fn from(x: ModifiedTimeBehavior) -> Self {
    let ModifiedTimeBehavior {
      automatic_mtime_strategy,
      explicit_mtime_timestamp,
    } = x;
    match explicit_mtime_timestamp {
      Some(timestamp) => Self::Explicit(timestamp),
      None => match automatic_mtime_strategy {
        lib_zip::AutomaticModifiedTimeStrategy::Reproducible => Self::Reproducible,
        lib_zip::AutomaticModifiedTimeStrategy::CurrentTime => Self::CurrentTime,
        lib_zip::AutomaticModifiedTimeStrategy::PreserveSourceTime => Self::PreserveSourceTime,
      },
    }
  }
}

#[pyclass]
pub enum CompressionMethod {
  Stored,
  Deflated,
  Bzip2,
  Zstd,
}

/* FIXME: remove CompressionMethod/CompressionOptions from lib_zip! */
impl From<lib_zip::CompressionMethod> for CompressionMethod {
  fn from(x: lib_zip::CompressionMethod) -> Self {
    match x {
      lib_zip::CompressionMethod::Stored => Self::Stored,
      lib_zip::CompressionMethod::Deflated => Self::Deflated,
      lib_zip::CompressionMethod::Bzip2 => Self::Bzip2,
      lib_zip::CompressionMethod::Zstd => Self::Zstd,
    }
  }
}

impl From<CompressionMethod> for lib_zip::CompressionMethod {
  fn from(x: CompressionMethod) -> Self {
    match x {
      CompressionMethod::Stored => Self::Stored,
      CompressionMethod::Deflated => Self::Deflated,
      CompressionMethod::Bzip2 => Self::Bzip2,
      CompressionMethod::Zstd => Self::Zstd,
    }
  }
}

#[pyclass]
pub struct CompressionOptions {
  pub compression_method: CompressionMethod,
  pub compression_level: Option<i8>,
}

impl TryFrom<CompressionOptions> for lib_zip::CompressionStrategy {
  type Error = lib_zip::ParseCompressionOptionsError;

  fn try_from(x: CompressionOptions) -> Result<Self, Self::Error> {
    let CompressionOptions {
      compression_method,
      compression_level,
    } = x;
    let compression_options = lib_zip::CompressionOptions {
      compression_method: compression_method.into(),
      compression_level,
    };
    Self::from_options(compression_options)
  }
}

#[pyclass]
pub struct EntryModifications {
  pub silent_external_prefix: Option<String>,
  pub own_prefix: Option<String>,
}


#[pyclass]
pub struct EntrySpecificationList(pub Vec<lib_zip::ZipEntrySpecification>);


#[pyclass]
pub enum Parallelism {
  Synchronous,
  ParallelMerge,
}

#[pyclass]
pub struct MedusaZip {
  pub input_files: Vec<FileSource>,
  pub zip_options: lib_zip::ZipOutputOptions,
  pub modifications: EntryModifications,
  pub parallelism: Parallelism,
}


pub(crate) fn zip_module(py: Python<'_>) -> PyResult<&PyModule> {
  let zip = PyModule::new(py, "zip")?;

  zip.add_class::<AutomaticModifiedTimeStrategy>()?;
  zip.add_class::<ModifiedTimeBehavior>()?;
  zip.add_class::<CompressionMethod>()?;
  zip.add_class::<CompressionOptions>()?;
  zip.add_class::<EntryModifications>()?;
  zip.add_class::<EntrySpecificationList>()?;
  zip.add_class::<Parallelism>()?;
  zip.add_class::<MedusaZip>()?;

  Ok(zip)
}
