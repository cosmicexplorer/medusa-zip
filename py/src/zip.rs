/*
 * Description: ???
 *
 * Copyright (C) 2023 Danny McClanahan <dmcC2@hypnicjerk.ai>
 * SPDX-License-Identifier: Apache-2.0
 *
 * Licensed under the Apache License, Version 2.0 (see LICENSE).
 */

//! ???

use crate::{destination::ZipFileWriter, FileSource};

use libmedusa_zip::{self as lib, zip as lib_zip};

use pyo3::{
  exceptions::{PyException, PyValueError},
  intern,
  prelude::*,
  types::PyType,
};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use zip::DateTime as ZipDateTime;


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


#[pyclass(name = "ZipDateTime")]
#[derive(Clone)]
pub struct ZipDateTimeWrapper {
  /* TODO: figure out a way to record only the timestamp and round-trip through OffsetDateTime
   * (possibly by editing types.rs in the zip crate) to avoid needing to retain the input
   * string! This also lets us make this Copy, along with ModifiedTimeBehavior and
   * ZipOutputOptions! */
  pub input_string: String,
  pub timestamp: ZipDateTime,
}

#[pymethods]
impl ZipDateTimeWrapper {
  #[classmethod]
  fn parse<'a>(_cls: &'a PyType, s: &str) -> PyResult<Self> {
    let parsed_offset = OffsetDateTime::parse(s, &Rfc3339)
      /* TODO: better error! */
      .map_err(|e| PyValueError::new_err(format!("{}", e)))?;
    let zip_time: ZipDateTime = parsed_offset.try_into()
      /* TODO: better error! */
      .map_err(|e| PyValueError::new_err(format!("{}", e)))?;
    Ok(Self {
      input_string: s.to_string(),
      timestamp: zip_time,
    })
  }

  fn __repr__(&self) -> String {
    let Self { input_string, .. } = self;
    format!("ZipDateTime.parse({:?})", input_string)
  }
}

impl From<ZipDateTimeWrapper> for ZipDateTime {
  fn from(x: ZipDateTimeWrapper) -> Self {
    let ZipDateTimeWrapper { timestamp, .. } = x;
    timestamp
  }
}


#[pyclass]
#[derive(Clone)]
pub struct ModifiedTimeBehavior {
  pub automatic_mtime_strategy: AutomaticModifiedTimeStrategy,
  pub explicit_mtime_timestamp: Option<ZipDateTimeWrapper>,
}

#[pymethods]
impl ModifiedTimeBehavior {
  #[classmethod]
  fn automatic(_cls: &PyType, automatic_mtime_strategy: AutomaticModifiedTimeStrategy) -> Self {
    Self::internal_automatic(automatic_mtime_strategy.into())
  }

  #[classmethod]
  fn explicit(_cls: &PyType, timestamp: ZipDateTimeWrapper) -> Self {
    Self::internal_explicit(timestamp.into())
  }

  fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
    let Self {
      automatic_mtime_strategy,
      explicit_mtime_timestamp,
    } = self;
    match explicit_mtime_timestamp {
      None => {
        let automatic_mtime_strategy = automatic_mtime_strategy.clone().into_py(py);
        let automatic_mtime_strategy: String = automatic_mtime_strategy
          .call_method0(py, intern!(py, "__repr__"))?
          .extract(py)?;
        Ok(format!(
          "ModifiedTimeBehavior.automatic({})",
          automatic_mtime_strategy
        ))
      },
      Some(explicit_mtime_timestamp) => {
        let explicit_mtime_timestamp = explicit_mtime_timestamp.clone().into_py(py);
        let explicit_mtime_timestamp: String = explicit_mtime_timestamp
          .call_method0(py, intern!(py, "__repr__"))?
          .extract(py)?;
        Ok(format!(
          "ModifiedTimeBehavior.explicit({})",
          explicit_mtime_timestamp
        ))
      },
    }
  }
}

impl ModifiedTimeBehavior {
  fn internal_automatic(automatic_mtime_strategy: AutomaticModifiedTimeStrategy) -> Self {
    Self {
      automatic_mtime_strategy,
      explicit_mtime_timestamp: Default::default(),
    }
  }

  fn internal_explicit(timestamp: ZipDateTimeWrapper) -> Self {
    Self {
      explicit_mtime_timestamp: Some(timestamp),
      automatic_mtime_strategy: lib_zip::AutomaticModifiedTimeStrategy::default().into(),
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
      Some(timestamp) => Self::Explicit(timestamp.into()),
      None => match automatic_mtime_strategy {
        AutomaticModifiedTimeStrategy::Reproducible => Self::Reproducible,
        AutomaticModifiedTimeStrategy::CurrentTime => Self::CurrentTime,
        AutomaticModifiedTimeStrategy::PreserveSourceTime => Self::PreserveSourceTime,
      },
    }
  }
}

