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
/* Note: run clippy with: rustup run nightly cargo-clippy! */
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

mod cli {
  mod args {
    use libmedusa_zip::{
      crawl::MedusaCrawlArgs,
      destination::DestinationBehavior,
      zip::{EntryModifications, ModifiedTimeBehaviorArgs, Parallelism, ZipOutputOptionsArgs},
    };

    use clap::{Args, Parser, Subcommand};

    use std::path::PathBuf;

    #[derive(Args, Debug)]
    pub struct Output {
      /// File path to write a zip to.
      #[arg(short, long)]
      pub output: PathBuf,
      /// How to initialize the zip file path to write results to.
      #[arg(value_enum, short, long, default_value_t)]
      pub destination_behavior: DestinationBehavior,
    }

    #[derive(Subcommand, Debug)]
    pub enum Command {
      /// Write a JSON object to stdout which contains all the file paths under
      /// the top-level `paths`.
      Crawl {
        #[command(flatten)]
        crawl: MedusaCrawlArgs,
      },
      /// Consume a JSON object from [`Self::Crawl`] over stdin and write those
      /// files into a zip file at `output`.
      Zip {
        #[command(flatten)]
        output: Output,
        #[command(flatten)]
        zip_options: ZipOutputOptionsArgs,
        #[command(flatten)]
        modifications: EntryModifications,
        #[arg(long, value_enum, default_value_t)]
        parallelism: Parallelism,
      },
      /// Merge the content of several zip files into one.
      Merge {
        #[command(flatten)]
        output: Output,
        /// ???
        #[command(flatten)]
        mtime_behavior: ModifiedTimeBehaviorArgs,
        /// ???
        #[arg()]
        source_zips_by_prefix: Vec<String>,
      },
      /// Perform a `crawl` and then a `zip` on its output in memory.
      CrawlZip {
        #[command(flatten)]
        crawl: MedusaCrawlArgs,
        #[command(flatten)]
        output: Output,
        #[command(flatten)]
        zip_options: ZipOutputOptionsArgs,
        #[command(flatten)]
        modifications: EntryModifications,
        #[arg(long, value_enum, default_value_t)]
        parallelism: Parallelism,
      },
      /// Perform a `zip` and then a `merge` without releasing the output file
      /// handle.
      ZipMerge {
        #[command(flatten)]
        output: Output,
        #[command(flatten)]
        zip_options: ZipOutputOptionsArgs,
        #[command(flatten)]
        modifications: EntryModifications,
        #[arg(long, value_enum, default_value_t)]
        parallelism: Parallelism,
        #[arg()]
        source_zips_by_prefix: Vec<String>,
      },
      /// Perform `crawl`, then a `zip` on its output in memory, then a `merge`
      /// into the same output file.
      CrawlZipMerge {
        #[command(flatten)]
        crawl: MedusaCrawlArgs,
        #[command(flatten)]
        output: Output,
        #[command(flatten)]
        zip_options: ZipOutputOptionsArgs,
        #[command(flatten)]
        modifications: EntryModifications,
        #[arg(long, value_enum, default_value_t)]
        parallelism: Parallelism,
        #[arg()]
        source_zips_by_prefix: Vec<String>,
      },
    }

    /// crawl file paths and produce zip files with some level of i/o and
    /// compute parallelism.
    #[derive(Parser, Debug)]
    #[command(author, version, about, long_about = None, verbatim_doc_comment)]
    pub struct Cli {
      #[command(subcommand)]
      pub command: Command,
    }
  }
  pub use args::{Cli, Command, Output};

  mod run {
    use super::{Cli, Command, Output};

    use libmedusa_zip::{
      crawl::{CrawlResult, MedusaCrawl},
      destination::OutputWrapper,
      merge::MedusaMerge,
    };

    use serde_json;
    use tokio::io::{self, AsyncReadExt};
    use zip::write::ZipWriter;

    use std::convert::TryInto;

    impl Output {
      pub async fn initialize(self) -> eyre::Result<ZipWriter<std::fs::File>> {
        let Self {
          output,
          destination_behavior,
        } = self;
        Ok(destination_behavior.initialize(&output).await?)
      }
    }

