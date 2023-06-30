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

use libmedusa_zip::{MedusaCrawl, MedusaZip, MedusaZipOptions, Reproducibility};

use std::fs::OpenOptions;
use std::path::PathBuf;

/* use zip::{result::ZipError, write::FileOptions, ZipArchive, ZipWriter}; */

/* use std::fs::OpenOptions; */
/* use std::io::Write; */

/* fn main() -> Result<(), ZipError> { */
/*   let mut archive = OpenOptions::new() */
/*     .write(true) */
/*     .create(true) */
/*     .truncate(true) */
/*     .open("asdf.zip")?; */

/*   { */
/*     let mut zip = ZipWriter::new(&mut archive); */
/*     let options = FileOptions::default(); */

/*     zip.start_file("asdf.txt", options)?; */
/*     zip.write_all(b"asdf\n")?; */

/*     zip.start_file("bsdf.txt", options)?; */
/*     zip.write_all(b"bsdf\n")?; */

/*     zip.add_directory("a", options)?; */
/*     zip.start_file("a/b.txt", options)?; */
/*     zip.write_all(b"ab\n")?; */

/*     zip.add_directory("x", options)?; */
/*     zip.start_file("x/b.txt", options)?; */
/*     zip.write_all(b"xb\n")?; */

/*     zip.finish()?; */
/*   } */

/*   archive.sync_all()?; */
/*   Ok(()) */
/* } */

#[tokio::main]
async fn main() {
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
}
