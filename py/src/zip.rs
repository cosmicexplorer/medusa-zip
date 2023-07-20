/*
 * Description: ???
 *
 * Copyright (C) 2023 Danny McClanahan <dmcC2@hypnicjerk.ai>
 * SPDX-License-Identifier: Apache-2.0
 *
 * Licensed under the Apache License, Version 2.0 (see LICENSE).
 */

//! ???

use libmedusa_zip::zip as lib_zip;

use pyo3::prelude::*;
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


#[pyclass]
#[derive(Copy, Clone)]
pub struct ModifiedTimeBehavior {
  automatic_mtime_strategy: AutomaticModifiedTimeStrategy,
  explicit_mtime_timestamp: Option<ZipDateTime>,
}

impl From<lib_zip::ModifiedTimeBehavior> for ModifiedTimeBehavior {
  fn from(x: lib_zip::ModifiedTimeBehavior) -> Self {}
}

impl From<ModifiedTimeBehavior> for lib_zip::ModifiedTimeBehavior {
  fn from(x: ModifiedTimeBehavior) -> Self {}
}


pub(crate) fn zip_module(py: Python<'_>) -> PyResult<&PyModule> {
  let zip = PyModule::new(py, "zip")?;

  zip.add_class::<ModifiedTimeBehavior>()?;

  Ok(zip)
}
