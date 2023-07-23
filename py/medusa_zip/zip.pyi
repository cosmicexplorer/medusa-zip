# Description: ???
#
# Copyright (C) 2023 Danny McClanahan <dmcC2@hypnicjerk.ai>
# SPDX-License-Identifier: Apache-2.0
#
# Licensed under the Apache License, Version 2.0 (see LICENSE).

from datetime import datetime
from typing import Iterable, Optional

from . import FileSource
from .destination import ZipFileWriter


class AutomaticModifiedTimeStrategy:
  Reproducible: 'AutomaticModifiedTimeStrategy'
  CurrentTime: 'AutomaticModifiedTimeStrategy'
  PreserveSourceTime: 'AutomaticModifiedTimeStrategy'

  def __int__(self) -> int: ...

  @classmethod
  def default(cls) -> 'AutomaticModifiedTimeStrategy': ...


class ZipDateTime:
  def __init__(self, year: int, month: int, day: int, hour: int, minute: int, second: int) -> None:
    ...

  @property
  def year(self) -> int: ...
  @property
  def month(self) -> int: ...
  @property
  def day(self) -> int: ...
  @property
  def hour(self) -> int: ...
  @property
  def minute(self) -> int: ...
  @property
  def second(self) -> int: ...

  @classmethod
  def from_datetime(cls, py_datetime: datetime) -> 'ZipDateTime': ...

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

  @classmethod
  def default(cls) -> 'ModifiedTimeBehavior': ...


class CompressionMethod:
  Stored: 'CompressionMethod'
  Deflated: 'CompressionMethod'

  def __int__(self) -> int: ...

  @classmethod
  def default(cls) -> 'CompressionMethod': ...


class CompressionOptions:
  def __init__(self, method: CompressionMethod, level: Optional[int]) -> None:
    ...

  @property
  def method(self) -> CompressionMethod: ...
  @property
  def level(self) -> Optional[int]: ...

  @classmethod
  def default(cls) -> 'CompressionOptions': ...


class ZipOutputOptions:
  def __init__(
    self,
    mtime_behavior: Optional[ModifiedTimeBehavior] = None,
    compression_options: Optional[CompressionOptions] = None,
  ) -> None:
    ...

  @property
  def mtime_behavior(self) -> ModifiedTimeBehavior: ...
  @property
  def compression_options(self) -> CompressionOptions: ...

  @classmethod
  def default(cls) -> 'ZipOutputOptions': ...


class EntryModifications:
  def __init__(
    self,
    silent_external_prefix: Optional[str] = None,
    own_prefix: Optional[str] = None,
  ) -> None:
    ...

  @property
  def silent_external_prefix(self) -> Optional[str]: ...
  @property
  def own_prefix(self) -> Optional[str]: ...

  @classmethod
  def default(cls) -> 'EntryModifications': ...


class Parallelism:
  Synchronous: 'Parallelism'
  ParallelMerge: 'Parallelism'

  def __int__(self) -> int: ...

  @classmethod
  def default(cls) -> 'Parallelism': ...


class MedusaZip:
  def __init__(
    self,
    input_files: Iterable[FileSource],
    zip_options: Optional[ZipOutputOptions] = None,
    modifications: Optional[EntryModifications] = None,
    parallelism: Optional[Parallelism] = None,
  ) -> None:
    ...

  async def zip(output_zip: ZipFileWriter) -> ZipFileWriter:
    ...

  def zip_sync(output_zip: ZipFileWriter) -> ZipFileWriter:
    ...
