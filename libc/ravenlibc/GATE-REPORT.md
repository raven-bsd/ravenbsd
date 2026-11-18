<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# ravenlibc libc gate report

Evaluated by `libc-gate` on 2026-11-18 from the C74hj audit tree (origin/master `b89e291b`).
C74hj re-evaluated every non-QEMU row into `build/libc-gate-audit/`; the table is
`cargo run -q -p libc-gate -- table --out build/libc-gate-audit`. The remaining NOT MET rows are
G1 donor data (1.2) and the size-budget lane (3.1); the world rows (2.1/2.2) are BLOCKED by the
missing `results/x86_64/world.json` and H-11/H-12 data. C74ib re-evaluated row 3.1 and the
aarch64 size options on `07054226` and wrote `build/libc-gate-c74ib/3.1.txt`; the 3.1 row uses
that evidence. The earlier C74h/C74z-S17/C74ht/C74ia evidence remains in the historical sections
below.

## 1. Gate table

| Row | Check | Status | Evidence | Note |
|---|---|---|---|---|
| 1 correctness | 1.1 symbol coverage (C1) | MET | build/libc-gate-audit/1.1.txt | exit 0 (C74hj) |
| 1 correctness | 1.1.pack packed symbol list (C1) | MET | build/libc-gate-audit/1.1.pack.txt | exit 0 (C74hj) |
| 1 correctness | 1.1.audit target-case symbol audit (C64j) | MET | build/libc-gate-audit/1.1.audit.txt | exit 0: required=66 present=66 missing=0 (C74hj) |
| 1 correctness | 1.2 donor suites G1 (C2-C6) | NOT MET | build/libc-gate-audit/1.2.txt | exit 3: expected 0; C2-C5 not run, C6 unvendored (C74hj) |
| 1 correctness | 1.3 LTP and pjdfstest (C7, C8) | INFO | build/libc-gate-audit/1.3.txt | owner-run LTP/pjdfstest results not supplied; INFO per C71/3.B.1.4 (C74hj) |
| 1 correctness | 1.4 differential tests (C9) | MET | build/libc-gate-audit/1.4.txt | 27/27 oracle families with `RAVENBSD_RESOURCES` (C74hj) |
| 1 correctness | 1.5 host unit tests | MET | build/libc-gate-audit/1.5.txt | 31 host crates via `cargo nextest run` (C74hj) |
| 1 correctness | 1.6 Miri (C11) | MET | build/libc-gate-c74fx/1.6.txt | `eval --only 1.6` exit 0; bounded runner 721 tests (715 library + 6 alignment), 6/6 pass; explicit integration exclusions (C74cw/C74fx) |
| 1 correctness | 1.7 lints (C16) | MET | build/libc-gate-audit/1.7.txt | `libc-lint: ok (4267 files scanned)` (C74hj) |
| 2 portability | 2.1 world check | BLOCKED | build/libc-gate-audit/2.1.txt | harness/list present; blocked on missing `results/x86_64/world.json` plus H-11 tarballs and the H-12 freeze (C74hj; the rows.toml note still names the absent `Cargo.toml`, stale) |
| 2 portability | 2.2 world build gate (C13) | BLOCKED | build/libc-gate-audit/2.2.txt | harness present; the gate cannot run without `results/x86_64/world.json`, H-11 tarballs and the H-12 freeze; no synthetic results (C74hj) |
| 3 size | 3.1 size budgets (C14) | NOT MET | build/libc-gate-c74ib/3.1.txt | exit 1: 64 pass, 13 fail, 65 known, 99 skipped, 0 budget-only; C74ia added the riscv64 libc.so.1 measured+3% budget; C74ib found no safe in-Owns aarch64 reduction (baseline S 1,363,752; ICF+opt-z+strip candidate needs out-of-Owns changes/qtests); 3 NEEDED rows are known; 10 static invariants + aarch64 libc.so.1 S + 2 riscv64 P9 static rows remain |
| 3 size | 3.2 boot path (C15) | INFO | build/libc-gate-audit/3.2.txt | PG-60 absent; 0-1 syscalls and <=250 instructions are not evaluable (C74hj) |
| 4 hardening | 4.1 fuzz replay | MET | build/libc-gate-audit/4.1.txt | exit 0 (C74hj) |
| 4 hardening | 4.2 fuzz soak (C10) | MET | build/libc-gate-audit/4.2.txt | 14 targets soaked >= 3600 s with zero crashes; `inet_text` 3601 s, 261,966,448 runs (C74k/C74hj) |
| 4 hardening | 4.3 hardened malloc default | MET | build/libc-gate-audit/4.3.txt | hardened backend selected; tiny not in libc.so.1 (C74hj) |
| 4 hardening | 4.4 ENTROPY_WEAK status (H-5) | INFO | build/libc-gate-audit/4.4.txt | ENTROPY_WEAK UNVERIFIED (C74hj); PG-52 release blocker, not a libc-gate blocker |
| 5 on-target | 5.1 on-target libc cases (C12) | MET | build/libc-gate/5.1.txt | dynamic/threads/signals/spawn and hello/stdio/time pass (C74u/C74ae); C74hj did not re-run (no QEMU), local evidence path absent |
| 5 on-target | 5.2.core-servers core-servers regression | MET | build/libc-gate/5.2.core-servers.txt | exit 0: minimal `core-servers` profile (36-77 s), 300 s case limit (C74n); C74hj did not re-run (no QEMU), local evidence path absent |
| 5 on-target | 5.2.file-tools file-tools regression | MET | build/libc-gate/5.2.file-tools.txt | exit 0: minimal `core-servers-files` profile; `df` reports `/`, `/tmp` and the device mounts from `/etc/mounts`; `tooltest: ok` (C74v); C74hj did not re-run (no QEMU), local evidence path absent |
| 5 on-target | 5.3 second architecture | INFO | build/libc-gate-audit/5.3.txt | aarch64: not delivered (C73 open item); owner decides whether G1 closes on x86_64 alone (C74hj) |
| 6 documentation | 6.1 page coverage | MET | build/libc-gate-audit/6.1.txt | exit 0 (C74hj) |
| 6 documentation | 6.2 render lint | MET | build/libc-gate-audit/6.2.txt | reader MET; the mandoc 1.14.6-1 transcript is C74ai (local transcript absent) |
| 6 documentation | 6.3 existing docs gate | MET | build/libc-gate-audit/6.3.txt | exit 0; `docs: 198 page(s) checked` (C74hj) |
| 6 documentation | 6.4 libc(7) | MET | build/libc-gate-audit/6.4.txt | docs/man7/libc.7 exists (C74hj) |

