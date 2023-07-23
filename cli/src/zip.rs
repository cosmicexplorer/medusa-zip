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

#[cfg(doc)]
use libmedusa_zip::merge::MergeGroup;
use libmedusa_zip::zip as lib_zip;

use clap::{
  builder::{TypedValueParser, ValueParserFactory},
  Args, ValueEnum,
};
use displaydoc::Display;
use eyre::{self, WrapErr};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use zip::DateTime as ZipDateTime;


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
  pub automatic_mtime_strategy: AutomaticModifiedTimeStrategy,
  /// Assign a single [RFC 3339] timestamp such as '1985-04-12T23:20:50.52Z' to
  /// every file and directory.
  ///
  /// Because zip files do not retain time zone information, we must provide UTC
  /// offsets whenever we interact with them. The timestamps will also be
  /// truncated to 2-second accuracy.
  ///
  /// [RFC 3339]: https://datatracker.ietf.org/doc/html/rfc3339#section-5.6
  #[arg(long, default_value = None)]
  pub explicit_mtime_timestamp: Option<ZipDateTimeWrapper>,
}

impl From<ModifiedTimeBehavior> for lib_zip::ModifiedTimeBehavior {
  fn from(x: ModifiedTimeBehavior) -> Self {
    let ModifiedTimeBehavior {
      automatic_mtime_strategy,
      explicit_mtime_timestamp,
    } = x;
    match explicit_mtime_timestamp {
      Some(ZipDateTimeWrapper(timestamp)) => Self::Explicit(timestamp),
      None => match automatic_mtime_strategy {
        AutomaticModifiedTimeStrategy::Reproducible => Self::Reproducible,
        AutomaticModifiedTimeStrategy::CurrentTime => Self::CurrentTime,
        AutomaticModifiedTimeStrategy::PreserveSourceTime => Self::PreserveSourceTime,
      },
    }
  }
}


#[derive(Copy, Clone, Default, Debug, Display, ValueEnum)]
pub enum CompressionMethod {
  /// uncompressed
  Stored,
  /// deflate-compressed
  #[default]
  Deflated,
}

impl From<CompressionMethod> for lib_zip::CompressionMethod {
  fn from(x: CompressionMethod) -> Self {
    match x {
      CompressionMethod::Stored => Self::Stored,
      CompressionMethod::Deflated => Self::Deflated,
    }
  }
}


#[derive(Copy, Clone, Default, Debug, Args)]
pub struct CompressionOptions {
  /// This method is a default set for the entire file.
  ///
  /// The [`zip`] library will internally set compression to
  /// [`CompressionMethod::Stored`] for extremely small directory entries
  /// regardless of this setting as an optimization.
  #[arg(value_enum, default_value_t, long)]
  pub compression_method: CompressionMethod,
  /* /// - [`CompressionMethod::Bzip2`]: 0..=9 (default 6)
   * /// - [`CompressionMethod::Zstd`]: -7..=22 (default 3)
   */
  /// The degree of computational effort to exert for the
  /// [`Self::compression_method`].
  ///
  /// Each compression method interprets this argument differently:
  /// - [`CompressionMethod::Stored`]: the program will error if this is
  ///   provided.
  /// - [`CompressionMethod::Deflated`]: 0..=9 (default 6)
  ///   - 0 is also mapped to "default".
  #[arg(long, default_value = None, requires = "compression_method", verbatim_doc_comment)]
  pub compression_level: Option<i8>,
}


#[derive(Copy, Clone, Default, Debug, Args)]
pub struct ZipOutputOptions {
  #[command(flatten)]
  pub mtime_behavior: ModifiedTimeBehavior,
  #[command(flatten)]
  pub compression_options: CompressionOptions,
}

impl TryFrom<ZipOutputOptions> for lib_zip::ZipOutputOptions {
  type Error = eyre::Report;

  fn try_from(x: ZipOutputOptions) -> Result<Self, Self::Error> {
    let ZipOutputOptions {
      mtime_behavior,
      compression_options:
        CompressionOptions {
          compression_method,
          compression_level,
        },
    } = x;
    let compression_method: lib_zip::CompressionMethod = compression_method.into();
    let mtime_behavior: lib_zip::ModifiedTimeBehavior = mtime_behavior.into();
    let compression_options =
      lib_zip::CompressionStrategy::from_method_and_level(compression_method, compression_level)
        .wrap_err("error parsing compression strategy")?;
    Ok(Self {
      mtime_behavior,
      compression_options,
    })
  }
}


#[derive(Clone, Default, Debug, Args)]
pub struct EntryModifications {
  /// This prefixes a directory path to every entry without creating any of its
  /// parent directories.
  ///
  /// These prefixes always come before any prefixes introduced by
  /// [`Self::own_prefix`].
  ///
  /// `--silent-external-prefix .deps` => `[.deps/a, .deps/b, ...]`
  #[arg(long, default_value = None)]
  /* FIXME: make these both EntryName (also, parse EntryName at clap validation time)! */
  pub silent_external_prefix: Option<String>,
  /// This prefixes a directory path to every entry, but this *will* create
  /// parent directory entries in the output file.
  ///
  /// `--own-prefix .deps` => `[.deps/, .deps/a, .deps/b, ...]`
  /* FIXME: explain how these work when stacked together! */
  #[arg(long, default_value = None)]
  pub own_prefix: Option<String>,
}

impl From<lib_zip::EntryModifications> for EntryModifications {
  fn from(x: lib_zip::EntryModifications) -> Self {
    let lib_zip::EntryModifications {
      silent_external_prefix,
      own_prefix,
    } = x;
    Self {
      silent_external_prefix,
      own_prefix,
    }
  }
}

impl From<EntryModifications> for lib_zip::EntryModifications {
  fn from(x: EntryModifications) -> Self {
    let EntryModifications {
      silent_external_prefix,
      own_prefix,
    } = x;
    Self {
      silent_external_prefix,
      own_prefix,
    }
  }
}


#[derive(Copy, Clone, Default, Debug, ValueEnum)]
pub enum Parallelism {
  /// Read source files and copy them to the output zip in order.
  Synchronous,
  /// Parallelize creation by splitting up the input into chunks.
  #[default]
  ParallelMerge,
}


impl From<lib_zip::Parallelism> for Parallelism {
  fn from(x: lib_zip::Parallelism) -> Self {
    match x {
      lib_zip::Parallelism::Synchronous => Self::Synchronous,
      lib_zip::Parallelism::ParallelMerge => Self::ParallelMerge,
    }
  }
}

impl From<Parallelism> for lib_zip::Parallelism {
  fn from(x: Parallelism) -> Self {
    match x {
      Parallelism::Synchronous => Self::Synchronous,
      Parallelism::ParallelMerge => Self::ParallelMerge,
    }
  }
}
