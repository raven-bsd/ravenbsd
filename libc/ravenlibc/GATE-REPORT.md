<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# ravenlibc libc gate report

Evaluated by `libc-gate` on 2026-11-17 from the C74d tree (origin/master `bd1d1de1`). C74f and
C74g refreshed the `libc-dynamic` row and the section 4 dynamic paragraph; the table verdict is
otherwise unchanged. Every status below has an evidence file under `build/libc-gate/`; the table
is `cargo run -q -p libc-gate -- table`. C74d refreshed the non-P1 rows (audit, G1, compat, Miri,
world, size, soak, mandoc) with their exact current commands and results; the remaining NOT MET
rows are P1b/P1c, the dynamic runtime, or unavailable host tooling, and are listed with their
owning lane.

## 1. Gate table

| Row | Check | Status | Evidence | Note |
|---|---|---|---|---|
| 1 correctness | 1.1 symbol coverage (C1) | MET | build/libc-gate/1.1.txt | exit 0 |
| 1 correctness | 1.1.pack packed symbol list (C1) | MET | build/libc-gate/1.1.pack.txt | exit 0 |
| 1 correctness | 1.1.audit target-case symbol audit (C64j) | NOT MET | build/libc-gate/1.1.audit.txt | exit 1: expected 0 |
| 1 correctness | 1.2 donor suites G1 (C2-C6) | NOT MET | build/libc-gate/1.2.txt | exit 3: expected 0 |
| 1 correctness | 1.3 LTP and pjdfstest (C7, C8) | INFO | build/libc-gate/1.3.txt | owner-run LTP/pjdfstest results not supplied; INFO per C71/3.B.1.4 |
| 1 correctness | 1.4 differential tests (C9) | NOT MET | build/libc-gate/1.4.txt | 27 oracle families |
| 1 correctness | 1.5 host unit tests | MET | build/libc-gate/1.5.txt | 31 host crates |
| 1 correctness | 1.6 Miri (C11) | BLOCKED | build/libc-gate/1.6.txt | preflight command failed |
| 1 correctness | 1.7 lints (C16) | MET | build/libc-gate/1.7.txt | exit 0 |
| 2 portability | 2.1 world check | BLOCKED | build/libc-gate/2.1.txt | missing tools/libc-world/Cargo.toml; C72 outputs are not in origin/master and H-11 tarballs are absent |
| 2 portability | 2.2 world build gate (C13) | BLOCKED | build/libc-gate/2.2.txt | missing tools/libc-world/Cargo.toml; C72 outputs are not in origin/master and H-11 tarballs are absent |
| 3 size | 3.1 size budgets (C14) | NOT MET | build/libc-gate/3.1.txt | exit 1: expected 0 |
| 3 size | 3.2 boot path (C15) | INFO | build/libc-gate/3.2.txt | PG-60 absent; 0-1 syscalls and <=250 instructions are not evaluable |
| 4 hardening | 4.1 fuzz replay | MET | build/libc-gate/4.1.txt | exit 0 |
| 4 hardening | 4.2 fuzz soak (C10) | NOT MET | build/libc-gate/4.2.txt | inet_text has no soak duration; inet_text has 1 crash(es); inet_text has no executions |
| 4 hardening | 4.3 hardened malloc default | MET | build/libc-gate/4.3.txt | hardened backend selected; tiny not in libc.so.1 |
| 4 hardening | 4.4 ENTROPY_WEAK status (H-5) | INFO | build/libc-gate/4.4.txt | ENTROPY_WEAK CLEAR; PG-52 release blocker, not a libc-gate blocker |
| 5 on-target | 5.1 on-target libc cases (C12) | NOT MET | build/libc-gate/5.1.txt | 7 qtest cases |
| 5 on-target | 5.2.core-servers core-servers regression | NOT MET | build/libc-gate/5.2.core-servers.txt | exit 1: expected 0 |
| 5 on-target | 5.2.file-tools file-tools regression | NOT MET | build/libc-gate/5.2.file-tools.txt | exit 1: expected 0 |
| 5 on-target | 5.3 second architecture | INFO | build/libc-gate/5.3.txt | aarch64: not delivered (C73 open item); owner decides whether G1 closes on x86_64 alone |
| 6 documentation | 6.1 page coverage | MET | build/libc-gate/6.1.txt | exit 0 |
| 6 documentation | 6.2 render lint | BLOCKED | build/libc-gate/6.2.txt | preflight command failed |
| 6 documentation | 6.3 existing docs gate | MET | build/libc-gate/6.3.txt | exit 0 |
| 6 documentation | 6.4 libc(7) | MET | build/libc-gate/6.4.txt | docs/man7/libc.7 exists |

Verdict: NOT MET

## 2. Verdict

**libc gate: NOT MET.**

