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

## Current verdict (2026-11-18, C74da round 3 on `02152587`)

**NOT MET.** `cargo run -q -p libc-gate -- verdict --out build/libc-gate` exits 1.

* MET: 1.1, 1.1.pack, 1.1.audit, 1.4, 1.5, 1.6, 1.7, 4.1, 4.2, 4.3, 6.1, 6.2, 6.3, 6.4.
  1.4 is the 27/27 oracle run with `RAVENBSD_RESOURCES` and the C74ij `strftime %s` fix.
* NOT MET: 1.2 (G1 **1/4**: C2 51.7 %, C3 34.4 %, C4 40.0 %, C5 94.7 % MET, C6 unvendored),
  3.1 (63 pass, 13 fail, 65 known, 99 skipped, 0 budget-only; all three `libc.so.1` budgets
  pass, the 10 static invariants, the P7 riscv64 server B row and two riscv64 P9 static rows
  remain; no budget raised).
* BLOCKED: 2.1/2.2 (`results/x86_64/world.json`, H-11 tarballs and H-12 freeze absent).
* STALE this round: 5.1, 5.3 (no QEMU; the 24 h soak owns it). The committed C74u/C74ae
  evidence remains green but was not re-run; 5.2 was re-run and is MET (C74n: 31.3 s
  `core-servers`, 53.3 s `file-tools`, both exit 0).

The exact evidence and the seven-case matrix are in `libc/ravenlibc/GATE-REPORT.md` and under
`build/libc-gate/`. C74h re-ran the full gate on `beedfbbd`: the symbol audit is now MET (66/66,
exit 0); the host-test row now runs through `cargo nextest run`; `libc-dynamic`, `libc-signals`,
`libc-spawn` and `libc-threads` pass; `libc-hello` failed only the P1b `string` check (auxv ok);
`libc-stdio` and `libc-time` remained P1b at that point (C74u/C74ae later closed them; 5.1 is MET
per the committed evidence). C74i then added the three missing loader SAFETY
comments: `libc-lint` now reports 26 violations, all outside the loader files (P1f/phil 14,
P1h/otto 8, P1i/tim 3, ABI/contract 1; C74l/C74i reduced that to 15, P1f 14 and ABI 1).
C74n re-ran 5.2 on `6af246ac`: `core-servers` passes in 31.3 s and `file-tools` in 53.3 s,
both in-tree and with `tooltest: ok`; C74v added the real `/etc/mounts` list and C74u closed
5.1 (`sscanf`, `tzname`). C74ij reconciled `strftime %s`; C74da round 3 re-ran 1.4
on `02152587` with `RAVENBSD_RESOURCES` (27/27 MET) and C74k recorded the one-hour
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
`fb65ec96`: 64 pass / 13 fail / 65 known / 99 skipped. C74da round 3 re-ran the full gate on
`02152587`: **63 pass / 13 fail / 65 known / 99 skipped**; all three DSO budgets pass, and the
13 are the 10 static invariants, the P7 riscv64 server **B** row and two riscv64 P9 static S/F
rows. C74ib's aarch64 DSO study is historical (C74id/C74ij brought aarch64 S to 990,615); no
budget was raised. C74in's full three-arch rebuild on `d0429e95` confirms 63/13 and attributes
the P7 riscv64 server B row to `.relro_padding`, not `.bss`: `.bss` is 11,552 in both builds,
while C74ij's +246-byte `.text` moved the RELRO start across the `0x22000` page boundary and
added 3,848 bytes of padding. Owner otto/C74ij; the 15,953 B budget option is not applied. The
gate cannot close until G1, the invariant/P7-riscv B decision and the blocked world rows are
resolved by their owners.

## 3.1 size triage (C74da round 3 on `02152587`; C74w/C74z/C74z-S17/C74ht/C74ia/C74ib historical)