#[pyclass]
#[derive(Copy, Clone)]
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
#[derive(Clone)]
pub struct CompressionOptions {
  #[pyo3(get, name = "method")]
  pub compression_method: CompressionMethod,
  #[pyo3(get, name = "level")]
  pub compression_level: Option<i8>,
}

#[pymethods]
impl CompressionOptions {
  #[new]
  fn new(method: CompressionMethod, level: Option<i8>) -> Self {
    Self {
      compression_method: method,
      compression_level: level,
    }
  }

  fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
    let Self {
      compression_method,
      compression_level,
    } = self;
    let method = compression_method.clone().into_py(py);
    let level = compression_level.clone().into_py(py);
    let method: String = method
      .call_method0(py, intern!(py, "__repr__"))?
      .extract(py)?;
    let level: String = level
      .call_method0(py, intern!(py, "__repr__"))?
      .extract(py)?;
    Ok(format!(
      "CompressionOptions(method={}, level={})",
      method, level
    ))
  }
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
#[derive(Clone)]
pub struct ZipOutputOptions {
  #[pyo3(get)]
  pub mtime_behavior: ModifiedTimeBehavior,
  #[pyo3(get)]
  pub compression_options: CompressionOptions,
}

#[pymethods]
impl ZipOutputOptions {
  #[new]
  fn new(mtime_behavior: ModifiedTimeBehavior, compression_options: CompressionOptions) -> Self {
    Self {
      mtime_behavior,
      compression_options,
    }
  }

  fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
    let Self {
      mtime_behavior,
      compression_options,
    } = self;
    let mtime_behavior = mtime_behavior.clone().into_py(py);
    let compression_options = compression_options.clone().into_py(py);
    let mtime_behavior: String = mtime_behavior
      .call_method0(py, intern!(py, "__repr__"))?
      .extract(py)?;
    let compression_options: String = compression_options
      .call_method0(py, intern!(py, "__repr__"))?
      .extract(py)?;
    Ok(format!(
      "ZipOutputOptions(mtime_behavior={}, compression_options={})",
      mtime_behavior, compression_options
    ))
  }
}


impl TryFrom<ZipOutputOptions> for lib_zip::ZipOutputOptions {
  type Error = lib_zip::ParseCompressionOptionsError;

  fn try_from(x: ZipOutputOptions) -> Result<Self, Self::Error> {
    let ZipOutputOptions {
      mtime_behavior,
      compression_options,
    } = x;
    let mtime_behavior: lib_zip::ModifiedTimeBehavior = mtime_behavior.into();
    let compression_options: lib_zip::CompressionStrategy = compression_options.try_into()?;
    Ok(Self {
      mtime_behavior,
      compression_options,
    })
  }
}


#[pyclass]
#[derive(Clone)]
pub struct EntryModifications {
  #[pyo3(get)]
  pub silent_external_prefix: Option<String>,
  #[pyo3(get)]
  pub own_prefix: Option<String>,
}

#[pymethods]
impl EntryModifications {
  #[new]
  fn new(silent_external_prefix: Option<String>, own_prefix: Option<String>) -> Self {
    Self {
      silent_external_prefix,
      own_prefix,
    }
  }

  fn __repr__(&self) -> String {
    let Self {
      silent_external_prefix,
      own_prefix,
    } = self;
    let silent_external_prefix = silent_external_prefix
      .as_ref()
      .map(|s| s.as_str())
      .unwrap_or("None");
    let own_prefix = own_prefix.as_ref().map(|s| s.as_str()).unwrap_or("None");
    format!(
      "EntryModifications(silent_external_prefix={}, own_prefix={})",
      silent_external_prefix, own_prefix
    )
  }
}

impl From<lib_zip::EntryModifications> for EntryModifications {
  fn from(x: lib_zip::EntryModifications) -> Self {
    let lib_zip::EntryModifications {
      silent_external_prefix,
      own_prefix,
    } = x;
    Self {
      silent_external_prefix,
      own_prefix,
    }
  }
}

impl From<EntryModifications> for lib_zip::EntryModifications {
  fn from(x: EntryModifications) -> Self {
    let EntryModifications {
      silent_external_prefix,
      own_prefix,
    } = x;
    Self {
      silent_external_prefix,
      own_prefix,
    }
  }
}


#[pyclass]
#[derive(Copy, Clone)]
pub enum Parallelism {
  Synchronous,
  ParallelMerge,
}

impl From<lib_zip::Parallelism> for Parallelism {
  fn from(x: lib_zip::Parallelism) -> Self {
    match x {
      lib_zip::Parallelism::Synchronous => Self::Synchronous,
      lib_zip::Parallelism::ParallelMerge => Self::ParallelMerge,
    }
  }
}

impl From<Parallelism> for lib_zip::Parallelism {
  fn from(x: Parallelism) -> Self {
    match x {
      Parallelism::Synchronous => Self::Synchronous,
      Parallelism::ParallelMerge => Self::ParallelMerge,
    }
  }
}


