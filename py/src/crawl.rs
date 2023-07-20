/*
 * Description: ???
 *
 * Copyright (C) 2023 Danny McClanahan <dmcC2@hypnicjerk.ai>
 * SPDX-License-Identifier: Apache-2.0
 *
 * Licensed under the Apache License, Version 2.0 (see LICENSE).
 */

//! ???

use pyo3::prelude::*;


#[pyfunction]
fn func() -> String { "func".to_string() }

pub(crate) fn crawl_module(py: Python<'_>) -> PyResult<&PyModule> {
  let crawl = PyModule::new(py, "crawl")?;
  crawl.add_function(wrap_pyfunction!(func, crawl)?)?;
  Ok(crawl)
}
