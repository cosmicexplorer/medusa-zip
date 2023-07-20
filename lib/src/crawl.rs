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
  util::clap_handlers, EntryModifications, EntryName, FileSource, MedusaNameFormatError, MedusaZip,
  Parallelism, ZipOutputOptions,
};

use async_recursion::async_recursion;
use clap::{
  builder::{TypedValueParser, ValueParserFactory},
  Args,
};
use displaydoc::Display;
use futures::{future::try_join_all, stream::StreamExt};
use rayon::prelude::*;
use regex::{Regex, RegexSet};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::{fs, io};
use tokio_stream::wrappers::ReadDirStream;

use std::{
  env, fmt,
  path::{Path, PathBuf},
};

#[derive(Debug, Display, Error)]
pub enum MedusaCrawlFormatError {
  /// path was absolute: {0}
  PathWasAbsolute(PathBuf),
}

#[derive(Debug, Display, Error)]
pub enum MedusaCrawlError {
  /// i/o error: {0}
  Io(#[from] io::Error),
  /// crawl input format error: {0}
  CrawlFormat(#[from] MedusaCrawlFormatError),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResolvedPath {
  /// The path *without* any symlink resolution.
  pub unresolved_path: PathBuf,
  /// The path *with* symlink resolution (may be the same, if the original
  /// path had no symlinks).
  pub resolved_path: PathBuf,
}

impl ResolvedPath {
  /* TODO: encapsulate this parsing into separate types! */
  pub(crate) fn clean_up_for_export(&mut self, cwd: &Path) {
    let Self {
      unresolved_path,
      resolved_path,
    } = self;
    if let Ok(stripped) = resolved_path.strip_prefix(".") {
      *resolved_path = stripped.to_path_buf();
    }
    if !resolved_path.is_absolute() {
      *resolved_path = cwd.join(&resolved_path);
    }
    if let Ok(stripped) = unresolved_path.strip_prefix(".") {
      *unresolved_path = stripped.to_path_buf();
    }
  }

  pub fn from_path(path: PathBuf) -> Self {
    Self {
      unresolved_path: path.clone(),
      resolved_path: path,
    }
  }

  fn join(self, path: &Path) -> Self {
    let Self {
      unresolved_path,
      resolved_path,
    } = self;
    Self {
      unresolved_path: unresolved_path.join(path),
      resolved_path: resolved_path.join(path),
    }
  }

  pub(crate) fn resolve_child_dir_entry(self, child: fs::DirEntry) -> Self {
    let file_name: PathBuf = child.file_name().into();
    self.join(&file_name)
  }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct CrawlResult {
  pub real_file_paths: Vec<ResolvedPath>,
}

impl CrawlResult {
  pub fn single(path: ResolvedPath) -> Self {
    Self {
      real_file_paths: vec![path],
    }
  }

  pub fn merge(results: Vec<Self>) -> Self {
    let merged_file_paths: Vec<ResolvedPath> = results
      .into_par_iter()
      .flat_map(|Self { real_file_paths }| real_file_paths)
      .collect();
    Self {
      real_file_paths: merged_file_paths,
    }
  }

  pub(crate) fn clean_up_for_export(&mut self, cwd: &Path) {
    let Self { real_file_paths } = self;
    real_file_paths
      .par_iter_mut()
      .for_each(|resolved_path| resolved_path.clean_up_for_export(cwd));
  }

  pub fn medusa_zip(
    self,
    zip_options: ZipOutputOptions,
    modifications: EntryModifications,
    parallelism: Parallelism,
  ) -> Result<MedusaZip, MedusaNameFormatError> {
    let Self { real_file_paths } = self;
    let input_files: Vec<FileSource> = real_file_paths
      .into_par_iter()
      .map(
        |ResolvedPath {
           unresolved_path,
           resolved_path,
         }| {
          let name = unresolved_path
            .into_os_string()
            .into_string()
            .expect("expected valid unicode path");
          Ok(FileSource {
            name: EntryName::validate(name)?,
            source: resolved_path,
          })
        },
      )
      .collect::<Result<Vec<FileSource>, _>>()?;
    Ok(MedusaZip {
      input_files,
      zip_options,
      modifications,
      parallelism,
    })
  }
}

#[derive(Clone, Default, Debug)]
pub struct Ignores {
  patterns: RegexSet,
}

impl Ignores {
  pub fn new(patterns: RegexSet) -> Self { Self { patterns } }

  pub fn should_ignore(&self, path: &Path) -> bool {
    let Self { patterns } = self;
    let path_str = format!("{}", path.display());
    patterns.is_match(&path_str)
  }
}

impl fmt::Display for Ignores {
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    let Self { patterns } = self;
    let quoted: Vec<String> = patterns
      .patterns()
      .iter()
      .map(|s| format!("'{}'", s))
      .collect();
    let joined: String = quoted.join(", ");
    write!(f, "[{}]", joined)
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

#[derive(Debug)]
enum Entry {
  Symlink(ResolvedPath),
  Directory(ResolvedPath),
  File(ResolvedPath),
}

impl Entry {
  fn as_resolved_path(&self) -> &ResolvedPath {
    match self {
      Self::Symlink(p) => p,
      Self::Directory(p) => p,
      Self::File(p) => p,
    }
  }

  pub fn should_ignore_this(&self, ignores: &Ignores) -> bool {
    let ResolvedPath {
      unresolved_path, ..
    } = self.as_resolved_path();
    /* NB: Because we are doing regex-based matching, we are intentionally not
     * taking into account matching against any idea of filesystem structure.
     * To this end, our "ignores" will not detect if a symlink leads to a
     * path which itself is ignored, but only whether the path
     * before expanding any symlinks matches the regex pattern. */
    ignores.should_ignore(unresolved_path)
  }
}

#[derive(Debug)]
enum Input {
  Path(ResolvedPath),
  /// The `ResolvedPath` corresponds to the parent directory.
  DirEntry(ResolvedPath, fs::DirEntry),
}

impl Input {
  async fn classify(self) -> Result<Entry, io::Error> {
    let (file_type, path) = match self {
      Self::Path(path) => {
        let file_type = fs::symlink_metadata(&path.resolved_path).await?.file_type();
        (file_type, path)
      },
      Self::DirEntry(parent_path, entry) => {
        let file_type = entry.file_type().await?;
        (file_type, parent_path.resolve_child_dir_entry(entry))
      },
    };
    if file_type.is_symlink() {
      Ok(Entry::Symlink(path))
    } else if file_type.is_dir() {
      Ok(Entry::Directory(path))
    } else {
      assert!(file_type.is_file());
      Ok(Entry::File(path))
    }
  }

  #[async_recursion]
  pub async fn crawl_single(self, ignores: &Ignores) -> Result<CrawlResult, MedusaCrawlError> {
    let classified = self.classify().await?;
    if classified.should_ignore_this(ignores) {
      return Ok(CrawlResult::default());
    }
    match classified {
      Entry::File(resolved_path) => Ok(CrawlResult::single(resolved_path)),
      Entry::Symlink(ResolvedPath {
        unresolved_path,
        resolved_path,
      }) => {
        /* Symlinks are resolved relative to the parent directory! */
        let resolved_parent_dir = resolved_path
          .parent()
          .expect("should always be a parent, even if empty");
        let new_path = resolved_parent_dir.join(fs::read_link(&resolved_path).await?);
        let inner = Self::Path(ResolvedPath {
          unresolved_path,
          resolved_path: new_path,
        });
        Ok(inner.crawl_single(ignores).await?)
      },
      Entry::Directory(parent_resolved_path) => {
        let results = ReadDirStream::new(fs::read_dir(&parent_resolved_path.resolved_path).await?)
          .then(|dir_entry| async {
            let inner = Self::DirEntry(parent_resolved_path.clone(), dir_entry?);
            inner.crawl_single(ignores).await
          })
          .collect::<Vec<Result<CrawlResult, MedusaCrawlError>>>()
          .await
          .into_iter()
          .collect::<Result<Vec<CrawlResult>, MedusaCrawlError>>()?;
        Ok(CrawlResult::merge(results))
      },
    }
  }
}

#[derive(Clone, Debug, Default, Args)]
pub struct MedusaCrawlArgs {
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

impl From<MedusaCrawlArgs> for MedusaCrawl {
  fn from(x: MedusaCrawlArgs) -> Self {
    let MedusaCrawlArgs {
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
      ignores: Ignores::new(ignore_patterns),
    }
  }
}

#[derive(Clone, Debug)]
pub struct MedusaCrawl {
  pub paths_to_crawl: Vec<PathBuf>,
  pub ignores: Ignores,
}

impl MedusaCrawl {
  pub async fn crawl_paths(self) -> Result<CrawlResult, MedusaCrawlError> {
    let Self {
      paths_to_crawl,
      ignores,
    } = self;

    let results: Vec<CrawlResult> = try_join_all(
      paths_to_crawl
        .into_iter()
        .map(|path| Input::Path(ResolvedPath::from_path(path)).crawl_single(&ignores)),
    )
    .await?;
    let mut result = CrawlResult::merge(results);

    let cwd = env::current_dir()?;
    result.clean_up_for_export(&cwd);

    Ok(result)
  }
}
