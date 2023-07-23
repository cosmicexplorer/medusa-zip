/*
 * Description: ???
 *
 * Copyright (C) 2023 Danny McClanahan <dmcC2@hypnicjerk.ai>
 * SPDX-License-Identifier: Apache-2.0
 *
 * Licensed under the Apache License, Version 2.0 (see LICENSE).
 */

//! ???

use crate::util::clap_handlers;

use libmedusa_zip::crawl as lib_crawl;

use clap::{
  builder::{TypedValueParser, ValueParserFactory},
  Args,
};
use regex::{Regex, RegexSet};
use serde::{Deserialize, Serialize};

use std::{fmt, path::PathBuf};


#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResolvedPath {
  pub unresolved_path: PathBuf,
  pub resolved_path: PathBuf,
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


#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct CrawlResult {
  pub real_file_paths: Vec<ResolvedPath>,
}

impl From<lib_crawl::CrawlResult> for CrawlResult {
  fn from(x: lib_crawl::CrawlResult) -> Self {
    let lib_crawl::CrawlResult { real_file_paths } = x;
    let real_file_paths: Vec<ResolvedPath> =
      real_file_paths.into_iter().map(|rp| rp.into()).collect();
    Self { real_file_paths }
  }
}

impl From<CrawlResult> for lib_crawl::CrawlResult {
  fn from(x: CrawlResult) -> Self {
    let CrawlResult { real_file_paths } = x;
    let real_file_paths: Vec<lib_crawl::ResolvedPath> =
      real_file_paths.into_iter().map(|rp| rp.into()).collect();
    Self { real_file_paths }
  }
}


#[derive(Clone, Debug)]
pub struct RegexWrapper(pub Regex);

impl fmt::Display for RegexWrapper {
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    let Self(p) = self;
    p.fmt(f)
  }
}

#[derive(Clone)]
pub struct RegexParser;

impl TypedValueParser for RegexParser {
  type Value = RegexWrapper;

  fn parse_ref(
    &self,
    cmd: &clap::Command,
    arg: Option<&clap::Arg>,
    value: &std::ffi::OsStr,
  ) -> Result<Self::Value, clap::Error> {
    let inner = clap::builder::StringValueParser::new();
    let val = inner.parse_ref(cmd, arg, value)?;

    let regex = Regex::new(&val).map_err(|e| {
      let mut err = clap_handlers::prepare_clap_error(cmd, arg, &val);
      clap_handlers::process_clap_error(
        &mut err,
        e,
        "Regular expressions are parsed using the rust regex crate. See https://docs.rs/regex/latest/regex/index.html#syntax for more details."
      );
      err
    })?;
    Ok(RegexWrapper(regex))
  }
}

impl ValueParserFactory for RegexWrapper {
  type Parser = RegexParser;

  fn value_parser() -> Self::Parser { RegexParser }
}


#[derive(Clone, Debug, Default, Args)]
pub struct MedusaCrawl {
  /// File, directory, or symlink paths to traverse.
  #[arg(short, long, default_values_t = vec![".".to_string()])]
  pub paths_to_crawl: Vec<String>,
  /// Regular expressions to filter out of any directory or file paths
  /// encountered when crawling.
  ///
  /// These patterns will not read through symlinks.
  #[arg(short, long, default_values_t = Vec::<RegexWrapper>::new())]
  pub ignore_patterns: Vec<RegexWrapper>,
}

impl From<MedusaCrawl> for lib_crawl::MedusaCrawl {
  fn from(x: MedusaCrawl) -> Self {
    let MedusaCrawl {
      paths_to_crawl,
      ignore_patterns,
    } = x;
    let ignore_patterns = RegexSet::new(
      ignore_patterns
        .into_iter()
        .map(|RegexWrapper(p)| p.as_str().to_string()),
    )
    .expect("constituent patterns were already validated");
    Self {
      paths_to_crawl: paths_to_crawl.into_iter().map(PathBuf::from).collect(),
      ignores: lib_crawl::Ignores::new(ignore_patterns),
    }
  }
}