Verdict: NOT MET

## 2. Verdict

**libc gate: NOT MET.**

Rows 1.2 (G1) and 3.1 (size) are NOT MET. Row 2 (portability) is BLOCKED.
Rows 1.6 (Miri), 1.7 (lints), 4.2 (soak), 5.1/5.2 (on-target, committed evidence; not re-run in
this audit because QEMU is out of scope) and 6.2/6.3 (mandoc/docs) are MET.
MET checks: symbol coverage, the packed symbol list, the target-case symbol audit, the 31 host
unit-test crates, the bounded Miri row (721 tests, 6/6 pass), the lint sweep (4267 files clean),
fuzz replay and the 14-target soak, the hardened allocator default, page coverage, the render
lint, the existing docs gate and `libc(7)`.
The gate cannot close because of G1 (1.2), the size-budget lane (3.1; C74ib: 64 pass, 13 fail,
65 known, 99 skipped; no safe in-Owns aarch64 shrink) and the blocked world rows (2.1/2.2).

`libc-dynamic`, `libc-signals`, `libc-spawn` and `libc-threads` are green. The historical C74h
target failures were P1b (`libc-hello` string, `libc-stdio`, `libc-time` sleep/usleep/tzname);
C74u/C74ae later closed them and 5.1 is MET per the committed evidence.

## 3. Rows changed by this refresh

* 1.1.audit: **MET**. `cargo xtask libc-audit` exits 0: required=66, present=66, missing=0; the
  `tests/libc/symbol-audit/expected.tsv` snapshot is no longer stale (C74e refresh).
