<!--
SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
-->

# raven-libc-compat

The OS-neutral BSD/GNU compatibility extras of ravenlibc (`donor-map/15` 3.B, ticket C65):
`vis`/`unvis`, `stringlist`, `humanize_number`/`expand_number`/`fmt_scaled`/`scan_scaled`,
`fparseln`, `getbsize`, `getcap`, `getttyent`, `getusershell`, `setmode`/`getmode`, `pidfile`,
`readpassphrase`, the BSD `uuid` API, `crypt()` (`$1$`, `$2a$`, `$2b$`, `$5$`, `$6$`),
`openpty`/`login_tty`/`forkpty`, `mntent`, `utmpx` stubs, `backtrace` and the `devname` stub.

`no_std` + `alloc`, class P (`#![forbid(unsafe_code)]`), deterministic: no clock, environment,
filesystem or randomness outside the seams in `src/seams.rs`. The C-ABI shell is
`crates/raven-libc/src/compat/**`; `exports.toml` is the export table (the `Symbol.map` entry is
deferred to C60).

Sources (licence read per file; substitutions are listed in the C65 report): NetBSD
`lib/libc/gen/{vis,unvis}.c` (BSD-3), FreeBSD `lib/libc/gen/stringlist.c` and
`lib/libutil/{humanize_number,expand_number,fparseln,pty,login_tty}.c` (BSD-2/BSD-3), OpenSSH's
`fmt_scaled.c` copy (BSD-3), OpenBSD `lib/libc/gen/{getbsize,getcap,getttyent,getusershell,
readpassphrase,setmode}.c` (BSD-3/ISC), NetBSD `lib/libc/uuid/*.c` (BSD-2), NetBSD
`lib/libcrypt/{bcrypt,blowfish}.c` (BSD-3) and musl `src/crypt/{crypt_md5,crypt_sha256,
crypt_sha512}.c` (MIT via `musl/COPYRIGHT`).

Tests: `cargo test -p raven-libc-compat` (unit tables, the 256-byte x 12-flag `vis` differential
against the C original, and the 200-spec x 16-mode x 4-umask `setmode` differential). The C35
oracle recipes are under `tests/libc/oracle/recipes/compat-*`.
