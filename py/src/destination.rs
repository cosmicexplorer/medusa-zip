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

use pyo3::{exceptions::PyIOError, prelude::*};
use zip::write::ZipWriter;

use std::{fs::File, path::PathBuf};


#[pyclass]
#[derive(Copy, Clone, PartialEq, Eq)]
pub enum DestinationBehavior {
  AlwaysTruncate,
  AppendOrFail,
  OptimisticallyAppend,
  AppendToNonZip,
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


#[pyclass]
#[derive(Clone)]
pub struct ZipFileWriter {
  pub output_path: PathBuf,
  pub zip_writer: lib_destination::OutputWrapper<ZipWriter<File>>,
}

#[pymethods]
impl ZipFileWriter {
  fn __str__(&self) -> String { format!("ZipFileWriter(output_path={:?}, ...)", &self.output_path) }
}


#[pymethods]
impl DestinationBehavior {
  #[cfg(feature = "asyncio")]
  fn initialize<'a>(&self, py: Python<'a>, path: PathBuf) -> PyResult<&'a PyAny> {
    let behavior: lib_destination::DestinationBehavior = (*self).into();
    pyo3_asyncio::tokio::future_into_py(py, async move {
      let output_zip_writer: ZipWriter<File> = behavior
        .initialize(&path)
        .await
        /* TODO: better error! */
        .map_err(|e| PyIOError::new_err(format!("{}", e)))?;
      let output_wrapper = lib_destination::OutputWrapper::wrap(output_zip_writer);
      Ok(ZipFileWriter {
        output_path: path,
        zip_writer: output_wrapper,
      })
    })
  }

  #[cfg(feature = "sync")]
  fn initialize_sync<'a>(&self, py: Python<'a>, path: PathBuf) -> PyResult<ZipFileWriter> {
    let handle = crate::TOKIO_RUNTIME.handle();
    let behavior: lib_destination::DestinationBehavior = (*self).into();
    py.allow_threads(move || {
      let output_zip_writer: ZipWriter<File> = handle
        .block_on(behavior.initialize(&path))
        /* TODO: better error! */
        .map_err(|e| PyIOError::new_err(format!("{}", e)))?;
      let output_wrapper = lib_destination::OutputWrapper::wrap(output_zip_writer);
      Ok::<_, PyErr>(ZipFileWriter {
        output_path: path,
        zip_writer: output_wrapper,
      })
    })
  }
}


pub(crate) fn destination_module(py: Python<'_>) -> PyResult<&PyModule> {
  let destination = PyModule::new(py, "destination")?;

  destination.add_class::<DestinationBehavior>()?;
  destination.add_class::<ZipFileWriter>()?;

  Ok(destination)
}
