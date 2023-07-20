medusa-zip
==========

High-performance parallelized implementations of common zip file operations.

*See discussion in https://github.com/pantsbuild/pex/issues/2158.*

# Crimes

This crate adds some hacks to the widely-used `zip` crate (see the diff at https://github.com/zip-rs/zip/compare/master...cosmicexplorer:zip:merge-entries?expand=1). When the `merge` feature is provided to this fork of `zip`, two crimes are unveiled:
1. [`merge_archive()`](https://github.com/cosmicexplorer/zip/blob/94c21b77b21db4133a210f335e0671f4ea85d6a0/src/write.rs#L483-L508):
    - This will copy over the contents of another zip file into the current one without deserializing any data.
    - **This enables parallelization of arbitrary zip commands, as multiple zip files can be created in parallel and then merged afterwards.**
2. [`finish_into_readable()`](https://github.com/cosmicexplorer/zip/blob/94c21b77b21db4133a210f335e0671f4ea85d6a0/src/write.rs#L327-L340):
    - Creating a writable `ZipWriter` and then converting it into a readable `ZipArchive` is a very common operation when merging zip files.
    - This likely has zero performance benefit, but it is a good example of the types of investigations you can do with the zip format, especially against the well-written `zip` crate.

## Compatibility
We mainly need compatibility with [`zipfile`](https://docs.python.org/3/library/zipfile.html) and [`zipimport`](https://docs.python.org/3/library/zipimport.html) (see https://github.com/pantsbuild/pex/issues/2158#issuecomment-1599348047). Also see [the `zipimport` PEP](https://peps.python.org/pep-0273/). **I currently believe that this program's output will work perfectly against `zipfile` and `zipimport`.**

# License
[Apache v2](./LICENSE).
