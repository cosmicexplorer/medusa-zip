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

use pyo3::{
  exceptions::PyIOError,
  intern,
  prelude::*,
  types::{PyBool, PyType},
};
use zip::write::ZipWriter;

use std::{fs::File, path::PathBuf};


#[pyclass]
#[derive(Copy, Clone)]
pub enum DestinationBehavior {
  AlwaysTruncate,
  AppendOrFail,
  OptimisticallyAppend,
  AppendToNonZip,
}

impl Default for DestinationBehavior {
  fn default() -> Self { lib_destination::DestinationBehavior::default().into() }
}

#[pymethods]
impl DestinationBehavior {
  #[classmethod]
  #[pyo3(name = "default")]
  fn py_default(_cls: &PyType) -> Self { Self::default() }

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
  fn initialize_sync(&self, py: Python, path: PathBuf) -> PyResult<ZipFileWriter> {
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
  #[pyo3(get)]
  pub output_path: PathBuf,
  pub zip_writer: lib_destination::OutputWrapper<ZipWriter<File>>,
}

#[pymethods]
impl ZipFileWriter {
  fn __str__(&self) -> String { format!("ZipFileWriter(output_path={:?}, ...)", &self.output_path) }

  #[cfg(feature = "asyncio")]
  fn finish<'a>(&self, py: Python<'a>) -> PyResult<&'a PyAny> {
    let Self {
      output_path,
      zip_writer,
    } = self.clone();
    pyo3_asyncio::tokio::future_into_py(py, async move {
      tokio::task::spawn_blocking(move || {
        let file = zip_writer
          .clone()
          .lease()
          .finish()
          .map_err(|e| PyIOError::new_err(format!("{}", e)))?;
        file
          .sync_all()
          .map_err(|e| PyIOError::new_err(format!("{}", e)))?;
        Ok::<_, PyErr>(output_path)
      })
      .await
      .map_err(|e| PyIOError::new_err(format!("{}", e)))?
    })
  }

  #[cfg(feature = "asyncio")]
  fn __aenter__<'a>(&self, py: Python<'a>) -> PyResult<&'a PyAny> {
    let obj = self.clone().into_py(py);
    pyo3_asyncio::tokio::future_into_py(py, async move { Ok(obj) })
  }

  #[cfg(feature = "asyncio")]
  fn __aexit__<'a>(
    &self,
    py: Python<'a>,
    _exc_type: &PyAny,
    _exc_val: &PyAny,
    _traceback: &PyAny,
  ) -> PyResult<&'a PyAny> {
    let obj = self.clone().into_py(py);
    let res =
      pyo3_asyncio::tokio::into_future(obj.call_method0(py, intern!(py, "finish"))?.as_ref(py))?;
    pyo3_asyncio::tokio::future_into_py(py, async move {
      let _res = res.await?;
      Ok(Python::with_gil(|py| {
        let ret: Py<PyBool> = PyBool::new(py, false).into();
        ret
      }))
    })
  }

  #[cfg(feature = "sync")]
  fn finish_sync(&self, py: Python) -> PyResult<PathBuf> {
    let handle = crate::TOKIO_RUNTIME.handle();
    let Self {
      output_path,
      zip_writer,
    } = self.clone();
    py.allow_threads(move || {
      handle.block_on(async move {
        handle
          .spawn_blocking(move || {
            let file = zip_writer
              .clone()
              .lease()
              .finish()
              .map_err(|e| PyIOError::new_err(format!("{}", e)))?;
            file
              .sync_all()
              .map_err(|e| PyIOError::new_err(format!("{}", e)))?;
            Ok::<_, PyErr>(output_path)
          })
          .await
          .map_err(|e| PyIOError::new_err(format!("{}", e)))?
      })
    })
  }

  #[cfg(feature = "sync")]
  fn __enter__(&self, py: Python) -> Py<PyAny> { self.clone().into_py(py) }

  #[cfg(feature = "sync")]
  fn __exit__<'a>(
    &self,
    py: Python<'a>,
    _exc_type: &PyAny,
    _exc_val: &PyAny,
    _traceback: &PyAny,
  ) -> PyResult<&'a PyBool> {
    let obj = self.clone().into_py(py);
    let _res = obj.call_method0(py, intern!(py, "finish_sync"))?;
    Ok(PyBool::new(py, false))
  }
}


pub(crate) fn destination_module(py: Python<'_>) -> PyResult<&PyModule> {
  let destination = PyModule::new(py, "destination")?;

  destination.add_class::<DestinationBehavior>()?;
  destination.add_class::<ZipFileWriter>()?;

  Ok(destination)
}
