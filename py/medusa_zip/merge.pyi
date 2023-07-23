# Description: ???
#
# Copyright (C) 2023 Danny McClanahan <dmcC2@hypnicjerk.ai>
# SPDX-License-Identifier: Apache-2.0
#
# Licensed under the Apache License, Version 2.0 (see LICENSE).

from pathlib import Path
from typing import Iterable, Optional, Union

from . import EntryName
from .destination import ZipFileWriter
from .zip import ModifiedTimeBehavior


class MergeGroup:
  def __init__(self, prefix: Optional[Union[str, EntryName]], sources: Iterable[Path]) -> None:
    ...

  @property
  def prefix(self) -> Optional[EntryName]: ...
  @property
  def sources(self) -> List[Path]: ...


class MedusaMerge:
  def __init__(self, groups: Iterable[MergeGroup]) -> None:
    ...

  @property
  def groups(self) -> List[MergeGroup]: ...

  async def merge(
    self,
    mtime_behavior: ModifiedTimeBehavior,
    output_zip: ZipFileWriter,
  ) -> ZipFileWriter:
    ...

  def merge_sync(
    self,
    mtime_behavior: ModifiedTimeBehavior,
    output_zip: ZipFileWriter,
  ) -> ZipFileWriter:
    ...
