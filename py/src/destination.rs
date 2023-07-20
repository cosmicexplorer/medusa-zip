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
pub struct ZipFileWriter(pub ZipWriter<File>);


#[pymethods]
impl DestinationBehavior {
  fn initialize<'a>(&self, py: Python<'a>, path: PathBuf) -> PyResult<&'a PyAny> {
    let behavior: lib_destination::DestinationBehavior = (*self).into();
    pyo3_asyncio::tokio::future_into_py(py, async move {
      let ret = behavior
        .initialize(&path)
        .await
        .map_err(|e| PyIOError::new_err(format!("{}", e)))?;
      Ok(ZipFileWriter(ret))
    })
  }
}


pub(crate) fn destination_module(py: Python<'_>) -> PyResult<&PyModule> {
  let destination = PyModule::new(py, "destination")?;

  destination.add_class::<DestinationBehavior>()?;
  destination.add_class::<ZipFileWriter>()?;

  Ok(destination)
}
