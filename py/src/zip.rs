/*
 * Description: ???
 *
 * Copyright (C) 2023 Danny McClanahan <dmcC2@hypnicjerk.ai>
 * SPDX-License-Identifier: Apache-2.0
 *
 * Licensed under the Apache License, Version 2.0 (see LICENSE).
 */

//! ???

use crate::{destination::ZipFileWriter, util::repr, FileSource};

use libmedusa_zip::{self as lib, zip as lib_zip};

use pyo3::{
  exceptions::{PyException, PyValueError},
  prelude::*,
  types::{PyDateAccess, PyDateTime, PyTimeAccess, PyType},
};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use zip::DateTime as ZipDateTime;


#[pyclass]
#[derive(Copy, Clone, Default)]
pub enum AutomaticModifiedTimeStrategy {
  #[default]
  Reproducible,
  CurrentTime,
  PreserveSourceTime,
}

#[pymethods]
impl AutomaticModifiedTimeStrategy {
  #[classmethod]
  #[pyo3(name = "default")]
  fn py_default(_cls: &PyType) -> Self { Self::default() }
}


#[pyclass(name = "ZipDateTime")]
#[derive(Copy, Clone)]
pub struct ZipDateTimeWrapper {
  pub timestamp: ZipDateTime,
}

#[pymethods]
impl ZipDateTimeWrapper {
  #[new]
  fn new(year: u16, month: u8, day: u8, hour: u8, minute: u8, second: u8) -> PyResult<Self> {
    let timestamp = ZipDateTime::from_date_and_time(year, month, day, hour, minute, second)
      /* TODO: better error! */
      .map_err(|e| PyValueError::new_err(format!("{}", e)))?;
    Ok(Self { timestamp })
  }

  #[getter]
  fn year(&self) -> u16 { self.timestamp.year() }

  #[getter]
  fn month(&self) -> u8 { self.timestamp.month() }

  #[getter]
  fn day(&self) -> u8 { self.timestamp.day() }

  #[getter]
  fn hour(&self) -> u8 { self.timestamp.hour() }

  #[getter]
  fn minute(&self) -> u8 { self.timestamp.minute() }

  #[getter]
  fn second(&self) -> u8 { self.timestamp.second() }

  #[classmethod]
  fn from_datetime(_cls: &PyType, py_datetime: &PyDateTime) -> PyResult<Self> {
    let year: u16 = py_datetime.get_year().try_into()
      /* TODO: better error! */
      .map_err(|e| PyValueError::new_err(format!("{}", e)))?;
    let month: u8 = py_datetime.get_month();
    let day: u8 = py_datetime.get_day();
    let hour: u8 = py_datetime.get_hour();
    let minute: u8 = py_datetime.get_minute();
    let second: u8 = py_datetime.get_second();
    Self::new(year, month, day, hour, minute, second)
  }

  /// Parse an [RFC 3339] timestamp with UTC offset such as
  /// '1985-04-12T23:20:50.52Z'.
  ///
  /// [RFC 3339]: https://datatracker.ietf.org/doc/html/rfc3339#section-5.6
  #[classmethod]
  fn parse(_cls: &PyType, s: &str) -> PyResult<Self> {
    let parsed_offset = OffsetDateTime::parse(s, &Rfc3339)
      /* TODO: better error! */
      .map_err(|e| PyValueError::new_err(format!("{}", e)))?;
    let timestamp: ZipDateTime = parsed_offset.try_into()
      /* TODO: better error! */
      .map_err(|e| PyValueError::new_err(format!("{}", e)))?;
    Ok(Self { timestamp })
  }

  fn __repr__(&self) -> String {
    format!(
      "ZipDateTime(year={}, month={}, day={}, hour={}, minute={}, second={})",
      self.year(),
      self.month(),
      self.day(),
      self.hour(),
      self.minute(),
      self.second(),
    )
  }
}

impl From<ZipDateTimeWrapper> for ZipDateTime {
  fn from(x: ZipDateTimeWrapper) -> Self {
    let ZipDateTimeWrapper { timestamp } = x;
    timestamp
  }
}

impl From<ZipDateTime> for ZipDateTimeWrapper {
  fn from(x: ZipDateTime) -> Self { Self { timestamp: x } }
}


#[pyclass]
#[derive(Copy, Clone)]
pub struct ModifiedTimeBehavior {
  pub automatic_mtime_strategy: AutomaticModifiedTimeStrategy,
  pub explicit_mtime_timestamp: Option<ZipDateTimeWrapper>,
}

