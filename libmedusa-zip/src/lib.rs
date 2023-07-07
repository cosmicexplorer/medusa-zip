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
pub struct EntryName(pub String);

impl EntryName {
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

pub mod zip {
  use super::{EntryName, FileSource};

  use clap::{Args, ValueEnum};
  use displaydoc::Display;
  use futures::{future::try_join_all, pin_mut, stream::StreamExt, try_join};
  use parking_lot::Mutex;
  use thiserror::Error;
  use tokio::{
    fs,
    io::{self, AsyncReadExt},
    sync::mpsc,
    task,
  };
  use tokio_stream::wrappers::UnboundedReceiverStream;
  use zip::{self, result::ZipError, ZipArchive, ZipWriter};

  use std::{
    cmp,
    io::{Cursor, Seek, Write},
    path::PathBuf,
    sync::Arc,
  };

  #[derive(Debug, Display, Error)]
  pub enum MedusaInputReadError {
    /// Source file {0:?} from crawl could not be accessed: {1}.
    SourceNotFound(PathBuf, #[source] io::Error),
  }

  /// All types of errors from the parallel zip process.
  #[derive(Debug, Display, Error)]
  pub enum MedusaZipError {
    /// i/o error: {0}
    Io(#[from] io::Error),
    /// zip error: {0}
    Zip(#[from] ZipError),
    /// join error: {0}
    Join(#[from] task::JoinError),
    /// error reading input file: {0}
    InputRead(#[from] MedusaInputReadError),
    /// error sending initial input: {0:?}
    InitialSend(#[from] mpsc::error::SendError<ZipEntrySpecification>),
    /// error sending intermediate input: {0:?}
    IntermediateSend(#[from] mpsc::error::SendError<IntermediateSingleEntry>),
  }

  #[derive(Copy, Clone, Default, Debug, ValueEnum)]
  pub enum Reproducibility {
    /// All modification times for entries will be set to 1980-01-1.
    #[default]
    Reproducible,
    /// Each file's modification time will be converted into a zip timestamp
    /// when it is entered into the archive.
    CurrentTime,
  }

  impl Reproducibility {
    pub(crate) fn zip_options(self) -> zip::write::FileOptions {
      match self {
        Reproducibility::CurrentTime => zip::write::FileOptions::default(),
        Reproducibility::Reproducible => {
          let time = zip::DateTime::from_date_and_time(1980, 1, 1, 0, 0, 0)
            .expect("zero date should be valid");
          zip::write::FileOptions::default().last_modified_time(time)
        },
      }
    }
  }

  #[derive(Copy, Clone, Default, Debug, Args)]
  pub struct MedusaZipOptions {
    /// Reproducibility behavior when generating zip archives.
    #[arg(value_enum, default_value_t, short, long)]
    pub reproducibility: Reproducibility,
  }

  pub enum ZipEntrySpecification {
    File(FileSource),
    Directory(EntryName),
  }

  struct EntrySpecificationList(pub Vec<ZipEntrySpecification>);

  impl EntrySpecificationList {
    pub fn from_file_specs(mut specs: Vec<FileSource>) -> Self {
      /* Sort the resulting files so we can expect them to (mostly) be an inorder
       * directory traversal. Directories with names less than top-level
       * files will be sorted above those top-level files, which matches pex's Chroot behavior. */
      specs.sort_unstable();
      /* TODO: check for duplicate names? */

      let mut ret: Vec<ZipEntrySpecification> = Vec::new();
      let mut previous_directory_components: Vec<String> = Vec::new();
      for FileSource { source, name } in specs.into_iter() {
        /* Split into directory components so we can add directory entries before any
         * files from that directory. */
        let current_directory_components = name.split_directory_components();

        /* Find the directory components shared between the previous and next
         * entries. */
        let mut shared_components: usize = 0;
        for i in 0..cmp::min(
          previous_directory_components.len(),
          current_directory_components.len(),
        ) {
          if previous_directory_components[i] != current_directory_components[i] {
            break;
          }
          shared_components += 1;
        }
        /* If all components are shared, then we don't need to introduce any new
         * directories. */
        if shared_components < current_directory_components.len() {
          for final_component_index in shared_components..current_directory_components.len() {
            /* Otherwise, we introduce a new directory for each new dir component of the
             * current entry. */
            let cur_intermediate_components =
              &current_directory_components[..=final_component_index];
            assert!(!cur_intermediate_components.is_empty());
            let cur_intermediate_directory: String = cur_intermediate_components.join("/");

            let intermediate_dir = EntryName::validate(cur_intermediate_directory)
              .expect("constructed virtual directory should be fine");
            ret.push(ZipEntrySpecification::Directory(intermediate_dir));
          }
        }
        /* Set the "previous" dir components to the components of the current entry. */
        previous_directory_components = current_directory_components;

        /* Finally we can just write the actual file now! */
        ret.push(ZipEntrySpecification::File(FileSource { source, name }));
      }

      Self(ret)
    }
  }

  pub enum IntermediateSingleEntry {
    File(EntryName, Vec<u8>),
    Directory(EntryName),
  }

  impl IntermediateSingleEntry {
    pub async fn zip_single(
      entry: ZipEntrySpecification,
      zip_options: zip::write::FileOptions,
    ) -> Result<Self, MedusaZipError> {
      match entry {
        ZipEntrySpecification::Directory(name) => Ok(Self::Directory(name)),
        ZipEntrySpecification::File(FileSource { name, source }) => {
          let mut input_file_contents = Vec::new();
          fs::OpenOptions::new()
            .read(true)
            .open(&source)
            .await
            .map_err(|e| MedusaInputReadError::SourceNotFound(source, e))?
            .read_to_end(&mut input_file_contents)
            .await?;

          let output_name = name.clone();
          /* TODO: consider async-zip crate at https://docs.rs/async_zip/latest/async_zip/ as well! */
          let output_zip = task::spawn_blocking(move || {
            let mut output = Cursor::new(Vec::new());
            {
              let mut out_zip = ZipWriter::new(&mut output);

              let EntryName(name) = output_name;
              out_zip.start_file(&name, zip_options)?;
              out_zip.write_all(&input_file_contents)?;

              out_zip.finish()?;
            }
            Ok::<Vec<u8>, MedusaZipError>(output.into_inner())
          })
          .await??;

          Ok(Self::File(name, output_zip))
        },
      }
    }
  }

  pub struct MedusaZip {
    pub input_files: Vec<FileSource>,
    pub options: MedusaZipOptions,
  }

  impl MedusaZip {
    pub async fn zip<Output>(self, output: Output) -> Result<(), MedusaZipError>
    where
      Output: Write + Seek + Send + 'static,
    {
      let Self {
        input_files,
        options,
      } = self;
      let MedusaZipOptions { reproducibility } = options;
      let zip_options = reproducibility.zip_options();

      let EntrySpecificationList(entries) = EntrySpecificationList::from_file_specs(input_files);

      let (unprocessed_tx, unprocessed_rx) = mpsc::unbounded_channel::<ZipEntrySpecification>();
      let unprocessed_entries = UnboundedReceiverStream::new(unprocessed_rx);
      /* Send these into the channel and block until they're done, but don't wait for it to join. */
      let initial_send = task::spawn(async move {
        for entry in entries.into_iter() {
          unprocessed_tx.send(entry)?;
        }
        Ok::<(), MedusaZipError>(())
      });

      let (compressed_tx, compressed_rx) = mpsc::unbounded_channel::<IntermediateSingleEntry>();
      let compressed_entries = UnboundedReceiverStream::new(compressed_rx);
      /* Compress individual entries. */
      let compress_send = task::spawn(async move {
        let process_stream =
          unprocessed_entries
            .ready_chunks(2000)
            .then(|unprocessed_entries| async move {
              try_join_all(
                unprocessed_entries
                  .into_iter()
                  .map(|entry| IntermediateSingleEntry::zip_single(entry, zip_options)),
              )
              .await
            });
        pin_mut!(process_stream);
        while let Some(compressed_entries) = process_stream.next().await {
          for entry in compressed_entries?.into_iter() {
            compressed_tx.send(entry)?;
          }
        }
        Ok::<(), MedusaZipError>(())
      });

      /* Write output zip by reconstituting individual entries. */
      let zip_write = task::spawn(async move {
        /* TODO: this all runs sequentially in the same thread, so it shouldn't need to wrap it in
         * a mutex, but the overhead is pretty low anyway so it probably doesn't matter. */
        let output_zip = Arc::new(Mutex::new(ZipWriter::new(output)));

        let compressed_entries = compressed_entries.ready_chunks(4000);
        pin_mut!(compressed_entries);
        while let Some(compressed_entries) = compressed_entries.next().await {
          let output_zip = output_zip.clone();
          task::spawn_blocking(move || {
            for entry in compressed_entries.into_iter() {
              match entry {
                IntermediateSingleEntry::Directory(EntryName(name)) => {
                  output_zip.lock().add_directory(&name, zip_options)?;
                },
                IntermediateSingleEntry::File(EntryName(name), single_member_archive) => {
                  let mut single_member_archive =
                    ZipArchive::new(Cursor::new(single_member_archive))?;
                  /* TODO: can we use .by_index_raw(0) instead? */
                  let member = single_member_archive.by_name(&name)?;
                  output_zip.lock().raw_copy_file(member)?;
                },
              }
            }
            Ok::<(), ZipError>(())
          })
          .await??;
        }

        task::spawn_blocking(move || {
          output_zip.lock().finish()?;
          Ok::<(), ZipError>(())
        })
        .await??;

        Ok::<(), MedusaZipError>(())
      });

      /* All of these futures are contained within a task::spawn and therefore a JoinHandle, and
       * require an additional unwrapping. */
      {
        let (initial_send, compress_send, zip_write) =
          try_join!(initial_send, compress_send, zip_write)?;
        initial_send?;
        compress_send?;
        zip_write?;
      }
      Ok(())
    }
  }
}
pub use crate::zip::{MedusaZip, MedusaZipError, MedusaZipOptions, Reproducibility};

pub mod crawl {
  use super::{EntryName, FileSource, MedusaNameFormatError, MedusaZip, MedusaZipOptions};

  use async_recursion::async_recursion;
  use displaydoc::Display;
  use futures::{future::try_join_all, stream::StreamExt};
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
    pub(crate) async fn crawl_single(self) -> Result<CrawlResult, MedusaCrawlError> {
      match self.classify().await? {
        Entry::File(resolved_path) => Ok(CrawlResult {
          real_file_paths: vec![resolved_path],
        }),
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
          Ok(inner.crawl_single().await?)
        },
        Entry::Directory(parent_resolved_path) => {
          let results =
            ReadDirStream::new(fs::read_dir(&parent_resolved_path.resolved_path).await?)
              .then(|dir_entry| async {
                let inner = Self::DirEntry(parent_resolved_path.clone(), dir_entry?);
                inner.crawl_single().await
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

  pub struct MedusaCrawl {
    pub paths_to_crawl: Vec<PathBuf>,
  }

  impl MedusaCrawl {
    pub async fn crawl_paths(self) -> Result<CrawlResult, MedusaCrawlError> {
      let Self { paths_to_crawl } = self;

      let results: Vec<CrawlResult> = try_join_all(
        paths_to_crawl
          .into_iter()
          .map(|path| Input::Path(ResolvedPath::from_path(path)).crawl_single()),
      )
      .await?;
      Ok(CrawlResult::merge(results))
    }
  }
}
pub use crawl::{CrawlResult, MedusaCrawl, MedusaCrawlError};

/* #[cfg(test)] */
/* mod test { */
/* use super::*; */

/* use proptest::{prelude::*, strategy::Strategy}; */
/* } */
/* use proptest::{prelude::*, strategy::Strategy}; */
/* } */