    impl Cli {
      pub async fn run(self) -> eyre::Result<()> {
        let Self { command } = self;

        match command {
          Command::Crawl { crawl } => {
            let crawl: MedusaCrawl = crawl.into();
            let crawl_result = crawl.crawl_paths().await?;
            let crawl_json = serde_json::to_string(&crawl_result)?;

            /* Print json serialization to stdout. */
            println!("{}", crawl_json);
          },
          Command::Zip {
            output,
            zip_options,
            modifications,
            parallelism,
          } => {
            /* Initialize output stream. */
            let output_zip = OutputWrapper::wrap(output.initialize().await?);

            /* Read json serialization from stdin. */
            let mut input_json: Vec<u8> = Vec::new();
            /* FIXME: convert this into *lines* of ResolvedPath (or something better)!! */
            io::stdin().read_to_end(&mut input_json).await?;
            let crawl_result: CrawlResult = serde_json::from_slice(&input_json)?;

            /* Apply options from command line to produce a zip spec. */
            let crawled_zip =
              crawl_result.medusa_zip(zip_options.try_into()?, modifications, parallelism)?;

            /* Do the parallel zip!!! */
            /* TODO: log the file output! */
            let _output_file_handle = crawled_zip.zip(output_zip).await?;
          },
          Command::Merge {
            output,
            mtime_behavior,
            source_zips_by_prefix,
          } => {
            /* Initialize output stream. */
            let output_zip = OutputWrapper::wrap(output.initialize().await?);

            let merge_spec = MedusaMerge::parse_from_args(source_zips_by_prefix.iter())?;
            /* Copy over constituent zips into current. */
            /* TODO: log the file output! */
            let _output_file_handle = merge_spec.merge(mtime_behavior.into(), output_zip).await?;
          },
          Command::CrawlZip {
            crawl,
            output,
            zip_options,
            modifications,
            parallelism,
          } => {
            /* Initialize output stream. */
            let output_zip = OutputWrapper::wrap(output.initialize().await?);

            let crawl: MedusaCrawl = crawl.into();
            /* Perform the actual crawl, traversing the filesystem in the process. */
            let crawl_result = crawl.crawl_paths().await?;

            /* Apply options from command line to produce a zip spec. */
            let crawled_zip =
              crawl_result.medusa_zip(zip_options.try_into()?, modifications, parallelism)?;

            /* Do the parallel zip over the crawled files!!! */
            /* TODO: log the file output! */
            let _output_file_handle = crawled_zip.zip(output_zip).await?;
          },
          Command::ZipMerge {
            output,
            zip_options,
            modifications,
            parallelism,
            source_zips_by_prefix,
          } => {
            /* Initialize output stream. */
            let output_zip = OutputWrapper::wrap(output.initialize().await?);

            /* Read json serialization from stdin. */
            let mut input_json: Vec<u8> = Vec::new();
            io::stdin().read_to_end(&mut input_json).await?;
            let crawl_result: CrawlResult = serde_json::from_slice(&input_json)?;

            /* Apply options from command line to produce a zip spec. */
            let crawled_zip =
              crawl_result.medusa_zip(zip_options.try_into()?, modifications, parallelism)?;

            /* Do the parallel zip!!! */
            let output_zip_file_handle = crawled_zip.zip(output_zip).await?;

            let merge_spec = MedusaMerge::parse_from_args(source_zips_by_prefix.iter())?;
            /* Copy over constituent zips into current. */
            /* TODO: log the file output! */
            let _output_file_handle = merge_spec
              .merge(zip_options.mtime_behavior.into(), output_zip_file_handle)
              .await?;
          },
          Command::CrawlZipMerge {
            crawl,
            output,
            zip_options,
            modifications,
            parallelism,
            source_zips_by_prefix,
          } => {
            /* Initialize output stream. */
            let output_zip = OutputWrapper::wrap(output.initialize().await?);

            let crawl: MedusaCrawl = crawl.into();
            let crawl_result = crawl.crawl_paths().await?;

            /* Apply options from command line to produce a zip spec. */
            let crawled_zip =
              crawl_result.medusa_zip(zip_options.try_into()?, modifications, parallelism)?;

            /* Do the parallel zip!!! */
            let output_zip_file_handle = crawled_zip.zip(output_zip).await?;

            let merge_spec = MedusaMerge::parse_from_args(source_zips_by_prefix.iter())?;
            /* Copy over constituent zips into current. */
            /* TODO: log the file output! */
            let _output_file_handle = merge_spec
              .merge(zip_options.mtime_behavior.into(), output_zip_file_handle)
              .await?;
          },
        }

        Ok(())
      }
    }
  }
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
  use clap::Parser as _;
  use eyre::WrapErr;

  let cli = cli::Cli::parse();
  cli.run().await.wrap_err("top-level error")?;
  Ok(())
}
