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

  use rayon::prelude::*;
  use tokio::runtime::Runtime;

  use std::{env, path::Path, time::Duration};

  /* #[static_init::dynamic] */

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
        (1000, (500, 500)),
        (
          Duration::from_secs(3),
          (Duration::from_secs(7), Duration::from_secs(7)),
        ),
        /* This says we don't care about changes under 15% for this *sync* benchmark, which is
         * a huge gap, but otherwise the synchronous benchmarks will constantly signal spurious
         * changes. */
        (0.1, (0.03, 0.15)),
        SamplingMode::Auto,
      ),
      (
        /* This file is 1.2M. */
        "Pygments-2.16.1-py3-none-any.whl",
        (1000, (100, 100)),
        (
          Duration::from_secs(5),
          (Duration::from_secs(8), Duration::from_secs(24)),
        ),
        (0.15, (0.07, 0.2)),
        SamplingMode::Auto,
      ),
      (
        /* This file is 9.7M. */
        "Babel-2.12.1-py3-none-any.whl",
        (1000, (80, 10)),
        (
          Duration::from_secs(3),
          (Duration::from_secs(35), Duration::from_secs(35)),
        ),
        /* 50% variation is within noise given our low sample size for the slow sync tests. */
        (0.2, (0.2, 0.5)),
        SamplingMode::Flat,
      ),
      /* ( */
      /*   /\* This file is 461M, or about half a gigabyte, with multiple individual very */
      /*    * large binary files. *\/ */
      /*   "tensorflow_gpu-2.5.3-cp38-cp38-manylinux2010_x86_64.whl", */
      /*   (100, (1, 1)), */
      /*   (Duration::from_secs(10), (Duration::ZERO, Duration::ZERO)), */
      /*   (0.1, (0.0, 0.0)), */
      /*   SamplingMode::Flat, */
      /* ), */
    ]
    .iter()
    {
      let target = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("benches")
        .join(filename);
      let zip_len = target.metadata().unwrap().len();

      let id = format!(
        "{}({} bytes)",
        &filename[..=filename.find('-').unwrap()],
        zip_len
      );

      group
        .sampling_mode(*mode)
        .throughput(Throughput::Bytes(zip_len as u64));

      /* FIXME: assigning `_` to the second arg of this tuple will destroy the
       * extract dir, which is only a silent error producing an empty file!!!
       * AWFUL UX!!! */
      let (input_files, extracted_dir) = lib::bench_utils::extract_example_zip(&target).unwrap();

      /* Compare the outputs of the two types of crawls. */
      let medusa_crawl_result = rt
        .block_on(lib::bench_utils::execute_medusa_crawl(extracted_dir.path()))
        .unwrap();
      let sync_crawl_result = lib::bench_utils::execute_basic_crawl(extracted_dir.path()).unwrap();
      assert_eq!(medusa_crawl_result, sync_crawl_result);

      /* Run the parallel filesystem crawl. */
      group
        .sample_size(*n_crawl)
        .measurement_time(*t_crawl)
        .noise_threshold(*noise_crawl);
      group.bench_function(
        BenchmarkId::new(&id, "<parallel crawling the extracted contents>"),
        |b| {
          b.to_async(&rt)
            .iter(|| lib::bench_utils::execute_medusa_crawl(extracted_dir.path()))
        },
      );
      /* Run the sync filesystem crawl. */
      group.bench_function(
        BenchmarkId::new(&id, "<sync crawling the extracted contents>"),
        |b| b.iter(|| lib::bench_utils::execute_basic_crawl(extracted_dir.path())),
      );

      if env::var_os("ONLY_CRAWL").is_some() {
        continue;
      }

      /* Run the parallel implementation. */
      group
        .sample_size(*n_p)
        .measurement_time(*t_p)
        .noise_threshold(*noise_p);
      let parallelism = lib::zip::Parallelism::ParallelMerge;
      group.bench_with_input(BenchmarkId::new(&id, parallelism), &parallelism, |b, p| {
        b.to_async(&rt)
          .iter(|| lib::bench_utils::execute_medusa_zip(input_files.clone(), *p));
      });
      let mut canonical_parallel_output = rt
        .block_on(lib::bench_utils::execute_medusa_zip(
          input_files.clone(),
          parallelism,
        ))
        .unwrap()
        .into_inner();
      let canonical_parallel_output =
        lib::bench_utils::hash_file_bytes(&mut canonical_parallel_output).unwrap();

      /* Run the sync implementation. */
      if env::var_os("NO_SYNC").is_none() {
        group
          .sample_size(*n_sync)
          .measurement_time(*t_sync)
          .noise_threshold(*noise_sync);
        /* FIXME: this takes >3x as long as sync zip! */
        /* Run the async version, but without any fancy queueing. */
        let parallelism = lib::zip::Parallelism::Synchronous;
        group.bench_with_input(BenchmarkId::new(&id, parallelism), &parallelism, |b, p| {
          b.to_async(&rt)
            .iter(|| lib::bench_utils::execute_medusa_zip(input_files.clone(), *p));
        });

        let canonical_sync = rt
          .block_on(lib::bench_utils::execute_medusa_zip(
            input_files.clone(),
            parallelism,
          ))
          .unwrap();
        let mut canonical_sync_filenames: Vec<_> =
          canonical_sync.file_names().map(|s| s.to_string()).collect();
        canonical_sync_filenames.par_sort_unstable();
        let mut canonical_sync = canonical_sync.into_inner();
        let canonical_sync = lib::bench_utils::hash_file_bytes(&mut canonical_sync).unwrap();
        assert_eq!(canonical_parallel_output, canonical_sync);

        /* Run the implementation based only off of the zip crate. We reuse the same
         * sampling presets under the assumption it will have a very similar
         * runtime. */
        group.bench_function(BenchmarkId::new(&id, "<sync zip crate>"), |b| {
          b.iter(|| lib::bench_utils::execute_basic_zip(input_files.clone()));
        });

        let canonical_basic = lib::bench_utils::execute_basic_zip(input_files.clone()).unwrap();
        /* We can't match our medusa zip file byte-for-byte against the zip crate
         * version, but we can at least check that they have the same
         * filenames. */
        let mut canonical_basic_filenames: Vec<_> = canonical_basic
          .file_names()
          .map(|s| s.to_string())
          .collect();
        canonical_basic_filenames.par_sort_unstable();
        /* NB: the zip crate basic impl does not introduce directory entries, so we
         * have to remove them here from the medusa zip to check equality. */
        let canonical_sync_filenames: Vec<_> = canonical_sync_filenames
          .into_par_iter()
          .filter(|name| !name.ends_with('/'))
          .collect();
        assert_eq!(canonical_sync_filenames, canonical_basic_filenames);
      }
    }

    group.finish();
  }
}

criterion_group!(benches, parallel_merge::bench_zips);
criterion_main!(benches);