* 1.2 G1: still NOT MET. `raven-atf-run gate --g1` exits 3: C2-C5 are 0 % (not run), C6 is
  unvendored; 0 of 4 measured rows (C71a/C71, owner: test/donor lane).
* 1.4: **MET**. C74q made `RAVENBSD_RESOURCES` discoverable and all 27 oracle families run
  (`libc-gate eval --only 1.4`).
* 1.5: **MET** through `cargo nextest run`; the gate row now uses nextest (C74h harness fix).
* 1.6 Miri: **MET**. C74cw wired the row to the bounded runner (`scripts/check-miri.sh` via the
  cargo entry point); C74fx re-ran `cargo run -q -p libc-gate -- eval --only 1.6 --out
  build/libc-gate-c74fx` with exit 0. The run is 721 tests (715 library + 6 alignment), 6/6
  pass, 0 UB; the explicit integration exclusions remain in
  `tests/libc/miri/integration-exclusions.txt`.
* 1.7: **MET**. `cargo xtask libc-lint` is clean after C74i/P1f5 and the frozen ABI sync.
* 2.1/2.2 world: BLOCKED. The harness and the 200-row list are present and `libc-world check`
  passes. The real build cannot start: `ravenbsd-resources/world-src` and a `donors.lock` world
  entry are absent (H-11) and every `world.toml` row is `PLACEHOLDER` (H-12), so the smoke5 probe
  produces 5/5 `placeholder` (`version not frozen (H-12)`) and 0 of 100 built. Owner: the tarball
  freeze/fetch (H-11/H-12); no result JSON was invented.
* 3.1 size: NOT MET. `cargo xtask libc-size --check` exits 1 with `64 pass, 13 fail, 65 known,
  99 skipped, 0 budget-only` after C74ia added the riscv64 `libc.so.1` budget on `fb65ec96`.
  All three `libc.so.1` budgets pass (x86_64, aarch64, riscv64); the three
  `INV-LIBC-SO-NEEDED` rows are known rule exceptions: each self-contained DSO has 0
  `DT_NEEDED` (`readelf -d`), while the hard-coded rule expects a self-NEEDED entry; a follow-up
  tooling ticket must change the expectation. C74ib tested the aarch64 DSO on `07054226`: the
  baseline is S 1,363,752; `--icf=all` alone reaches 1,306,012 (still 6,012 over), and
  `--icf=all` + `opt-level=z` + `--strip-debug` reaches 1,255,136 / F 2,451,200 but requires
  `xtask/src/libc_link.rs` and `Cargo.toml`/`libc_pack` changes outside Owns plus qtests.
  `force-unwind-tables=no` (S 1,200,188) is rejected because it removes `.eh_frame`/backtrace.
  No safe in-Owns reduction exists, so the aarch64 `libc.so.1` S row remains FAIL. The remaining
  13 real fails are the 10 static invariants, the aarch64 S row, and the two riscv64 P9 static
  S/F rows grown by C74hs (permanent parity; reviewer budget values in TRIAGE, not applied). The
  exact tables are in the C74ib report and `tests/libc/size/TRIAGE.md`.
* 4.2 soak: MET. The 2026-11-17 `inet_text` one-hour soak completed 3601 s with
  261,966,448 runs and 0 crashes; the other 13 targets completed 3600 s with 0 crashes (14
  targets total, C74k/C74hj).
* 5.1: **MET**. `libc-hello`, `libc-stdio` and `libc-time` pass after C74u/C74ae; the dynamic,
  signals, spawn and threads cases are green. C74hj did not re-run the qtests (no QEMU); the
  local evidence path is absent and the status is from the committed C74u/C74ae evidence.
* 5.2: **MET**. C74n replaced the shell-inheriting profiles with minimal `core-servers` and
  `core-servers-files` profiles. C74v implemented the RavenBSD `read_fs_list` from the real
  `/etc/mounts` table, so `df` reports `/`, `/tmp` and the device schemes; `core-servers` and
  `file-tools` both exit 0 (`tooltest: ok`, `svcd: settled`, `init exited (0)`). C74hj did not
  re-run the qtests (no QEMU); the local evidence paths are absent.