Rows 1 (correctness), 3 (size), 4 (hardening) and 5 (on-target) are NOT MET. Rows 2
(portability) and 6 (documentation) are BLOCKED. MET checks: symbol coverage and the packed
symbol list, the 31 host unit-test crates, fuzz replay, the hardened allocator default, page
coverage, the existing docs gate and `libc(7)`. The gate cannot close.

C57c and C76 are merged, so the old `libc-threads` list-corruption blocker is gone. The thread
case now advances through `mutex/cond/once/tsd/tls/rwlock/sem/barrier` and times out before
`detach` returns. The remaining on-target gaps are unchanged in kind and are listed in section 4.

## 3. Rows changed by this refresh

* 1.1.audit: still NOT MET. `cargo xtask libc-audit` reports 66 required, 66 present, 0 missing;
  the `tests/libc/symbol-audit/expected.tsv` snapshot is stale (13 C64i-era MISSING rows are now
  PRESENT in the generated matrix). The snapshot is outside C74d's Owns (C64j/C64l follow-up).
* 1.2 G1: `raven-atf-run gate --g1` exits 3: C2-C5 are 0 % (not run) and C6 is unvendored; 0 of
  4 measured rows (C71a/C71).
* 1.4: 23 of 27 oracle families pass; `compat-crypt`, `compat-humanize`, `compat-setmode` and
  `compat-vis` fail because `raven-libc-oracle` has no `oracle-<family>` feature (C35/C71).
* 1.6 Miri: UNAVAILABLE. The nightly Miri preflight reports that `cargo-miri` is not installed
  for the nightly toolchain; the pinned stable channel has no Miri.
* 1.7: MET, exit 0; `libc-lint` reports 0 violations (the C74b `sem_trywait.rs` fix).
* 2.1/2.2 world: UNAVAILABLE. `cargo run -p libc-world` fails because the `libc-world` package
  and `tools/libc-world/Cargo.toml` are absent; C72 outputs and H-11 tarballs are not in master.
* 3.1 size: NOT MET. `cargo xtask libc-size --check` exits 1 with `6 pass, 63 fail, 68 known,
  102 skipped, 0 budget-only`; budgets were not lowered.
* 4.2 soak: NOT MET. `tests/libc/fuzz/SOAK.md` records `inet_text` crashing before one hour; the
  other 13 targets completed 3600 s with 0 crashes.
* 5.1: only `libc-signals`, `libc-spawn` and `libc-threads` pass; section 4 has the exact
  case output.
* 5.2: both inherited qtests fail with timeouts. `core-servers` times out after 120 s and
  `file-tools` after 240 s; both serial logs stop at `loader: RavenBSD loader 0.1 (x86_64)` and
  never reach `init`. This is a kernel/server-lane regression.
* 6.2 mandoc: UNAVAILABLE. `which mandoc` exits 1; the render lint cannot run in this environment.

## 4. Seven-case matrix (OBSERVED, 2026-11-17)

Each row is a direct `cargo xtask qtest --case <case> --arch x86_64` run.

| Case | Result | Evidence and exact gap |
|---|---|---|
| libc-hello | FAIL | `libc-hello: auxv ok` after C74c publishes `AT_PHNUM`; the only failure is `FAIL string` (line 195), a P1b string routine. |
| libc-stdio | FAIL | `libc-stdio: printf ok`, then `process: child killed by page fault at 0x3`; the first stdio file/stream check after printf. |
| libc-time | ERROR (build) | The C fixture does not link: `rust-lld: undefined symbol: usleep`, `sleep`, `tzname`. These are genuinely absent from `libc.a` (`nm` shows only `nanosleep`); `sleep`/`usleep` are unistd and `tzname` is time (P1b), all outside C74b's Owns. C74b makes the sysroot repeatable but does not add these symbols. |
| libc-threads | PASS | `cargo xtask qtest --case libc-threads --arch x86_64` exit 0 after `libc-pack` skips the five compiler wrappers. |
| libc-signals | PASS | `cargo xtask qtest --case libc-signals --arch x86_64` exit 0. |
| libc-dynamic | PASS | `cargo xtask qtest --case libc-dynamic --arch x86_64` exit 0: `interp`, `needed`, `dlopen`, `dlsym`, `init`, `error`, `global`, `libc`, `tls`, `iterate`, `plug fini`, `dlclose`, `closed dlsym`, `version`, `done`, `init exited (0)`. C74g implements the target rtld: self-relocation, pre-mapped main registration, `DT_NEEDED`, RELA/RELR/PLT, RUNPATH/RPATH, lazy dynamic TLS, `dl*` forwarding, and `AT_ENTRY` transfer. |
| libc-spawn | PASS | `cargo xtask qtest --case libc-spawn --arch x86_64` exit 0. |

Supporting acceptance:

```
$ cargo xtask libc-lint
libc-lint: ok (3989 files scanned)
exit 0

$ cargo run -q -p raven-atf-run -- gate --g1
G1: NOT MET (0 of 4 measured rows)
exit 3
```

