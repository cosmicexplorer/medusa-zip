# Description: ???
#
# Copyright (C) 2023 Danny McClanahan <dmcC2@hypnicjerk.ai>
# SPDX-License-Identifier: Apache-2.0
#
# Licensed under the Apache License, Version 2.0 (see LICENSE).

from pathlib import Path
from typing import Iterable, List, Optional


class ResolvedPath:
  def __init__(self, *, unresolved_path: Path, resolved_path: Path) -> None:
    ...

  @property
  def unresolved_path(self) -> Path: ...
  @property
  def resolved_path(self) -> Path: ...


class CrawlResult:
  def __init__(self, real_file_paths: Iterable[ResolvedPath]) -> None:
    ...

  @property
  def real_file_paths(self) -> Iterable[ResolvedPath]: ...


class Ignores:
  def __init__(self, patterns: Optional[List[str]] = None) -> None:
    ...


class MedusaCrawl:
  def __init__(self, paths_to_crawl: List[Path], ignores: Ignores) -> None:
    ...

  @property
  def paths_to_crawl(self) -> List[Path]: ...
  @property
  def ignores(self) -> Ignores: ...

  async def crawl_paths(self) -> CrawlResult: ...

  def crawl_paths_sync(self) -> CrawlResult: ...
