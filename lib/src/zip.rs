/*
 * Description: ???
 *
 * Copyright (C) 2023 Danny McClanahan <dmcC2@hypnicjerk.ai>
 * SPDX-License-Identifier: Apache-2.0
 *
 * Licensed under the Apache License, Version 2.0 (see LICENSE).
 */

//! ???

use crate::{destination::OutputWrapper, EntryName, FileSource, MedusaNameFormatError};

use cfg_if::cfg_if;
use displaydoc::Display;
use futures::stream::StreamExt;
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use rayon::prelude::*;
use static_init;
use tempfile;
use thiserror::Error;
use time::{error::ComponentRange, OffsetDateTime, UtcOffset};
use tokio::{
  fs, io,
  sync::{mpsc, oneshot},
  task,
};
use tokio_stream::wrappers::ReceiverStream;
use zip::{
  self,
  result::{DateTimeRangeError, ZipError},
  write::FileOptions as ZipLibraryFileOptions,
  CompressionMethod as ZipCompressionMethod, DateTime as ZipDateTime, ZipArchive, ZipWriter,
  ZIP64_BYTES_THR,
};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::{
  cmp,
  io::{Seek, Write},
  mem, num, ops,
  path::{Path, PathBuf},
  sync::Arc,
};

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
  /// error processing zip file entry options: {0}
  ProcessZipOptions(#[from] InitializeZipOptionsError),
  /// error receiving from a oneshot channel: {0}
  OneshotRecv(#[from] oneshot::error::RecvError),
  /// error sending intermediate archiev: {0}
  Send(#[from] mpsc::error::SendError<ZipArchive<tempfile::SpooledTempFile>>),
}

pub trait DefaultInitializeZipOptions {
  #[must_use]
  fn set_zip_options_static(&self, options: ZipLibraryFileOptions) -> ZipLibraryFileOptions;
}

#[derive(Debug, Display, Error)]
pub enum InitializeZipOptionsError {
  /// i/o error: {0}
  Io(#[from] io::Error),
  /// date/time was out of range of a valid zip date: {0}
  InvalidDateTime(#[from] DateTimeRangeError),
  /// date/time was out of range for a valid date at all: {0}
  InvalidOffsetDateTime(#[from] ComponentRange),
}

pub trait InitializeZipOptionsForSpecificFile {
  #[must_use]
  fn set_zip_options_for_file(
    &self,
    options: ZipLibraryFileOptions,
    metadata: &std::fs::Metadata,
  ) -> Result<ZipLibraryFileOptions, InitializeZipOptionsError>;
}

const MINIMUM_ZIP_TIME: ZipDateTime = ZipDateTime::zero();

/* The `time` crate is extremely touchy about only ever extracting the local
 * UTC offset within a single-threaded environment, which means it cannot be
 * called anywhere reachable from the main function if we use #[tokio::main].
 * static_init instead runs it at program initialization time. */
#[static_init::dynamic]
static LOCAL_UTC_OFFSET: UtcOffset =
  UtcOffset::current_local_offset().expect("failed to capture local UTC offset");

/* We could use the dynamic initialization order to avoid fetching the local
 * offset twice, but that requires an unsafe block, which we'd prefer to
 * avoid in this crate. */
#[static_init::dynamic]
static CURRENT_LOCAL_TIME: OffsetDateTime =
  OffsetDateTime::now_local().expect("failed to capture local UTC offset");

static CURRENT_ZIP_TIME: Lazy<ZipDateTime> = Lazy::new(|| {
  (*CURRENT_LOCAL_TIME)
    .try_into()
    .expect("failed to convert local time into zip time at startup")
});

/* FIXME: establish one canonical place (probably the CLI help?) where the
 * definition of these repeated enum cases are specified. */
#[derive(Copy, Clone, Default, Debug)]
pub enum ModifiedTimeBehavior {
  #[default]
  Reproducible,
  CurrentTime,
  PreserveSourceTime,
  Explicit(ZipDateTime),
}

impl DefaultInitializeZipOptions for ModifiedTimeBehavior {
  #[must_use]
  fn set_zip_options_static(&self, options: ZipLibraryFileOptions) -> ZipLibraryFileOptions {
    match self {
      Self::Reproducible => options.last_modified_time(MINIMUM_ZIP_TIME),
      Self::CurrentTime => options.last_modified_time(*CURRENT_ZIP_TIME),
      Self::PreserveSourceTime => Self::CurrentTime.set_zip_options_static(options),
      Self::Explicit(timestamp) => options.last_modified_time(*timestamp),
    }
  }
}

impl InitializeZipOptionsForSpecificFile for ModifiedTimeBehavior {
  #[must_use]
  fn set_zip_options_for_file(
    &self,
    options: ZipLibraryFileOptions,
    metadata: &std::fs::Metadata,
  ) -> Result<ZipLibraryFileOptions, InitializeZipOptionsError> {
    match self {
      Self::Reproducible => Ok(options.last_modified_time(MINIMUM_ZIP_TIME)),
      Self::CurrentTime => Ok(options.last_modified_time(*CURRENT_ZIP_TIME)),
      Self::PreserveSourceTime => {
        /* NB: this is not blocking, but will Err on platforms without this available
         * (the docs don't specify which platforms:
         * https://doc.rust-lang.org/nightly/std/fs/struct.Metadata.html#method.modified). */
        let modified_time = metadata.modified()?;
        let modified_time: ZipDateTime = OffsetDateTime::from(modified_time)
          .to_offset(*LOCAL_UTC_OFFSET)
          .try_into()?;
        Ok(options.last_modified_time(modified_time))
      },
      Self::Explicit(timestamp) => Ok(options.last_modified_time(*timestamp)),
    }
  }
}

struct PreservePermsBehavior;

impl InitializeZipOptionsForSpecificFile for PreservePermsBehavior {
  #[must_use]
  fn set_zip_options_for_file(
    &self,
    options: ZipLibraryFileOptions,
    metadata: &std::fs::Metadata,
  ) -> Result<ZipLibraryFileOptions, InitializeZipOptionsError> {
    let permissions = metadata.permissions();
    cfg_if! {
      if #[cfg(unix)] {
        let permissions_mode: u32 = permissions.mode();
        Ok(options.unix_permissions(permissions_mode))
      } else {
        /* For non-unix, just don't bother trying to provide the same bits. */
        Ok(options)
      }
    }
  }
}

/* TODO: make this configurable! */
const SMALL_FILE_FOR_NO_COMPRESSION_MAX_SIZE: usize = 1_000;

struct SmallFileBehavior;

impl InitializeZipOptionsForSpecificFile for SmallFileBehavior {
  #[must_use]
  fn set_zip_options_for_file(
    &self,
    options: ZipLibraryFileOptions,
    metadata: &std::fs::Metadata,
  ) -> Result<ZipLibraryFileOptions, InitializeZipOptionsError> {
    if metadata.len() <= SMALL_FILE_FOR_NO_COMPRESSION_MAX_SIZE.try_into().unwrap() {
      Ok(
        options
          .compression_method(ZipCompressionMethod::Stored)
          .compression_level(None),
      )
    } else {
      Ok(options)
    }
  }
}

struct LargeFileBehavior;

impl InitializeZipOptionsForSpecificFile for LargeFileBehavior {
  #[must_use]
  fn set_zip_options_for_file(
    &self,
    options: ZipLibraryFileOptions,
    metadata: &std::fs::Metadata,
  ) -> Result<ZipLibraryFileOptions, InitializeZipOptionsError> {
    Ok(options.large_file(metadata.len() > ZIP64_BYTES_THR))
  }
}

#[derive(Copy, Clone, Default, Debug, Display)]
pub enum CompressionMethod {
  /// uncompressed
  Stored,
  /// deflate-compressed
  #[default]
  Deflated,
}

#[derive(Copy, Clone, Debug)]
pub enum CompressionStrategy {
  Stored,
  Deflated(Option<u8>),
}

impl Default for CompressionStrategy {
  fn default() -> Self { Self::Deflated(Some(6)) }
}

#[derive(Debug, Display, Error)]
pub enum ParseCompressionOptionsError {
  /// "stored" (uncompressed) does not accept a compression level (was: {0})
  CompressionLevelWithStored(i8),
  /// compression level {1} was invalid for method {0} which accepts {2:?}
  InvalidCompressionLevel(CompressionMethod, i8, ops::RangeInclusive<i8>),
  /// error converting from int (this should never happen!): {0}
  TryFromInt(#[from] num::TryFromIntError),
}

impl CompressionStrategy {
  const DEFLATE_RANGE: ops::RangeInclusive<i8> = ops::RangeInclusive::new(0, 9);

  pub fn from_method_and_level(
    method: CompressionMethod,
    level: Option<i8>,
  ) -> Result<Self, ParseCompressionOptionsError> {
    match method {
      CompressionMethod::Stored => match level {
        None => Ok(Self::Stored),
        Some(level) => Err(ParseCompressionOptionsError::CompressionLevelWithStored(
          level,
        )),
      },
      CompressionMethod::Deflated => match level {
        None => Ok(Self::Deflated(None)),
        Some(level) => {
          if Self::DEFLATE_RANGE.contains(&level) {
            Ok(Self::Deflated(Some(level.try_into()?)))
          } else {
            Err(ParseCompressionOptionsError::InvalidCompressionLevel(
              method,
              level,
              Self::DEFLATE_RANGE,
            ))
          }
        },
      },
    }
  }
}

impl DefaultInitializeZipOptions for CompressionStrategy {
  #[must_use]
  fn set_zip_options_static(&self, options: ZipLibraryFileOptions) -> ZipLibraryFileOptions {
    let (method, level): (ZipCompressionMethod, Option<i8>) = match self {
      Self::Stored => (ZipCompressionMethod::Stored, None),
      Self::Deflated(level) => (
        ZipCompressionMethod::Deflated,
        level.map(|l| {
          l.try_into()
            .expect("these values have already been checked")
        }),
      ),
    };
    options
      .compression_method(method)
      .compression_level(level.map(|l| {
        l.try_into()
          .expect("these values have already been checked")
      }))
  }
}


#[derive(Copy, Clone, Default, Debug)]
pub struct ZipOutputOptions {
  pub mtime_behavior: ModifiedTimeBehavior,
  pub compression_options: CompressionStrategy,
}


#[derive(Clone, Default, Debug)]
pub struct EntryModifications {
  /// This prefixes a directory path to every entry without creating any of its
  /// parent directories.
  ///
  /// These prefixes always come before any prefixes introduced by
  /// [`Self::own_prefix`].
  ///
  /// `--silent-external-prefix .deps` => `[.deps/a, .deps/b, ...]`
  /* FIXME: make these both EntryName (also, parse EntryName at clap validation time)! */
  pub silent_external_prefix: Option<String>,
  /// This prefixes a directory path to every entry, but this *will* create
  /// parent directory entries in the output file.
  ///
  /// `--own-prefix .deps` => `[.deps/, .deps/a, .deps/b, ...]`
  /* FIXME: explain how these work when stacked together! */
  pub own_prefix: Option<String>,
}

#[derive(Debug, Display, Error)]
pub enum InputConsistencyError {
  /// name {0} was duplicated for source paths {1:?} and {2:?}
  DuplicateName(EntryName, PathBuf, PathBuf),
  /// error in name formatting: {0}
  NameFormat(#[from] MedusaNameFormatError),
}

#[derive(Clone, Debug)]
pub enum ZipEntrySpecification {
  File(FileSource),
  Directory(EntryName),
}

struct EntrySpecificationList(pub Vec<ZipEntrySpecification>);

pub fn calculate_new_rightmost_components<'a, T>(
  previous_directory_components: &[T],
  current_directory_components: &'a [T],
) -> impl Iterator<Item=&'a [T]>+'a
where
  T: Eq,
{
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
  (shared_components..current_directory_components.len()).map(|final_component_index| {
    /* Otherwise, we introduce a new directory for each new dir component of the
     * current entry. */
    let cur_intermediate_components = &current_directory_components[..=final_component_index];
    assert!(!cur_intermediate_components.is_empty());
    cur_intermediate_components
  })
}

impl EntrySpecificationList {
  fn sort_and_deduplicate(specs: &mut Vec<FileSource>) -> Result<(), InputConsistencyError> {
    /* Sort the resulting files so we can expect them to (mostly) be an inorder
     * directory traversal. Note that directories with names less than top-level
     * files will be sorted above those top-level files. */
    specs.par_sort_unstable();

    /* Check for duplicate names. */
    {
      let i = EntryName::empty();
      let p = PathBuf::from("");
      let mut prev_name: &EntryName = &i;
      let mut prev_path: &Path = &p;
      for FileSource { source, name } in specs.iter() {
        if name == prev_name {
          return Err(InputConsistencyError::DuplicateName(
            name.clone(),
            prev_path.to_path_buf(),
            source.clone(),
          ));
        }
        prev_name = name;
        prev_path = source;
      }
    }

    Ok(())
  }

  pub fn from_file_specs(
    mut specs: Vec<FileSource>,
    modifications: EntryModifications,
  ) -> Result<Self, InputConsistencyError> {
    Self::sort_and_deduplicate(&mut specs)?;

    let mut ret: Vec<ZipEntrySpecification> = Vec::new();

    let cached_prefix: EntryName = {
      /* TODO: make EntryName work more cleanly for directories and files! */
      let EntryModifications {
        silent_external_prefix,
        own_prefix,
      } = modifications;
      let silent_external_prefix: Vec<String> = silent_external_prefix
        .map(EntryName::validate)
        .transpose()?
        .map(|name| {
          name
            .all_components()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
        })
        .unwrap_or_default();
      let own_prefix: Vec<String> = own_prefix
        .map(EntryName::validate)
        .transpose()?
        .map(|name| {
          name
            .all_components()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
        })
        .unwrap_or_default();

      let mut cur_prefix: Vec<String> = silent_external_prefix;
      for component in own_prefix.into_iter() {
        cur_prefix.push(component);
        let cur_intermediate_directory: String = cur_prefix.join("/");
        let intermediate_dir = EntryName::validate(cur_intermediate_directory)
          .expect("constructed virtual directory should be fine");
        ret.push(ZipEntrySpecification::Directory(intermediate_dir));
      }
      if cur_prefix.is_empty() {
        EntryName::empty()
      } else {
        EntryName::validate(cur_prefix.join("/")).unwrap()
      }
    };

    let mut previous_directory_components: Vec<&str> = Vec::new();

    /* NB: .iter_mut() is used here to enable the use of &str references in
     * previous_directory_components! */
    for FileSource { source, name } in specs.iter_mut() {
      /* Split into directory components so we can add directory entries before any
       * files from that directory. */
      let current_directory_components: Vec<&str> = name.parent_components().collect();

      for new_rightmost_components in calculate_new_rightmost_components(
        &previous_directory_components,
        &current_directory_components,
      ) {
        let cur_intermediate_directory: String = new_rightmost_components.join("/");
        let mut intermediate_dir = EntryName::validate(cur_intermediate_directory)
          .expect("constructed virtual directory should be fine");
        intermediate_dir.add_prefix(&cached_prefix);
        ret.push(ZipEntrySpecification::Directory(intermediate_dir));
      }
      /* Set the "previous" dir components to the components of the current entry. */
      previous_directory_components = current_directory_components;

      /* Finally we can just write the actual file now! */
      let mut name = name.clone();
      name.add_prefix(&cached_prefix);
      ret.push(ZipEntrySpecification::File(FileSource {
        source: mem::take(source),
        name,
      }));
    }

    Ok(Self(ret))
  }
}

#[derive(Debug, Display, Error)]
pub enum MedusaInputReadError {
  /// Source file {0:?} from crawl could not be accessed: {1}.
  SourceNotFound(PathBuf, #[source] io::Error),
  /// error creating in-memory immediate file: {0}
  Zip(#[from] ZipError),
  /// error joining: {0}
  Join(#[from] task::JoinError),
  /// failed to send intermediate entry: {0:?}
  Send(#[from] mpsc::error::SendError<IntermediateSingleEntry>),
  /// failed to parse zip output options: {0}
  InitZipOptions(#[from] InitializeZipOptionsError),
}

#[derive(Debug)]
pub enum IntermediateSingleEntry {
  Directory(EntryName),
  File(oneshot::Receiver<Result<ZipArchive<tempfile::SpooledTempFile>, MedusaInputReadError>>),
}

const PER_FILE_SPOOL_THRESHOLD: usize = 3_000;

impl IntermediateSingleEntry {
  pub async fn open_handle(
    entry: ZipEntrySpecification,
    mut zip_options: zip::write::FileOptions,
    options_initializers: Arc<ZipOptionsInitializers>,
  ) -> Result<Self, MedusaInputReadError> {
    match entry {
      /* If it's a directory, we don't need any more info. */
      ZipEntrySpecification::Directory(name) => Ok(Self::Directory(name)),
      /* If it's a file, we're need to extract its contents. */
      ZipEntrySpecification::File(FileSource { name, source }) => {
        /* Get the file handle */
        let handle = fs::OpenOptions::new()
          .read(true)
          .open(&source)
          .await
          .map_err(|e| MedusaInputReadError::SourceNotFound(source.clone(), e))?;
        /* Get the filesystem metadata for this file. */
        let metadata = handle
          .metadata()
          .await
          .map_err(|e| MedusaInputReadError::SourceNotFound(source.clone(), e))?;
        /* Configure the zip options for this file, such as compression, given the
         * metadata. */
        zip_options = options_initializers.set_zip_options_for_file(zip_options, &metadata)?;

        /* Create the spooled temporary zip file. */
        let mut zip_output: ZipWriter<tempfile::SpooledTempFile> = task::spawn_blocking(|| {
          let temp_file = tempfile::spooled_tempfile(PER_FILE_SPOOL_THRESHOLD);
          let zip_wrapper = ZipWriter::new(temp_file);
          Ok::<_, MedusaInputReadError>(zip_wrapper)
        })
        .await??;

        /* We can send a oneshot::Receiver over an mpsc::bounded() channel in order
         * to force our receiving send of this the mpsc::bounded() to await
         * until the oneshot::Receiver is complete. */
        let (tx, rx) =
          oneshot::channel::<Result<ZipArchive<tempfile::SpooledTempFile>, MedusaInputReadError>>();

        let mut handle = handle.into_std().await;
        task::spawn(async move {
          let completed_single_zip: Result<
            ZipArchive<tempfile::SpooledTempFile>,
            MedusaInputReadError,
          > = task::spawn_blocking(move || {
            /* In parallel, we will be writing this input file out to a spooled temporary
             * zip containing just this one entry. */
            zip_output.start_file(name.into_string(), zip_options)?;
            std::io::copy(&mut handle, &mut zip_output)
              .map_err(|e| MedusaInputReadError::SourceNotFound(source.clone(), e))?;
            let temp_zip = zip_output.finish_into_readable()?;
            Ok::<ZipArchive<_>, MedusaInputReadError>(temp_zip)
          })
          .await
          .expect("joining should not fail");
          tx.send(completed_single_zip)
            .expect("rx should always be open");
        });
        /* NB: not awaiting this spawned task! */

        Ok(Self::File(rx))
      },
    }
  }
}

#[derive(Copy, Clone, Default, Debug)]
pub enum Parallelism {
  /// Read source files and copy them to the output zip in order.
  Synchronous,
  /// Parallelize creation by splitting up the input into chunks.
  #[default]
  ParallelMerge,
}

#[derive(Clone)]
pub struct MedusaZip {
  pub input_files: Vec<FileSource>,
  pub zip_options: ZipOutputOptions,
  pub modifications: EntryModifications,
  pub parallelism: Parallelism,
}

/* TODO: make these configurable!!! */
const INTERMEDIATE_CHUNK_SIZE: usize = 2000;
const MAX_PARALLEL_INTERMEDIATES: usize = 12;
const PER_INTERMEDIATE_FILE_IO_QUEUE_LENGTH: usize = 20;
const INTERMEDIATE_OUTPUT_SPOOL_THRESHOLD: usize = 20_000;

pub struct ZipOptionsInitializers {
  pub initializers: Vec<Box<dyn InitializeZipOptionsForSpecificFile+Send+Sync>>,
}

impl ZipOptionsInitializers {
  pub fn set_zip_options_for_file(
    &self,
    mut options: zip::write::FileOptions,
    metadata: &std::fs::Metadata,
  ) -> Result<zip::write::FileOptions, InitializeZipOptionsError> {
    let Self { initializers } = self;
    for initializer in initializers.iter() {
      options = initializer.set_zip_options_for_file(options, metadata)?;
    }
    Ok(options)
  }
}

impl MedusaZip {
  async fn zip_intermediate(
    entries: &[ZipEntrySpecification],
    zip_options: zip::write::FileOptions,
    options_initializers: Arc<ZipOptionsInitializers>,
  ) -> Result<ZipArchive<tempfile::SpooledTempFile>, MedusaZipError> {
    /* (1) Create unnamed filesystem-backed temp file handle. */
    let intermediate_output = task::spawn_blocking(|| {
      let temp_file = tempfile::spooled_tempfile(INTERMEDIATE_OUTPUT_SPOOL_THRESHOLD);
      let zip_wrapper = ZipWriter::new(temp_file);
      Ok::<_, MedusaZipError>(zip_wrapper)
    })
    .await??;

    /* (2) Map to individual file handles and/or in-memory "immediate" zip files. */
    let (handle_tx, handle_rx) =
      mpsc::channel::<IntermediateSingleEntry>(PER_INTERMEDIATE_FILE_IO_QUEUE_LENGTH);
    let entries = entries.to_vec();
    let handle_stream_task = task::spawn(async move {
      for entry in entries.into_iter() {
        let handle =
          IntermediateSingleEntry::open_handle(entry, zip_options, options_initializers.clone())
            .await?;
        handle_tx.send(handle).await?;
      }
      Ok::<(), MedusaInputReadError>(())
    });
    let mut handle_jobs = ReceiverStream::new(handle_rx);

    /* (3) Add file entries, in order. */
    let intermediate_output = Arc::new(Mutex::new(intermediate_output));
    while let Some(intermediate_entry) = handle_jobs.next().await {
      let intermediate_output = intermediate_output.clone();
      match intermediate_entry {
        IntermediateSingleEntry::Directory(name) => {
          task::spawn_blocking(move || {
            let mut intermediate_output = intermediate_output.lock();
            intermediate_output.add_directory(name.into_string(), zip_options)?;
            Ok::<(), ZipError>(())
          })
          .await??;
        },
        IntermediateSingleEntry::File(tmp_merge_archive) => {
          let tmp_merge_archive: ZipArchive<tempfile::SpooledTempFile> =
            tmp_merge_archive.await??;
          task::spawn_blocking(move || {
            let mut intermediate_output = intermediate_output.lock();
            intermediate_output.merge_archive(tmp_merge_archive)?;
            Ok::<(), ZipError>(())
          })
          .await??;
        },
      }
    }
    handle_stream_task.await??;

    /* (4) Convert the intermediate write archive into a file-backed read
     * archive. */
    let temp_for_read = task::spawn_blocking(move || {
      let mut zip_wrapper = Arc::into_inner(intermediate_output)
        .expect("no other references should exist to intermediate_output")
        .into_inner();
      let temp_file = zip_wrapper.finish_into_readable()?;
      Ok::<_, ZipError>(temp_file)
    })
    .await??;

    Ok(temp_for_read)
  }

  fn options_initializers(mtime_behavior: ModifiedTimeBehavior) -> ZipOptionsInitializers {
    ZipOptionsInitializers {
      initializers: vec![
        Box::new(mtime_behavior),
        Box::new(PreservePermsBehavior),
        Box::new(SmallFileBehavior),
        Box::new(LargeFileBehavior),
      ],
    }
  }

  async fn zip_parallel<Output>(
    entries: Vec<ZipEntrySpecification>,
    output_zip: OutputWrapper<ZipWriter<Output>>,
    zip_options: zip::write::FileOptions,
    mtime_behavior: ModifiedTimeBehavior,
  ) -> Result<(), MedusaZipError>
  where
    Output: Write+Seek+Send+'static,
  {
    let options_initializers = Arc::new(Self::options_initializers(mtime_behavior));

    let (intermediate_tx, intermediate_rx) =
      mpsc::channel::<ZipArchive<tempfile::SpooledTempFile>>(MAX_PARALLEL_INTERMEDIATES);
    let mut handle_intermediates = ReceiverStream::new(intermediate_rx);

    /* (1) Split into however many subtasks (which may just be one) to do
     * "normally". */
    let intermediate_stream_task = task::spawn(async move {
      for entry_chunk in entries.chunks(INTERMEDIATE_CHUNK_SIZE) {
        let archive =
          Self::zip_intermediate(entry_chunk, zip_options, options_initializers.clone()).await?;
        intermediate_tx.send(archive).await?;
      }
      Ok::<(), MedusaZipError>(())
    });

    /* (2) ??? */
    while let Some(intermediate_archive) = handle_intermediates.next().await {
      let output_zip = output_zip.clone();
      task::spawn_blocking(move || {
        output_zip.lease().merge_archive(intermediate_archive)?;
        Ok::<(), MedusaZipError>(())
      })
      .await??;
    }
    intermediate_stream_task.await??;

    Ok(())
  }

  async fn zip_synchronous<Output>(
    entries: Vec<ZipEntrySpecification>,
    output_zip: OutputWrapper<ZipWriter<Output>>,
    zip_options: zip::write::FileOptions,
    mtime_behavior: ModifiedTimeBehavior,
  ) -> Result<(), MedusaZipError>
  where
    Output: Write+Seek+Send+'static,
  {
    let options_initializers = Self::options_initializers(mtime_behavior);
    for entry in entries.into_iter() {
      let output_zip = output_zip.clone();
      match entry {
        ZipEntrySpecification::Directory(name) => {
          task::spawn_blocking(move || {
            let mut output_zip = output_zip.lease();
            output_zip.add_directory(name.into_string(), zip_options)?;
            Ok::<(), ZipError>(())
          })
          .await??;
        },
        ZipEntrySpecification::File(FileSource { name, source }) => {
          let f = fs::OpenOptions::new()
            .read(true)
            .open(&source)
            .await
            .map_err(|e| MedusaInputReadError::SourceNotFound(source, e))?;
          let metadata = f.metadata().await?;
          let zip_options =
            options_initializers.set_zip_options_for_file(zip_options, &metadata)?;
          let mut f = f.into_std().await;
          task::spawn_blocking(move || {
            let mut output_zip = output_zip.lease();
            output_zip.start_file(name.into_string(), zip_options)?;
            std::io::copy(&mut f, &mut *output_zip)?;
            Ok::<(), MedusaZipError>(())
          })
          .await??;
        },
      }
    }

    Ok(())
  }

  pub async fn zip<Output>(
    self,
    output_zip: OutputWrapper<ZipWriter<Output>>,
  ) -> Result<OutputWrapper<ZipWriter<Output>>, MedusaZipError>
  where
    Output: Write+Seek+Send+'static,
  {
    let Self {
      input_files,
      zip_options: ZipOutputOptions {
        mtime_behavior,
        compression_options,
      },
      modifications,
      parallelism,
    } = self;

    let EntrySpecificationList(entries) = task::spawn_blocking(move || {
      EntrySpecificationList::from_file_specs(input_files, modifications)
    })
    .await??;

    let static_options_initializers: Vec<Box<dyn DefaultInitializeZipOptions+Send+Sync>> =
      vec![Box::new(mtime_behavior), Box::new(compression_options)];
    let mut zip_options = ZipLibraryFileOptions::default();
    for initializer in static_options_initializers.into_iter() {
      zip_options = initializer.set_zip_options_static(zip_options);
    }

    match parallelism {
      Parallelism::Synchronous => {
        Self::zip_synchronous(entries, output_zip.clone(), zip_options, mtime_behavior).await?;
      },
      Parallelism::ParallelMerge => {
        Self::zip_parallel(entries, output_zip.clone(), zip_options, mtime_behavior).await?;
      },
    }

    Ok(output_zip)
  }
}
