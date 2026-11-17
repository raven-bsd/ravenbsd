<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# `filesystem.mmap-file` evidence - P5b

`test.sh` runs the `posix-fs` qtest. The fixture:

* opens a ramfs file through `file:` and calls `OP_MMAP`;
* maps the attached pager-backed `MemObject` shared and private;
* writes a shared page, calls `msync(MS_SYNC)`, and verifies the file contents;
* writes a private page and verifies the file did not change;
* unmaps both mappings;
* rejects a mapping whose offset is at or past EOF.

The UFS probe also maps `/ufs/mmap`, writes through the mapping, calls `msync`, and verifies
the UFS file, exercising `fsd-ufs` `pread`/`pwrite` through the same vfsd pager.
