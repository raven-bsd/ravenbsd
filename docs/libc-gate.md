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

## Current verdict (2026-11-18, C74z-S17; 1.6 refreshed by C74fx)

**NOT MET.** `cargo run -q -p libc-gate -- verdict` exits 1.

* MET: 1.1, 1.1.pack, 1.1.audit, 1.4, 1.5, 1.6, 5.1, 5.2, 4.1, 4.3, 6.1, 6.3, 6.4.
* NOT MET: 1.2 (G1 0/4; C74x qemu backend C4 7/15, C5 12/18), 1.7 (15 lint violations:
  P1f 14, ABI 1), 3.1 (C74z-S17: 63 pass, 10 fail, 62 known, 102 skipped, 0 budget-only;
  17 legitimate-row +3% budgets applied; only the 10 invariant ceilings remain),
  4.2 (one-hour `inet_text` soak recorded; row re-eval pending).
* BLOCKED: 2.1/2.2 (`tools/libc-world` and C72 outputs), 6.2 (`mandoc`).

The exact evidence and the seven-case matrix are in `libc/ravenlibc/GATE-REPORT.md` and under
`build/libc-gate/`. C74h re-ran the full gate on `beedfbbd`: the symbol audit is now MET (66/66,
exit 0); the host-test row now runs through `cargo nextest run`; `libc-dynamic`, `libc-signals`,
`libc-spawn` and `libc-threads` pass; `libc-hello` fails only the P1b `string` check (auxv ok);
`libc-stdio` and `libc-time` remain P1b. C74i then added the three missing loader SAFETY
comments: `libc-lint` now reports 26 violations, all outside the loader files (P1f/phil 14,
P1h/otto 8, P1i/tim 3, ABI/contract 1; C74l/C74i reduced that to 15, P1f 14 and ABI 1).
C74n closed 5.2.core-servers, C74v closed 5.2.file-tools with the real `/etc/mounts` list, and
C74u closed 5.1 (`sscanf`, `tzname`). C74q closed 1.4 (27/27) and C74k recorded the one-hour
`inet_text` soak. C74cw wired row 1.6 to the bounded Miri runner and C74fx re-ran
`cargo run -q -p libc-gate -- eval --only 1.6 --out build/libc-gate-c74fx`: exit 0, **MET**
(721 Miri tests: 715 library + 6 alignment, 6/6 pass, explicit integration exclusions). The gate
cannot close until G1, the size-budget cluster, the remaining lint backlog, and the missing
world/mandoc tooling are resolved by their owners.

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

3.1 stays NOT MET with exactly the 10 invariant rows. The exact budget diff and current/budget
table are in the C74z-S17 section of `tests/libc/size/TRIAGE.md` and the C74z-S17 report.
