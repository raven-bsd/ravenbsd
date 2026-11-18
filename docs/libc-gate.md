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

## Current verdict (2026-11-17, C74z-S16)

**NOT MET.** `cargo run -q -p libc-gate -- verdict` exits 1.

* MET: 1.1, 1.1.pack, 1.1.audit, 1.4, 1.5, 5.1, 5.2, 4.1, 4.3, 6.1, 6.3, 6.4.
* NOT MET: 1.2 (G1 0/4; C74x qemu backend C4 7/15, C5 12/18), 1.7 (15 lint violations:
  P1f 14, ABI 1), 3.1 (C74z-S16 reconciliation: 46 pass, 28 fail, 69 known, 105 skipped, 0
  budget-only; 28 legitimate-row +3% budgets applied; shrink-first/held/invariant rows remain),
  4.2 (one-hour `inet_text` soak recorded; row re-eval pending).
* BLOCKED: 1.6 (Miri), 2.1/2.2 (`tools/libc-world` and C72 outputs), 6.2 (`mandoc`).

The exact evidence and the seven-case matrix are in `libc/ravenlibc/GATE-REPORT.md` and under
`build/libc-gate/`. C74h re-ran the full gate on `beedfbbd`: the symbol audit is now MET (66/66,
exit 0); the host-test row now runs through `cargo nextest run`; `libc-dynamic`, `libc-signals`,
`libc-spawn` and `libc-threads` pass; `libc-hello` fails only the P1b `string` check (auxv ok);
`libc-stdio` and `libc-time` remain P1b. C74i then added the three missing loader SAFETY
comments: `libc-lint` now reports 26 violations, all outside the loader files (P1f/phil 14,
P1h/otto 8, P1i/tim 3, ABI/contract 1; C74l/C74i reduced that to 15, P1f 14 and ABI 1).
C74n closed 5.2.core-servers, C74v closed 5.2.file-tools with the real `/etc/mounts` list, and
C74u closed 5.1 (`sscanf`, `tzname`). C74q closed 1.4 (27/27) and C74k recorded the one-hour
`inet_text` soak. The gate cannot close until G1, the size-budget cluster, the remaining lint
backlog, and the missing Miri/world/mandoc tooling are resolved by their owners.

## 3.1 size triage (C74w/C74z/C74z-S16; OBSERVED on `1355d85c`)

`cargo xtask libc-size --check` reports `46 pass, 28 fail, 69 known, 105 skipped, 0 budget-only`
after the S14/S15a/S5/S6/S13 merges and the C74z-S16 reconciliation. The full three-arch rebuild
before the budget edit was `31 pass, 43 fail` (the directive's 32/50 last run lacked a full arch
build); `tests/libc/size/reconcile.py` compared the S12 before run with the rebuild. The 28
legitimate P0/P1/P4/P6 and x86_64/aarch64 P3 blocks were re-frozen at measured + 3 percent; the
P2/P5/P7/riscv64 P3 and P8/P9 blocks were left unchanged. The remaining 28 rows:

* **17 budget/rule rows:** P2 aarch64/riscv64 S/F (S14 rework), P3 riscv64 S/F/B (S14/S13),
  P5 x86_64 server / aarch64 server / riscv64 server B-only (S5 layout), P7 x86_64 tiny/server
  B-only, P7 aarch64/riscv64 S/F/B (S6), P8 aarch64 tiny/server B-only (held) and the
  `P7-R-14` iconv member rule.
* **10 invariants:** `INV-S-P2-P0` aarch64/riscv64 tiny/server and `INV-S-P3-P2` x86_64,
  aarch64 and riscv64 tiny/server. These are real rule failures, not budget rows; the P2/P3
  owners and the reviewer for the limit must decide.
* Boot rows pass; P9 remains green. S15a removed the `dtoa` floor, S6/S5 removed most P7/P5 S
  growth, S14 made the x86_64 P2 rows pass, and S13 now links the real riscv64 printf path.
* `B` is the Berkeley `llvm-size` bss field (all NOBITS, including `.relro_padding`), not only
  the `.bss` section. `build/size/libc-size.json`/`.md` are run artifacts and leave `budget_* = 0`
  and `status = ok`; the console and `build/libc-gate-c74z-s16/3.1.txt` are the check evidence.

3.1 stays NOT MET because the 17 budget/rule and 10 invariant rows do not fit. The exact
current/budget table and owners are in the C74z-S16 section of `tests/libc/size/TRIAGE.md` and
the C74z-S16 report.
