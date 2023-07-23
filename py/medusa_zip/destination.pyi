# Description: ???
#
# Copyright (C) 2023 Danny McClanahan <dmcC2@hypnicjerk.ai>
# SPDX-License-Identifier: Apache-2.0
#
# Licensed under the Apache License, Version 2.0 (see LICENSE).

from pathlib import Path
from typing import Any


class ZipFileWriter:
  @property
  def output_path(self) -> Path: ...

  async def finish(self) -> Path: ...

  def finish_sync(self) -> Path: ...

  def __enter__(self) -> 'ZipFileWriter': ...

  def __exit__(self, exc_type: Any, exc_val: Any, traceback: Any) -> bool: ...

  async def __aenter__(self) -> 'ZipFileWriter': ...

  async def __aexit__(self, exc_type: Any, exc_val: Any, traceback: Any) -> bool: ...


class DestinationBehavior:
  AlwaysTruncate: 'DestinationBehavior'
  AppendOrFail: 'DestinationBehavior'
  OptimisticallyAppend: 'DestinationBehavior'
  AppendToNonZip: 'DestinationBehavior'

  def __int__(self) -> int: ...

  @classmethod
  def default(cls) -> 'DestinationBehavior': ...

  async def initialize(self, path: Path) -> ZipFileWriter:
    ...

  def initialize_sync(self, path: Path) -> ZipFileWriter:
    ...
