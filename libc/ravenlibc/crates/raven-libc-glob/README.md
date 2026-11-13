<!--
SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
-->

# raven-libc-glob

`fnmatch`, `glob` and `wordexp` for ravenlibc; pure logic, `no_std` + `alloc`, class P
(`#![forbid(unsafe_code)]`), no global state, no `errno` and no `std` outside tests. The C-ABI
shell owns `glob_t`, `wordexp_t` and the exported names; this crate owns the engines.

Sources (donor-map 07 sections 3.2-3.4):

* `src/fnmatch.rs` - COPIED from `musl:src/regex/fnmatch.c`, MIT via `musl/COPYRIGHT`, pinned at
  `7813e9119eb5fe681b6750b8a033a0490bca0941`. The "Sea of Stars" algorithm, the FreeBSD flag
  *values* (A-glob-3), and `FNM_LEADING_DIR`; invalid bytes are unmatchable.
* `src/glob.rs` - COPIED from `freebsd:lib/libc/gen/glob.c`, BSD-3-Clause, pinned at
  `133344405f532bd54ff3629a77d4c4e062f01fdb`. Includes `GLOB_BRACE`, `GLOB_TILDE`, `GLOB_QUOTE`,
  `GLOB_NOMAGIC`, `GLOB_LIMIT`, the donor's non-recursive `match()` (A-glob-2) and H-GL-1 (the
  128-brace and `PATH_MAX` checks apply even without `GLOB_LIMIT`).
* `src/wordexp.rs` - COPIED from `musl:src/misc/wordexp.c`, MIT via `musl/COPYRIGHT`. The
  `WRDE_NOCMD` pre-scan and the `sh -c 'eval "set --;printf %s\\\\0 x $1"'` script; the fork/pipe
  code is replaced by the `ShellRun` trait.

Seams: `DirSys` is *path based* (the `glob_t` `ALTDIRFUNC` hooks); `GlobEnv` provides
`secure_getenv("HOME")`/`getlogin`/`getpwnam`/`getpwuid` so the crate can never call `getenv`;
`PatternLocale` (over `raven-libc-ctype`) provides `MB_CUR_MAX`, `mbtowc`, `wctype`, `iswctype`,
`towupper` and `towlower`. `raven-libc-fts` has a *different*, descriptor-based `DirSys`.

Known differences: `WRDE_UNDEF` is accepted but the shell expands an undefined variable to the
empty string (musl's behaviour; doc 07 7.3); `strcoll` sorting is byte order; `GLOB_PERIOD`,
`GLOB_STAR`, `GLOB_NO_DOTDIRS`, `GLOB_TILDE_CHECK`, `GLOB_ONLYDIR` and `GLOB_KEEPSTAT` are absent
(OI-3/RD-38).

Tests: `cargo test -p raven-libc-glob` (unit, the 184 OpenBSD `fnm_test.in` vectors and the 138
FreeBSD `fnmatch_testcases.h` vectors as generated data, the NetBSD `t_fnmatch.c` cases, the
path-based fake tree, `/bin/sh` integration and a real-temp-directory backend). The C35 oracle
families for glob are not wired: the harness has no filesystem-tree case type and adding one
would change C35 code; the crate's integration tests and the host backend are the deliverable
(see the C46 report).
