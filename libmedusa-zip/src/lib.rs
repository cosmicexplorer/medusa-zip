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
/* #![warn(missing_docs)] */
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
use thiserror::Error;

use std::cmp;
use std::fmt;
use std::path::PathBuf;

/// Allowed zip format quirks that we refuse to handle right now.
#[derive(Debug, Display, Error)]
pub enum MedusaNameFormatError {
  /// name is empty
  NameIsEmpty,
  /// name starts with '/': {0}
  NameStartsWithSlash(String),
  /// name ends with '/': {0}
  NameEndsWithSlash(String),
  /// name has '//': {0}
  NameHasDoubleSlash(String),
}

#[derive(Clone, Debug, PartialOrd, Ord, PartialEq, Eq, Hash)]
pub struct EntryName(String);

impl fmt::Display for EntryName {
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    let Self(name) = self;
    write!(f, "'{}'", name)
  }
}

impl EntryName {
  pub fn into_string(self) -> String {
    let Self(name) = self;
    name
  }

  pub fn validate(name: String) -> Result<Self, MedusaNameFormatError> {
    if name.is_empty() {
      Err(MedusaNameFormatError::NameIsEmpty)
    } else if name.starts_with('/') {
      /* We won't produce any non-relative paths. */
      Err(MedusaNameFormatError::NameStartsWithSlash(name.to_string()))
    } else if name.ends_with('/') {
      /* We only enter file names. */
      Err(MedusaNameFormatError::NameEndsWithSlash(name.to_string()))
    } else if name.contains("//") {
      Err(MedusaNameFormatError::NameHasDoubleSlash(name.to_string()))
    } else {
      Ok(Self(name))
    }
  }

  pub fn split_directory_components(&self) -> Vec<String> {
    let Self(name) = self;
    let mut dir_components: Vec<String> = name.split('/').map(|s| s.to_string()).collect();
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
  fn cmp(&self, other: &Self) -> cmp::Ordering {
    self.name.cmp(&other.name)
  }
}

mod destination;
pub use destination::DestinationBehavior;

mod zip;
pub use crate::zip::{MedusaZip, MedusaZipError, MedusaZipOptions, Reproducibility};

mod crawl;
pub use crawl::{CrawlResult, MedusaCrawl, MedusaCrawlError};

/* #[cfg(test)] */
/* mod test { */
/* use super::*; */

/* use proptest::{prelude::*, strategy::Strategy}; */
/* } */
/* use proptest::{prelude::*, strategy::Strategy}; */
/* } */
