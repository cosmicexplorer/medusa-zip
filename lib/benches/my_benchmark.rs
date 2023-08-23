/*
 * Description: ???
 *
 * Copyright (C) 2023 Danny McClanahan <dmcC2@hypnicjerk.ai>
 * SPDX-License-Identifier: Apache-2.0
 *
 * Licensed under the Apache License, Version 2.0 (see LICENSE).
 */

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};

mod parallel_merge {
  use super::*;

  use libmedusa_zip::{self as lib, destination::OutputWrapper};

  use tempfile;
  use tokio::runtime::Runtime;
  use zip::{self, result::ZipError};

  use std::{fs, io, time::Duration};

  /* This file is 461M, or about half a gigabyte, with multiple individual very
   * large binary files. */
  /* const LARGE_ZIP_CONTENTS: &'static [u8] = */
  /* include_bytes!("tensorflow_gpu-2.5.3-cp38-cp38-manylinux2010_x86_64.whl"); */

  /* This file is 37K. */
  const SMALLER_ZIP_CONTENTS: &'static [u8] = include_bytes!("Keras-2.4.3-py2.py3-none-any.whl");

  fn prepare_memory_zip(
    zip_contents: &[u8],
  ) -> Result<(Vec<lib::FileSource>, tempfile::TempDir), ZipError> {
    /* Create the temp dir to extract into. */
    let extract_dir = tempfile::tempdir()?;

    /* Load the zip archive from memory. */
    let reader = io::Cursor::new(zip_contents);
    let mut large_zip = zip::ZipArchive::new(reader)?;

    /* Extract the zip's contents. */
    large_zip.extract(extract_dir.path())?;

    /* Generate the input to a MedusaZip by associating the (relative) file names
     * from the zip to their (absolute) extracted output paths. */
    let input_files: Vec<lib::FileSource> = large_zip.file_names()
    /* Ignore any directories, which are not represented in FileSource structs. */
    .filter(|f| !f.ends_with('/'))
    .map(|f| {
      let absolute_path = extract_dir.path().join(f);
      assert!(fs::metadata(&absolute_path).unwrap().is_file());
      let name = lib::EntryName::validate(f.to_string()).unwrap();
      lib::FileSource {
        name,
        source: absolute_path,
      }
    }).collect();

    Ok((input_files, extract_dir))
  }


  async fn create_basic_zip(
    input_files: Vec<lib::FileSource>,
    parallelism: lib::zip::Parallelism,
  ) -> Result<zip::ZipArchive<fs::File>, lib::zip::MedusaZipError> {
    let zip_spec = lib::zip::MedusaZip {
      input_files,
      zip_options: lib::zip::ZipOutputOptions::default(),
      modifications: lib::zip::EntryModifications::default(),
      parallelism,
    };
    let output_zip = OutputWrapper::wrap(zip::ZipWriter::new(tempfile::tempfile()?));
    let mut output_zip = zip_spec.zip(output_zip).await?.reclaim();
    Ok(output_zip.finish_into_readable()?)
  }


  pub fn bench_zips(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("zip");
    /* group.measurement_time(Duration::from_secs(3258)); */

    for (id, zip_contents) in [
      ("Keras-2.4.3-py2.py3-none-any.whl", SMALLER_ZIP_CONTENTS),
      /* ("tensorflow_gpu-2.5.3-cp38-cp38-manylinux2010_x86_64.whl", LARGE_ZIP_CONTENTS), */
    ]
    .iter()
    {
      let (input_files, _td) = prepare_memory_zip(zip_contents).unwrap();
      group.bench_with_input(
        BenchmarkId::new(*id, "ParallelMerge"),
        &lib::zip::Parallelism::ParallelMerge,
        |b, p| {
          b.to_async(&rt)
            .iter(|| create_basic_zip(input_files.clone(), *p));
        },
      );
      group.bench_with_input(
        BenchmarkId::new(*id, "Synchronous"),
        &lib::zip::Parallelism::Synchronous,
        |b, p| {
          b.to_async(&rt)
            .iter(|| create_basic_zip(input_files.clone(), *p));
        },
      );
    }
  }
}

criterion_group!(benches, parallel_merge::bench_zips);
criterion_main!(benches);