#[pyclass]
#[derive(Clone)]
pub struct MedusaZip {
  pub input_files: Vec<FileSource>,
  pub zip_options: ZipOutputOptions,
  pub modifications: EntryModifications,
  pub parallelism: Parallelism,
}

#[pymethods]
impl MedusaZip {
  #[new]
  fn new(
    input_files: &PyAny,
    zip_options: ZipOutputOptions,
    modifications: EntryModifications,
    parallelism: Parallelism,
  ) -> PyResult<Self> {
    let input_files: Vec<FileSource> = input_files
      .iter()?
      .map(|f| f.and_then(PyAny::extract::<FileSource>))
      .collect::<PyResult<_>>()?;
    Ok(Self {
      input_files,
      zip_options,
      modifications,
      parallelism,
    })
  }

  fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
    let Self {
      input_files,
      zip_options,
      modifications,
      parallelism,
    } = self;
    let input_files = input_files.clone().into_py(py);
    let zip_options = zip_options.clone().into_py(py);
    let modifications = modifications.clone().into_py(py);
    let parallelism = parallelism.clone().into_py(py);
    let input_files: String = input_files
      .call_method0(py, intern!(py, "__repr__"))?
      .extract(py)?;
    let zip_options: String = zip_options
      .call_method0(py, intern!(py, "__repr__"))?
      .extract(py)?;
    let modifications: String = modifications
      .call_method0(py, intern!(py, "__repr__"))?
      .extract(py)?;
    let parallelism: String = parallelism
      .call_method0(py, intern!(py, "__repr__"))?
      .extract(py)?;
    Ok(format!(
      "MedusaZip(input_files={}, zip_options={}, modifications={}, parallelism={})",
      input_files, zip_options, modifications, parallelism
    ))
  }

  #[cfg(feature = "asyncio")]
  fn zip<'a>(&self, py: Python<'a>, output_zip: ZipFileWriter) -> PyResult<&'a PyAny> {
    let zip: lib_zip::MedusaZip = self.clone().try_into()?;
    let ZipFileWriter {
      output_path,
      zip_writer,
    } = output_zip;
    pyo3_asyncio::tokio::future_into_py(py, async move {
      let zip_writer = zip.zip(zip_writer)
        .await
        /* TODO: better error! */
        .map_err(|e| PyException::new_err(format!("{}", e)))?;
      let output_zip = ZipFileWriter {
        output_path,
        zip_writer,
      };
      Ok::<_, PyErr>(output_zip)
    })
  }

  #[cfg(feature = "sync")]
  fn zip_sync<'a>(&self, py: Python<'a>, output_zip: ZipFileWriter) -> PyResult<ZipFileWriter> {
    let handle = crate::TOKIO_RUNTIME.handle();
    let zip: lib_zip::MedusaZip = self.clone().try_into()?;
    let ZipFileWriter {
      output_path,
      zip_writer,
    } = output_zip;
    py.allow_threads(move || {
      let zip_writer = handle.block_on(zip.zip(zip_writer))
        /* TODO: better error! */
        .map_err(|e| PyException::new_err(format!("{}", e)))?;
      let output_zip = ZipFileWriter {
        output_path,
        zip_writer,
      };
      Ok::<_, PyErr>(output_zip)
    })
  }
}


impl TryFrom<MedusaZip> for lib_zip::MedusaZip {
  type Error = PyErr;

  fn try_from(x: MedusaZip) -> Result<Self, Self::Error> {
    let MedusaZip {
      input_files,
      zip_options,
      modifications,
      parallelism,
    } = x;
    let input_files: Vec<lib::FileSource> = input_files
      .into_iter()
      .map(lib::FileSource::try_from)
      .collect::<Result<Vec<_>, _>>()
      /* TODO: better error! */
      .map_err(|e| PyValueError::new_err(format!("{}", e)))?;
    let zip_options: lib_zip::ZipOutputOptions = zip_options.try_into()
      /* TODO: better error! */
      .map_err(|e| PyValueError::new_err(format!("{}", e)))?;
    let modifications: lib_zip::EntryModifications = modifications.into();
    let parallelism: lib_zip::Parallelism = parallelism.into();
    Ok(Self {
      input_files,
      zip_options,
      modifications,
      parallelism,
    })
  }
}


pub(crate) fn zip_module(py: Python<'_>) -> PyResult<&PyModule> {
  let zip = PyModule::new(py, "zip")?;

  zip.add_class::<AutomaticModifiedTimeStrategy>()?;
  zip.add_class::<ZipDateTimeWrapper>()?;
  zip.add_class::<ModifiedTimeBehavior>()?;
  zip.add_class::<CompressionMethod>()?;
  zip.add_class::<CompressionOptions>()?;
  zip.add_class::<ZipOutputOptions>()?;
  zip.add_class::<EntryModifications>()?;
  zip.add_class::<Parallelism>()?;
  zip.add_class::<MedusaZip>()?;

  Ok(zip)
}
