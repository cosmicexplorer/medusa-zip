/*
 * Description: ???
 *
 * Copyright (C) 2023 Danny McClanahan <dmcC2@hypnicjerk.ai>
 * SPDX-License-Identifier: Apache-2.0
 *
 * Licensed under the Apache License, Version 2.0 (see LICENSE).
 */

//! ???

use crate::EntryName;

use libmedusa_zip::{self as lib, merge as lib_merge};

use pyo3::{
  exceptions::{PyException, PyValueError},
  intern,
  prelude::*,
  types::PyList,
};

use std::{
  convert::{TryFrom, TryInto},
  path::PathBuf,
};


#[pyclass]
#[derive(Clone)]
struct MergeGroup {
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
      if p.is_instance_of::<EntryName>()? {
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
struct MedusaMerge {
  #[pyo3(get)]
  pub groups: Vec<MergeGroup>,
}

#[pymethods]
impl MedusaMerge {
  #[new]
  fn new(groups: Option<&PyAny>) -> PyResult<Self> {
    let groups: Vec<MergeGroup> = groups
      .map(|gs| {
        gs.iter()?
          .map(|g| g.and_then(PyAny::extract::<MergeGroup>))
          .collect::<PyResult<_>>()
      })
      .transpose()?
      .unwrap_or_default();
    Ok(Self { groups })
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