* 6.2 mandoc: **MET**. mandoc 1.14.6-1 lints and renders the 17 gate pages; the reproducible
  command is `sh scripts/check-mandoc.sh` (lint at `-Wwarning` plus `-Tutf8`).

## 4. Seven-case matrix (OBSERVED, 2026-11-17, C74h)

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
repairs the dynamic fixture's TLS constructor/build. The remaining libc failures in the historical
C74h run were P1b (`libc-hello` string, `libc-stdio`, `libc-time` `sleep`/`usleep`/`tzname`);
C74u/C74ae later closed them and 5.1 is MET per the committed evidence.

## 5. Other rows

* 1.3 LTP/pjdfstest: INFO, no owner-supplied runs.
* 1.6 Miri: MET, `build/libc-gate-c74fx/1.6.txt`; bounded runner 721 tests (715 library + 6
  alignment), 6/6 pass, 0 UB; explicit integration exclusions.
* 1.7 lints: MET, `build/libc-gate-audit/1.7.txt`; `libc-lint: ok (4267 files scanned)` (C74hj).
* 2.1/2.2: BLOCKED, harness/list present; `results/x86_64/world.json` plus the H-11
  `world-src`/`donors.lock` entry and the H-12 freeze are absent. The stale rows.toml note still
  names `tools/libc-world/Cargo.toml`, which now exists (flagged by C74hj).
* 3.1: NOT MET, `libc-size check: 64 pass, 13 fail, 65 known, 99 skipped, 0 budget-only`;
  C74ia applied the riscv64 `libc.so.1` measured+3% budget and recorded the third NEEDED rule
  exception; all three DSO budgets pass. C74ib found no safe in-Owns aarch64 reduction (the
  ICF+opt-z+strip candidate needs out-of-Owns changes and qtests), so the 10 static invariants,
  the aarch64 `libc.so.1` S ceiling and the 2 riscv64 P9 static rows remain; table in
  `tests/libc/size/TRIAGE.md`.
* 4.2: MET, 14 targets soaked >= 3600 s with zero crashes (`inet_text` 3601 s, 261,966,448
  runs); C74k/C74hj.
* 5.1/5.2: MET per the committed C74u/C74ae/C74n/C74v evidence; C74hj did not re-run the qtests
  (no QEMU) and the local evidence paths are absent.
* 6.2: MET, `build/libc-gate-audit/6.2.txt` reader plus the C74ai mandoc 1.14.6-1 transcript
  (the local transcript is absent).

## 6. C74h evidence refresh (OBSERVED, origin/master `beedfbbd`; 3.1 re-evaluated by C74ib on `07054226`)

| Row | Command | Result |
|---|---|---|
| 1.1.audit | `cargo xtask libc-audit` | exit 0; 66 required / 66 present / 0 missing → MET |
| 1.2 G1 | `cargo run -q -p raven-atf-run -- gate --g1` | exit 3; C2-C5 0% not run, C6 unvendored; G1 NOT MET (0/4) |
| 1.4 compat | `cargo xtask libc-oracle list` + `diff {family}` | 27/27 families MET with `RAVENBSD_RESOURCES` (C74q) |
| 1.5 host tests | `cargo nextest run` per crate | exit 0; 31 crates MET (gate row changed from `cargo test` to nextest) |
| 1.6 Miri | `cargo run -q -p libc-gate -- eval --only 1.6 --out build/libc-gate-c74fx` | exit 0; 721 tests (715 library + 6 alignment), 6/6 pass, 0 UB; explicit integration exclusions → MET (C74cw/C74fx) |
| 1.7 lints | `cargo xtask libc-lint` | exit 0; clean after C74i/P1f5/ABI sync → MET |
| 2.1/2.2 world | `libc-world check` / `build --subset smoke5` / `gate` | check exit 0 (200 packages, 197.5 weighted); smoke5 5/5 `placeholder` (H-12), 0/100 built; H-11 `world-src`/`donors.lock` absent → BLOCKED |
| 3.1 size | `cargo xtask libc-size --check` | exit 1; 64 pass, 13 fail, 65 known, 99 skipped, 0 budget-only; C74ib found no safe in-Owns aarch64 reduction (baseline S 1,363,752; ICF+opt-z+strip candidate needs out-of-Owns changes/qtests); 3 NEEDED rule exceptions; 10 static invariants + aarch64 libc.so.1 S + 2 riscv64 P9 static rows remain |
| 4.2 soak | `tests/libc/fuzz/SOAK.md` | MET: 2026-11-17 one-hour `inet_text` soak, 3601 s, 261,966,448 runs, 0 crashes (C74k) |
| 5.1 target | qtest JSON from `eval` | dynamic, signals, spawn, threads, hello, stdio and time pass (C74u/C74ae) → MET |
| 5.2 kernel | `qtest core-servers` / `qtest file-tools` | both MET: minimal profiles; `df` reads `/etc/mounts`; `tooltest: ok`, `init exited (0)` |
| 6.2 mandoc | `scripts/check-mandoc.sh` + `libc-gate eval --only 6.2` | exit 0; 17 pages lint clean with mandoc 1.14.6-1 → MET |

