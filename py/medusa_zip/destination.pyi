# Description: ???
#
# Copyright (C) 2023 Danny McClanahan <dmcC2@hypnicjerk.ai>
# SPDX-License-Identifier: Apache-2.0
#
# Licensed under the Apache License, Version 2.0 (see LICENSE).

from pathlib import Path


class ZipFileWriter:
  ...


class DestinationBehavior:
  AlwaysTruncate: 'DestinationBehavior'
  AppendOrFail: 'DestinationBehavior'
  OptimisticallyAppend: 'DestinationBehavior'
  AppendToNonZip: 'DestinationBehavior'

  def __int__(self) -> int: ...

  async def initialize(self, path: Path) -> ZipFileWriter:
    ...
