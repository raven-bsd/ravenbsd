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
| `reconcile.py` | bounded before/after `libc-size` reconciliation helper (C74bw) |
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

## Reconciliation helper

`reconcile.py` reads two `libc-size` JSON runs, matches the static Rust rows by
`(arch, linkage, set, lang, probe)`, and prints the C74z-S12 legitimate-growth rows with the
proposed `ceil(current * 103 / 100)` budgets. Rows it cannot classify (P2/P5/P7 and riscv64 P3)
are listed with `HOLD` and never auto-raised. It does not read or write `BUDGETS.toml`.

```sh
python3 tests/libc/size/reconcile.py --before old.json --after build/size/libc-size.json
python3 tests/libc/size/reconcile.py --self-test
```

The helper bounds each input to 8 MiB and 10,000 rows. `--self-test` checks the empty diff, a
P0 shrink (+3% budget diff), the P2 hold, the P3 arch split and missing/added rows.

## C74dl residual audit

C74dl audited the 28-row 3.1 residual on `origin/master` `94d35d15` (C74cx was not present):
`cargo xtask libc-size --check` reports **46 pass / 28 fail / 69 known / 105 skipped / 0
budget-only**. The exact matrix is in `TRIAGE.md`; the short result is:

* **17 budget/layout rows, 0 shrink-first.** The B overruns are `.relro_padding` (B includes all
  NOBITS; `.bss` is 11.4-11.6k everywhere). The P2/P3 S/F rows are the real `vformat_dyn`/
  `dtoa` path after S14/S13 and the P7 S/F rows are the real `tzload`/`tzparse` code after S6.
  Their budgets are stale (x86_64 P2 even passes on a pre-S14 budget), so the reviewer
  re-freezes them at measured + 3 percent.
* **1 rule/probe bug: `P7-R-14`.** The `raven_libc_iconv` codegen unit is pulled only for
  `<alloc::raw_vec::RawVecInner>::deallocate` and drop glue (plus 0x14 `.eh_frame`); there is no
  iconv conversion symbol. The rule matches the member name, so it false-positives. Fix the
  matcher (symbols, or exclude allocator-only instantiations), do not weaken the iconv rule.
* **10 stale invariant ceilings.** The `INV-S-P2-P0` 5,120 and `INV-S-P3-P2` 10,240 limits are
  hard-coded `PROPOSED` constants, below the real printf/dtoa deltas on every arch. The x86_64
  P2-P0 rows are already `KNOWN`; the reviewer raises the ceilings or adds the equivalent
  known-violation rows.

C74dl applies no budget, rule or source change; **3.1 remains NOT MET** at 46/28.

## B1-B3 seeds

`qtest/libc-start-syscalls.toml` (B1), `qtest/libc-start-cycles.toml` (B2) and
`qtest/libc-start-env.toml` (B3) are in the format of `tests/qtest/cases/boot-uname.toml`. They
need `PG-1`, `PG-49` and `PG-50`; the commands are commented `# needs PG-...` in each file and
`C73` copies the files into `tests/qtest/cases/`.
