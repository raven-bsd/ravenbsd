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

## Current verdict (2026-11-17, C74w)

**NOT MET.** `cargo run -q -p libc-gate -- verdict` exits 1.

* MET: 1.1, 1.1.pack, 1.1.audit, 1.4, 1.5, 5.1, 5.2, 4.1, 4.3, 6.1, 6.3, 6.4.
* NOT MET: 1.2 (G1 0/4; C74x qemu backend C4 7/15, C5 12/18), 1.7 (15 lint violations:
  P1f 14, ABI 1), 3.1 (C74w triage: 5 pass, 58 fail, 69 known, 105 skipped, 0 budget-only),
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

## 3.1 size triage (C74w, OBSERVED on `19b6f229`)

`cargo xtask libc-size --check` reports `5 pass, 58 fail, 69 known, 105 skipped, 0 budget-only`
(not the C74h `4/48/25/282/40`). The full 58-row table, deltas and per-row owners are in
`tests/libc/size/TRIAGE.md`. The classification:

* 54 budget overruns, 52 with real `S`/`F` growth since the C69 freeze (x86_64 small,
  aarch64/riscv64 3-12 percent; P9 full link +306k/+322k and +141k/+159k `S`). Two x86_64 P5
  rows are B-only. Two more fails are the missing x86_64 P9 budgets and two are boot rows.
* The aarch64/riscv64 B budgets are stale: the old B budget is 6.9k/7.8k versus 14.3k on x86_64
  for the same source, while the current `.bss` section is 11,392 on all three. Re-freezing that
  baseline is a budget-owner decision, not a C74w edit.
* x86_64 P9 tiny/server have **no budget** at all; aarch64/riscv64 P9 do. The harness reports
  `no budget` as FAIL. The budget owner must set or explicitly waive the full-link envelope.
* Boot: x86_64 start instructions 1,245 > 908; aarch64 syscalls 8 > 3 and instructions 3,403 >
  1,847. Real start/TLS/allocator growth (dave/C57, kevin/C49).
* `B` is the Berkeley `llvm-size` bss field (all NOBITS, including `.relro_padding`), not only
  the `.bss` section. `build/size/libc-size.json`/`.md` are run artifacts and leave `budget_* = 0`
  and `status = ok`; the console and `build/libc-gate-c74w/3.1.txt` are the check evidence.

No budget or libc body is changed by C74w; 3.1 stays NOT MET. Shrink candidates with the largest
current symbols are the ctype Unicode tables (carl/C39), fmt `vformat` monomorphizations
(carl/C43), dtoa (carl/C42), the compat static buffers (carl/C64d), resolv/net (dave/C51), time
(mel/C44) and the TLS/pages/streams statics (dave/C57/C54, carl/C64d, kevin/C49).
=======
P1h/otto 8, P1i/tim 3, ABI/contract 1). C74n closed 5.2.core-servers with a minimal profile; C74v
implemented the RavenBSD `read_fs_list` from the real `/etc/mounts` table, so `df` reports the
mounted filesystems and 5.2.file-tools passes too. Row 5.2 is now MET. The gate cannot close
until the size-budget lane, the G1 donor data, the `inet_text` soak, the three P1b target cases,
the lint backlog, and the missing Miri/world/mandoc tooling are resolved by their owners.
>>>>>>> origin/carl/C74v
