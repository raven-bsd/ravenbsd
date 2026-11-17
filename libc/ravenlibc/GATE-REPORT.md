<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# ravenlibc libc gate report

Evaluated by `libc-gate` on 2026-11-17 from the C74h tree (origin/master `beedfbbd`). C74h ran
`libc-gate eval` and refreshed every row; C74g had already made `libc-dynamic` pass. Every status
below has an evidence file under `build/libc-gate/`; the table is
`cargo run -q -p libc-gate -- table`. C74z-S10 re-evaluated row 3.1 on `e414e47d` and wrote
`build/libc-gate-c74z-s10/3.1.txt`; the row below uses that evidence. The remaining NOT MET rows
are P1b/P1c work, lint fixes, missing donor data, a size-budget lane, a fuzz soak, and unavailable
host tooling, and are listed with their owning lane.

## 1. Gate table

| Row | Check | Status | Evidence | Note |
|---|---|---|---|---|
| 1 correctness | 1.1 symbol coverage (C1) | MET | build/libc-gate/1.1.txt | exit 0 |
| 1 correctness | 1.1.pack packed symbol list (C1) | MET | build/libc-gate/1.1.pack.txt | exit 0 |
| 1 correctness | 1.1.audit target-case symbol audit (C64j) | MET | build/libc-gate/1.1.audit.txt | exit 0: required=66 present=66 missing=0 |
| 1 correctness | 1.2 donor suites G1 (C2-C6) | NOT MET | build/libc-gate/1.2.txt | exit 3: expected 0 |
| 1 correctness | 1.3 LTP and pjdfstest (C7, C8) | INFO | build/libc-gate/1.3.txt | owner-run LTP/pjdfstest results not supplied; INFO per C71/3.B.1.4 |
| 1 correctness | 1.4 differential tests (C9) | NOT MET | build/libc-gate/1.4.txt | 26 families cannot find ravenbsd-resources in the minion clone; `tls-layout` is MET |
| 1 correctness | 1.5 host unit tests | MET | build/libc-gate/1.5.txt | 31 host crates via `cargo nextest run` |
| 1 correctness | 1.6 Miri (C11) | BLOCKED | build/libc-gate/1.6.txt | preflight command failed |
| 1 correctness | 1.7 lints (C16) | NOT MET | build/libc-gate/1.7.txt | 17 violations: 3 in `ld/src/raw/exports.rs` (C74g SAFETY comments), 14 pre-existing |
| 2 portability | 2.1 world check | BLOCKED | build/libc-gate-c74ah/2.1.txt | harness present; `libc-world check` passes (200 packages, 197.5 weighted); blocked on `results/x86_64/world.json` because `ravenbsd-resources/world-src` and a `donors.lock` world entry are absent (H-11) and the list is PLACEHOLDER (H-12) |
| 2 portability | 2.2 world build gate (C13) | BLOCKED | build/libc-gate-c74ah/2.2.txt | harness present; real `build --subset smoke5` yields 5/5 `placeholder` (`version not frozen (H-12)`), gate 0/100 built; blocked on H-11 tarballs + H-12 freeze; no synthetic results |
| 3 size | 3.1 size budgets (C14) | NOT MET | build/libc-gate-c74z-s10/3.1.txt | exit 1: 11 pass, 52 fail, 69 known, 105 skipped, 0 budget-only; C74z-S10 re-froze the six P9 S/F rows; remaining 44 S/F, 6 B-only, 2 boot |
| 3 size | 3.2 boot path (C15) | INFO | build/libc-gate/3.2.txt | PG-60 absent; 0-1 syscalls and <=250 instructions are not evaluable |
| 4 hardening | 4.1 fuzz replay | MET | build/libc-gate/4.1.txt | exit 0 |
| 4 hardening | 4.2 fuzz soak (C10) | NOT MET | build/libc-gate/4.2.txt | inet_text has no soak duration; inet_text has 1 crash(es); inet_text has no executions |
| 4 hardening | 4.3 hardened malloc default | MET | build/libc-gate/4.3.txt | hardened backend selected; tiny not in libc.so.1 |
| 4 hardening | 4.4 ENTROPY_WEAK status (H-5) | INFO | build/libc-gate/4.4.txt | ENTROPY_WEAK CLEAR; PG-52 release blocker, not a libc-gate blocker |
| 5 on-target | 5.1 on-target libc cases (C12) | NOT MET | build/libc-gate/5.1.txt | 7 qtest cases: dynamic, threads, signals, spawn pass; hello/stdio/time fail (P1b) |
| 5 on-target | 5.2.core-servers core-servers regression | MET | build/libc-gate/5.2.core-servers.txt | exit 0: minimal `core-servers` profile (36-77 s), 300 s case limit |
| 5 on-target | 5.2.file-tools file-tools regression | MET | build/libc-gate/5.2.file-tools.txt | exit 0: minimal `core-servers-files` profile; `df` reports `/`, `/tmp` and the device mounts from `/etc/mounts`; `tooltest: ok` |
| 5 on-target | 5.3 second architecture | INFO | build/libc-gate/5.3.txt | aarch64: not delivered (C73 open item); owner decides whether G1 closes on x86_64 alone |
| 6 documentation | 6.1 page coverage | MET | build/libc-gate/6.1.txt | exit 0 |
| 6 documentation | 6.2 render lint | BLOCKED | build/libc-gate/6.2.txt | preflight command failed |
| 6 documentation | 6.3 existing docs gate | MET | build/libc-gate/6.3.txt | exit 0 |
| 6 documentation | 6.4 libc(7) | MET | build/libc-gate/6.4.txt | docs/man7/libc.7 exists |

