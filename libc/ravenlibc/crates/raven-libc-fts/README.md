<!--
SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
-->

# raven-libc-fts

`fts_open`/`fts_openat`/`fts_read`/`fts_children`/`fts_set`/`fts_close`, `ftw`, `nftw` and the
`DIR` iteration helpers (`opendir`, `fdopendir`, `readdir`, `readdir_r`, `closedir`, `rewinddir`,
`seekdir`, `telldir`, `dirfd`, `posix_getdents`, `scandir`, `alphasort`, `versionsort`) for
ravenlibc. Pure logic, `no_std` + `alloc`, class P (`#![forbid(unsafe_code)]`); the C-ABI shell
owns `DIR`, `FTS`, `FTSENT` and `struct dirent`.

Sources (donor-map 07 sections 3.5-3.6):

* `src/fts.rs` - COPIED from `freebsd:lib/libc/gen/fts.c`, BSD-3-Clause, pinned at
  `133344405f532bd54ff3629a77d4c4e062f01fdb`. Arena nodes, `fts_openat`, cycle detection,
  comparator sorting. Hardening: **H-FTS-1** `FTS_NOCHDIR` is forced (no `chdir`; `fts_accpath`
  is `fts_path`), **H-FTS-2** `O_NOFOLLOW` on every physical-walk `openat`, **H-FTS-3** the
  opened directory is `fstat`ed and its `st_dev`/`st_ino` re-checked, **H-FTS-4** an `EMFILE`
  releases the open node descriptors and retries the full path, **H-FTS-5** the path is capped at
  `PATH_MAX` and an over-long node is `FTS_ERR`/`ENAMETOOLONG`.
* `src/ftw.rs`, `src/nftw.rs` - COPIED from `freebsd:lib/libc/gen/{ftw,nftw}.c`, ISC (Todd C.
  Miller 2003/2004, DARPA paragraph kept). `FTW_CHDIR` is a no-op under forced `FTS_NOCHDIR`;
  `nftw` skips `FTS_DC`, `ftw` reports `ELOOP` (the donor behaviour).
* `src/dirent/*` - COPIED from `musl:src/dirent/*`, MIT via `musl/COPYRIGHT`, pinned at
  `7813e9119eb5fe681b6750b8a033a0490bca0941`. The record parser is ours (Linux `dirent64`,
  checked arithmetic; a malformed record is `EIO`, `RD-new`); `getdents` `ENOENT` is end of
  stream (A-dir-1); `scandir` uses a tolerant bottom-up merge sort (`RD-new`), `INT_MAX` ->
  `EOVERFLOW` and `try_reserve` -> `ENOMEM`.

## The OI-2 / RD-37 contract

**Every `d_off` of a `getdents` record is a valid `lseek` cookie for that directory.**
`telldir` returns the `d_off` of the last returned entry (initially 0) and `seekdir(off)` is
`lseek(fd, off, SEEK_SET)` followed by emptying the buffer; no per-`DIR` position list exists.
`PG-30` owns the server side. `raven_libc_fts::check_cookie_contract(&mut sys, dir)` (doc hidden)
verifies the contract on a backend and is the platform ticket's hook; the fallback if the
contract is refused is the FreeBSD `telldir.c` position list, not built here.

The `DirSys` seam is descriptor based (`openat`, `fstatat`, `fstat`, `getdents`, `lseek`,
`close`, plus the provided `fcntl_getfl` for the `O_PATH` check in `fdopendir`). It is a
different trait from `raven-libc-glob`'s path-based one on purpose.

The `testutil` feature exports the std-free fake tree (`src/fake.rs`, doc 07 4.5) used by unit
tests and available to the platform/shell lanes. Tests: `cargo test -p raven-libc-fts
--features testutil`. The FreeBSD `fts_*_test.c`, `opendir_test.c`, `scandir_test.c` and
`dir2_test.c` cases are ported by name from the pinned git objects (the sparse checkout does not
materialise them); the two pointer cases (`fts_get_clientptr`/`fts_get_stream`) stay in the C
shell, and `fdscandir`/`scandirat` are FreeBSD-only extensions not in doc 07 P04.
