#!/usr/bin/env python

import logging
import os
import sys
import tempfile
import zipfile
from pathlib import Path

from medusa_zip.crawl import *
from medusa_zip.zip import *
from medusa_zip.destination import *

logger = logging.getLogger(__name__)


def main(zip_src: str, out_zip: str) -> None:
  zip_src = Path(zip_src).absolute()
  logger.info(f"zip_src = {zip_src}")
  out_zip = Path(out_zip).absolute()
  logger.info(f"out_zip = {out_zip}")

  with tempfile.TemporaryDirectory() as td:
    with zipfile.ZipFile(zip_src, mode="r") as zf:
      zf.extractall(path=td)

    os.chdir(td)

    crawl_spec = MedusaCrawl(['.'])
    crawl_result = crawl_spec.crawl_paths_sync()

    zip_spec = crawl_result.medusa_zip(parallelism=Parallelism.ParallelMerge)
    out_handle = DestinationBehavior.AlwaysTruncate.initialize_sync(out_zip)

    zip_spec.zip_sync(out_handle)
    out_handle.finish_sync()


if __name__ == '__main__':
  main(*sys.argv[1:])
