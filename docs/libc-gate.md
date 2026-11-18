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

## Current verdict (2026-11-18, C74hj audit on `b89e291b`; 3.1 re-evaluated by C74ia; C74ib size study)

**NOT MET.** `cargo run -q -p libc-gate -- verdict --out build/libc-gate-audit` exits 1.

* MET: 1.1, 1.1.pack, 1.1.audit, 1.4, 1.5, 1.6, 1.7, 4.1, 4.2, 4.3, 5.1, 5.2, 6.1, 6.2, 6.3, 6.4.
  5.1/5.2 are MET per the committed C74u/C74ae/C74n/C74v evidence; they were not re-run in this
  audit (no QEMU) and no local evidence file is present.
* NOT MET: 1.2 (G1 0/4; C2-C5 not run, C6 unvendored), 3.1 (C74ia on `fb65ec96`: 64 pass,
  13 fail, 65 known, 99 skipped, 0 budget-only; all three `libc.so.1` budgets pass, the three
  `INV-LIBC-SO-NEEDED` rows are known rule exceptions, and the 10 static invariants, the real
  aarch64 `libc.so.1` S ceiling and two riscv64 P9 static rows remain; C74ib found no safe
  in-Owns aarch64 reduction).
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
6.2/6.3 and 6.4 are MET; 1.2 and 3.1 remain NOT MET and 2.1/2.2 remain BLOCKED. The C74hj 3.1
count moved from 10 fail/102 skip to 12 fail/101 skip; C74ht then rebuilt the x86_64/aarch64
`libc.so.1` rows and reconciled the dynamic budgets on `6cb10821`: 65 pass / 11 fail / 64 known /
100 skipped. C74ia added the riscv64 `libc.so.1` budget and the third NEEDED known row on
`fb65ec96`: 64 pass / 13 fail / 65 known / 99 skipped. All three DSO budgets pass; the remaining
13 are the 10 static invariants, the real aarch64 `libc.so.1` S overrun, and two riscv64 P9
static S/F rows grown by the C74hs compat exports (not budgeted by C74ia). C74ib then tested
aarch64 DSO reductions on `07054226`: `--icf=all` alone reaches 1,306,012 (still 6,012 over),
and `--icf=all` + `opt-level=z` + `--strip-debug` reaches 1,255,136 / F 2,451,200 but needs
`xtask/src/libc_link.rs` and `Cargo.toml` profile changes outside Owns plus qtests. No safe
in-Owns reduction exists; the aarch64 S row remains. The gate cannot close until G1, the
invariant/S decision and the blocked world rows are resolved by their owners.

## 3.1 size triage (C74w/C74z/C74z-S17/C74ht/C74ia/C74ib; OBSERVED on `07054226`)

`cargo xtask libc-size --check` reports `64 pass, 13 fail, 65 known, 99 skipped, 0 budget-only`
after C74ia added the riscv64 `libc.so.1` budget. The 17 static budget/layout rows are the
C74z-S17 re-freeze; no invariant ceiling is changed.

* **Dynamic `libc.so.1` budgets:** all three pass. x86_64 S 1,286,594 / F 3,009,232 (budget
  1,325,192 / 3,099,509); aarch64 S 1,363,752 / F 2,501,104 (budget 1,404,682 / 2,576,138);
  riscv64 S 1,010,867 / F 2,490,256 (budget 1,041,194 / 2,564,964). All carry
  `SONAME libc.so.1`, `FLAGS BIND_NOW`, `FLAGS_1 NOW NODELETE` and **0 `DT_NEEDED`**.
* **`INV-LIBC-SO-NEEDED` rule bug:** the hard-coded rule expects `needed == 1`, but a
  self-contained `libc.so.1` has 0; `readelf -d` shows no self-NEEDED entry. The source is
  outside the ticket Owns (no source edits), so all three rows are reviewed known exceptions. A
  follow-up tooling ticket must change the expectation to 0; these rows are not passes.
* **C74ib aarch64 size study:** baseline S 1,363,752 / F 2,501,104; `.text` 1,013,616 and
  `.eh_frame` 167,204 dominate. `--icf=all` alone reaches 1,306,012 (still 6,012 over);
  `--icf=all` + `opt-level=z` reaches 1,255,136 but F grows to 2,893,064; adding `--strip-debug`
  gives F 2,451,200. The only S-passing safe-ish candidate needs `xtask/src/libc_link.rs` and
  `Cargo.toml`/`libc_pack` changes outside Owns plus qtests. `force-unwind-tables=no` (S
  1,200,188) is rejected because it removes `.eh_frame`/backtrace. The exact section, symbol,
  candidate and riscv P9 budget-review tables are in the C74ib TRIAGE section.
* **13 real fails remain:** the 10 `INV-S-P2-P0`/`INV-S-P3-P2` static invariants, the real
  aarch64 `INV-LIBC-SO-S` overrun (1,363,752 > 1,300,000; ceiling not raised), and the two
  riscv64 P9 static rows grown by the C74hs compat exports (tiny S 904,084 > 868,494 / F
  905,672 > 870,128; server S 964,226 > 924,842 / F 965,816 > 926,473). The P9 growth is
  permanent parity; the reviewer values are in TRIAGE, not applied.
* Boot rows pass. `B` is the Berkeley `llvm-size` bss field (all NOBITS, including
  `.relro_padding`), not only the `.bss` section. `build/size/libc-size.json`/`.md` are run
  artifacts and leave `budget_* = 0` and `status = ok`; the console and
  `build/libc-gate-c74ib/3.1.txt` are the check evidence.

3.1 stays NOT MET with exactly 13 real fails. The exact dynamic budget diff is in the C74ia
section of `tests/libc/size/TRIAGE.md`; the aarch64 candidate evidence and riscv P9 review values
are in the C74ib section. The C74ib report records the infeasibility inside Owns.
