# Description: ???
#
# Copyright (C) 2023 Danny McClanahan <dmcC2@hypnicjerk.ai>
# SPDX-License-Identifier: Apache-2.0
#
# Licensed under the Apache License, Version 2.0 (see LICENSE).

from pathlib import Path


class EntryName:
  def __init__(self, x: str, /) -> None: ...


class FileSource:
  def __init__(self, name: EntryName, source: Path) -> None: ...

  @property
  def name(self) -> EntryName: ...
  @property
  def source(self) -> Path: ...
