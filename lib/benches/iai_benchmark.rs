/*
 * Description: ???
 *
 * Copyright (C) 2023 Danny McClanahan <dmcC2@hypnicjerk.ai>
 * SPDX-License-Identifier: Apache-2.0
 *
 * Licensed under the Apache License, Version 2.0 (see LICENSE).
 */

use iai;


mod setup {
  use libmedusa_zip as lib;

  use uuid::Uuid;
  use zip::result::ZipError;

  use std::{
    env, fs, io,
    path::{Path, PathBuf},
  };

  /* Hacky reimplementation of tempfile::TempDir since that one panics when
   * being invoked within a static initializer. */
  pub struct DeleteDirOnDrop(pub PathBuf);

  impl AsRef<Path> for DeleteDirOnDrop {
    fn as_ref(&self) -> &Path { self.0.as_ref() }
  }

  impl Drop for DeleteDirOnDrop {
    fn drop(&mut self) {
      let p: &Path = self.as_ref();
      eprintln!("dropping tmp subdir {}", p.display());
      fs::remove_dir_all(p).unwrap();
    }
  }

  pub fn create_parent_temp_dir() -> Result<DeleteDirOnDrop, io::Error> {
    let new_dirname: String = format!("{}", Uuid::new_v4());
    let result_dir = env::temp_dir().join(new_dirname);
    /* Our new UUID should ensure this is thread/process-safe. */
    fs::create_dir(&result_dir)?;
    dbg!(&result_dir);
    Ok(DeleteDirOnDrop(result_dir))
  }

  fn get_example_zip_path(filename: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
      .join("benches")
      .join(filename)
  }

  pub fn extract_example(
    filename: &str,
    tmp_root: &Path,
  ) -> Result<(Vec<lib::FileSource>, PathBuf), ZipError> {
    let full_zip_path = get_example_zip_path(filename);
    let unique_tmp_subdir = tmp_root.join(format!("{}", Uuid::new_v4()));
    fs::create_dir(&unique_tmp_subdir)?;
    let result = lib::bench_utils::extract_example_zip(&full_zip_path, &unique_tmp_subdir)?;
    Ok((result, unique_tmp_subdir))
  }
}


mod parallel_merge {
  use super::*;

  use libmedusa_zip as lib;

  use once_cell::sync::OnceCell;
  use tokio::runtime::Runtime;
  use zip;

  use std::{fs, path::PathBuf};

  #[static_init::dynamic(100, drop)]
  static PARENT_EXTRACT_DIR: setup::DeleteDirOnDrop = setup::create_parent_temp_dir().unwrap();

  static RUNTIME: OnceCell<Runtime> = OnceCell::new();

  pub fn setup_tokio_runtime() { RUNTIME.set(Runtime::new().unwrap()).unwrap(); }

  pub mod keras {
    use super::*;

    static KERAS_EXTRACTED: OnceCell<(Vec<lib::FileSource>, PathBuf)> = OnceCell::new();

    pub fn setup_keras() {
      KERAS_EXTRACTED
        .set(
          setup::extract_example("Keras-2.4.3-py2.py3-none-any.whl", unsafe {
            &*PARENT_EXTRACT_DIR.as_ref()
          })
          .unwrap(),
        )
        .unwrap();
    }

    pub fn keras_sync_crawl() -> lib::crawl::CrawlResult {
      let (_, keras_extracted) = KERAS_EXTRACTED.wait();
      lib::bench_utils::execute_basic_crawl(&keras_extracted).unwrap()
    }

    pub fn keras_medusa_crawl() -> lib::crawl::CrawlResult {
      let (_, keras_extracted) = KERAS_EXTRACTED.wait();
      let runtime = RUNTIME.wait();
      runtime
        .block_on(lib::bench_utils::execute_medusa_crawl(&keras_extracted))
        .unwrap()
    }

    pub fn keras_parallel_zip() -> zip::ZipArchive<fs::File> {
      let (keras_files, _) = KERAS_EXTRACTED.wait();
      let runtime = RUNTIME.wait();
      runtime
        .block_on(lib::bench_utils::execute_medusa_zip(
          keras_files.clone(),
          lib::zip::Parallelism::ParallelMerge,
        ))
        .unwrap()
    }

    pub fn keras_sync_zip() -> zip::ZipArchive<fs::File> {
      let (keras_files, _) = KERAS_EXTRACTED.wait();
      let runtime = RUNTIME.wait();
      runtime
        .block_on(lib::bench_utils::execute_medusa_zip(
          keras_files.clone(),
          lib::zip::Parallelism::Synchronous,
        ))
        .unwrap()
    }

    pub fn keras_basic_zip() -> zip::ZipArchive<fs::File> {
      let (keras_files, _) = KERAS_EXTRACTED.wait();
      lib::bench_utils::execute_basic_zip(keras_files.clone()).unwrap()
    }
  }

