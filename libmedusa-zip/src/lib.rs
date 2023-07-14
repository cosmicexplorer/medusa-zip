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

/* TODO: rustfmt breaks multiline comments when used one on top of another! (each with its own
 * pair of delimiters)
 * Note: run clippy with: rustup run nightly cargo-clippy! */
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
  clippy::single_component_path_imports
)]
/* Default isn't as big a deal as people seem to think it is. */
#![allow(clippy::new_without_default, clippy::new_ret_no_self)]
/* Arc<Mutex> can be more clear than needing to grok Orderings. */
#![allow(clippy::mutex_atomic)]

use displaydoc::Display;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use std::{cmp, fmt, path::PathBuf};

/// Allowed zip format quirks that we refuse to handle right now.
#[derive(Debug, Display, Error)]
pub enum MedusaNameFormatError {
  /// name is empty
  NameIsEmpty,
  /// name starts with '/': {0}
  NameStartsWithSlash(String),
  /// name starts wtih './': {0}
  NameStartsWithDotSlash(String),
  /// name ends with '/': {0}
  NameEndsWithSlash(String),
  /// name has '//': {0}
  NameHasDoubleSlash(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EntryName(String);

impl fmt::Display for EntryName {
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    let Self(name) = self;
    write!(f, "'{}'", name)
  }
}

/* FIXME: cache the splitting by components instead of doing it upon every
 * cmp! */
impl cmp::PartialOrd for EntryName {
  fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
    self
      .split_components()
      .partial_cmp(&other.split_components())
  }
}

impl cmp::Ord for EntryName {
  fn cmp(&self, other: &Self) -> cmp::Ordering {
    self.split_components().cmp(&other.split_components())
  }
}

impl EntryName {
  pub(crate) fn empty() -> Self { Self("".to_string()) }

  pub(crate) fn into_string(self) -> String {
    let Self(name) = self;
    name
  }

  pub(crate) fn prefix(&mut self, prefix: &str) {
    if prefix.is_empty() {
      return;
    }
    let Self(name) = self;
    *name = format!("{}/{}", prefix, name);
  }

  pub fn validate(name: String) -> Result<Self, MedusaNameFormatError> {
    if name.is_empty() {
      Err(MedusaNameFormatError::NameIsEmpty)
    } else if name.starts_with('/') {
      /* We won't produce any non-relative paths. */
      Err(MedusaNameFormatError::NameStartsWithSlash(name.to_string()))
    } else if name.starts_with("./") {
      /* We refuse to try to process ./ paths, asking the user to strip them
       * instead. */
      Err(MedusaNameFormatError::NameStartsWithDotSlash(
        name.to_string(),
      ))
    } else if name.ends_with('/') {
      /* We only enter file names. */
      Err(MedusaNameFormatError::NameEndsWithSlash(name.to_string()))
    } else if name.contains("//") {
      Err(MedusaNameFormatError::NameHasDoubleSlash(name.to_string()))
    } else {
      Ok(Self(name))
    }
  }

  pub fn split_components(&self) -> Vec<&str> {
    let Self(name) = self;
    name.split('/').collect()
  }

  pub(crate) fn directory_components(&self) -> Vec<&str> {
    let mut dir_components = self.split_components();
    /* Discard the file name itself. */
    dir_components
      .pop()
      .expect("a split should always be non-empty");

    dir_components
  }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileSource {
  pub name: EntryName,
  pub source: PathBuf,
}

/* Implement {Partial,}Ord to sort a vector of these by name without
 * additional allocation, because Vec::sort_by_key() gets mad if the key
 * possesses a lifetime, otherwise requiring the `name` string to be
 * cloned. */
impl cmp::PartialOrd for FileSource {
  fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
    self.name.partial_cmp(&other.name)
  }
}

impl cmp::Ord for FileSource {
  fn cmp(&self, other: &Self) -> cmp::Ordering { self.name.cmp(&other.name) }
}

/* FIXME: make these modules public! */
mod destination;
pub use destination::{DestinationBehavior, DestinationError};

mod crawl;
pub use crawl::{CrawlResult, MedusaCrawl, MedusaCrawlError};

mod zip;
pub use crate::zip::{
  EntryModifications, MedusaZip, MedusaZipError, ModifiedTimeBehavior, Parallelism,
  ZipOutputOptions,
};

mod merge;
pub use merge::{MedusaMerge, MedusaMergeError, MergeGroup};

/* FIXME: add tests! */
/* #[cfg(test)] */
/* mod test { */
/* use super::*; */

/* use proptest::{prelude::*, strategy::Strategy}; */
/* } */
/* use proptest::{prelude::*, strategy::Strategy}; */
/* } */
