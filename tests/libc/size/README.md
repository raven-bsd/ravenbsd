# ravenlibc size fixtures (`tests/libc/size/`)

<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

Sources, rule tables, the frozen budget and the baseline result of the size gate
(`donor-map/16` Part A, ticket C69). The tool is `tools/libc-size` (`cargo xtask libc-size`).

## Layout

| Path | What |
|---|---|
| `probes/rust/pN.rs`, `probes/c/pN.c` | P0-P9, identical behaviour in both languages |
| `rules.toml` | the probe rules of `16` 5.B |
| `size-allowlist.toml` | reviewer-owned exceptions (R-1, R-5, R-10, R-30) |
| `hooks.toml` | the 17 rows of `16` 3.A.4.3 |
| `hot.toml` | the hot symbols of `16` 3.A.3 |
| `malloc-roots.toml` | `BP-2` roots and reviewed exceptions |
| `BUDGETS.toml` | **generated** by `libc-size freeze`; measured + 3 percent |
| `results/libc-size.json`, `.md` | the committed first-measurement baseline |
| `qtest/*.toml` | B1-B3 seeds; C73 copies them to `tests/qtest/cases/` |

## Probes

`P0` floor, `P1` write, `P2` `%d`, `P3` `%f`, `P4` pthread, `P5` getaddrinfo, `P6` dlopen,
`P7` strftime, `P8` qsort/bsearch/strtol/regex, `P9` the `-u`-everything full link. `P2m` is
listed `n/a`: no `--printf=m` mechanism exists in this wave.

The probe build is the protocol of `C69` 4.4: `crt1.o`, `libc.a`, `--gc-sections`,
`relocation-model=static`, `-no-pie`, `opt-level=z|s`, then `llvm-strip --strip-all`. The C
twin uses freestanding clang flags against the sysroot headers; clang 14 spells the target
flag `-target`, so the crate passes that form.

## Budgets and known violations

`BUDGETS.toml` is never hand-edited: `libc-size freeze --results build/size/libc-size.json`
writes it. A budget is `ceil(measured * 103 / 100)` (`R-24`, headroom exactly 3 percent). The
first measurement's `[[known-violation]]` rows keep `check` reporting `KNOWN`; an invariant is
not weakened by a frozen row, and the reviewer removes the row after a fix.

While `C62` has not landed, every dynamic and `libc.so.1` row is
`skipped: libc.so.1 absent (C62 not landed)` with the exact path `build/<arch>/libc.so.1`. No
dynamic number in this directory is measured or invented.

## B1-B3 seeds

`qtest/libc-start-syscalls.toml` (B1), `qtest/libc-start-cycles.toml` (B2) and
`qtest/libc-start-env.toml` (B3) are in the format of `tests/qtest/cases/boot-uname.toml`. They
need `PG-1`, `PG-49` and `PG-50`; the commands are commented `# needs PG-...` in each file and
`C73` copies the files into `tests/qtest/cases/`.
