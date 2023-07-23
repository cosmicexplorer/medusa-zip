# Description: ???
#
# Copyright (C) 2023 Danny McClanahan <dmcC2@hypnicjerk.ai>
# SPDX-License-Identifier: Apache-2.0
#
# Licensed under the Apache License, Version 2.0 (see LICENSE).

from typing import Iterable, Optional

from . import FileSource
from .destination import ZipFileWriter


class AutomaticModifiedTimeStrategy:
  Reproducible: 'AutomaticModifiedTimeStrategy'
  CurrentTime: 'AutomaticModifiedTimeStrategy'
  PreserveSourceTime: 'AutomaticModifiedTimeStrategy'

  def __int__(self) -> int: ...


class ZipDateTime:
  @classmethod
  def parse(cls, s: str) -> 'ZipDateTime': ...


class ModifiedTimeBehavior:
  @classmethod
  def automatic(
    cls,
    automatic_mtime_strategy: AutomaticModifiedTimeStrategy,
  ) -> 'ModifiedTimeBehavior':
    ...

  @classmethod
  def explicit(
    cls,
    timestamp: ZipDateTime,
  ) -> 'ModifiedTimeBehavior':
    ...


class CompressionMethod:
  Stored: 'CompressionMethod'
  Deflated: 'CompressionMethod'
  Bzip2: 'CompressionMethod'
  Zstd: 'CompressionMethod'

  def __int__(self) -> int: ...


class CompressionOptions:
  def __init__(self, method: CompressionMethod, level: Optional[int]) -> None:
    ...

  @property
  def method(self) -> CompressionMethod: ...
  @property
  def level(self) -> Optional[int]: ...


class ZipOutputOptions:
  def __init__(
    self,
    mtime_behavior: ModifiedTimeBehavior,
    compression_options: CompressionOptions,
  ) -> None:
    ...

  @property
  def mtime_behavior(self) -> ModifiedTimeBehavior: ...
  @property
  def compression_options(self) -> CompressionOptions: ...


class EntryModifications:
  def __init__(
    self,
    silent_external_prefix: Optional[str],
    own_prefix: Optional[str],
  ) -> None:
    ...

  @property
  def silent_external_prefix(self) -> Optional[str]: ...
  @property
  def own_prefix(self) -> Optional[str]: ...


class Parallelism:
  Synchronous: 'Parallelism'
  ParallelMerge: 'Parallelism'

  def __int__(self) -> int: ...


class MedusaZip:
  def __init__(
    self,
    input_files: Iterable[FileSource],
    zip_options: ZipOutputOptions,
    modifications: EntryModifications,
    parallelism: Parallelism,
  ) -> None:
    ...

  async def zip(output_zip: ZipFileWriter) -> ZipFileWriter:
    ...

  def zip_sync(output_zip: ZipFileWriter) -> ZipFileWriter:
    ...