impl Default for ModifiedTimeBehavior {
  fn default() -> Self { lib_zip::ModifiedTimeBehavior::default().into() }
}

#[pymethods]
impl ModifiedTimeBehavior {
  #[classmethod]
  fn automatic(_cls: &PyType, automatic_mtime_strategy: AutomaticModifiedTimeStrategy) -> Self {
    Self::internal_automatic(automatic_mtime_strategy)
  }

  #[classmethod]
  fn explicit(_cls: &PyType, timestamp: ZipDateTimeWrapper) -> Self {
    Self::internal_explicit(timestamp)
  }

  #[classmethod]
  #[pyo3(name = "default")]
  fn py_default(_cls: &PyType) -> Self {
    Self::internal_automatic(AutomaticModifiedTimeStrategy::default())
  }

  fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
    let Self {
      automatic_mtime_strategy,
      explicit_mtime_timestamp,
    } = self;
    match explicit_mtime_timestamp {
      None => {
        let automatic_mtime_strategy = repr(py, *automatic_mtime_strategy)?;
        Ok(format!(
          "ModifiedTimeBehavior.automatic({})",
          automatic_mtime_strategy
        ))
      },
      Some(explicit_mtime_timestamp) => {
        let explicit_mtime_timestamp = repr(py, *explicit_mtime_timestamp)?;
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
      explicit_mtime_timestamp: None,
    }
  }

