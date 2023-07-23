# Description: ???
#
# Copyright (C) 2023 Danny McClanahan <dmcC2@hypnicjerk.ai>
# SPDX-License-Identifier: Apache-2.0
#
# Licensed under the Apache License, Version 2.0 (see LICENSE).

from pathlib import Path
from typing import Iterable, Optional

from .zip import EntryModifications, MedusaZip, Parallelism, ZipOutputOptions


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

  def medusa_zip(
    self,
    zip_options: Optional[ZipOutputOptions] = None,
    modifications: Optional[EntryModifications] = None,
    parallelism: Optional[Parallelism] = None,
  ) -> MedusaZip:
    ...


class Ignores:
  def __init__(self, patterns: Optional[Iterable[str]] = None) -> None:
    ...

  @classmethod
  def default(cls) -> 'Ignores': ...


class MedusaCrawl:
  def __init__(self, paths_to_crawl: Iterable[Path], ignores: Optional[Ignores] = None) -> None:
    ...

  @property
  def paths_to_crawl(self) -> Iterable[Path]: ...
  @property
  def ignores(self) -> Ignores: ...

  async def crawl_paths(self) -> CrawlResult: ...

  def crawl_paths_sync(self) -> CrawlResult: ...
