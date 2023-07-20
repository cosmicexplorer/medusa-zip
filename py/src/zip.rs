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


#[pyclass]
#[derive(Clone)]
pub struct ModifiedTimeBehavior;

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
