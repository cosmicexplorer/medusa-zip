# Description: ???
#
# Copyright (C) 2023 Danny McClanahan <dmcC2@hypnicjerk.ai>
# SPDX-License-Identifier: Apache-2.0
#
# Licensed under the Apache License, Version 2.0 (see LICENSE).

from pathlib import Path
from typing import Iterable, Optional, Union

from . import EntryName


class MergeGroup:
  def __init__(self, prefix: Optional[Union[str, EntryName]], sources: Iterable[Path]) -> None:
    ...
