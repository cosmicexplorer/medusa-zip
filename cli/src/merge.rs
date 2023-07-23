/*
 * Description: ???
 *
 * Copyright (C) 2023 Danny McClanahan <dmcC2@hypnicjerk.ai>
 * SPDX-License-Identifier: Apache-2.0
 *
 * Licensed under the Apache License, Version 2.0 (see LICENSE).
 */

//! ???

use libmedusa_zip::{self as lib, merge as lib_merge};

use clap::Args;
use eyre::{self, WrapErr};

use std::{mem, path::PathBuf};


#[derive(Clone, Debug, Args)]
pub struct MedusaMerge {
  #[arg()]
  pub source_zips_by_prefix: Vec<String>,
}

impl TryFrom<MedusaMerge> for lib_merge::MedusaMerge {
  type Error = eyre::Report;

  fn try_from(x: MedusaMerge) -> Result<Self, Self::Error> {
    let MedusaMerge {
      source_zips_by_prefix,
    } = x;

    let mut ret: Vec<lib_merge::MergeGroup> = Vec::new();
    /* Each prefix is itself legitimately an Option (to avoid EntryName being
     * empty), so we wrap it again. */
    let mut current_prefix: Option<Option<lib::EntryName>> = None;
    let mut current_sources: Vec<PathBuf> = Vec::new();
    for arg in source_zips_by_prefix.into_iter() {
      let arg: &str = arg.as_ref();
      /* If we are starting a new prefix: */
      if arg.starts_with('+') && arg.ends_with('/') {
        let new_prefix = &arg[1..arg.len() - 1];
        let new_prefix: Option<lib::EntryName> = if new_prefix.is_empty() {
          None
        } else {
          Some(lib::EntryName::validate(new_prefix.to_string()).wrap_err("failed to parse entry")?)
        };
        if let Some(prefix) = current_prefix.take() {
          let group = lib_merge::MergeGroup {
            prefix,
            sources: mem::take(&mut current_sources),
          };
          ret.push(group);
        } else {
          /* Only None on the very first iteration of the loop. */
          assert!(current_sources.is_empty());
        }
        current_prefix = Some(new_prefix);
      } else {
        /* If no prefixes have been declared, assume they begin with an empty prefix. */
        current_prefix.get_or_insert(None);
        current_sources.push(PathBuf::from(arg));
      }
    }
    if let Some(prefix) = current_prefix {
      let group = lib_merge::MergeGroup {
        prefix,
        sources: current_sources,
      };
      ret.push(group);
    }
    Ok(Self { groups: ret })
  }
}