Verdict: NOT MET

## 2. Verdict

**libc gate: NOT MET.**

Rows 1 (correctness), 3 (size), 4 (hardening) and 5 (on-target) are NOT MET. Rows 2
(portability) and 6 (documentation) are BLOCKED. MET checks: symbol coverage, the packed symbol
list, the target-case symbol audit (now exit 0), the 31 host unit-test crates, fuzz replay, the
hardened allocator default, page coverage, the existing docs gate and `libc(7)`. The gate cannot
close because of the lint violations, missing G1/donor data, the size-budget lane, the
`inet_text` soak, the three P1b target cases, the core-servers/file-tools regression and
unavailable Miri/world/mandoc.

`libc-dynamic`, `libc-signals`, `libc-spawn` and `libc-threads` are green. The remaining target
failures are P1b (`libc-hello` string, `libc-stdio`, `libc-time` sleep/usleep/tzname).

## 3. Rows changed by this refresh

* 1.1.audit: **MET**. `cargo xtask libc-audit` exits 0: required=66, present=66, missing=0; the
  `tests/libc/symbol-audit/expected.tsv` snapshot is no longer stale (C74e refresh).
* 1.2 G1: still NOT MET. `raven-atf-run gate --g1` exits 3: C2-C5 are 0 % (not run), C6 is
  unvendored; 0 of 4 measured rows (C71a/C71, owner: test/donor lane).
* 1.4: NOT MET in this clone. `libc-oracle list` finds 27 families, but `diff <family>` exits 1
  before running the oracle because `ravenbsd-resources` is not discoverable from
  `/media/bo/.../minions/carl/ravenbsd`; only model-only `tls-layout` passes. Owner: integration
  copy or a resources path in the harness (C35/C71).
* 1.5: **MET** through `cargo nextest run`; the gate row now uses nextest (C74h harness fix).
* 1.6 Miri: BLOCKED. The nightly Miri preflight reports `cargo-miri` is not installed.
* 1.7: **NOT MET**. `cargo xtask libc-lint all` reports 17 violations: the C74g loader adds three
  missing `// SAFETY:` blocks in `ld/src/raw/exports.rs:591/594/596`; the other 14 are
  pre-existing `one-symbol`/`std-path`/`abi-freeze` findings in the libc crate. Owner: C74g
  follow-up plus the P1e/P1h/P1i lanes.
* 2.1/2.2 world: BLOCKED. The harness and the 200-row list are present and `libc-world check`
  passes. The real build cannot start: `ravenbsd-resources/world-src` and a `donors.lock` world
  entry are absent (H-11) and every `world.toml` row is `PLACEHOLDER` (H-12), so the smoke5 probe
  produces 5/5 `placeholder` (`version not frozen (H-12)`) and 0 of 100 built. Owner: the tarball
  freeze/fetch (H-11/H-12); no result JSON was invented.
* 3.1 size: NOT MET. `cargo xtask libc-size --check` exits 1 with `11 pass, 52 fail, 69 known,
  105 skipped, 0 budget-only`; C74z-S10 re-froze the six P9 S/F budgets on `e414e47d` after
  C39m/C74zb at measured + 3 percent, leaving 44 real S/F, 6 B-only and 2 boot rows. The P9
  envelope is closed; the shrink plan is in `tests/libc/size/TRIAGE.md`.
* 4.2 soak: NOT MET. `tests/libc/fuzz/SOAK.md` records `inet_text` crashing before one hour; the
  other 13 targets completed 3600 s with 0 crashes. Owner: test/fuzz lane.
