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

use libmedusa_zip::{CrawlResult, MedusaCrawl, MedusaZip, MedusaZipOptions, Reproducibility};

use clap::Parser;
use serde_json;

use std::fs::OpenOptions;
use std::io::{self, Read};
use std::path::PathBuf;

mod cli {
  use libmedusa_zip::{MedusaZipOptions, Reproducibility};

  use clap::{Args, Parser, Subcommand, ValueEnum};

  use std::path::PathBuf;

  #[derive(Clone, Debug, Default, ValueEnum)]
  pub enum CliReproducibility {
    #[default]
    Reproducible,
    CurrentTime,
  }

  impl From<CliReproducibility> for Reproducibility {
    fn from(r: CliReproducibility) -> Self {
      match r {
        CliReproducibility::Reproducible => Self::Reproducible,
        CliReproducibility::CurrentTime => Self::CurrentTime,
      }
    }
  }

  impl From<Reproducibility> for CliReproducibility {
    fn from(r: Reproducibility) -> Self {
      match r {
        Reproducibility::Reproducible => Self::Reproducible,
        Reproducibility::CurrentTime => Self::CurrentTime,
      }
    }
  }

  #[derive(Args, Clone, Debug)]
  pub struct ZipOptions {
    #[arg(value_enum, default_value_t, short, long)]
    reproducibility: CliReproducibility,
  }

  impl From<ZipOptions> for MedusaZipOptions {
    fn from(o: ZipOptions) -> Self {
      let ZipOptions { reproducibility } = o;
      Self {
        reproducibility: reproducibility.into(),
      }
    }
  }

  impl From<MedusaZipOptions> for ZipOptions {
    fn from(o: MedusaZipOptions) -> Self {
      let MedusaZipOptions { reproducibility } = o;
      Self {
        reproducibility: reproducibility.into(),
      }
    }
  }

  #[derive(Subcommand, Debug)]
  pub enum Command {
    Crawl {
      paths: Vec<PathBuf>,
    },
    Zip {
      output: PathBuf,
      #[command(flatten)]
      options: ZipOptions,
    },
    TempDemo,
  }

  #[derive(Parser, Debug)]
  #[command(author, version, about, long_about = None)]
  pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
  }
}
use cli::*;

#[tokio::main]
async fn main() {
  let Cli { command } = Cli::parse();

  match command {
    Command::Crawl { paths } => {
      let crawl = MedusaCrawl {
        paths_to_crawl: paths,
      };
      let crawl_result = crawl.crawl_paths().await.expect("crawling failed");
      let crawl_json = serde_json::to_string(&crawl_result).expect("serializing crawl failed");

      /* Print json serialization to stdout. */
      println!("{}", crawl_json);
    },
    Command::Zip { output, options } => {
      /* Read json serialization from stdin. */
      let mut input_json: Vec<u8> = Vec::new();
      io::stdin()
        .lock()
        .read_to_end(&mut input_json)
        .expect("reading stdin failed");
      let crawl_result: CrawlResult =
        serde_json::from_slice(&input_json).expect("deserializing crawl failed");
      let crawled_zip = crawl_result.medusa_zip(options.into());

      let output_file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&output)
        .expect("output zip open failed");
      crawled_zip.zip(output_file).await.expect("zipping failed");

      eprintln!("wrote to: {}", output.display());
    },
    Command::TempDemo => {
      let crawl = MedusaCrawl {
        paths_to_crawl: vec![PathBuf::from("tmp3")],
      };
      let crawl_result = crawl.crawl_paths().await.expect("crawling failed");
      println!("crawl_result = {:?}", crawl_result);
      let crawled_zip = crawl_result.medusa_zip(MedusaZipOptions {
        reproducibility: Reproducibility::Reproducible,
      });
      let crawled_output_path = PathBuf::from("asdf3.zip");
      let crawled_output_file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&crawled_output_path)
        .expect("file open failed");
      crawled_zip
        .zip(crawled_output_file)
        .await
        .expect("zipping failed");
      println!("wrote to: {}", crawled_output_path.display());

      let zip_spec = MedusaZip {
        input_paths: vec![
          (PathBuf::from("tmp/asdf.txt"), "asdf.txt".to_string()),
          (PathBuf::from("tmp/bsdf.txt"), "bsdf.txt".to_string()),
          (PathBuf::from("tmp/a/b.txt"), "a/b.txt".to_string()),
          (PathBuf::from("tmp/x/b.txt"), "x/b.txt".to_string()),
        ],
        options: MedusaZipOptions {
          reproducibility: Reproducibility::Reproducible,
        },
      };
      let output_path = PathBuf::from("asdf2.zip");
      let output_file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&output_path)
        .expect("file open failed");

      zip_spec.zip(output_file).await.expect("zipping failed");
      println!("wrote to: {}", output_path.display());
    },
  }
}
