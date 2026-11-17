<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# P1d - POSIX libc gap audit and slice plan

Status: audit for the P1c split. No libc, header, test or generated POSIX file was edited.

## Sources and commands

* Header/interface result rows: `docs/posix/CONFORMANCE.md` (last gate, 2026-11-17T04:23:51Z,
  `1145 pass, 655 fail, 10 skip`; header `40/46`, interface `925/497`).
* Header and interface metadata: `docs/posix/GAPS.md` (`GAPS.md` is generated from `profile.toml`).
* Failure-class extraction:

```sh
awk -F'|' '/^\| `header\./ && /\| fail \|/ { ... }' docs/posix/CONFORMANCE.md
awk -F'|' '/^\| `interface\./ && /\| fail \|/ { ... }' docs/posix/CONFORMANCE.md
```

* Per-header join (GAPS interface table -> CONFORMANCE fail rows) was used to produce the table
  below. The join is an analysis script, not a product tool; slice ownership paths are **PROPOSED**
  from symbol probes in `libc/ravenlibc/crates/**`.
* Targeted probes:

```sh
cc -I ./libc/ravenlibc/include -I ./libc/ravenlibc/arch/x86_64/include -fsyntax-only \
   -std=c17 -D_POSIX_C_SOURCE=202405L -D_XOPEN_SOURCE=800 -Wno-unused-value \
   ./build/posix-gate/x86_64/interface_141.c   # cfmakeraw
cc ... ./build/posix-gate/x86_64/interface_148.c  # chroot
```

Both fail with `‘<name>’ undeclared here`, so the `declaration did not compile` class is a header
prototype/type gap, not a missing `libc.a` symbol.

## Where P1 stands

| class | rows |
|---|---:|
| failing `header.*` rows | **46** |
| failing `interface.*` rows | **497** |
| - interface: profile status missing (no symbol) | **416** |
| - interface: declaration did not compile | **81** |
| P1b-owned header rows (stdio/time/wchar) | **4** |
| P1b-owned interface rows | **67** (65 missing + 2 compile) |
| P1c header rows | **42** |
| P1c interface rows | **430** (351 missing + 79 compile) |

Every failing `header.*` row is `profile status partial`: the header exists, but at least one
required declaration is absent. `ndbm.h` and `trace.h` are header-only failures (no failing
interface row).

## Failure classes

| class | meaning | action |
|---|---|---|
| `profile status missing` | the symbol is absent from the frozen `libc.a` symbol set / no implementation | implement or explicitly waive the interface |
| `declaration did not compile` | the header exists but the probe cannot take the symbol's address | fix the prototype, typedef, macro or guard in that header |
| `profile status partial` (header) | at least one declaration in the header is absent | close the header after its interfaces compile |
| `profile status missing` (utility) | out of P1d scope; see `P1d-D1` | refresh the utility seed/profile in the owning lane |

## Failing rows per header (exact counts)

`missing` is `profile status missing`; `decl` is `declaration did not compile`; `header` marks a
failing `header.*` row. A `-` in the slice column means P1b-owned and excluded.

| header | missing | decl | total | header | slice |
|---|---:|---:|---:|---|---|
| `math.h` | 60 | 21 | 81 | yes | `P1c-math` |
| `complex.h` | 86 | 0 | 86 | yes | `P1c-math` |
| `fenv.h` | 15 | 0 | 15 | yes | `P1c-math` |
| `unistd.h` | 35 | 16 | 51 | yes | `P1c-unistd-io` |
| `fcntl.h` | 10 | 2 | 12 | yes | `P1c-unistd-io` |
| `sys/mman.h` | 11 | 0 | 11 | yes | `P1c-unistd-io` |
| `sys/stat.h` | 7 | 4 | 11 | yes | `P1c-unistd-io` |
| `sys/uio.h` | 4 | 0 | 4 | yes | `P1c-unistd-io` |
| `sys/resource.h` | 2 | 1 | 3 | yes | `P1c-unistd-io` |
| `sys/select.h` | 2 | 0 | 2 | yes | `P1c-unistd-io` |
| `sys/wait.h` | 0 | 2 | 2 | yes | `P1c-unistd-io` |
| `sys/statvfs.h` | 1 | 0 | 1 | yes | `P1c-unistd-io` |
| `dirent.h` | 1 | 1 | 2 | yes | `P1c-unistd-io` |
| `poll.h` | 1 | 0 | 1 | yes | `P1c-unistd-io` |
| `ulimit.h` | 1 | 0 | 1 | yes | `P1c-unistd-io` |
| `utime.h` | 1 | 0 | 1 | yes | `P1c-unistd-io` |
| `termios.h` | 12 | 1 | 13 | yes | `P1c-ipc-tty` |
| `aio.h` | 16 | 0 | 16 | yes | `P1c-ipc-tty` |
| `mqueue.h` | 10 | 0 | 10 | yes | `P1c-ipc-tty` |
| `stropts.h` | 7 | 0 | 7 | yes | `P1c-ipc-tty` |
| `sys/shm.h` | 5 | 0 | 5 | yes | `P1c-ipc-tty` |
| `sys/msg.h` | 4 | 0 | 4 | yes | `P1c-ipc-tty` |
| `sys/sem.h` | 4 | 0 | 4 | yes | `P1c-ipc-tty` |
| `sys/ipc.h` | 1 | 0 | 1 | yes | `P1c-ipc-tty` |
| `nl_types.h` | 5 | 0 | 5 | yes | `P1c-locale-names` |
| `syslog.h` | 5 | 0 | 5 | yes | `P1c-locale-names` |
| `search.h` | 5 | 0 | 5 | yes | `P1c-locale-names` |
| `uchar.h` | 6 | 0 | 6 | yes | `P1c-locale-names` |
| `netdb.h` | 0 | 6 | 6 | yes | `P1c-locale-names` |
| `net/if.h` | 4 | 0 | 4 | yes | `P1c-locale-names` |
| `grp.h` | 1 | 4 | 5 | yes | `P1c-locale-names` |
| `iconv.h` | 3 | 0 | 3 | yes | `P1c-locale-names` |
| `libgen.h` | 2 | 0 | 2 | yes | `P1c-locale-names` |
| `monetary.h` | 2 | 0 | 2 | yes | `P1c-locale-names` |
| `pwd.h` | 1 | 1 | 2 | yes | `P1c-locale-names` |
| `regex.h` | 2 | 0 | 2 | yes | `P1c-locale-names` |
| `ftw.h` | 2 | 0 | 2 | yes | `P1c-locale-names` |
| `fmtmsg.h` | 1 | 0 | 1 | yes | `P1c-locale-names` |
| `inttypes.h` | 1 | 0 | 1 | yes | `P1c-locale-names` |
| `utmpx.h` | 1 | 1 | 2 | yes | `P1c-locale-names` |
| `stdlib.h` | 6 | 3 | 9 | yes | `P1c-core-header` |
| `signal.h` | 2 | 3 | 5 | yes | `P1c-core-header` |
| `stdarg.h` | 4 | 0 | 4 | yes | `P1c-core-header` |
| `string.h` | 0 | 4 | 4 | no | `P1c-core-header` |
| `strings.h` | 0 | 2 | 2 | no | `P1c-core-header` |
| `sched.h` | 0 | 3 | 3 | no | `P1c-core-header` |
| `dlfcn.h` | 0 | 2 | 2 | yes | `P1c-core-header` |
| `ctype.h` | 0 | 1 | 1 | no | `P1c-core-header` |
| `assert.h` | 1 | 0 | 1 | no | `P1c-core-header` |
| `stddef.h` | 1 | 0 | 1 | no | `P1c-core-header` |
| `sys/socket.h` | 0 | 1 | 1 | no | `P1c-core-header` |
| `ndbm.h` | 0 | 0 | 0 | yes | `P1c-core-header` |
| `trace.h` | 0 | 0 | 0 | yes | `P1c-core-header` |
| `stdio.h` | 11 | 0 | 11 | yes | P1b-owned |
| `time.h` | 7 | 1 | 8 | yes | P1b-owned |
| `sys/time.h` | 6 | 1 | 7 | yes | P1b-owned |
| `wchar.h` | 41 | 0 | 41 | yes | P1b-owned |

The table has 54 interface headers plus the two header-only rows; the 46 header failures are the
`header=yes` rows. P1b owns `stdio.h`, `time.h`, `sys/time.h` and `wchar.h` (4 header rows, 67
interface rows) and is excluded from every P1c slice.

## P1b-owned groups (exclude from P1c)

| group | actual rows | note |
|---|---:|---|
| `stdio.h` | 11 interface, 1 header | P1b stdio slice |
| `time.h` + `sys/time.h` | 15 interface, 2 headers | P1b time slice |
| `wchar.h` | 41 interface, 1 header | P1b wchar slice |

These are live work in P1b; P1c must not duplicate them.

## Proposed P1c slices

Slices are **PROPOSED** and disjoint at the file/module-glob level. Each slice closes its header
rows only when the listed interfaces compile and link. Acceptance for every slice is:

```
cargo nextest run -p <slice crates>
cargo xtask posix-check --profile posix --arch x86_64 --host-only
make check-fast
```

Run the full `posix-check` only after the wave's P1b slice has landed, so the matrix does not
race Bob's REL6 gate. `--host-only` skips qtests but still compiles every header/interface probe.

| slice | header rows | interface missing | interface decl | interface total | primary owning path (PROPOSED) |
|---|---:|---:|---:|---:|---|
| `P1c-math` | 3 | 161 | 21 | 182 | `libc/ravenlibc/crates/raven-libc-m/**` |
| `P1c-unistd-io` | 12 | 76 | 26 | 102 | `libc/ravenlibc/crates/{raven-libc-platform,raven-libc-proc,raven-libc-sys}/**` + `raven-libc/src/{unistd,compat}/**` |
| `P1c-ipc-tty` | 8 | 59 | 1 | 60 | new `libc/ravenlibc/crates/raven-libc-ipc/**` + `raven-libc/src/compat/**` |
| `P1c-locale-names` | 15 | 41 | 12 | 53 | `{raven-libc-locale,raven-libc-iconv,raven-libc-nss,raven-libc-net,raven-libc-regex,raven-libc-pwd,raven-libc-fts}/**` + `raven-libc/src/{locale,netdb,syslog}/**` |
| `P1c-core-header` | 4 | 14 | 19 | 33 | `{raven-libc-str,raven-libc-stdlib,raven-libc-signal}/**` + `raven-libc/src/{assert,stdlib,string,signal,sched,dl}/**` |

Totals: 42 header rows and 430 interface rows, matching the P1c remainder.

### P1c-math

* Headers: `math.h`, `complex.h`, `fenv.h`; 161 missing, 21 compile failures.
* Owns: `libc/ravenlibc/crates/raven-libc-m/**`, `libc/ravenlibc/include/{math,complex,fenv}.h`,
  `tests/libc/posix/math/**`.
* Notes: `math.h` has the compile class (21), so its prototypes/macros need the first pass;
  `complex.h` is the largest missing-symbol group and is self-contained in `complex.rs`.
* Acceptance: `cargo nextest run -p raven-libc-m && sh tests/libc/posix/math/check-math.sh`.

### P1c-unistd-io

* Headers: `unistd.h`, `fcntl.h`, `poll.h`, `dirent.h`, `utime.h`, `ulimit.h`,
  `sys/{mman,stat,statvfs,uio,select,resource,wait}.h`; 76 missing, 26 compile.
* Owns: `libc/ravenlibc/crates/{raven-libc-platform,raven-libc-proc,raven-libc-sys}/**`,
  `libc/ravenlibc/crates/raven-libc/src/{unistd,compat}/**`, the listed headers,
  `tests/libc/posix/headers/**`.
* Notes: the compile class concentrates in `unistd.h` (16) and `sys/stat.h` (4); the missing class
  includes `openat`, `fpathconf`, `pathconf`, `getrlimit`, `waitid` and the mmap family.
* Acceptance: `cargo nextest run -p raven-libc-platform -p raven-libc-proc -p raven-libc-sys`.

### P1c-ipc-tty

* Headers: `aio.h`, `mqueue.h`, `stropts.h`, `termios.h`, `sys/{ipc,msg,sem,shm}.h`; 59 missing,
  1 compile (`cfmakeraw`).
* Owns: new `libc/ravenlibc/crates/raven-libc-ipc/**`, `libc/ravenlibc/crates/raven-libc/src/compat/**`,
  the listed headers, `tests/libc/posix/headers/**`.
* Notes: this is the only slice that needs a new crate; the headers exist but have almost no
  implementation. `cfmakeraw` is the probe example for the compile class.
* Acceptance: `cargo nextest run -p raven-libc-ipc -p raven-libc`.

### P1c-locale-names

* Headers: `nl_types.h`, `syslog.h`, `search.h`, `uchar.h`, `netdb.h`, `net/if.h`, `grp.h`,
  `iconv.h`, `libgen.h`, `monetary.h`, `pwd.h`, `regex.h`, `ftw.h`, `fmtmsg.h`, `inttypes.h`,
  `utmpx.h`; 41 missing, 12 compile.
* Owns: `libc/ravenlibc/crates/{raven-libc-locale,raven-libc-iconv,raven-libc-nss,raven-libc-net,raven-libc-regex,raven-libc-pwd,raven-libc-fts}/**`,
  `libc/ravenlibc/crates/raven-libc/src/{locale,netdb,syslog}/**`, the listed headers,
  `tests/libc/posix/headers/**`.
* Notes: `netdb.h`, `grp.h`, `pwd.h`, `utmpx.h` are the compile-class carriers; the missing class
  covers `catopen`/`catgets`, `strfmon`, `getdate`, `hsearch`, glob/regcomp and the NSS lookups.
* Acceptance: `cargo nextest run -p raven-libc-locale -p raven-libc-iconv -p raven-libc-nss -p raven-libc-net -p raven-libc-regex -p raven-libc-pwd -p raven-libc-fts`.

### P1c-core-header

* Header rows: `stdlib.h`, `signal.h`, `ndbm.h`, `trace.h` (the four failing header rows); the
  interfaces also touch `dlfcn.h`, `string.h`, `strings.h`, `sched.h`, `ctype.h`, `assert.h`,
  `stdarg.h`, `stddef.h`, `sys/socket.h`.
* Counts: 14 missing, 19 compile (the highest compile share).
* Owns: `libc/ravenlibc/crates/{raven-libc-str,raven-libc-stdlib,raven-libc-signal}/**`,
  `libc/ravenlibc/crates/raven-libc/src/{assert,stdlib,string,signal,sched,dl}/**`, the listed
  headers, `tests/libc/posix/headers/**`.
* Notes: the compile class here is mostly prototypes/macros (`string.h`, `signal.h`, `sched.h`,
  `dlfcn.h`); `ndbm.h`/`trace.h` are header-only and can be explicitly waived or stubbed.
* Acceptance: `cargo nextest run -p raven-libc-str -p raven-libc-stdlib -p raven-libc-signal`.

## Defects and observations

| id | finding | impact |
|---|---|---|
| `P1d-D1` | `cargo xtask posix-gaps --check` and `cargo xtask posix-gaps` exit 1 with 20 `utility ... exists but is marked missing` lines (`at batch nm strip uucp uustat uux talk cflow ctags cxref gencat iconv lex locale mail mailx yacc asa cal`). This is profile/seed drift from the utility lanes, not P1 header/interface data. | the two P1d acceptance commands do not reach their normal success line until the owning lane refreshes the utility seed/profile; the audit uses the last `CONFORMANCE.md` plus `GAPS.md`, which remain valid |
| `P1d-D2` | `ndbm.h` and `trace.h` are `partial` with no failing interface row | each needs either a minimal stub or a written waiver in its P1c slice |
| `P1d-D3` | the `declaration did not compile` class is 81 rows (16% of interface failures), not a link-symbol problem | header/prototype work, not just adding symbols to `libc.a` |

No `make check-fast` gate is claimed for P1d: the deliverable is a document and the two
`posix-gaps` commands are the ticket's acceptance. `cargo xtask posix-gaps --check` exit code 1 is
recorded above with its exact cause.

## Recommended order

1. `P1c-math` (self-contained, largest single group, 182 rows).
2. `P1c-unistd-io` (largest compile class after math; unblocks most C programs).
3. `P1c-core-header` (smallest, closes the compile-heavy base headers).
4. `P1c-locale-names` (mostly missing symbols; depends on P1c-core-header's string/ctype work).
5. `P1c-ipc-tty` (needs a new crate; can run in parallel because its Owns are disjoint).