  pub mod pygments {
    use super::*;

    static PYGMENTS_EXTRACTED: OnceCell<(Vec<lib::FileSource>, PathBuf)> = OnceCell::new();

    pub fn setup_pygments() {
      PYGMENTS_EXTRACTED
        .set(
          setup::extract_example("Pygments-2.16.1-py3-none-any.whl", unsafe {
            &*PARENT_EXTRACT_DIR.as_ref()
          })
          .unwrap(),
        )
        .unwrap();
    }

    pub fn pygments_sync_crawl() -> lib::crawl::CrawlResult {
      let (_, pygments_extracted) = PYGMENTS_EXTRACTED.wait();
      lib::bench_utils::execute_basic_crawl(&pygments_extracted).unwrap()
    }

    pub fn pygments_medusa_crawl() -> lib::crawl::CrawlResult {
      let (_, pygments_extracted) = PYGMENTS_EXTRACTED.wait();
      let runtime = RUNTIME.wait();
      runtime
        .block_on(lib::bench_utils::execute_medusa_crawl(&pygments_extracted))
        .unwrap()
    }


    pub fn pygments_parallel_zip() -> zip::ZipArchive<fs::File> {
      let (pygments_files, _) = PYGMENTS_EXTRACTED.wait();
      let runtime = RUNTIME.wait();
      runtime
        .block_on(lib::bench_utils::execute_medusa_zip(
          pygments_files.clone(),
          lib::zip::Parallelism::ParallelMerge,
        ))
        .unwrap()
    }

    pub fn pygments_sync_zip() -> zip::ZipArchive<fs::File> {
      let (pygments_files, _) = PYGMENTS_EXTRACTED.wait();
      let runtime = RUNTIME.wait();
      runtime
        .block_on(lib::bench_utils::execute_medusa_zip(
          pygments_files.clone(),
          lib::zip::Parallelism::Synchronous,
        ))
        .unwrap()
    }

    pub fn pygments_basic_zip() -> zip::ZipArchive<fs::File> {
      let (pygments_files, _) = PYGMENTS_EXTRACTED.wait();
      lib::bench_utils::execute_basic_zip(pygments_files.clone()).unwrap()
    }
  }

  pub mod babel {
    use super::*;

    static BABEL_EXTRACTED: OnceCell<(Vec<lib::FileSource>, PathBuf)> = OnceCell::new();

    pub fn setup_babel() {
      BABEL_EXTRACTED
        .set(
          setup::extract_example("Babel-2.12.1-py3-none-any.whl", unsafe {
            &*PARENT_EXTRACT_DIR.as_ref()
          })
          .unwrap(),
        )
        .unwrap();
    }

    pub fn babel_sync_crawl() -> lib::crawl::CrawlResult {
      let (_, babel_extracted) = BABEL_EXTRACTED.wait();
      lib::bench_utils::execute_basic_crawl(&babel_extracted).unwrap()
    }

    pub fn babel_medusa_crawl() -> lib::crawl::CrawlResult {
      let (_, babel_extracted) = BABEL_EXTRACTED.wait();
      let runtime = RUNTIME.wait();
      runtime
        .block_on(lib::bench_utils::execute_medusa_crawl(&babel_extracted))
        .unwrap()
    }

    pub fn babel_parallel_zip() -> zip::ZipArchive<fs::File> {
      let (babel_files, _) = BABEL_EXTRACTED.wait();
      let runtime = RUNTIME.wait();
      runtime
        .block_on(lib::bench_utils::execute_medusa_zip(
          babel_files.clone(),
          lib::zip::Parallelism::ParallelMerge,
        ))
        .unwrap()
    }

    pub fn babel_sync_zip() -> zip::ZipArchive<fs::File> {
      let (babel_files, _) = BABEL_EXTRACTED.wait();
      let runtime = RUNTIME.wait();
      runtime
        .block_on(lib::bench_utils::execute_medusa_zip(
          babel_files.clone(),
          lib::zip::Parallelism::Synchronous,
        ))
        .unwrap()
    }

    pub fn babel_basic_zip() -> zip::ZipArchive<fs::File> {
      let (babel_files, _) = BABEL_EXTRACTED.wait();
      lib::bench_utils::execute_basic_zip(babel_files.clone()).unwrap()
    }
  }
}
use parallel_merge::{babel::*, keras::*, pygments::*, setup_tokio_runtime};

iai::setup_main!(
  setup_tokio_runtime, setup_keras, setup_pygments, setup_babel, :
  keras_sync_crawl, keras_medusa_crawl,
  keras_parallel_zip, keras_sync_zip, keras_basic_zip,
  pygments_sync_crawl, pygments_medusa_crawl,
  pygments_parallel_zip, pygments_sync_zip, pygments_basic_zip,
  babel_sync_crawl, babel_medusa_crawl,
  babel_parallel_zip, babel_sync_zip, babel_basic_zip,
);
