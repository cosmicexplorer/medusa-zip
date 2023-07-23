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

use displaydoc::Display;
use thiserror::Error;

use std::{cmp, fmt, ops::Range, path::PathBuf};

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

/* TODO: figure out how to make this represent both file and directory names
 * without coughing up blood. */
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct EntryName {
  name: String,
  components: Vec<Range<usize>>,
}

impl fmt::Display for EntryName {
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result { write!(f, "'{}'", self.name) }
}

impl cmp::PartialOrd for EntryName {
  fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> { Some(self.cmp(other)) }
}

impl cmp::Ord for EntryName {
  fn cmp(&self, other: &Self) -> cmp::Ordering {
    self.components_vec().cmp(&other.components_vec())
  }
}

impl EntryName {
  pub(crate) fn is_empty(&self) -> bool { self.name.is_empty() }

  pub(crate) fn empty() -> Self {
    Self {
      name: "".to_string(),
      components: Vec::new(),
    }
  }

  pub fn into_string(self) -> String {
    if self.is_empty() {
      panic!("attempted to write an empty EntryName!");
    }
    self.name
  }

  pub(crate) fn add_prefix(&mut self, prefix: &Self) {
    if prefix.is_empty() {
      return;
    }
    self.name = format!("{}/{}", prefix.name, self.name);
    self.components = Self::split_indices(&self.name);
  }

  fn split_indices(s: &str) -> Vec<Range<usize>> {
    let mut prev_begin: usize = 0;
    let mut components: Vec<Range<usize>> = Vec::new();
    for (match_start, matched_str) in s.match_indices('/') {
      components.push(prev_begin..match_start);
      prev_begin = match_start + matched_str.len();
    }
    components.push(prev_begin..s.len());
    components
  }

  fn iter_components(&self, range: Range<usize>) -> impl Iterator<Item=&str> {
    self.components[range].iter().map(|r| &self.name[r.clone()])
  }

  pub fn validate(name: String) -> Result<Self, MedusaNameFormatError> {
    if name.is_empty() {
      Err(MedusaNameFormatError::NameIsEmpty)
    } else if name.starts_with('/') {
      /* We won't produce any non-relative paths. */
      Err(MedusaNameFormatError::NameStartsWithSlash(name))
    } else if name.starts_with("./") {
      /* We refuse to try to process ./ paths, asking the user to strip them
       * instead. */
      Err(MedusaNameFormatError::NameStartsWithDotSlash(name))
    } else if name.ends_with('/') {
      /* We only enter file names. */
      Err(MedusaNameFormatError::NameEndsWithSlash(name))
    } else if name.contains("//") {
      Err(MedusaNameFormatError::NameHasDoubleSlash(name))
    } else {
      let components = Self::split_indices(&name);
      Ok(Self { name, components })
    }
  }

  pub fn all_components(&self) -> impl Iterator<Item=&str> {
    self.iter_components(0..self.components.len())
  }

  fn components_vec(&self) -> Vec<&str> { self.all_components().collect() }

  pub(crate) fn parent_components(&self) -> impl Iterator<Item=&str> {
    self.iter_components(0..self.components.len() - 1)
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
  fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> { Some(self.cmp(other)) }
}

impl cmp::Ord for FileSource {
  fn cmp(&self, other: &Self) -> cmp::Ordering { self.name.cmp(&other.name) }
}

pub mod destination;

pub mod crawl;

pub mod zip;

pub mod merge;

/* FIXME: add tests! */
/* #[cfg(test)] */
/* mod test { */
/* use super::*; */

/* use proptest::{prelude::*, strategy::Strategy}; */
/* } */
/* use proptest::{prelude::*, strategy::Strategy}; */
/* } */
