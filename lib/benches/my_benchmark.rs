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

  use std::{env, fs, io, path::Path, time::Duration};

  fn extract_example_zip(
    target: &Path,
  ) -> Result<(Vec<lib::FileSource>, tempfile::TempDir), ZipError> {
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

  async fn execute_medusa_crawl(
    extracted_dir: &Path,
  ) -> Result<lib::crawl::CrawlResult, lib::crawl::MedusaCrawlError> {
    let ignores = lib::crawl::Ignores::default();
    let crawl_spec = lib::crawl::MedusaCrawl::for_single_dir(extracted_dir.to_path_buf(), ignores);
    crawl_spec.crawl_paths().await
  }

  async fn execute_medusa_zip(
    input_files: Vec<lib::FileSource>,
    parallelism: lib::zip::Parallelism,
  ) -> Result<zip::ZipArchive<fs::File>, lib::zip::MedusaZipError> {
    let zip_spec = lib::zip::MedusaZip {
      input_files,
      zip_options: lib::zip::ZipOutputOptions {
        mtime_behavior: lib::zip::ModifiedTimeBehavior::Explicit(zip::DateTime::zero()),
        compression_options: lib::zip::CompressionStrategy::Deflated(Some(6)),
      },
      modifications: lib::zip::EntryModifications::default(),
      parallelism,
    };
    let output_zip =
      lib::destination::OutputWrapper::wrap(zip::ZipWriter::new(tempfile::tempfile()?));
    let mut output_zip = zip_spec.zip(output_zip).await?.reclaim();
    Ok(output_zip.finish_into_readable()?)
  }

  fn execute_basic_zip(
    input_files: Vec<lib::FileSource>,
  ) -> Result<zip::ZipArchive<fs::File>, ZipError> {
    let mut output_zip = zip::ZipWriter::new(tempfile::tempfile()?);

    let options = zip::write::FileOptions::default()
      .compression_method(zip::CompressionMethod::Deflated)
      .compression_level(Some(6))
      .last_modified_time(zip::DateTime::zero());
    for lib::FileSource { name, source } in input_files.into_iter() {
      let mut in_f = fs::OpenOptions::new().read(true).open(source)?;
      output_zip.start_file(name.into_string(), options)?;
      io::copy(&mut in_f, &mut output_zip)?;
    }

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

    for (
      filename,
      (n_crawl, (n_p, n_sync)),
      (t_crawl, (t_p, t_sync)),
      (noise_crawl, (noise_p, noise_sync)),
      mode,
    ) in [
      (
        /* This file is 37K. */
        "Keras-2.4.3-py2.py3-none-any.whl",
        (500, (500, 500)),
        (Duration::from_secs(3), (Duration::from_secs(7), Duration::from_secs(7))),
        /* This says we don't care about changes under 15% for this *sync* benchmark, which is
         * a huge gap, but otherwise the synchronous benchmarks will constantly signal spurious
         * changes. */
        (0.05, (0.03, 0.15)),
        SamplingMode::Auto,
      ),
      (
        /* This file is 1.2M. */
        "Pygments-2.16.1-py3-none-any.whl",
        (100, (100, 100)),
        (Duration::from_secs(5), (Duration::from_secs(8), Duration::from_secs(24))),
        (0.05, (0.07, 0.2)),
        SamplingMode::Auto,
      ),
      (
        /* This file is 9.7M. */
        "Babel-2.12.1-py3-none-any.whl",
        (100, (80, 10)),
        (Duration::from_secs(3), (Duration::from_secs(35), Duration::from_secs(35))),
        /* 50% variation is within noise given our low sample size for the slow sync tests. */
        (0.1, (0.2, 0.5)),
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

      group.throughput(Throughput::Bytes(zip_len as u64));

      /* FIXME: assigning `_` to the second arg of this tuple will destroy the
       * extract dir, which is only a silent error producing an empty file!!!
       * AWFUL UX!!! */
      let (input_files, extracted_dir) = extract_example_zip(&target).unwrap();

      /* Run the parallel filesystem crawl. */
      group
        .sample_size(*n_crawl)
        .measurement_time(*t_crawl)
        .noise_threshold(*noise_crawl);
      group.bench_function(
        BenchmarkId::new(&id, "<crawling the extracted contents>"),
        |b| {
          b.to_async(&rt)
            .iter(|| execute_medusa_crawl(extracted_dir.path()))
        },
      );

      group.sampling_mode(*mode);

      /* Run the parallel implementation. */
      let parallelism = lib::zip::Parallelism::ParallelMerge;
      group
        .sample_size(*n_p)
        .measurement_time(*t_p)
        .noise_threshold(*noise_p);
      group.bench_with_input(BenchmarkId::new(&id, parallelism), &parallelism, |b, p| {
        b.to_async(&rt)
          .iter(|| execute_medusa_zip(input_files.clone(), *p));
      });

      /* Run the sync implementation. */
      if env::var_os("NO_SYNC").is_none() {
        let parallelism = lib::zip::Parallelism::Synchronous;
        group
          .sample_size(*n_sync)
          .measurement_time(*t_sync)
          .noise_threshold(*noise_sync);
        group.bench_with_input(BenchmarkId::new(&id, parallelism), &parallelism, |b, p| {
          b.to_async(&rt)
            .iter(|| execute_medusa_zip(input_files.clone(), *p));
        });

        /* Run the implementation based only off of the zip crate. We reuse the same
         * sampling presets under the assumption it will have a very similar
         * runtime. */
        group.bench_function(BenchmarkId::new(&id, "<sync zip crate>"), |b| {
          b.iter(|| execute_basic_zip(input_files.clone()));
        });
      }
    }

    group.finish();
  }
}

criterion_group!(benches, parallel_merge::bench_zips);
criterion_main!(benches);
