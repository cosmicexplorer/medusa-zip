/*
 * Description: ???
 *
 * Copyright (C) 2023 Danny McClanahan <dmcC2@hypnicjerk.ai>
 * SPDX-License-Identifier: Apache-2.0
 *
 * Licensed under the Apache License, Version 2.0 (see LICENSE).
 */

//! ???

use crate::{EntryName, FileSource};

use clap::{Args, ValueEnum};
use displaydoc::Display;
use futures::{
  future::try_join_all,
  pin_mut,
  stream::{self, StreamExt},
};
use parking_lot::Mutex;
use rayon::prelude::*;
use tempfile::tempfile;
use thiserror::Error;
use tokio::{fs, io, task};
use zip::{self, result::ZipError, ZipArchive, ZipWriter};

use std::{
  cmp,
  io::{Seek, Write},
  path::PathBuf,
  sync::Arc,
};

#[derive(Debug, Display, Error)]
pub enum MedusaInputReadError {
  /// Source file {0:?} from crawl could not be accessed: {1}.
  SourceNotFound(PathBuf, #[source] io::Error),
}

#[derive(Debug, Display, Error)]
pub enum InputConsistencyError {
  /// name {0} was duplicated for source paths {1:?} and {2:?}
  DuplicateName(EntryName, PathBuf, PathBuf),
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
  /// error reconciling input sources: {0}
  InputConsistency(#[from] InputConsistencyError),
  /// error reading input file: {0}
  InputRead(#[from] MedusaInputReadError),
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

#[derive(Clone, Debug)]
enum ZipEntrySpecification {
  File(FileSource),
  Directory(EntryName),
}

struct EntrySpecificationList(pub Vec<ZipEntrySpecification>);

impl EntrySpecificationList {
  pub fn from_file_specs(mut specs: Vec<FileSource>) -> Result<Self, InputConsistencyError> {
    /* Sort the resulting files so we can expect them to (mostly) be an inorder
     * directory traversal. Directories with names less than top-level
     * files will be sorted above those top-level files, which matches pex's Chroot behavior. */
    specs.par_sort_unstable();
    /* Check for duplicate names. */
    {
      let mut prev_name = EntryName("".to_string());
      let mut prev_path = PathBuf::from("");
      for FileSource { source, name } in specs.iter() {
        if name == &prev_name {
          return Err(InputConsistencyError::DuplicateName(
            name.clone(),
            prev_path,
            source.clone(),
          ));
        }
        prev_name = name.clone();
        prev_path = source.clone();
      }
    }

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
          let cur_intermediate_components = &current_directory_components[..=final_component_index];
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

    Ok(Self(ret))
  }
}

/* TODO: read file into mem or keep it as a handle! */
/* enum  */

enum IntermediateSingleEntry {
  File(EntryName, std::fs::File),
  Directory(EntryName),
}

impl IntermediateSingleEntry {
  pub async fn open_handle(entry: ZipEntrySpecification) -> Result<Self, MedusaInputReadError> {
    match entry {
      ZipEntrySpecification::Directory(name) => Ok(Self::Directory(name)),
      ZipEntrySpecification::File(FileSource { name, source }) => {
        let handle = fs::OpenOptions::new()
          .read(true)
          .open(&source)
          .await
          .map_err(|e| MedusaInputReadError::SourceNotFound(source, e))?;
        Ok(Self::File(name, handle.into_std().await))
      },
    }
  }
}

pub struct MedusaZip {
  pub input_files: Vec<FileSource>,
  pub options: MedusaZipOptions,
}

/* FIXME: make the later zips have more files than the earlier ones, so they can take longer to
 * complete (need to fully pipeline to make this useful)! */
const ENTRIES_PER_INTERMEDIATE: usize = 1_000;

impl MedusaZip {
  async fn zip_intermediate(
    entries: &[ZipEntrySpecification],
    zip_options: zip::write::FileOptions,
  ) -> Result<ZipArchive<std::fs::File>, MedusaZipError> {
    /* (2) Create unnamed filesystem-backed temp file handle. */
    let intermediate_output = Arc::new(Mutex::new(
      task::spawn_blocking(|| {
        let temp_file = tempfile()?;
        let zip_wrapper = ZipWriter::new(temp_file);
        Ok::<_, MedusaZipError>(zip_wrapper)
      })
      .await??,
    ));

    /* (3) Map to individual *file handles*; no reads yet. */
    let handle_stream =
      stream::iter(entries.to_vec()).then(|entry| IntermediateSingleEntry::open_handle(entry));
    pin_mut!(handle_stream);

    /* (4) Add file entries, in order. */
    while let Some(handle) = handle_stream.next().await {
      let intermediate_output = intermediate_output.clone();
      match handle? {
        IntermediateSingleEntry::Directory(name) => {
          task::spawn_blocking(move || {
            let mut intermediate_output = intermediate_output.lock();
            intermediate_output.add_directory(name.into_string(), zip_options)?;
            Ok::<(), MedusaZipError>(())
          })
          .await??;
        },
        IntermediateSingleEntry::File(name, mut handle) => {
          task::spawn_blocking(move || {
            let mut intermediate_output = intermediate_output.lock();
            intermediate_output.start_file(name.into_string(), zip_options)?;
            std::io::copy(&mut handle, &mut *intermediate_output)?;
            Ok::<(), MedusaZipError>(())
          })
          .await??;
        },
      }
    }

    let temp_for_read = task::spawn_blocking(move || {
      let mut zip_wrapper = Arc::into_inner(intermediate_output)
        .expect("no other references should exist to intermediate_output")
        .into_inner();
      let temp_file = zip_wrapper.finish()?;
      ZipArchive::new(temp_file)
    })
    .await??;

    Ok::<_, MedusaZipError>(temp_for_read)
  }

  pub async fn zip<Output>(self, output_zip: ZipWriter<Output>) -> Result<Output, MedusaZipError>
  where
    Output: Write + Seek + Send + 'static,
  {
    let Self {
      input_files,
      options,
    } = self;
    let MedusaZipOptions { reproducibility } = options;
    let zip_options = reproducibility.zip_options();

    let EntrySpecificationList(entries) =
      task::spawn_blocking(move || EntrySpecificationList::from_file_specs(input_files)).await??;

    /* (1) Split into however many subtasks (which may just be one) to do "normally". */
    /* TODO: fully recursive? or just one level of recursion? */
    let ordered_intermediates = try_join_all(
      entries
        .chunks(ENTRIES_PER_INTERMEDIATE)
        .map(|entries| Self::zip_intermediate(entries, zip_options)),
    )
    .await?;

    /* TODO: start piping in the first intermediate file as soon as it's ready! */
    let output_zip = Arc::new(Mutex::new(output_zip));
    for intermediate_zip in ordered_intermediates.into_iter() {
      let output_zip = output_zip.clone();
      task::spawn_blocking(move || {
        output_zip.lock().merge_archive(intermediate_zip)?;
        Ok::<(), MedusaZipError>(())
      })
      .await??;
    }

    let output_handle = task::spawn_blocking(move || {
      let mut output_zip = Arc::into_inner(output_zip)
        .expect("no other references should exist to output_zip")
        .into_inner();
      let output_handle = output_zip.finish()?;
      Ok::<Output, MedusaZipError>(output_handle)
    })
    .await??;

    Ok(output_handle)
  }
}
