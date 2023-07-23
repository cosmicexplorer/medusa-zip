/*
 * Description: ???
 *
 * Copyright (C) 2023 Danny McClanahan <dmcC2@hypnicjerk.ai>
 * SPDX-License-Identifier: Apache-2.0
 *
 * Licensed under the Apache License, Version 2.0 (see LICENSE).
 */

//! ???

use crate::{
  util::repr,
  zip::{EntryModifications, MedusaZip, Parallelism, ZipOutputOptions},
};

use libmedusa_zip::{crawl as lib_crawl, zip as lib_zip};

use pyo3::{
  exceptions::{PyException, PyValueError},
  prelude::*,
  types::PyType,
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

  fn medusa_zip(
    &self,
    zip_options: Option<ZipOutputOptions>,
    modifications: Option<EntryModifications>,
    parallelism: Option<Parallelism>,
  ) -> PyResult<MedusaZip> {
    let zip_options: lib_zip::ZipOutputOptions = zip_options
      .unwrap_or_default()
      .try_into()
      /* TODO: better error! */
      .map_err(|e| PyException::new_err(format!("{}", e)))?;
    let modifications: lib_zip::EntryModifications = modifications.unwrap_or_default().into();
    let parallelism: lib_zip::Parallelism = parallelism.unwrap_or_default().into();
    let crawl_result: lib_crawl::CrawlResult = self.clone().into();
    let medusa_zip = crawl_result
      .medusa_zip(zip_options, modifications, parallelism)
      /* TODO: better error! */
      .map_err(|e| PyException::new_err(format!("{}", e)))?;
    let medusa_zip: MedusaZip = medusa_zip.into();
    Ok(medusa_zip)
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

impl From<CrawlResult> for lib_crawl::CrawlResult {
  fn from(x: CrawlResult) -> Self {
    let CrawlResult { real_file_paths } = x;
    Self {
      real_file_paths: real_file_paths.into_iter().map(|rp| rp.into()).collect(),
    }
  }
}


#[pyclass]
#[derive(Clone)]
pub struct Ignores {
  pub patterns: RegexSet,
}

impl Default for Ignores {
  fn default() -> Self { lib_crawl::Ignores::default().into() }
}

#[pymethods]
impl Ignores {
  #[new]
  fn new(patterns: Option<&PyAny>) -> PyResult<Self> {
    let patterns: Vec<&str> = patterns
      .map(|patterns| {
        let ret: Vec<&str> = patterns
          .iter()?
          .map(|p| p.and_then(PyAny::extract::<&str>))
          .collect::<PyResult<_>>()?;
        Ok::<_, PyErr>(ret)
      })
      .transpose()?
      .unwrap_or_default();
    /* TODO: better error! */
    let patterns = RegexSet::new(patterns).map_err(|e| PyValueError::new_err(format!("{}", e)))?;
    Ok(Self { patterns })
  }

  #[classmethod]
  #[pyo3(name = "default")]
  fn py_default(_cls: &PyType) -> Self { Self::default() }

  fn __repr__(&self) -> String { format!("Ignores(patterns={:?})", self.patterns.patterns()) }
}

impl From<Ignores> for lib_crawl::Ignores {
  fn from(x: Ignores) -> Self {
    let Ignores { patterns } = x;
    Self { patterns }
  }
}

impl From<lib_crawl::Ignores> for Ignores {
  fn from(x: lib_crawl::Ignores) -> Self {
    let lib_crawl::Ignores { patterns } = x;
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
  fn new(paths_to_crawl: &PyAny, ignores: Option<Ignores>) -> PyResult<Self> {
    let ignores = ignores.unwrap_or_default();
    let paths_to_crawl: Vec<PathBuf> = paths_to_crawl
      .iter()?
      .map(|p| p.and_then(PyAny::extract::<PathBuf>))
      .collect::<PyResult<_>>()?;
    Ok(Self {
      paths_to_crawl,
      ignores,
    })
  }

  fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
    let Self {
      paths_to_crawl,
      ignores,
    } = self;
    let paths_to_crawl = repr(py, paths_to_crawl.clone())?;
    let ignores = repr(py, ignores.clone())?;
    Ok(format!(
      "MedusaCrawl(paths_to_crawl={}, ignores={})",
      paths_to_crawl, ignores
    ))
  }

  #[cfg(feature = "asyncio")]
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

  #[cfg(feature = "sync")]
  fn crawl_paths_sync(&self, py: Python) -> PyResult<CrawlResult> {
    let handle = crate::TOKIO_RUNTIME.handle();
    let crawl: lib_crawl::MedusaCrawl = self.clone().into();
    py.allow_threads(move || {
      let ret: PyResult<CrawlResult> = handle.block_on(crawl
        .crawl_paths())
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
