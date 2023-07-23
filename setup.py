# Description: ???
#
# Copyright (C) 2023 Danny McClanahan <dmcC2@hypnicjerk.ai>
# SPDX-License-Identifier: Apache-2.0
#
# Licensed under the Apache License, Version 2.0 (see LICENSE).

from setuptools import setup
from setuptools_rust import Binding, RustExtension

# NB: we need to keep the entire cargo workspace underneath the setup.py and MANIFEST.in in order to
# generate a working sdist! That means putting this all at the workspace root.

setup(
    # Version/name/etc are managed by hand in pyproject.toml.
    rust_extensions=[
        RustExtension(
          "pymedusa_zip.pymedusa_zip",
          path="py/Cargo.toml",
          # FIXME: toggle this on only if the "asyncio" extra (in pyproject.toml) is provided!
          features=["asyncio", "sync"],
          binding=Binding.PyO3),
    ],
    zip_safe=False,
)