## 7. C74hj audit (OBSERVED, origin/master `b89e291b`, 2026-11-18)

```
cargo run -q -p libc-gate -- eval --only 1.1,1.2,1.4,1.5,1.7,2.1,2.2,3.1,4.1,4.2,4.3,6.1,6.2,6.3,6.4 --out build/libc-gate-audit
cargo run -q -p libc-gate -- eval --only 1.3,3.2,4.4,5.3 --out build/libc-gate-audit
cargo run -q -p libc-gate -- verdict --out build/libc-gate-audit
```

Result: `libc gate: NOT MET` (exit 1).

* MET: 1.1, 1.1.pack, 1.1.audit, 1.4, 1.5, 1.6, 1.7, 4.1, 4.2, 4.3, 6.1, 6.2, 6.3, 6.4.
* NOT MET: 1.2, 3.1.
* BLOCKED: 2.1, 2.2.
* INFO: 1.3, 3.2, 4.4, 5.3.
* 5.1/5.2: MET per the committed C74u/C74ae/C74n/C74v evidence. The audit did not re-run
  QEMU; `verdict` reports them missing locally because their evidence paths are absent.

Stale items flagged (gate-row/source edits are out of scope):

* `rows.toml` notes still say 1.4 cannot start (current 27/27), 1.7 has 26 violations (current
  clean, 4267 files), 3.1 is the C74z-S11 17/48/69/105 run (current 64/13/65/99), 4.2 has an
  `inet_text` crash (current 14 targets/0 crashes), 5.1 has P1b failures (current MET per
  committed evidence), 6.2 has no mandoc (current reader MET), and 2.1/2.2 name a missing
  `tools/libc-world/Cargo.toml` (the file exists; the missing item is `results/x86_64/world.json`).
* 5.1/5.2 and the C74ai 6.2 transcript have no local evidence file; 6.2's audit evidence is a
  reader status without a command line.
* 3.1 moved from 10 fail/102 skip to 12 fail/101 skip in the C74hj audit; C74ht then rebuilt the
  dynamic rows, and C74ia added the riscv64 DSO budget: 64 pass/13 fail/65 known/99 skipped. C74ib
  tested the aarch64 DSO size options and found no safe in-Owns reduction; the 13 are the 10
  static invariants, the real aarch64 `libc.so.1` S overrun, and the 2 riscv64 P9 static rows
  grown by C74hs; the 3 NEEDED rows are known rule exceptions.
* 4.4 reports ENTROPY_WEAK UNVERIFIED (the old note said CLEAR).
* The C74h section header commit `beedfbbd` and the 3.1 C74z-S17 `10d8c9dd` are historical; this
  audit is on `b89e291b`, and the C74ib 3.1 evidence is on the newer `07054226`.

`cargo run -q -p libc-gate -- eval` evaluated 90 checks; `table` and `verdict` exit 1 with
`NOT MET`. `scripts/libc-symbol-audit.sh` and `cargo xtask libc-audit` both exit 0. The blockers
are G1 (1.2), the size lane (3.1) and the blocked world rows (2.1/2.2); the on-target rows are
MET per the committed evidence.
