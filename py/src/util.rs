/*
 * Description: ???
 *
 * Copyright (C) 2023 Danny McClanahan <dmcC2@hypnicjerk.ai>
 * SPDX-License-Identifier: Apache-2.0
 *
 * Licensed under the Apache License, Version 2.0 (see LICENSE).
 */

//! ???

use pyo3::{intern, prelude::*, types::PyAny};

/* TODO: figure out if we can make this avoid .clone()ing the input arg! */
pub fn repr<I: IntoPy<Py<PyAny>>>(py: Python<'_>, arg: I) -> PyResult<String> {
  arg
    .into_py(py)
    .call_method0(py, intern!(py, "__repr__"))?
    .extract(py)
}
