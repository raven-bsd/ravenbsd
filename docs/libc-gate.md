<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# libc gate

The libc gate is the six-row acceptance gate for ravenlibc: correctness, portability, size,
hardening, on-target cases and documentation. The check definitions live in
`tests/libc/gate/rows.toml`; the evaluator is the `libc-gate` crate. A row is MET only when every
required check in it is MET; BLOCKED checks must be unblocked by their owning ticket, not by
lowering the threshold or editing expected output.

## Running it

```sh
cargo xtask libc-pack --arch x86_64
cargo xtask libc-link --arch x86_64
cargo run -q -p libc-gate -- eval
cargo run -q -p libc-gate -- table
cargo run -q -p libc-gate -- verdict
```

`eval` writes one evidence file per check under `build/libc-gate/`; `table` renders the markdown
table for `libc/ravenlibc/GATE-REPORT.md`; `verdict` exits 0 only when the gate is MET. The base
gates are `cargo xtask libc-lint` and `make check-fast`.

`libc-pack` stages `build/<arch>/sysroot` (crt objects, `libc.a`, stub archives, headers, and the
dynamic `libc.so.1`/`ld-elf.so.1` once `libc-link` has built them), so the C target cases compile
against a complete sysroot without a manual staging step.

The seven on-target cases are:

```sh
cargo xtask qtest --case libc-hello --arch x86_64
cargo xtask qtest --case libc-stdio --arch x86_64
cargo xtask qtest --case libc-time --arch x86_64
cargo xtask qtest --case libc-threads --arch x86_64
cargo xtask qtest --case libc-signals --arch x86_64
cargo xtask qtest --case libc-dynamic --arch x86_64
cargo xtask qtest --case libc-spawn --arch x86_64
```

The donor G1 suite is `cargo run -q -p raven-atf-run -- gate --g1`.

## Current verdict (2026-11-18, C74hj audit on `b89e291b`)

**NOT MET.** `cargo run -q -p libc-gate -- verdict --out build/libc-gate-audit` exits 1.

* MET: 1.1, 1.1.pack, 1.1.audit, 1.4, 1.5, 1.6, 1.7, 4.1, 4.2, 4.3, 5.1, 5.2, 6.1, 6.2, 6.3, 6.4.
  5.1/5.2 are MET per the committed C74u/C74ae/C74n/C74v evidence; they were not re-run in this
  audit (no QEMU) and no local evidence file is present.
* NOT MET: 1.2 (G1 0/4; C2-C5 not run, C6 unvendored), 3.1 (C74hj audit: 63 pass, 12 fail,
  62 known, 101 skipped, 0 budget-only; the 12 invariant-ceiling rows remain).
* BLOCKED: 2.1/2.2 (the harness and list are present; `results/x86_64/world.json` plus the
  H-11 tarballs and H-12 freeze are absent).

The exact evidence and the seven-case matrix are in `libc/ravenlibc/GATE-REPORT.md` and under
`build/libc-gate/`. C74h re-ran the full gate on `beedfbbd`: the symbol audit is now MET (66/66,
exit 0); the host-test row now runs through `cargo nextest run`; `libc-dynamic`, `libc-signals`,
`libc-spawn` and `libc-threads` pass; `libc-hello` failed only the P1b `string` check (auxv ok);
`libc-stdio` and `libc-time` remained P1b at that point (C74u/C74ae later closed them; 5.1 is MET
per the committed evidence). C74i then added the three missing loader SAFETY
comments: `libc-lint` now reports 26 violations, all outside the loader files (P1f/phil 14,
P1h/otto 8, P1i/tim 3, ABI/contract 1; C74l/C74i reduced that to 15, P1f 14 and ABI 1).
C74n closed 5.2.core-servers, C74v closed 5.2.file-tools with the real `/etc/mounts` list, and
C74u closed 5.1 (`sscanf`, `tzname`). C74q closed 1.4 (27/27) and C74k recorded the one-hour
`inet_text` soak. C74cw wired row 1.6 to the bounded Miri runner and C74fx re-ran
`cargo run -q -p libc-gate -- eval --only 1.6 --out build/libc-gate-c74fx`: exit 0, **MET**
(721 Miri tests: 715 library + 6 alignment, 6/6 pass, explicit integration exclusions).
C74hj audited every row on `b89e291b` with
`cargo run -q -p libc-gate -- eval --only 1.1,1.2,1.4,1.5,1.7,2.1,2.2,3.1,4.1,4.2,4.3,6.1,6.2,6.3,6.4 --out build/libc-gate-audit`
and `verdict --out build/libc-gate-audit`: 1.7 (4267 files clean), 4.2 (14 targets, 0 crashes),
6.2/6.3 and 6.4 are MET; 1.2 and 3.1 remain NOT MET and 2.1/2.2 remain BLOCKED. The 3.1 audit
count moved from 10 fail/102 skip to 12 fail/101 skip; the two new failures need the size owner.
The gate cannot close until G1, the size-budget cluster and the blocked world rows are resolved by
their owners.

## 3.1 size triage (C74w/C74z/C74z-S17; OBSERVED on `10d8c9dd`)

`cargo xtask libc-size --check` reports `63 pass, 10 fail, 62 known, 102 skipped, 0 budget-only`
after C74z-S17 re-froze the 17 legitimate budget/layout rows at the current + 3 percent and the
C74cx matcher fix landed. The 17 rows were the C74dl audit's legitimate set: P2 aarch64/riscv64
S/F, P3 riscv64 S/F/B, P5 x86_64/aarch64/riscv64 server B-only, P7 x86_64 tiny/server B-only,
P7 aarch64/riscv64 S/F/B, and P8 aarch64 tiny/server B-only. No invariant ceiling is changed.

* **P7-R-14:** the C74cx fix (`forbid_symbols` for the iconv lifecycle entry points) removes the
  allocator-CGU false positive; S17 has no `P7-R-14` FAIL. The x86_64 `locale*` `KNOWN` rows are
  pre-existing first-measurement exceptions.
* **10 invariants remain:** `INV-S-P2-P0` aarch64/riscv64 tiny/server and `INV-S-P3-P2` x86_64,
  aarch64 and riscv64 tiny/server. Current deltas: 26,671/29,159 and 19,075/23,421 vs 5,120;
  17,160/23,104, 16,964/18,272 and 10,804/12,400 vs 10,240. These are real P2/P3 relationships;
  the reviewer/owner must raise the hard-coded ceilings or add equivalent known-violation rows.
* Boot rows pass; P9 remains green. `B` is the Berkeley `llvm-size` bss field (all NOBITS,
  including `.relro_padding`), not only the `.bss` section. `build/size/libc-size.json`/`.md` are
  run artifacts and leave `budget_* = 0` and `status = ok`; the console and
  `build/libc-gate-c74z-s17/3.1.txt` are the check evidence.

3.1 stays NOT MET with the invariant rows; C74hj's row re-run on `b89e291b` reports
`libc-size check: 63 pass, 12 fail, 62 known, 101 skipped, 0 budget-only` (two more failures and
one fewer skip than the C74z-S17 baseline; the two new failures still need size-owner triage).
The exact budget diff and C74z-S17 table are in `tests/libc/size/TRIAGE.md` and the C74z-S17
report.
