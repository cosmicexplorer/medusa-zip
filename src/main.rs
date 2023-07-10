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
    use libmedusa_zip::{DestinationBehavior, MedusaZipOptions};

    use clap::{Args, Parser, Subcommand};

    use std::path::PathBuf;

    #[derive(Args, Debug)]
    pub struct Output {
      /// File path to write a zip to.
      #[arg(short, long)]
      pub output: PathBuf,
      /// How to initialize the zip file path to write results to.
      #[arg(value_enum, short, long, default_value_t)]
      pub behavior: DestinationBehavior,
    }

    #[derive(Subcommand, Debug)]
    pub enum Command {
      /// Write a JSON object to stdout which contains all the file paths under
      /// the top-level `paths`.
      Crawl {
        /// ???
        #[arg()]
        paths: Vec<PathBuf>,
        /// File path regex patterns to ignore.
        #[arg(short, long, default_values_t = Vec::<String>::new())]
        ignores: Vec<String>,
      },
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
        /// ???
        sources: Vec<PathBuf>,
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
      CrawlResult, DestinationError, MedusaCrawl, MedusaCrawlError, MedusaNameFormatError,
      MedusaZipError,
    };

    use displaydoc::Display;
    use regex::{self, RegexSet};
    use thiserror::Error;
    use zip::{read::ZipArchive, result::ZipError, write::ZipWriter};

    use serde_json;

    use std::{
      fs,
      io::{self, Read},
    };

    impl Output {
      pub async fn initialize(self) -> Result<ZipWriter<std::fs::File>, DestinationError> {
        let Self { output, behavior } = self;
        behavior.initialize(&output).await
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
      /// error creating output zip file: {0}
      Destination(#[from] DestinationError),
      /// error writing to output zip: {0}
      OutputZip(#[from] ZipError),
      /// error generating pattern from ignore regex: {0}
      Regex(#[from] regex::Error),
    }

    impl Cli {
      pub async fn run(self) -> Result<(), MedusaCliError> {
        let Self { command } = self;

        match command {
          Command::Crawl { paths, ignores } => {
            let crawl = MedusaCrawl {
              paths_to_crawl: paths,
              ignore_patterns: RegexSet::new(ignores)?,
            };
            let crawl_result = crawl.crawl_paths().await?;
            let crawl_json = serde_json::to_string(&crawl_result)?;

            /* Print json serialization to stdout. */
            println!("{}", crawl_json);
          },
          Command::Zip { output, options } => {
            /* Initialize output stream. */
            let output_zip = output.initialize().await?;

            /* Read json serialization from stdin. */
            let mut input_json: Vec<u8> = Vec::new();
            io::stdin().lock().read_to_end(&mut input_json)?;
            let crawl_result: CrawlResult = serde_json::from_slice(&input_json)?;

            /* Apply options from command line to produce a zip spec. */
            let crawled_zip = crawl_result.medusa_zip(options)?;

            /* Do the parallel zip!!! */
            /* TODO: log the file output! */
            let _output_file_handle = crawled_zip.zip(output_zip).await?;
          },
          Command::Merge { output, sources } => {
            /* Initialize output stream. */
            let mut output_zip = output.initialize().await?;

            /* Copy over constituent zips into current. */
            for source_zip_path in sources.into_iter() {
              let source_file = fs::OpenOptions::new().read(true).open(&source_zip_path)?;
              let source_zip = ZipArchive::new(source_file)?;
              output_zip.merge_archive(source_zip)?;
            }

            let _output_file_handle = output_zip.finish()?;
          },
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
