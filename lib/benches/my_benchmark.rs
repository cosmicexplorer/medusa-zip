/*
 * Description: ???
 *
 * Copyright (C) 2023 Danny McClanahan <dmcC2@hypnicjerk.ai>
 * SPDX-License-Identifier: Apache-2.0
 *
 * Licensed under the Apache License, Version 2.0 (see LICENSE).
 */

use criterion::{
  criterion_group, criterion_main, BenchmarkId, Criterion, SamplingMode, Throughput,
};

mod parallel_merge {
  use super::*;

  use libmedusa_zip as lib;

  use tempfile;
  use tokio::runtime::Runtime;
  use zip::{self, result::ZipError};

  use std::{fs, path::Path, time::Duration};

  fn extract_example_zip(
    target: &Path,
  ) -> Result<(Vec<lib::FileSource>, tempfile::TempDir), ZipError> {
    /* Create the temp dir to extract into. */
    let extract_dir = tempfile::tempdir()?;

    /* Load the zip archive from file. */
    let handle = fs::OpenOptions::new().read(true).open(target)?;
    let mut zip_archive = zip::ZipArchive::new(handle)?;

    /* Extract the zip's contents. */
    zip_archive.extract(extract_dir.path())?;

    /* Generate the input to a MedusaZip by associating the (relative) file names
     * from the zip to their (absolute) extracted output paths. */
    let input_files: Vec<lib::FileSource> = zip_archive.file_names()
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
    let output_zip =
      lib::destination::OutputWrapper::wrap(zip::ZipWriter::new(tempfile::tempfile()?));
    let mut output_zip = zip_spec.zip(output_zip).await?.reclaim();
    Ok(output_zip.finish_into_readable()?)
  }

  pub fn bench_zips(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("zip");
    group
      /* Increases analysis time (short compared to benchmark) but reduces
       * resampling errors (seems to make p-values smaller across repeated runs
       * of the same code (which should be correct, as it is the same code)). */
      .nresamples(300000)
      /* Only 1% of identical benchmarks should register as different due to noise, but we may miss
       * some small true changes. */
      .significance_level(0.01);

    for (filename, (n_p, n_sync), (t_p, t_sync), (noise_p, noise_sync), sampling_mode) in [
      (
        /* This file is 37K. */
        "Keras-2.4.3-py2.py3-none-any.whl",
        (500, 500),
        (Duration::from_secs(7), Duration::from_secs(7)),
        /* This says we don't care about changes under 15%, which is a huge gap,
         * but otherwise the synchronous benchmarks will constantly signal spurious changes. */
        (0.03, 0.15),
        SamplingMode::Auto,
      ),
      (
        /* This file is 1.2M. */
        "Pygments-2.16.1-py3-none-any.whl",
        (100, 100),
        (Duration::from_secs(8), Duration::from_secs(24)),
        (0.1, 0.2),
        SamplingMode::Auto,
      ),
      (
        /* This file is 9.7M. */
        "Babel-2.12.1-py3-none-any.whl",
        (80, 10),
        (Duration::from_secs(24), Duration::from_secs(35)),
        (0.2, 0.3),
        SamplingMode::Flat,
      ),
      /* ( */
      /*   /\* This file is 461M, or about half a gigabyte, with multiple individual very */
      /*    * large binary files. *\/ */
      /*   "tensorflow_gpu-2.5.3-cp38-cp38-manylinux2010_x86_64.whl", */
      /*   10, */
      /*   Duration::from_secs(330), */
      /* ), */
    ]
    .iter()
    {
      let target = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("benches")
        .join(filename);
      let zip_len = target.metadata().unwrap().len();

      let id = format!("{}({} bytes)", filename, zip_len);

      group
        .throughput(Throughput::Bytes(zip_len as u64))
        .sampling_mode(*sampling_mode);

      /* FIXME: assigning `_` to the second arg of this tuple will destroy the
       * extract dir, which is only a silent error producing an empty file!!!
       * AWFUL UX!!! */
      let (input_files, _tmp_extract_dir) = extract_example_zip(&target).unwrap();

      /* Run the parallel implementation. */
      let parallelism = lib::zip::Parallelism::ParallelMerge;
      group
        .sample_size(*n_p)
        .measurement_time(*t_p)
        .noise_threshold(*noise_p);
      group.bench_with_input(BenchmarkId::new(&id, parallelism), &parallelism, |b, p| {
        b.to_async(&rt)
          .iter(|| create_basic_zip(input_files.clone(), *p));
      });

      /* Run the sync implementation. */
      let parallelism = lib::zip::Parallelism::Synchronous;
      group
        .sample_size(*n_sync)
        .measurement_time(*t_sync)
        .noise_threshold(*noise_sync);
      group.bench_with_input(BenchmarkId::new(&id, parallelism), &parallelism, |b, p| {
        b.to_async(&rt)
          .iter(|| create_basic_zip(input_files.clone(), *p));
      });
    }

    group.finish();
  }
}

criterion_group!(benches, parallel_merge::bench_zips);
criterion_main!(benches);
