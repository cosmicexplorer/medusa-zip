/*
 * Description: ???
 *
 * Copyright (C) 2023 Danny McClanahan <dmcC2@hypnicjerk.ai>
 * SPDX-License-Identifier: Apache-2.0
 *
 * Licensed under the Apache License, Version 2.0 (see LICENSE).
 */

//! ???

use crate::{EntryName, FileSource, MedusaNameFormatError, MedusaZip, MedusaZipOptions};

use async_recursion::async_recursion;
use displaydoc::Display;
use futures::{future::try_join_all, stream::StreamExt};
use regex::RegexSet;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::{fs, io};
use tokio_stream::wrappers::ReadDirStream;

use std::path::{Path, PathBuf};

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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CrawlResult {
  pub real_file_paths: Vec<ResolvedPath>,
}

impl CrawlResult {
  pub(crate) fn merge(results: Vec<Self>) -> Self {
    let merged_file_paths: Vec<ResolvedPath> = results
      .into_iter()
      .flat_map(|Self { real_file_paths }| real_file_paths)
      .collect();
    Self {
      real_file_paths: merged_file_paths,
    }
  }

  pub fn medusa_zip(self, options: MedusaZipOptions) -> Result<MedusaZip, MedusaNameFormatError> {
    let Self { real_file_paths } = self;
    let input_files: Vec<FileSource> = real_file_paths
      .into_iter()
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
      options,
    })
  }
}

#[derive(Debug)]
enum Entry {
  Symlink(ResolvedPath),
  Directory(ResolvedPath),
  File(ResolvedPath),
}

#[derive(Debug)]
pub enum Input {
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
  pub(crate) async fn crawl_single(
    self,
    ignore_patterns: &RegexSet,
  ) -> Result<CrawlResult, MedusaCrawlError> {
    match self.classify().await? {
      Entry::File(resolved_path) => {
        let unresolved_path_str = format!("{}", &resolved_path.unresolved_path.display());
        let should_ignore_path = ignore_patterns.is_match(&unresolved_path_str);
        Ok(CrawlResult {
          real_file_paths: if should_ignore_path {
            vec![]
          } else {
            vec![resolved_path]
          },
        })
      },
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
        Ok(inner.crawl_single(ignore_patterns).await?)
      },
      Entry::Directory(parent_resolved_path) => {
        let results = ReadDirStream::new(fs::read_dir(&parent_resolved_path.resolved_path).await?)
          .then(|dir_entry| async {
            let inner = Self::DirEntry(parent_resolved_path.clone(), dir_entry?);
            inner.crawl_single(ignore_patterns).await
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

#[derive(Default)]
pub struct MedusaCrawl {
  pub paths_to_crawl: Vec<PathBuf>,
  pub ignore_patterns: RegexSet,
}

impl MedusaCrawl {
  pub async fn crawl_paths(self) -> Result<CrawlResult, MedusaCrawlError> {
    let Self {
      paths_to_crawl,
      ignore_patterns,
    } = self;

    let results: Vec<CrawlResult> = try_join_all(
      paths_to_crawl
        .into_iter()
        .map(|path| Input::Path(ResolvedPath::from_path(path)).crawl_single(&ignore_patterns)),
    )
    .await?;
    Ok(CrawlResult::merge(results))
  }
}
