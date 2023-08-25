/*
 * Description: ???
 *
 * Copyright (C) 2023 Danny McClanahan <dmcC2@hypnicjerk.ai>
 * SPDX-License-Identifier: Apache-2.0
 *
 * Licensed under the Apache License, Version 2.0 (see LICENSE).
 */

//! ???

use crate::{
  crawl as lib_crawl, destination::OutputWrapper, zip as lib_zip, EntryName, FileSource,
};

use generic_array::{typenum::U32, GenericArray};
use rayon::prelude::*;
use sha3::{Digest, Sha3_256};
use tempfile;
use walkdir::WalkDir;
use zip::{self, result::ZipError};

use std::{
  fs,
  io::{self, Read, Seek},
  path::Path,
};


pub fn hash_file_bytes(f: &mut fs::File) -> Result<GenericArray<u8, U32>, io::Error> {
  f.rewind()?;

  let mut hasher = Sha3_256::new();
  let mut buf: Vec<u8> = Vec::new();
  /* TODO: how to hash in chunks at a time? */
  f.read_to_end(&mut buf)?;
  hasher.update(buf);

  Ok(hasher.finalize())
}


pub fn extract_example_zip(
  target: &Path,
) -> Result<(Vec<FileSource>, tempfile::TempDir), ZipError> {
  /* Create the temp dir to extract into. */
  let extract_dir = tempfile::tempdir()?;

  /* Load the zip archive from file. */
  let handle = fs::OpenOptions::new().read(true).open(target)?;
  let mut zip_archive = zip::ZipArchive::new(handle)?;

  /* Extract the zip's contents. */
  /* FIXME: make a parallelized zip extractor too! */
  zip_archive.extract(extract_dir.path())?;

  /* Generate the input to a MedusaZip by associating the (relative) file names
   * from the zip to their (absolute) extracted output paths. */
  let input_files: Vec<FileSource> = zip_archive.file_names()
      /* Ignore any directories, which are not represented in FileSource structs. */
      .filter(|f| !f.ends_with('/'))
      .map(|f| {
        let absolute_path = extract_dir.path().join(f);
        assert!(fs::metadata(&absolute_path).unwrap().is_file());
        let name = EntryName::validate(f.to_string()).unwrap();
        FileSource {
          name,
          source: absolute_path,
        }
      }).collect();

  Ok((input_files, extract_dir))
}


pub async fn execute_medusa_crawl(
  extracted_dir: &Path,
) -> Result<lib_crawl::CrawlResult, lib_crawl::MedusaCrawlError> {
  let ignores = lib_crawl::Ignores::default();
  let crawl_spec = lib_crawl::MedusaCrawl::for_single_dir(extracted_dir.to_path_buf(), ignores);
  let mut crawl_result = crawl_spec.crawl_paths().await?;
  /* This gets us something deterministic that  we can compare to the output of
   * execute_basic_crawl(). */
  crawl_result.real_file_paths.par_sort_by_cached_key(
    |lib_crawl::ResolvedPath {
       unresolved_path, ..
     }| unresolved_path.clone(),
  );
  Ok(crawl_result)
}

pub fn execute_basic_crawl(extracted_dir: &Path) -> Result<lib_crawl::CrawlResult, io::Error> {
  let mut real_file_paths: Vec<lib_crawl::ResolvedPath> = Vec::new();
  for entry in WalkDir::new(extracted_dir)
    .follow_links(false)
    .sort_by_file_name()
  {
    let entry = entry?;
    if entry.file_type().is_dir() {
      continue;
    }

    let unresolved_path = entry
      .path()
      .strip_prefix(extracted_dir)
      .unwrap()
      .to_path_buf();
    let rp = if entry.path_is_symlink() {
      lib_crawl::ResolvedPath {
        unresolved_path,
        resolved_path: fs::read_link(entry.path())?,
      }
    } else {
      lib_crawl::ResolvedPath {
        unresolved_path,
        resolved_path: entry.path().to_path_buf(),
      }
    };
    real_file_paths.push(rp);
  }

  let mut ret = lib_crawl::CrawlResult { real_file_paths };
  ret.clean_up_for_export(extracted_dir);
  Ok(ret)
}

pub async fn execute_medusa_zip(
  input_files: Vec<FileSource>,
  parallelism: lib_zip::Parallelism,
) -> Result<zip::ZipArchive<fs::File>, lib_zip::MedusaZipError> {
  let zip_spec = lib_zip::MedusaZip {
    input_files,
    zip_options: lib_zip::ZipOutputOptions {
      mtime_behavior: lib_zip::ModifiedTimeBehavior::Explicit(zip::DateTime::zero()),
      compression_options: lib_zip::CompressionStrategy::Deflated(Some(6)),
    },
    modifications: lib_zip::EntryModifications::default(),
    parallelism,
  };
  let output_zip = OutputWrapper::wrap(zip::ZipWriter::new(tempfile::tempfile()?));
  let mut output_zip = zip_spec.zip(output_zip).await?.reclaim();
  Ok(output_zip.finish_into_readable()?)
}

pub fn execute_basic_zip(
  input_files: Vec<FileSource>,
) -> Result<zip::ZipArchive<fs::File>, ZipError> {
  let mut output_zip = zip::ZipWriter::new(tempfile::tempfile()?);

  let options = zip::write::FileOptions::default()
    .compression_method(zip::CompressionMethod::Deflated)
    .compression_level(Some(6))
    .last_modified_time(zip::DateTime::zero());
  for FileSource { name, source } in input_files.into_iter() {
    let mut in_f = fs::OpenOptions::new().read(true).open(source)?;
    output_zip.start_file(name.into_string(), options)?;
    io::copy(&mut in_f, &mut output_zip)?;
  }

  Ok(output_zip.finish_into_readable()?)
}
