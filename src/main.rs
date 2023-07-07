/*
 * Description: Crawl file paths and produce zip files with some level of i/o
 * and compute parallelism.
 *
 * Copyright (C) 2023 Danny McClanahan <dmcC2@hypnicjerk.ai>
 * SPDX-License-Identifier: Apache-2.0
 *
 * Licensed under the Apache License, Version 2.0 (see LICENSE).
 */

//! Crawl file paths and produce zip files with some level of i/o and compute
//! parallelism.

/* These clippy lint descriptions are purely non-functional and do not affect the functionality
 * or correctness of the code. */
#![warn(missing_docs)]
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

use clap::Parser as _;

mod cli {
  mod args {
    use libmedusa_zip::MedusaZipOptions;

    use clap::{Args, Parser, Subcommand};

    use std::path::PathBuf;

    #[derive(Args, Debug)]
    pub struct Output {
      /// File path to write a zip to.
      #[arg(short, long, default_value = None)]
      pub output: Option<PathBuf>,
    }

    #[derive(Subcommand, Debug)]
    pub enum Command {
      /// Write a JSON object to stdout which contains all the file paths under
      /// the top-level `paths`.
      Crawl { paths: Vec<PathBuf> },
      /// Consume a JSON object from [`Self::Crawl`] over stdin and write those
      /// files into a zip file at `output`.
      Zip {
        #[command(flatten)]
        output: Output,
        #[command(flatten)]
        options: MedusaZipOptions,
      },
      /// Merge the content of several zip files into one.
      Merge {
        #[command(flatten)]
        output: Output,
      },
    }

    /// Crawl file paths and produce zip files with some level of i/o and
    /// compute parallelism.
    #[derive(Parser, Debug)]
    #[command(author, version, about, long_about = None)]
    pub struct Cli {
      #[command(subcommand)]
      pub command: Command,
    }
  }
  pub use args::{Cli, Command, Output};

  mod run {
    use super::{Cli, Command, Output};

    use libmedusa_zip::{
      CrawlResult, MedusaCrawl, MedusaCrawlError, MedusaNameFormatError, MedusaZipError,
    };

    use displaydoc::Display;
    use thiserror::Error;

    use serde_json;

    use std::{
      fs::{self, OpenOptions},
      io::{self, Cursor, Read, Seek, Write},
    };

    pub enum OutputStream {
      File(fs::File),
      Stdout(Cursor<Vec<u8>>),
    }

    impl OutputStream {
      pub fn from_args(o: Output) -> Result<Self, io::Error> {
        let Output { output } = o;
        match output {
          None => Ok(Self::Stdout(Cursor::new(Vec::new()))),
          Some(path) => Ok(Self::File(
            OpenOptions::new()
              .write(true)
              .create(true)
              .append(true)
              .open(&path)?,
          )),
        }
      }
    }

    impl io::Read for OutputStream {
      fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
          Self::File(f) => f.read(buf),
          Self::Stdout(c) => c.read(buf),
        }
      }

      fn read_vectored(&mut self, bufs: &mut [io::IoSliceMut<'_>]) -> io::Result<usize> {
        match self {
          Self::File(f) => f.read_vectored(bufs),
          Self::Stdout(c) => c.read_vectored(bufs),
        }
      }
    }

    impl io::Write for OutputStream {
      fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
          Self::File(f) => f.write(buf),
          Self::Stdout(c) => c.write(buf),
        }
      }

      fn write_vectored(&mut self, bufs: &[io::IoSlice<'_>]) -> io::Result<usize> {
        match self {
          Self::File(f) => f.write_vectored(bufs),
          Self::Stdout(c) => c.write_vectored(bufs),
        }
      }

      fn flush(&mut self) -> io::Result<()> {
        match self {
          Self::File(f) => f.flush(),
          Self::Stdout(c) => c.flush(),
        }
      }
    }

    impl io::Seek for OutputStream {
      fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        match self {
          Self::File(f) => f.seek(pos),
          Self::Stdout(c) => c.seek(pos),
        }
      }
    }

    impl Drop for OutputStream {
      fn drop(&mut self) {
        match self {
          Self::Stdout(c) => {
            c.rewind().expect("error rewinding during drop");
            let mut stdout = io::stdout().lock();
            io::copy(c, &mut stdout).expect("error copying to stdout during drop");
          },
          _ => (),
        }
      }
    }

    #[derive(Debug, Display, Error)]
    pub enum MedusaCliError {
      /// error performing parallel zip: {0}
      MedusaZip(#[from] MedusaZipError),
      /// error performing parallel crawl: {0}
      MedusaCrawl(#[from] MedusaCrawlError),
      /// error in zip entry name: {0}
      MedusaNameFormat(#[from] MedusaNameFormatError),
      /// error performing top-level i/o: {0}
      Io(#[from] io::Error),
      /// error de/serializing json: {0}
      Json(#[from] serde_json::Error),
    }

    impl Cli {
      pub async fn run(self) -> Result<(), MedusaCliError> {
        let Self { command } = self;

        match command {
          Command::Crawl { paths } => {
            let crawl = MedusaCrawl {
              paths_to_crawl: paths,
            };
            let crawl_result = crawl.crawl_paths().await?;
            let crawl_json = serde_json::to_string(&crawl_result)?;

            /* Print json serialization to stdout. */
            println!("{}", crawl_json);
          },
          Command::Zip { output, options } => {
            /* Initialize output file. */
            let output_file = OutputStream::from_args(output)?;

            /* Read json serialization from stdin. */
            let mut input_json: Vec<u8> = Vec::new();
            io::stdin().lock().read_to_end(&mut input_json)?;
            let crawl_result: CrawlResult = serde_json::from_slice(&input_json)?;

            /* Apply options from command line to produce a zip spec. */
            let crawled_zip = crawl_result.medusa_zip(options)?;

            crawled_zip.zip(output_file).await?;
          },
          Command::Merge { output } => {},
        }

        Ok(())
      }
    }
  }
}

#[tokio::main]
async fn main() {
  let cli = cli::Cli::parse();

  cli.run().await.expect("top-level error");
}
