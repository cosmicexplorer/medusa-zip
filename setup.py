# Description: ???
#
# Copyright (C) 2023 Danny McClanahan <dmcC2@hypnicjerk.ai>
# SPDX-License-Identifier: Apache-2.0
#
# Licensed under the Apache License, Version 2.0 (see LICENSE).

from setuptools import setup
from setuptools_rust import RustBin

setup(
    # Version/name/etc are managed by hand in pyproject.toml.
    rust_extensions=[RustBin("medusa-zip")],
)
