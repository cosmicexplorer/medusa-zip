/*
 * Description: ???
 *
 * Copyright (C) 2023 Danny McClanahan <dmcC2@hypnicjerk.ai>
 * SPDX-License-Identifier: Apache-2.0
 *
 * Licensed under the Apache License, Version 2.0 (see LICENSE).
 */

//! ???

/* These clippy lint descriptions are purely non-functional and do not affect the functionality
 * or correctness of the code. */
// #![warn(missing_docs)]

/* Note: run clippy with: rustup run nightly cargo-clippy! */
#![deny(unsafe_code)]
/* Ensure any doctest warnings fails the doctest! */
#![doc(test(attr(deny(warnings))))]
/* Enable all clippy lints except for many of the pedantic ones. It's a shame this needs to be
 * copied and pasted across crates, but there doesn't appear to be a way to include inner
 * attributes from a common source. */
#![deny(
  clippy::all,
  clippy::default_trait_access,
  clippy::expl_impl_clone_on_copy,
  clippy::if_not_else,
  clippy::needless_continue,
  clippy::single_match_else,
  clippy::unseparated_literal_suffix,
  clippy::used_underscore_binding
)]
/* It is often more clear to show that nothing is being moved. */
#![allow(clippy::match_ref_pats)]
/* Subjective style. */
#![allow(
  clippy::derived_hash_with_manual_eq,
  clippy::len_without_is_empty,
  clippy::redundant_field_names,
  clippy::too_many_arguments,
  clippy::single_component_path_imports,
  clippy::double_must_use
)]
/* Default isn't as big a deal as people seem to think it is. */
#![allow(clippy::new_without_default, clippy::new_ret_no_self)]
/* Arc<Mutex> can be more clear than needing to grok Orderings. */
#![allow(clippy::mutex_atomic)]

use libmedusa_zip as lib;

use pyo3::{exceptions::PyValueError, intern, prelude::*};

use std::{convert::TryFrom, path::PathBuf};


#[pyclass]
#[derive(Clone)]
pub struct EntryName(pub String);

#[pymethods]
impl EntryName {
  #[new]
  fn new(name: String) -> PyResult<Self> {
    /* TODO: better error! */
    let parsed =
      lib::EntryName::validate(name).map_err(|e| PyValueError::new_err(format!("{}", e)))?;
    Ok(parsed.into())
  }

  fn __repr__(&self) -> String { format!("EntryName({:?})", &self.0) }

  fn __str__(&self) -> String { self.0.clone() }
}

impl TryFrom<EntryName> for lib::EntryName {
  type Error = lib::MedusaNameFormatError;

  fn try_from(x: EntryName) -> Result<Self, Self::Error> {
    let EntryName(x) = x;
    Self::validate(x)
  }
}

impl From<lib::EntryName> for EntryName {
  fn from(x: lib::EntryName) -> Self { Self(x.into_string()) }
}


#[pyclass]
pub struct FileSource {
  #[pyo3(get)]
  pub name: EntryName,
  #[pyo3(get)]
  pub source: PathBuf,
}

#[pymethods]
impl FileSource {
  #[new]
  fn new(name: EntryName, source: PathBuf) -> Self { Self { name, source } }

  fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
    let name = self.name.clone().into_py(py);
    let name: String = name
      .call_method0(py, intern!(py, "__repr__"))?
      .extract(py)?;
    Ok(format!(
      "FileSource(name={}, source={:?})",
      name, &self.source
    ))
  }
}


fn add_submodule(parent: &PyModule, py: Python<'_>, child: &PyModule) -> PyResult<()> {
  parent.add_submodule(child)?;
  py.import("sys")?
    .getattr("modules")?
    .set_item(format!("{}.{}", parent.name()?, child.name()?), child)?;
  Ok(())
}

/// A Python module implemented in Rust. The name of this function must match
/// the `lib.name` setting in the `Cargo.toml`, else Python will not be able to
/// import the module.
#[pymodule]
fn pymedusa_zip(py: Python<'_>, medusa_zip: &PyModule) -> PyResult<()> {
  let crawl = crawl::crawl_module(py)?;
  add_submodule(medusa_zip, py, crawl)?;
  let merge = merge::merge_module(py)?;
  add_submodule(medusa_zip, py, merge)?;
  let destination = destination::destination_module(py)?;
  add_submodule(medusa_zip, py, destination)?;
  let zip = zip::zip_module(py)?;
  add_submodule(medusa_zip, py, zip)?;

  medusa_zip.add_class::<EntryName>()?;
  medusa_zip.add_class::<FileSource>()?;

  Ok(())
}


mod crawl;
mod destination;
mod merge;
mod zip;
