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
 * or correctness of the code.
 * TODO: #![warn(missing_docs)]
 * TODO: rustfmt breaks multiline comments when used one on top of another! (each with its own
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
  clippy::derive_hash_xor_eq,
  clippy::len_without_is_empty,
  clippy::redundant_field_names,
  clippy::too_many_arguments
)]
/* Default isn't as big a deal as people seem to think it is. */
#![allow(clippy::new_without_default, clippy::new_ret_no_self)]
/* Arc<Mutex> can be more clear than needing to grok Orderings. */
#![allow(clippy::mutex_atomic)]

use displaydoc::Display;
use futures::future::try_join_all;
use thiserror::Error;
use tokio::{io::AsyncReadExt, task};
use zip::{self, result::ZipError, ZipArchive, ZipWriter};

use std::cmp;
use std::io::{Cursor, Read, Seek, Write};
use std::path::PathBuf;

#[derive(Debug, Display, Error)]
pub enum MedusaZipFormatError {
  /// name is empty
  NameIsEmpty,
  /// name starts with '/': {0}
  NameStartsWithSlash(String),
  /// name ends with '/': {0}
  NameEndsWithSlash(String),
  /// name has '//': {0}
  NameHasDoubleSlash(String),
}

#[derive(Debug, Display, Error)]
pub enum MedusaZipError {
  /// i/o error: {0}
  Io(#[from] std::io::Error),
  /// zip error: {0}
  Zip(#[from] ZipError),
  /// join error: {0}
  Join(#[from] task::JoinError),
  /// zip format error: {0}
  ZipFormat(#[from] MedusaZipFormatError),
}

#[derive(Copy, Clone)]
pub enum Reproducibility {
  Reproducible,
  CurrentTime,
}

impl Reproducibility {
  pub fn zip_options(self) -> zip::write::FileOptions {
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

#[derive(Copy, Clone)]
pub struct MedusaZipOptions {
  pub reproducibility: Reproducibility,
}

#[derive(PartialEq, Eq)]
struct IntermediateSingleZip {
  pub name: String,
  pub single_member_archive: Vec<u8>,
}

impl cmp::PartialOrd for IntermediateSingleZip {
  fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
    self.name.partial_cmp(&other.name)
  }
}

impl cmp::Ord for IntermediateSingleZip {
  fn cmp(&self, other: &Self) -> cmp::Ordering {
    self.name.cmp(&other.name)
  }
}

struct IntermediateZipCollection(pub Vec<IntermediateSingleZip>);

impl IntermediateZipCollection {
  fn validate_name(name: &str) -> Result<(), MedusaZipFormatError> {
    if name.is_empty() {
      Err(MedusaZipFormatError::NameIsEmpty)
    } else if name.starts_with('/') {
      /* We won't produce any non-relative paths. */
      Err(MedusaZipFormatError::NameStartsWithSlash(name.to_string()))
    } else if name.ends_with('/') {
      /* We only enter file names. */
      Err(MedusaZipFormatError::NameEndsWithSlash(name.to_string()))
    } else if name.find("//").is_some() {
      Err(MedusaZipFormatError::NameHasDoubleSlash(name.to_string()))
    } else {
      Ok(())
    }
  }

  fn split_directory_components(name: &str) -> Vec<&str> {
    let mut dir_components: Vec<&str> = name.split('/').collect();
    /* Discard the file name itself. */
    dir_components
      .pop()
      .expect("a split should always be non-empty");

    dir_components
  }

  pub fn write_zip<W: Write + Seek>(
    self,
    medusa_zip_options: MedusaZipOptions,
    w: W,
  ) -> Result<(), MedusaZipError> {
    let Self(mut intermediate_zips) = self;
    let mut output_zip = ZipWriter::new(w);
    let MedusaZipOptions { reproducibility } = medusa_zip_options;
    let zip_options = reproducibility.zip_options();

    /* Sort the resulting files so we can expect them to (mostly) be an inorder directory traversal.
     * Directories with names less than top-level files will be sorted above those top-level files,
     * which matches the behavior of python zipfile. */
    intermediate_zips.sort_unstable();

    /* Loop over each entry and write it to the output zip. */
    let mut previous_directory_components: Vec<&str> = Vec::new();
    for IntermediateSingleZip {
      name,
      single_member_archive,
    } in intermediate_zips.iter()
    {
      Self::validate_name(name)?;

      /* Split into directory components so we can add directory entries before any files from that
       * directory. */
      let current_directory_components = Self::split_directory_components(name);

      /* Find the directory components shared between the previous and next entries. */
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
      /* If all components are shared, then we don't need to introduce any new directories. */
      if shared_components < current_directory_components.len() {
        for final_component_index in shared_components..current_directory_components.len() {
          /* Otherwise, we introduce a new directory for each new dir component of the current
           * entry. */
          let cur_intermediate_components = &current_directory_components[..=final_component_index];
          assert!(cur_intermediate_components.len() > 0);
          let cur_intermediate_directory: String = cur_intermediate_components.join("/");
          output_zip.add_directory(&cur_intermediate_directory, zip_options)?;
        }
      }
      /* Set the "previous" dir components to the components of the current entry. */
      previous_directory_components = current_directory_components;

      /* Finally we can just write the actual file now! */
      let mut single_member_zip = ZipArchive::new(Cursor::new(single_member_archive))?;
      /* TODO: can we use .by_index_raw(0) instead? */
      let member = single_member_zip.by_name(&name)?;
      output_zip.raw_copy_file(member)?;
    }

    output_zip.finish()?;

    Ok(())
  }
}

pub struct MedusaZip {
  pub input_paths: Vec<(PathBuf, String)>,
  pub options: MedusaZipOptions,
}

impl MedusaZip {
  async fn zip_single(
    input_path: PathBuf,
    output_name: String,
    medusa_zip_options: MedusaZipOptions,
  ) -> Result<IntermediateSingleZip, MedusaZipError> {
    let mut input_file_contents = Vec::new();
    let MedusaZipOptions { reproducibility } = medusa_zip_options;
    tokio::fs::OpenOptions::new()
      .read(true)
      .open(&input_path)
      .await?
      .read_to_end(&mut input_file_contents)
      .await?;

    let zip_options = reproducibility.zip_options();

    let name = output_name.clone();
    /* TODO: consider async-zip crate at https://docs.rs/async_zip/latest/async_zip/ as well! */
    let output_zip = task::spawn_blocking(move || {
      let mut output = Cursor::new(Vec::new());
      {
        let mut out_zip = ZipWriter::new(&mut output);

        out_zip.start_file(&output_name, zip_options)?;
        out_zip.write_all(&input_file_contents)?;

        out_zip.finish()?;
      }
      Ok::<Vec<u8>, MedusaZipError>(output.into_inner())
    })
    .await??;

    Ok(IntermediateSingleZip {
      name,
      single_member_archive: output_zip,
    })
  }

  pub async fn zip<Output>(self, output: Output) -> Result<(), MedusaZipError>
  where
    Output: Write + Seek + Send + 'static,
  {
    let Self {
      input_paths,
      options,
    } = self;

    let intermediate_zips: Vec<IntermediateSingleZip> = try_join_all(
      input_paths
        .into_iter()
        .map(|(input_path, output_name)| Self::zip_single(input_path, output_name, options)),
    )
    .await?;
    let intermediate_zips = IntermediateZipCollection(intermediate_zips);

    task::spawn_blocking(move || {
      intermediate_zips.write_zip(options, output)?;

      Ok::<(), MedusaZipError>(())
    });
    Ok(())
  }
}

/* #[derive(Debug, Display, Error)] */
/* pub enum MedusaCrawlFormatError { */
/*   /// path was absolute: {0} */
/*   PathWasAbsolute(PathBuf), */
/* } */

/* #[derive(Debug, Display, Error)] */
/* pub enum MedusaCrawlError { */
/*   /// i/o error: {0} */
/*   Io(#[from] std::io::Error), */
/*   /// crawl input format error: {0} */
/*   Format(#[from] MedusaCrawlFormatError), */
/* } */

/* pub struct MedusaCrawl { */
/*   pub paths_to_crawl: Vec<PathBuf>, */
/* } */

/* pub struct CrawlResult { */
/*   pub real_file_paths: Vec<PathBuf>, */
/* } */

/* impl CrawlResult { */
/*   pub fn medusa_zip(self, options: MedusaZipOptions) -> MedusaZip { */
/*     let Self { real_file_paths } = self; */
/*     let input_paths: Vec<(PathBuf, String)> = real_file_paths */
/*       .into_iter() */
/*       .map(|path| { */
/*         let name = path */
/*           .clone() */
/*           .into_os_string() */
/*           .into_string() */
/*           .expect("expected valid unicode path"); */
/*         (path, name) */
/*       }) */
/*       .collect(); */
/*     MedusaZip { */
/*       input_paths, */
/*       options, */
/*     } */
/*   } */
/* } */

/* impl MedusaCrawl { */
/*   pub async fn crawl_paths(self) -> Result<CrawlResult, MedusaCrawlError> {} */
/* } */

/* struct IntermediateCrawl { */
/*   pub prefix: PathBuf, */
/*   pub dirs: Vec<PathBuf>, */
/*   pub files: Vec<PathBuf>, */
/*   pub links: Vec<PathBuf>, */
/* } */

/* impl IntermediateCrawl { */
/*   pub fn expansion_remaining(&self) -> bool { */
/*     !(self.dirs.is_empty() && self.links.is_empty()) */
/*   } */

/*   pub async fn iterate_crawl(self) -> Self {} */
/* } */

/* #[cfg(test)] */
/* mod test { */
/*   use super::*; */

/*   /\* use proptest::{prelude::*, strategy::Strategy}; *\/ */
/* } */
