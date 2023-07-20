/*
 * Description: ???
 *
 * Copyright (C) 2023 Danny McClanahan <dmcC2@hypnicjerk.ai>
 * SPDX-License-Identifier: Apache-2.0
 *
 * Licensed under the Apache License, Version 2.0 (see LICENSE).
 */

//! ???

use libmedusa_zip::crawl as lib_crawl;

use pyo3::{
  exceptions::{PyException, PyValueError},
  intern,
  prelude::*,
  types::PyList,
};
use regex::RegexSet;

use std::path::PathBuf;

#[pyclass]
#[derive(Clone)]
pub struct ResolvedPath {
  #[pyo3(get)]
  pub unresolved_path: PathBuf,
  #[pyo3(get)]
  pub resolved_path: PathBuf,
}

#[pymethods]
impl ResolvedPath {
  #[new]
  #[pyo3(signature = (*, unresolved_path, resolved_path))]
  fn new(unresolved_path: PathBuf, resolved_path: PathBuf) -> Self {
    Self {
      unresolved_path,
      resolved_path,
    }
  }

  /* See https://pyo3.rs/v0.19.1/class/object for more info. */
  fn __repr__(&self) -> String {
    format!(
      "ResolvedPath(unresolved_path={:?}, resolved_path={:?})",
      &self.unresolved_path, &self.resolved_path
    )
  }
}

impl From<ResolvedPath> for lib_crawl::ResolvedPath {
  fn from(x: ResolvedPath) -> Self {
    let ResolvedPath {
      unresolved_path,
      resolved_path,
    } = x;
    Self {
      unresolved_path,
      resolved_path,
    }
  }
}

impl From<lib_crawl::ResolvedPath> for ResolvedPath {
  fn from(x: lib_crawl::ResolvedPath) -> Self {
    let lib_crawl::ResolvedPath {
      unresolved_path,
      resolved_path,
    } = x;
    Self {
      unresolved_path,
      resolved_path,
    }
  }
}

#[pyclass]
#[derive(Clone)]
pub struct CrawlResult {
  #[pyo3(get)]
  pub real_file_paths: Vec<ResolvedPath>,
}

#[pymethods]
impl CrawlResult {
  #[new]
  fn new(real_file_paths: &PyAny) -> PyResult<Self> {
    let real_file_paths: Vec<ResolvedPath> = real_file_paths
      .iter()?
      .map(|rp| rp.and_then(PyAny::extract::<ResolvedPath>))
      .collect::<PyResult<_>>()?;
    Ok(Self { real_file_paths })
  }

  fn __repr__(&self, py: Python<'_>) -> String {
    let real_file_paths = self.real_file_paths.clone().into_py(py);
    format!("CrawlResult(real_file_paths={})", real_file_paths)
  }
}

impl From<lib_crawl::CrawlResult> for CrawlResult {
  fn from(x: lib_crawl::CrawlResult) -> Self {
    let lib_crawl::CrawlResult { real_file_paths } = x;
    Self {
      real_file_paths: real_file_paths
        .into_iter()
        .map(ResolvedPath::from)
        .collect(),
    }
  }
}

#[pyclass]
#[derive(Clone)]
pub struct Ignores {
  pub patterns: RegexSet,
}

#[pymethods]
impl Ignores {
  #[new]
  fn new(patterns: Option<&PyList>) -> PyResult<Self> {
    let patterns: Vec<&str> = patterns
      .map(|list| {
        let ret: Vec<&str> = list.extract()?;
        Ok::<_, PyErr>(ret)
      })
      .transpose()?
      .unwrap_or_default();
    /* TODO: better error! */
    let patterns = RegexSet::new(patterns).map_err(|e| PyValueError::new_err(format!("{}", e)))?;
    Ok(Self { patterns })
  }

  fn __repr__(&self) -> String { format!("Ignores(patterns={:?})", self.patterns.patterns()) }
}

impl From<Ignores> for lib_crawl::Ignores {
  fn from(x: Ignores) -> Self {
    let Ignores { patterns } = x;
    Self { patterns }
  }
}

#[pyclass]
#[derive(Clone)]
pub struct MedusaCrawl {
  #[pyo3(get)]
  pub paths_to_crawl: Vec<PathBuf>,
  #[pyo3(get)]
  pub ignores: Ignores,
}

#[pymethods]
impl MedusaCrawl {
  #[new]
  fn new(paths_to_crawl: &PyList, ignores: Ignores) -> PyResult<Self> {
    let paths_to_crawl: Vec<PathBuf> = paths_to_crawl.extract()?;
    Ok(Self {
      paths_to_crawl,
      ignores,
    })
  }

  fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
    let ignores = self.ignores.clone().into_py(py);
    let ignores: String = ignores
      .call_method0(py, intern!(py, "__repr__"))?
      .extract(py)?;
    Ok(format!(
      "MedusaCrawl(paths_to_crawl={:?}, ignores={})",
      &self.paths_to_crawl, ignores
    ))
  }

  fn crawl_paths<'a>(&self, py: Python<'a>) -> PyResult<&'a PyAny> {
    let crawl: lib_crawl::MedusaCrawl = self.clone().into();
    pyo3_asyncio::tokio::future_into_py(py, async move {
      let ret: PyResult<CrawlResult> = crawl
        .crawl_paths()
        .await
        /* TODO: better error! */
        .map_err(|e| PyException::new_err(format!("{}", e)))
        .map(|cr| cr.into());
      ret
    })
  }
}

impl From<MedusaCrawl> for lib_crawl::MedusaCrawl {
  fn from(x: MedusaCrawl) -> Self {
    let MedusaCrawl {
      paths_to_crawl,
      ignores,
    } = x;
    Self {
      paths_to_crawl,
      ignores: ignores.into(),
    }
  }
}

pub(crate) fn crawl_module(py: Python<'_>) -> PyResult<&PyModule> {
  let crawl = PyModule::new(py, "crawl")?;

  crawl.add_class::<ResolvedPath>()?;
  crawl.add_class::<CrawlResult>()?;
  crawl.add_class::<Ignores>()?;
  crawl.add_class::<MedusaCrawl>()?;

  Ok(crawl)
}