`cargo xtask libc-size --check` reports **63 pass, 13 fail, 65 known, 99 skipped, 0
budget-only** on `02152587`. The 17 static budget/layout rows are the C74z-S17 re-freeze; no
invariant ceiling is changed.

* **Dynamic `libc.so.1` budgets:** all three pass. x86_64 S 1,290,274 / F 3,015,272 (budget
  1,325,192 / 3,099,509); aarch64 S 990,615 / F 1,851,600 (budget 1,404,682 / 2,576,138);
  riscv64 S 765,115 / F 1,867,768 (budget 1,041,194 / 2,564,964). All carry
  `SONAME libc.so.1`, `FLAGS BIND_NOW`, `FLAGS_1 NOW NODELETE` and **0 `DT_NEEDED`**.
* **`INV-LIBC-SO-NEEDED` rule bug:** the hard-coded rule expects `needed == 1`, but a
  self-contained `libc.so.1` has 0; `readelf -d` shows no self-NEEDED entry. The source is
  outside the ticket Owns (no source edits), so all three rows are reviewed known exceptions. A
  follow-up tooling ticket must change the expectation to 0; these rows are not passes.
* **Current 13 fails:** the 10 static `INV-S-P2-P0`/`INV-S-P3-P2` invariants, the P7 riscv64
  server B row (15,488 > 12,385), and the two riscv64 P9 static S/F rows.
* **C74in P7 B attribution:** `.bss` is 11,552 in both the pre-C74ij and current p7 builds; the
  current B 15,488 is 11,552 + 3,936 `.relro_padding` (pre-C74ij: 11,552 + 88). C74ij's
  `strftime.rs` `%s` branch (lines 167-190) grew `.text` 58,110 -> 58,356, moving GNU_RELRO
  from `0x21698` to `0x21790` and its page end from `0x22000` to `0x23000`. Owner otto/C74ij;
  this is a layout side effect, not a static buffer. The measured + 3 percent B option is
  **15,953**; applying it gives 64 pass / 12 fail. No budget is changed.
* **C74ib aarch64 size study (historical):** baseline S 1,363,752 / F 2,501,104; `.text`
  1,013,616 and `.eh_frame` 167,204 dominate. `--icf=all` alone reaches 1,306,012 (still 6,012
  over); `--icf=all` + `opt-level=z` reaches 1,255,136 but F grows to 2,893,064; adding
  `--strip-debug` gives F 2,451,200. The only S-passing safe-ish candidate needed
  `xtask/src/libc_link.rs` and `Cargo.toml`/`libc_pack` changes outside Owns plus qtests; C74id
  later landed the DSO-only change. `force-unwind-tables=no` (S 1,200,188) is rejected because
  it removes `.eh_frame`/backtrace. The exact section, symbol, candidate and riscv P9 budget
  tables are in the C74ib TRIAGE section.
* **13 real fails remain:** the 10 `INV-S-P2-P0`/`INV-S-P3-P2` static invariants, the P7
  riscv64 server B row (`.relro_padding` layout, attributed above), and the two riscv64 P9
  static rows (current tiny S 906,278 > 868,494 / F 907,864 > 870,128; server S 966,556 >
  924,842 / F 968,144 > 926,473; permanent C74hs parity; reviewer values in TRIAGE, not
  applied). The aarch64 `INV-LIBC-SO-S` row passes at S 990,615 (C74id).
* Boot rows pass. `B` is the Berkeley `llvm-size` bss field (all NOBITS, including
  `.relro_padding`), not only the `.bss` section. `build/size/libc-size.json`/`.md` are run
  artifacts and leave `budget_* = 0` and `status = ok`; the console and
  `build/libc-gate-c74in/3.1.txt` are the check evidence.

3.1 stays NOT MET with exactly 13 real fails. The exact 13-row table and P7 attribution are in
the C74in section of `tests/libc/size/TRIAGE.md`; the dynamic budget diff is in C74ia and the
riscv P9 review values are in C74in.
