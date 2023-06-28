medusa-zip
==========

A library/binary for parallel zip creation. See discussion in https://github.com/pantsbuild/pex/issues/2158.

# TODO
*Iterations:*
1. [ ] Write out all paths into their own zip files in a temp dir, then use `raw_copy_file()` to copy over their contents into the final zip.
2. [ ] Hack the `zip` library (changes may not be necessary?) to enable creation of intermediate `ZipFile` objects in memory.
    - See `raw_copy_file_rename()` and `finish_file()` methods.
    - Should be able to use `io::Cursor::new(Vec::new())` to create in-memory zip streams.

## Optimizations
1. [ ] Use `mmap` or something else to page to disk if the intermediate `ZipFile` objects get too large.
2. [ ] See whether sections of a zip file spanning multiple file entries can be copied over with memcpy or something else with low overhead. If so, try splitting up the sorted list of file paths into chunks, creating an intermediate zip for each chunk, then copying over the contents of each chunked zip with that bulk copy method.
3. [ ] See whether zip files can be created without sorting the entries somehow.
    - It seems like `zipimport` will convert module paths to file names and scan the zip directly, so as long as it unzips properly with `zipfile`, we should be good (?)!

## Compatibility
We mainly need compatibility with [`zipfile`](https://docs.python.org/3/library/zipfile.html) and [`zipimport`](https://docs.python.org/3/library/zipimport.html) (see https://github.com/pantsbuild/pex/issues/2158#issuecomment-1599348047). Also see [the `zipimport` PEP](https://peps.python.org/pep-0273/).

# License
[Apache v2](./LICENSE).
