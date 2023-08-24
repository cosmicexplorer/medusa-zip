#!/usr/bin/env python

from __future__ import annotations

import os
import shutil
import sys
import tempfile
import zipfile
from pathlib import Path

from medusa_zip.crawl import *
from medusa_zip.zip import *
from medusa_zip.destination import *


def main(zip_src: str, out_zip: str, zipfile_zip: str | None = None) -> None:
  zip_src = Path(zip_src).absolute()
  print(f"zip_src = {zip_src}", file=sys.stderr)
  out_zip = Path(out_zip).absolute()
  print(f"out_zip = {out_zip}", file=sys.stderr)
  if zipfile_zip is not None:
    zipfile_zip = Path(zipfile_zip).absolute()
    print(f"zipfile_zip = {zipfile_zip}", file=sys.stderr)

  with tempfile.TemporaryDirectory() as td:
    with zipfile.ZipFile(zip_src, mode="r") as zf:
      zf.extractall(path=td)
    print(f"zip extracted into temp dir at {td}", file=sys.stderr)

    os.chdir(td)

    crawl_spec = MedusaCrawl(['.'])
    crawl_result = crawl_spec.crawl_paths_sync()
    print(f"crawled temp dir ({len(crawl_result.real_file_paths)} files)", file=sys.stderr)

    parallelism = Parallelism.ParallelMerge
    zip_spec = crawl_result.medusa_zip(parallelism=parallelism)
    out_handle = DestinationBehavior.AlwaysTruncate.initialize_sync(out_zip)

    zip_spec.zip_sync(out_handle)
    out_handle.finish_sync()
    print(f"zipped with parallelism {parallelism}", file=sys.stderr)

    if zipfile_zip is not None:
      with zipfile.ZipFile(zipfile_zip, mode="w", compression=zipfile.ZIP_DEFLATED) as zipfile_zf:
        for rp in crawl_result.real_file_paths:
          # FIXME: no intermediate directories written!
          with Path(rp.resolved_path).open(mode="rb") as in_f,\
               zipfile_zf.open(str(rp.unresolved_path), mode="w") as out_f:
            shutil.copyfileobj(in_f, out_f)
      print(f"zipped with ZipFile into {zipfile_zip}", file=sys.stderr)


if __name__ == '__main__':
  main(*sys.argv[1:])
