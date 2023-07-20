/*
 * Description: ???
 *
 * Copyright (C) 2023 Danny McClanahan <dmcC2@hypnicjerk.ai>
 * SPDX-License-Identifier: Apache-2.0
 *
 * Licensed under the Apache License, Version 2.0 (see LICENSE).
 */

//! ???

use libmedusa_zip::merge as lib_merge;

use pyo3::{
  exceptions::{PyException, PyValueError},
  prelude::*,
  types::PyList,
};


/* #[pyclass] */
/* #[derive(Clone)] */
/* struct MergeGroup { */
/*   pub prefix:  */
/* } */


pub(crate) fn merge_module(py: Python<'_>) -> PyResult<&PyModule> {
  let merge = PyModule::new(py, "merge")?;

  Ok(merge)
}