* 5.1: `libc-dynamic`, `libc-signals`, `libc-spawn` and `libc-threads` pass; `libc-hello`,
  `libc-stdio` and `libc-time` remain P1b. Section 4 has the exact output.
* 5.2: **MET**. C74n replaced the shell-inheriting profiles with minimal `core-servers` and
  `core-servers-files` profiles. C74v implemented the RavenBSD `read_fs_list` from the real
  `/etc/mounts` table, so `df` reports `/`, `/tmp` and the device schemes; `core-servers` and
  `file-tools` both exit 0 (`tooltest: ok`, `svcd: settled`, `init exited (0)`).
* 6.2 mandoc: BLOCKED. `which mandoc` exits 1; the render lint cannot run.

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
repairs the dynamic fixture's TLS constructor/build. The remaining libc failures are P1b
(`libc-hello` string, `libc-stdio`, `libc-time` `sleep`/`usleep`/`tzname`).

## 5. Other rows

* 1.3 LTP/pjdfstest: INFO, no owner-supplied runs.
* 1.6 Miri: BLOCKED, `build/libc-gate/1.6.txt`; `cargo-miri` is not installed for nightly.
* 1.7 lints: NOT MET, `build/libc-gate/1.7.txt`; 17 violations (3 C74g SAFETY comments, 14
  pre-existing).
* 2.1/2.2: BLOCKED, harness/list present and `check` green; H-11 `world-src`/`donors.lock` entry
  and H-12 freeze are absent, so the real smoke5 build is 5/5 `placeholder` and 0/100 built.
* 3.1: NOT MET, `libc-size check: 11 pass, 52 fail, 69 known, 105 skipped, 0 budget-only`;
  C74z-S10 P9 S/F re-freeze (44 S/F + 6 B-only + 2 boot remain); shrink plan in
  `tests/libc/size/TRIAGE.md`.
* 4.2: NOT MET, the `inet_text` soak has no duration, one crash and no executions.
* 6.2: BLOCKED, `which mandoc` exits 1.

## 6. C74h evidence refresh (OBSERVED, origin/master `beedfbbd`; 3.1 re-evaluated by C74z-S10 on `e414e47d`)

| Row | Command | Result |
|---|---|---|
| 1.1.audit | `cargo xtask libc-audit` | exit 0; 66 required / 66 present / 0 missing → MET |
| 1.2 G1 | `cargo run -q -p raven-atf-run -- gate --g1` | exit 3; C2-C5 0% not run, C6 unvendored; G1 NOT MET (0/4) |
| 1.4 compat | `cargo xtask libc-oracle list` + `diff {family}` | 27 families; 26 exit 1 because `ravenbsd-resources` is not discoverable from this clone; `tls-layout` MET |
| 1.5 host tests | `cargo nextest run` per crate | exit 0; 31 crates MET (gate row changed from `cargo test` to nextest) |
| 1.6 Miri | nightly Miri preflight | exit 1; `cargo-miri` not installed for nightly → BLOCKED |
| 1.7 lints | `cargo xtask libc-lint all` | exit 1; 17 violations (3 C74g SAFETY comments, 14 pre-existing) → NOT MET |
| 2.1/2.2 world | `libc-world check` / `build --subset smoke5` / `gate` | check exit 0 (200 packages, 197.5 weighted); smoke5 5/5 `placeholder` (H-12), 0/100 built; H-11 `world-src`/`donors.lock` absent → BLOCKED |
| 3.1 size | `cargo xtask libc-size --check` | exit 1; 11 pass, 52 fail, 69 known, 105 skipped, 0 budget-only; C74z-S10 P9 S/F re-freeze (44 S/F + 6 B-only + 2 boot remain); shrink plan in `tests/libc/size/TRIAGE.md` |
| 4.2 soak | `tests/libc/fuzz/SOAK.md` | `inet_text` crashed before 1 h; 13 targets 3600 s/0 crashes → NOT MET |
| 5.1 target | qtest JSON from `eval` | dynamic, signals, spawn, threads pass; hello, stdio, time fail (P1b) |
| 5.2 kernel | `qtest core-servers` / `qtest file-tools` | both MET: minimal profiles; `df` reads `/etc/mounts`; `tooltest: ok`, `init exited (0)` |
| 6.2 mandoc | `which mandoc` | exit 1; not installed → BLOCKED |

`cargo run -q -p libc-gate -- eval` evaluated 90 checks; `table` and `verdict` exit 1 with
`NOT MET`. `scripts/libc-symbol-audit.sh` and `cargo xtask libc-audit` both exit 0. The only
remaining on-target blockers are P1b (`libc-hello` string, `libc-stdio`, `libc-time`).