C74b also made `cargo xtask libc-pack` stage `build/<arch>/sysroot` (crt objects, `libc.a`, stub
archives, base/arch/generated headers, and `libc.so.1`/`ld-elf.so.1` when produced by
`libc-link`) and write the generated `Symbols.list` beside the archive instead of dirtying
`libc/ravenlibc/abi/Symbols.list`. This removes the manual sysroot staging step; it does not
change the missing `sleep`/`usleep`/`tzname` exports or the thread/dynamic/hello failures, which
live in crates and linker paths outside C74b's Owns.

C74c publishes `AT_PHDR`/`AT_PHENT`/`AT_PHNUM` from the retained startup auxv, so
`getauxval(AT_PHNUM)` is nonzero and `libc-hello: auxv ok`; `libc-link` stages the freshly linked
`libc.so.1`/`ld-elf.so.1` into the sysroot, and `libc-pack` skips the five compiler wrappers
(`stddef.h`, `stdbool.h`, `stdatomic.h`, `stdarg.h`, `float.h`) when staging headers. With that,
`libc-threads` **passes**. The dynamic test links at `--image-base=0x400000` and the shape check
accepts it (`shape: ok`). C74f traced the old `libc-dynamic` `EINVAL` to `copy_tail` mapping its
initfs source `SHARED`; with the source map private the child reached `ld-elf.so.1`. C74g
implements the target start path (self-relocation, main registration from `AT_PHDR`, `DT_NEEDED`,
RELA/RELR/PLT, TLS, `dl*` forwarding and `AT_ENTRY` transfer), so `libc-dynamic` now passes through
`done` with `init exited (0)`. C74g also adds `AT_BASE`/`AT_ENTRY` to the libc auxiliary lookup and
repairs the dynamic fixture's TLS constructor/build. The remaining libc failures are P1b
(`libc-hello` string, `libc-stdio`, `libc-time` `sleep`/`usleep`/`tzname`).

## 5. Other rows

* 1.3 LTP/pjdfstest: INFO, no owner-supplied runs.
* 1.6 Miri: UNAVAILABLE, `build/libc-gate/1.6.txt`; `cargo-miri` is not installed for the
  nightly toolchain.
* 2.1/2.2: UNAVAILABLE, `tools/libc-world/Cargo.toml`, C72 outputs and H-11 tarballs are absent.
* 3.1: NOT MET, `libc-size check: 6 pass, 63 fail, 68 known, 102 skipped, 0 budget-only`; the
  failures include P0/P1/P4/P6 size, `INV-LIBC-SO-NEEDED` on all three arches, boot instruction
  limits and `libc.so.1` size.
* 4.2: NOT MET, the `inet_text` soak has no duration, one crash and no executions.
* 6.2: BLOCKED, `which mandoc` exits 1.

## 6. C74d non-P1 evidence refresh (OBSERVED, origin/master `bd1d1de1`)

| Row | Command | Result |
|---|---|---|
| 1.1.audit | `cargo xtask libc-audit` | exit 1; 66 required / 66 present / 0 missing; stale `tests/libc/symbol-audit/expected.tsv` (out of Owns) |
| 1.2 G1 | `cargo run -q -p raven-atf-run -- gate --g1` | exit 3; C2-C5 0% not run, C6 unvendored; G1 NOT MET (0/4) |
| 1.4 compat | `cargo xtask libc-oracle list` + `diff {family}` | 4 of 27 families fail: `compat-crypt`, `compat-humanize`, `compat-setmode`, `compat-vis` lack `oracle-*` features |
| 1.6 Miri | nightly Miri preflight | exit 1; `cargo-miri` not installed for nightly → UNAVAILABLE |
| 2.1/2.2 world | `cargo run -q -p libc-world -- check` | exit 101; `libc-world` package not in the workspace → UNAVAILABLE |
| 3.1 size | `cargo xtask libc-size --check` | exit 1; 6 pass, 63 fail, 68 known, 102 skipped |
| 4.2 soak | `tests/libc/fuzz/SOAK.md` | `inet_text` crashed before 1 h; 13 targets 3600 s/0 crashes → NOT MET |
| 6.2 mandoc | `which mandoc` | exit 1; not installed → UNAVAILABLE |

`sh scripts/libc-symbol-audit.sh` exits 1 for the same stale snapshot; the required 66 symbols are
all PRESENT. `cargo xtask libc-lint` is clean (`0 violations`), and `cargo run -q -p libc-gate --
verdict` remains `NOT MET` (exit 1) because rows 1.1.audit, 1.2, 1.4, 3.1, 4.2, 5.1 and 5.2 are
red and 1.6/2.1/2.2/6.2 are unavailable in this environment. The only remaining on-target
blockers are P1b/P1c (`libc-hello` string, `libc-stdio`, `libc-time`, `libc-dynamic` runtime).
