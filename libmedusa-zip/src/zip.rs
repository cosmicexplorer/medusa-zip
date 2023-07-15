/*
 * Description: ???
 *
 * Copyright (C) 2023 Danny McClanahan <dmcC2@hypnicjerk.ai>
 * SPDX-License-Identifier: Apache-2.0
 *
 * Licensed under the Apache License, Version 2.0 (see LICENSE).
 */

//! ???

#[cfg(doc)]
use crate::MergeGroup;
use crate::{util::clap_handlers, EntryName, FileSource, MedusaNameFormatError};

use cfg_if::cfg_if;
use clap::{
  builder::{TypedValueParser, ValueParserFactory},
  Args, ValueEnum,
};
use displaydoc::Display;
use futures::{future::try_join_all, stream::StreamExt};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use rayon::prelude::*;
use static_init;
use tempfile::tempfile;
use thiserror::Error;
use time::{
  error::ComponentRange, format_description::well_known::Rfc3339, OffsetDateTime, UtcOffset,
};
use tokio::{fs, io, sync::mpsc, task};
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
  convert::TryInto,
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
  /// error parsing zip output options: {0}
  ParseZipOptions(#[from] ParseCompressionOptionsError),
  /// error processing zip file entry options: {0}
  ProcessZipOptions(#[from] InitializeZipOptionsError),
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

#[derive(Copy, Clone, Default, Debug, ValueEnum)]
pub enum AutomaticModifiedTimeStrategy {
  /// All modification times for entries will be set to 1980-01-1 at 00:00:00.
  #[default]
  Reproducible,
  /// All modification times for entries will be set to a single timestamp,
  /// recorded at the beginning of the program's runtime.
  CurrentTime,
  /// Each file's modification time on disk will be converted into a zip
  /// timestamp when it is entered into the archive.
  ///
  /// NOTE: this does not apply to directories, as this program does not copy
  /// directory paths from disk, but instead synthesizes them as necessary
  /// into the output file based upon the file list. This virtualization of
  /// directories is currently necessary to make zip file merging unambiguous,
  /// which is key to this program's ability to parallelize.
  ///
  /// **When this setting is provided, unlike files, directories will instead
  /// have the same behavior as if [`current-time`](Self::CurrentTime) was
  /// provided.**
  ///
  /// As a result, this setting should probably not be provided for the
  /// `merge` operation, as merging zips does not read any file entries from
  /// disk itself, but instead simply copies them over verbatim from the
  /// source zip files, and only inserts directories as specified by the
  /// [`prefix`](MergeGroup::prefix) provided in each each [`MergeGroup`].
  PreserveSourceTime,
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

impl DefaultInitializeZipOptions for AutomaticModifiedTimeStrategy {
  #[must_use]
  fn set_zip_options_static(&self, options: ZipLibraryFileOptions) -> ZipLibraryFileOptions {
    match self {
      Self::Reproducible => options.last_modified_time(MINIMUM_ZIP_TIME),
      Self::CurrentTime => options.last_modified_time(*CURRENT_ZIP_TIME),
      Self::PreserveSourceTime => Self::CurrentTime.set_zip_options_static(options),
    }
  }
}

impl InitializeZipOptionsForSpecificFile for AutomaticModifiedTimeStrategy {
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
    }
  }
}

#[derive(Copy, Clone, Debug)]
pub struct ZipDateTimeWrapper(pub ZipDateTime);

#[derive(Clone)]
pub struct ZipDateTimeParser;

impl TypedValueParser for ZipDateTimeParser {
  type Value = ZipDateTimeWrapper;

  fn parse_ref(
    &self,
    cmd: &clap::Command,
    arg: Option<&clap::Arg>,
    value: &std::ffi::OsStr,
  ) -> Result<Self::Value, clap::Error> {
    let inner = clap::builder::StringValueParser::new();
    let val = inner.parse_ref(cmd, arg, value)?;

    let parsed_offset = OffsetDateTime::parse(&val, &Rfc3339).map_err(|e| {
      let mut err = clap_handlers::prepare_clap_error(cmd, arg, &val);
      clap_handlers::process_clap_error(
        &mut err,
        e,
        "Provide a string which can be formatted according to RFC 3339, such as '1985-04-12T23:20:50.52Z'. See https://datatracker.ietf.org/doc/html/rfc3339#section-5.6 for details.",
      );
      err
    })?;
    let zip_time: ZipDateTime = parsed_offset.try_into().map_err(|e| {
      let mut err = clap_handlers::prepare_clap_error(cmd, arg, &val);
      clap_handlers::process_clap_error(
        &mut err,
        e,
        "The zip implementation used by this program only supports years from 1980-2107.",
      );
      err
    })?;
    Ok(ZipDateTimeWrapper(zip_time))
  }
}

impl ValueParserFactory for ZipDateTimeWrapper {
  type Parser = ZipDateTimeParser;

  fn value_parser() -> Self::Parser { ZipDateTimeParser }
}

#[derive(Copy, Clone, Debug, Default, Args)]
pub struct ModifiedTimeBehavior {
  /// Assign timestamps to the entries of the output zip file according to some
  /// formula.
  #[arg(
    value_enum,
    default_value_t,
    long,
    conflicts_with = "explicit_mtime_timestamp"
  )]
  automatic_mtime_strategy: AutomaticModifiedTimeStrategy,
  /// Assign a single [RFC 3339] timestamp such as '1985-04-12T23:20:50.52Z' to
  /// every file and directory.
  ///
  /// Because zip files do not retain time zone information, we must provide UTC
  /// offsets whenever we interact with them. The timestamps will also be
  /// truncated to 2-second accuracy.
  ///
  /// [RFC 3339]: https://datatracker.ietf.org/doc/html/rfc3339#section-5.6
  #[arg(long, default_value = None)]
  explicit_mtime_timestamp: Option<ZipDateTimeWrapper>,
}

