/*
 * Description: ???
 *
 * Copyright (C) 2023 Danny McClanahan <dmcC2@hypnicjerk.ai>
 * SPDX-License-Identifier: Apache-2.0
 *
 * Licensed under the Apache License, Version 2.0 (see LICENSE).
 */

//! ???

use crate::{destination::ZipFileWriter, util::repr, zip::ModifiedTimeBehavior, EntryName};

use libmedusa_zip::{self as lib, merge as lib_merge, zip as lib_zip};

use pyo3::{
  exceptions::{PyException, PyValueError},
  intern,
  prelude::*,
};

use std::path::PathBuf;


#[pyclass]
#[derive(Clone)]
pub struct MergeGroup {
  #[pyo3(get)]
  pub prefix: Option<EntryName>,
  #[pyo3(get)]
  pub sources: Vec<PathBuf>,
}

#[pymethods]
impl MergeGroup {
  #[new]
  #[pyo3(signature = (prefix, sources))]
  fn new(py: Python<'_>, prefix: Option<&PyAny>, sources: &PyAny) -> PyResult<Self> {
    let prefix: Option<EntryName> = prefix.map(|p| {
      if p.is_instance_of::<EntryName>() {
        Ok(p.extract()?)
      } else {
        let p = p.into_py(py);
        let p: String = p.call_method0(py, intern!(py, "__str__"))?.extract(py)?;
        let entry_name = lib::EntryName::validate(p)
          /* TODO: better error! */
          .map_err(|e| PyValueError::new_err(format!("{}", e)))?;
        Ok(entry_name.into())
      }
    }).transpose()
      /* TODO: better error! */
      .map_err(|e: PyErr| PyValueError::new_err(format!("{}", e)))?;
    let sources: Vec<PathBuf> = sources
      .iter()?
      .map(|s| s.and_then(PyAny::extract::<PathBuf>))
      .collect::<PyResult<_>>()?;
    Ok(Self { prefix, sources })
  }

  fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
    let Self { prefix, sources } = self;
    let prefix = repr(py, prefix.clone())?;
    let sources = repr(py, sources.clone())?;
    Ok(format!(
      "MergeGroup(prefix={}, sources={})",
      prefix, sources
    ))
  }
}

impl TryFrom<MergeGroup> for lib_merge::MergeGroup {
  type Error = lib::MedusaNameFormatError;

  fn try_from(x: MergeGroup) -> Result<Self, Self::Error> {
    let MergeGroup { prefix, sources } = x;
    Ok(Self {
      prefix: prefix.map(|p| p.try_into()).transpose()?,
      sources,
    })
  }
}

impl From<lib_merge::MergeGroup> for MergeGroup {
  fn from(x: lib_merge::MergeGroup) -> Self {
    let lib_merge::MergeGroup { prefix, sources } = x;
    Self {
      prefix: prefix.map(|p| p.into()),
      sources,
    }
  }
}

#[pyclass]
#[derive(Clone)]
pub struct MedusaMerge {
  #[pyo3(get)]
  pub groups: Vec<MergeGroup>,
}

#[pymethods]
impl MedusaMerge {
  #[new]
  fn new(groups: &PyAny) -> PyResult<Self> {
    let groups: Vec<MergeGroup> = groups
      .iter()?
      .map(|g| g.and_then(PyAny::extract::<MergeGroup>))
      .collect::<PyResult<_>>()?;
    Ok(Self { groups })
  }

  fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
    let Self { groups } = self;
    let groups = repr(py, groups.clone())?;
    Ok(format!("MedusaMerge(groups={})", groups))
  }

  #[cfg(feature = "asyncio")]
  fn merge<'a>(
    &self,
    py: Python<'a>,
    mtime_behavior: ModifiedTimeBehavior,
    output_zip: ZipFileWriter,
  ) -> PyResult<&'a PyAny> {
    let merge: lib_merge::MedusaMerge = self
      .clone()
      .try_into()
      /* TODO: better error! */
      .map_err(|e| PyValueError::new_err(format!("{}", e)))?;
    let mtime_behavior: lib_zip::ModifiedTimeBehavior = mtime_behavior.into();
    let ZipFileWriter {
      output_path,
      zip_writer,
    } = output_zip;
    pyo3_asyncio::tokio::future_into_py(py, async move {
      let zip_writer = merge
        .merge(mtime_behavior, zip_writer)
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
  fn merge_sync(
    &self,
    py: Python,
    mtime_behavior: ModifiedTimeBehavior,
    output_zip: ZipFileWriter,
  ) -> PyResult<ZipFileWriter> {
    let handle = crate::TOKIO_RUNTIME.handle();
    let merge: lib_merge::MedusaMerge = self
      .clone()
      .try_into()
      /* TODO: better error! */
      .map_err(|e| PyValueError::new_err(format!("{}", e)))?;
    let mtime_behavior: lib_zip::ModifiedTimeBehavior = mtime_behavior.into();
    let ZipFileWriter {
      output_path,
      zip_writer,
    } = output_zip;
    py.allow_threads(move || {
      let zip_writer = handle.block_on(merge.merge(mtime_behavior, zip_writer))
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

impl TryFrom<MedusaMerge> for lib_merge::MedusaMerge {
  type Error = lib::MedusaNameFormatError;

  fn try_from(x: MedusaMerge) -> Result<Self, Self::Error> {
    let MedusaMerge { groups } = x;
    Ok(Self {
      groups: groups
        .into_iter()
        .map(|g| g.try_into())
        .collect::<Result<Vec<_>, _>>()?,
    })
  }
}

impl From<lib_merge::MedusaMerge> for MedusaMerge {
  fn from(x: lib_merge::MedusaMerge) -> Self {
    let lib_merge::MedusaMerge { groups } = x;
    Self {
      groups: groups.into_iter().map(|g| g.into()).collect(),
    }
  }
}


pub(crate) fn merge_module(py: Python<'_>) -> PyResult<&PyModule> {
  let merge = PyModule::new(py, "merge")?;

  merge.add_class::<MergeGroup>()?;
  merge.add_class::<MedusaMerge>()?;

  Ok(merge)
}