  fn internal_explicit(timestamp: ZipDateTimeWrapper) -> Self {
    Self {
      explicit_mtime_timestamp: Some(timestamp),
      automatic_mtime_strategy: AutomaticModifiedTimeStrategy::default(),
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

impl From<lib_zip::ModifiedTimeBehavior> for ModifiedTimeBehavior {
  fn from(x: lib_zip::ModifiedTimeBehavior) -> Self {
    match x {
      lib_zip::ModifiedTimeBehavior::Explicit(timestamp) => {
        Self::internal_explicit(timestamp.into())
      },
      lib_zip::ModifiedTimeBehavior::Reproducible => {
        Self::internal_automatic(AutomaticModifiedTimeStrategy::Reproducible)
      },
      lib_zip::ModifiedTimeBehavior::CurrentTime => {
        Self::internal_automatic(AutomaticModifiedTimeStrategy::CurrentTime)
      },
      lib_zip::ModifiedTimeBehavior::PreserveSourceTime => {
        Self::internal_automatic(AutomaticModifiedTimeStrategy::PreserveSourceTime)
      },
    }
  }
}

#[pyclass]
#[derive(Copy, Clone)]
pub enum CompressionMethod {
  Stored,
  Deflated,
}

impl Default for CompressionMethod {
  fn default() -> Self { lib_zip::CompressionMethod::default().into() }
}

#[pymethods]
impl CompressionMethod {
  #[classmethod]
  #[pyo3(name = "default")]
  fn py_default(_cls: &PyType) -> Self { Self::default() }
}

impl From<CompressionMethod> for lib_zip::CompressionMethod {
  fn from(x: CompressionMethod) -> Self {
    match x {
      CompressionMethod::Stored => Self::Stored,
      CompressionMethod::Deflated => Self::Deflated,
    }
  }
}

impl From<lib_zip::CompressionMethod> for CompressionMethod {
  fn from(x: lib_zip::CompressionMethod) -> Self {
    match x {
      lib_zip::CompressionMethod::Stored => Self::Stored,
      lib_zip::CompressionMethod::Deflated => Self::Deflated,
    }
  }
}


#[pyclass]
#[derive(Copy, Clone)]
pub struct CompressionOptions {
  #[pyo3(get)]
  pub method: CompressionMethod,
  #[pyo3(get)]
  pub level: Option<i8>,
}

impl Default for CompressionOptions {
  fn default() -> Self { lib_zip::CompressionStrategy::default().into() }
}

#[pymethods]
impl CompressionOptions {
  #[new]
  fn new(method: CompressionMethod, level: Option<i8>) -> Self { Self { method, level } }

  #[classmethod]
  #[pyo3(name = "default")]
  fn py_default(_cls: &PyType) -> Self { Self::default() }

  fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
    let Self { method, level } = self;
    let method = repr(py, *method)?;
    let level = repr(py, *level)?;
    Ok(format!(
      "CompressionOptions(method={}, level={})",
      method, level
    ))
  }
}

impl TryFrom<CompressionOptions> for lib_zip::CompressionStrategy {
  type Error = lib_zip::ParseCompressionOptionsError;

  fn try_from(x: CompressionOptions) -> Result<Self, Self::Error> {
    let CompressionOptions { method, level } = x;
    let method: lib_zip::CompressionMethod = method.into();
    Self::from_method_and_level(method, level)
  }
}

impl From<lib_zip::CompressionStrategy> for CompressionOptions {
  fn from(x: lib_zip::CompressionStrategy) -> Self {
    let (method, level) = match x {
      lib_zip::CompressionStrategy::Stored => (CompressionMethod::Stored, None),
      /* TODO: avoid unchecked cast here! */
      lib_zip::CompressionStrategy::Deflated(level) => {
        (CompressionMethod::Deflated, level.map(|l| l as i8))
      },
    };
    Self { method, level }
  }
}


#[pyclass]
#[derive(Copy, Clone)]
pub struct ZipOutputOptions {
  #[pyo3(get)]
  pub mtime_behavior: ModifiedTimeBehavior,
  #[pyo3(get)]
  pub compression_options: CompressionOptions,
}

impl Default for ZipOutputOptions {
  fn default() -> Self { lib_zip::ZipOutputOptions::default().into() }
}

#[pymethods]
impl ZipOutputOptions {
  #[new]
  fn new(
    mtime_behavior: Option<ModifiedTimeBehavior>,
    compression_options: Option<CompressionOptions>,
  ) -> Self {
    let mtime_behavior = mtime_behavior.unwrap_or_default();
    let compression_options = compression_options.unwrap_or_default();
    Self {
      mtime_behavior,
      compression_options,
    }
  }

  #[classmethod]
  #[pyo3(name = "default")]
  fn py_default(_cls: &PyType) -> Self { Self::default() }

  fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
    let Self {
      mtime_behavior,
      compression_options,
    } = self;
    let mtime_behavior = repr(py, *mtime_behavior)?;
    let compression_options = repr(py, *compression_options)?;
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

impl From<lib_zip::ZipOutputOptions> for ZipOutputOptions {
  fn from(x: lib_zip::ZipOutputOptions) -> Self {
    let lib_zip::ZipOutputOptions {
      mtime_behavior,
      compression_options,
    } = x;
    let mtime_behavior: ModifiedTimeBehavior = mtime_behavior.into();
    let compression_options: CompressionOptions = compression_options.into();
    Self {
      mtime_behavior,
      compression_options,
    }
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

impl Default for EntryModifications {
  fn default() -> Self { lib_zip::EntryModifications::default().into() }
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

  #[classmethod]
  #[pyo3(name = "default")]
  fn py_default(_cls: &PyType) -> Self { Self::default() }

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

impl Default for Parallelism {
  fn default() -> Self { lib_zip::Parallelism::default().into() }
}

#[pymethods]
impl Parallelism {
  #[classmethod]
  #[pyo3(name = "default")]
  fn py_default(_cls: &PyType) -> Self { Self::default() }
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
    zip_options: Option<ZipOutputOptions>,
    modifications: Option<EntryModifications>,
    parallelism: Option<Parallelism>,
  ) -> PyResult<Self> {
    let zip_options = zip_options.unwrap_or_default();
    let modifications = modifications.unwrap_or_default();
    let parallelism = parallelism.unwrap_or_default();
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
    let input_files = repr(py, input_files.clone())?;
    let zip_options = repr(py, *zip_options)?;
    let modifications = repr(py, modifications.clone())?;
    let parallelism = repr(py, *parallelism)?;
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
      /* TODO: make a wrapper for this packing/unpacking of ZipFileWriter! */
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
  fn zip_sync(&self, py: Python, output_zip: ZipFileWriter) -> PyResult<ZipFileWriter> {
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

impl From<lib_zip::MedusaZip> for MedusaZip {
  fn from(x: lib_zip::MedusaZip) -> Self {
    let lib_zip::MedusaZip {
      input_files,
      zip_options,
      modifications,
      parallelism,
    } = x;
    let input_files: Vec<FileSource> = input_files.into_iter().map(|fs| fs.into()).collect();
    let zip_options: ZipOutputOptions = zip_options.into();
    let modifications: EntryModifications = modifications.into();
    let parallelism: Parallelism = parallelism.into();
    Self {
      input_files,
      zip_options,
      modifications,
      parallelism,
    }
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