impl ModifiedTimeBehavior {
  pub fn automatic(automatic_mtime_strategy: AutomaticModifiedTimeStrategy) -> Self {
    Self {
      automatic_mtime_strategy,
      ..Default::default()
    }
  }

  pub fn explicit(explicit_mtime_timestamp: ZipDateTime) -> Self {
    Self {
      explicit_mtime_timestamp: Some(ZipDateTimeWrapper(explicit_mtime_timestamp)),
      ..Default::default()
    }
  }
}

impl DefaultInitializeZipOptions for ModifiedTimeBehavior {
  #[must_use]
  fn set_zip_options_static(&self, options: ZipLibraryFileOptions) -> ZipLibraryFileOptions {
    let Self {
      automatic_mtime_strategy,
      explicit_mtime_timestamp,
    } = self;
    match explicit_mtime_timestamp {
      None => automatic_mtime_strategy.set_zip_options_static(options),
      Some(ZipDateTimeWrapper(timestamp)) => options.last_modified_time(*timestamp),
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
    let Self {
      automatic_mtime_strategy,
      explicit_mtime_timestamp,
    } = self;
    match explicit_mtime_timestamp {
      None => automatic_mtime_strategy.set_zip_options_for_file(options, metadata),
      Some(ZipDateTimeWrapper(timestamp)) => Ok(options.last_modified_time(*timestamp)),
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

#[derive(Copy, Clone, Default, Debug, Display, ValueEnum)]
pub enum CompressionMethod {
  /// uncompressed
  Stored,
  /// deflate-compressed
  #[default]
  Deflated,
  /// bzip2-compressed
  Bzip2,
  /// zstd-compressed
  Zstd,
}

#[derive(Copy, Clone, Default, Debug, Args)]
pub struct CompressionOptions {
  #[arg(value_enum, default_value_t, long)]
  pub compression_method: CompressionMethod,
  /// The degree of computational effort to exert for the [`Self::compression_method`].
  ///
  /// Each compression method interprets this argument differently:
  /// - [`CompressionMethod::Stored`]: the program will error if this is provided.
  /// - [`CompressionMethod::Deflated`]: 0..=9 (default 6)
  /// - [`CompressionMethod::Bzip2`]: 0..=9 (default 6)
  /// - [`CompressionMethod::Zstd`]: -7..=22 (default 3)
  ///   - 0 is also mapped to "default".
  #[arg(long, default_value = None, requires = "compression_method", verbatim_doc_comment)]
  pub compression_level: Option<i8>,
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

enum CompressionStrategy {
  Stored,
  Deflated(Option<u8>),
  Bzip2(Option<u8>),
  Zstd(Option<i8>),
}

impl CompressionStrategy {
  const BZIP2_RANGE: ops::RangeInclusive<i8> = ops::RangeInclusive::new(0, 9);
  const DEFLATE_RANGE: ops::RangeInclusive<i8> = ops::RangeInclusive::new(0, 9);
  const ZSTD_RANGE: ops::RangeInclusive<i8> = ops::RangeInclusive::new(-7, 22);

  pub fn from_options(options: CompressionOptions) -> Result<Self, ParseCompressionOptionsError> {
    let CompressionOptions {
      compression_method,
      compression_level,
    } = options;
    match compression_method.clone() {
      CompressionMethod::Stored => match compression_level {
        None => Ok(Self::Stored),
        Some(level) => Err(ParseCompressionOptionsError::CompressionLevelWithStored(
          level,
        )),
      },
      CompressionMethod::Deflated => match compression_level {
        None => Ok(Self::Deflated(None)),
        Some(level) => {
          if Self::DEFLATE_RANGE.contains(&level) {
            Ok(Self::Deflated(Some(level.try_into()?)))
          } else {
            Err(ParseCompressionOptionsError::InvalidCompressionLevel(
              compression_method,
              level,
              Self::DEFLATE_RANGE,
            ))
          }
        },
      },
      CompressionMethod::Bzip2 => match compression_level {
        None => Ok(Self::Bzip2(None)),
        Some(level) => {
          if Self::BZIP2_RANGE.contains(&level) {
            Ok(Self::Bzip2(Some(level.try_into()?)))
          } else {
            Err(ParseCompressionOptionsError::InvalidCompressionLevel(
              compression_method,
              level,
              Self::BZIP2_RANGE,
            ))
          }
        },
      },
      CompressionMethod::Zstd => match compression_level {
        None => Ok(Self::Zstd(None)),
        Some(level) => {
          if Self::ZSTD_RANGE.contains(&level) {
            Ok(Self::Zstd(Some(level)))
          } else {
            Err(ParseCompressionOptionsError::InvalidCompressionLevel(
              compression_method,
              level,
              Self::ZSTD_RANGE,
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
      Self::Bzip2(level) => (
        ZipCompressionMethod::Bzip2,
        level.map(|l| {
          l.try_into()
            .expect("these values have already been checked")
        }),
      ),
      Self::Zstd(level) => (ZipCompressionMethod::Zstd, *level),
    };
    options
      .compression_method(method)
      .compression_level(level.map(|l| {
        l.try_into()
          .expect("these values have already been checked")
      }))
  }
}

#[derive(Copy, Clone, Default, Debug, Args)]
pub struct ZipOutputOptions {
  #[command(flatten)]
  pub mtime_behavior: ModifiedTimeBehavior,
  #[command(flatten)]
  pub compression_options: CompressionOptions,
}

#[derive(Clone, Default, Debug, Args)]
pub struct EntryModifications {
  #[arg(long, default_value = None)]
  pub silent_external_prefix: Option<String>,
  #[arg(long, default_value = None)]
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
      /* FIXME: perform this validation in  clap Arg derivation for EntryName! */
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

    /* TODO: explain why .iter_mut() is used here (to share dir components) over
     * .into_iter()! */
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
  File(EntryName, fs::File),
  ImmediateFile(ZipArchive<std::io::Cursor<Vec<u8>>>),
}

const SMALL_FILE_MAX_SIZE: usize = 10_000;

impl IntermediateSingleEntry {
  pub async fn open_handle(
    entry: ZipEntrySpecification,
    mut zip_options: zip::write::FileOptions,
    options_initializers: Arc<ZipOptionsInitializers>,
  ) -> Result<Self, MedusaInputReadError> {
    match entry {
      ZipEntrySpecification::Directory(name) => Ok(Self::Directory(name)),
      ZipEntrySpecification::File(FileSource { name, source }) => {
        let handle = fs::OpenOptions::new()
          .read(true)
          .open(&source)
          .await
          .map_err(|e| MedusaInputReadError::SourceNotFound(source.clone(), e))?;
        let metadata = handle
          .metadata()
          .await
          .map_err(|e| MedusaInputReadError::SourceNotFound(source, e))?;

        let reported_len: usize = metadata.len() as usize;

        /* If the file is large, we avoid trying to read it yet. */
        /* FIXME: handle the case of extremely large files: begin trying to buffer
         * large files in memory ahead of time, but only up to a certain
         * number. This will allow a single intermediate zip to start
         * buffering the results to multiple large files at once instead
         * of getting blocked on a single processor thread. */
        /* NB: can do this by converting a Self::File() into a stream that writes a
         * zip archive into a tempfile (not just in-mem), then returns a
         * ZipArchive of the tempfile. */
        if reported_len > SMALL_FILE_MAX_SIZE {
          Ok(Self::File(name, handle))
        } else {
          /* Otherwise, we enter the file into a single-entry zip. */
          let buf = std::io::Cursor::new(Vec::new());
          let mut mem_zip = ZipWriter::new(buf);

          zip_options = options_initializers.set_zip_options_for_file(zip_options, &metadata)?;

          /* FIXME: quit out of buffering if the file is actually larger than
           * reported!!! Also consider doing an async seek of the file to see where it
           * ends; this does not seem too bad of an idea actually. */
          let mut handle = handle.into_std().await;
          let mem_zip = task::spawn_blocking(move || {
            mem_zip.start_file(name.into_string(), zip_options)?;
            std::io::copy(&mut handle, &mut mem_zip)?;
            let buf = mem_zip.finish()?;
            let mem_zip = ZipArchive::new(buf)?;
            Ok::<_, ZipError>(mem_zip)
          })
          .await??;

          Ok(Self::ImmediateFile(mem_zip))
        }
      },
    }
  }
}

#[derive(Copy, Clone, Default, Debug, ValueEnum)]
pub enum Parallelism {
  /// Read source files and copy them to the output zip in order.
  #[default]
  Synchronous,
  /// Parallelize creation by splitting up the input into chunks.
  ParallelMerge,
}

pub struct MedusaZip {
  pub input_files: Vec<FileSource>,
  pub zip_options: ZipOutputOptions,
  pub modifications: EntryModifications,
  pub parallelism: Parallelism,
}

/* FIXME: make the later zips have more files than the earlier ones, so they
 * can take longer to complete (need to fully pipeline to make this useful)! */
const INTERMEDIATE_ZIP_THREADS: usize = 20;

/* TODO: make these configurable!!! */
const PARALLEL_ENTRIES: usize = 20;

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
      options = initializer.set_zip_options_for_file(options, &metadata)?;
    }
    Ok(options)
  }
}

impl MedusaZip {
  async fn zip_intermediate(
    entries: &[ZipEntrySpecification],
    zip_options: zip::write::FileOptions,
    options_initializers: Arc<ZipOptionsInitializers>,
  ) -> Result<ZipArchive<std::fs::File>, MedusaZipError> {
    /* (1) Create unnamed filesystem-backed temp file handle. */
    let intermediate_output = task::spawn_blocking(|| {
      let temp_file = tempfile()?;
      let zip_wrapper = ZipWriter::new(temp_file);
      Ok::<_, MedusaZipError>(zip_wrapper)
    })
    .await??;

    /* (2) Map to individual file handles and/or in-memory "immediate" zip files. */
    let (handle_tx, handle_rx) = mpsc::channel::<IntermediateSingleEntry>(PARALLEL_ENTRIES);
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
        IntermediateSingleEntry::File(name, handle) => {
          let mut handle = handle.into_std().await;
          task::spawn_blocking(move || {
            let mut intermediate_output = intermediate_output.lock();
            intermediate_output.start_file(name.into_string(), zip_options)?;
            std::io::copy(&mut handle, &mut *intermediate_output)?;
            Ok::<(), ZipError>(())
          })
          .await??;
        },
        IntermediateSingleEntry::ImmediateFile(archive) => {
          task::spawn_blocking(move || {
            let mut intermediate_output = intermediate_output.lock();
            intermediate_output.merge_archive(archive)?;
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
      let temp_file = zip_wrapper.finish()?;
      ZipArchive::new(temp_file)
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
    output_zip: Arc<Mutex<ZipWriter<Output>>>,
    zip_options: zip::write::FileOptions,
    mtime_behavior: ModifiedTimeBehavior,
  ) -> Result<(), MedusaZipError>
  where
    Output: Write+Seek+Send+'static,
  {
    let options_initializers = Arc::new(Self::options_initializers(mtime_behavior));
    /* (1) Split into however many subtasks (which may just be one) to do
     * "normally". */
    /* TODO: fully recursive? or just one level of recursion? */
    let chunk_size: usize = if entries.len() >= INTERMEDIATE_ZIP_THREADS {
      entries.len() / INTERMEDIATE_ZIP_THREADS
    } else {
      entries.len()
    };
    let ordered_intermediates = try_join_all(
      entries
        .chunks(chunk_size)
        .map(|entries| Self::zip_intermediate(entries, zip_options, options_initializers.clone())),
    )
    .await?;

    /* TODO: start piping in the first intermediate file as soon as it's ready! */
    for intermediate_zip in ordered_intermediates.into_iter() {
      let output_zip = output_zip.clone();
      task::spawn_blocking(move || {
        output_zip.lock().merge_archive(intermediate_zip)?;
        Ok::<(), MedusaZipError>(())
      })
      .await??;
    }

    Ok(())
  }

  async fn zip_synchronous<Output>(
    entries: Vec<ZipEntrySpecification>,
    output_zip: Arc<Mutex<ZipWriter<Output>>>,
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
            let mut output_zip = output_zip.lock();
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
            let mut output_zip = output_zip.lock();
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

  pub async fn zip<Output>(self, output_zip: ZipWriter<Output>) -> Result<Output, MedusaZipError>
  where Output: Write+Seek+Send+'static {
    let Self {
      input_files,
      zip_options: ZipOutputOptions {
        mtime_behavior,
        compression_options,
      },
      modifications,
      parallelism,
    } = self;
    let compression_options = CompressionStrategy::from_options(compression_options)?;

    let EntrySpecificationList(entries) = task::spawn_blocking(move || {
      EntrySpecificationList::from_file_specs(input_files, modifications)
    })
    .await??;

    let static_options_initializers: Vec<Box<dyn DefaultInitializeZipOptions>> =
      vec![Box::new(mtime_behavior), Box::new(compression_options)];
    let mut zip_options = ZipLibraryFileOptions::default();
    for initializer in static_options_initializers.into_iter() {
      zip_options = initializer.set_zip_options_static(zip_options);
    }

    let output_zip = Arc::new(Mutex::new(output_zip));
    match parallelism {
      Parallelism::Synchronous => {
        Self::zip_synchronous(entries, output_zip.clone(), zip_options, mtime_behavior).await?;
      },
      Parallelism::ParallelMerge => {
        Self::zip_parallel(entries, output_zip.clone(), zip_options, mtime_behavior).await?;
      },
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
