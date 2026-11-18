<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# RavenBSD 0.1.0 libc gate 3.1 — invariant decision paper

- Date: 2026-11-18 (C74hw refresh)
- Product base: `6cb10821` (origin/master; C74hp/C74hr measured here)
- Authors: mark (C74du original), jerry (C74hw refresh)
- Status: **3.1 NOT MET.** This is a decision paper, not a gate claim and not a rule edit.
- Scope: the 10 `INV-S-P2-P0`/`INV-S-P3-P2` invariants, the `libc.so.1` dynamic rows exposed by
  C74hp/C74hr, and the riscv64 blocker (C74hs). No QEMU; no tag.
- Evidence: counts and measured values are **OBSERVED** from the commands in Reproduction on
  `6cb10821`; the A1/A2 ceilings are **PROPOSED** and not applied.

## Decision requested

Two decisions are needed; the gate stays NOT MET until both and the other libc-gate rows close.

1. **The ten static invariants** (the C74du decision):
   - **A. Approve a reviewed ceiling update** with the measured values below (A1 global
     measured+3% or A2 per-arch/set measured+3%), so the 10 invariant rows pass while the
     invariants remain regression detectors; the size lane implements the rule change in a
     separate approved action.
   - **B. Keep the current 5,120/10,240 ceilings**: the 10 rows stay FAIL, 3.1 stays NOT MET, and
     0.1.0 remains NO-GO unless the P2/P3 paths are shrunk below the old ceilings.
2. **The dynamic `libc.so.1` rows** (new in C74hp/C74hr):
   - **D1. Keep the dynamic libc out of the 3.1 count** until C62/C74hs lands: state that the gate
     measures the C62 state (no `build/<arch>/libc.so.1`), where the dynamic rows skip and only
     the ten invariants fail.
   - **D2. Put the dynamic libc under the gate now**: approve real `libc_so` S/F budgets, resolve
     the `DT_NEEDED` semantic (`INV-LIBC-SO-NEEDED` = 1 vs `libc-link` requiring 0), address the
     aarch64 S row, and have C74hs enable or explicitly waive riscv64.

`cargo xtask libc-size --check` on `6cb10821` (C74hp/C74hr/C74hw):

- with the C74hr refreshed artifacts in `build/`: **63 pass, 15 fail, 62 known, 100 skipped, 0
  budget-only** (the ten invariants + five dynamic rows);
- in the C62 state (no `build/<arch>/libc.so.1`): **63 pass, 10 fail, 62 known, 102 skipped, 0
  budget-only** (exactly the ten invariants).

The C74du baseline (`10d8c9dd`: 46 pass, 30 fail, 62 known, 101 skipped) is historical. C74z-S17
re-froze the 17 legitimate budget rows and C74cx fixed `P7-R-14`; on `6cb10821` the only static
fails are the ten invariants. The dynamic rows appear only when `build/` contains linked
`libc.so.1` artifacts, which is why C74du did not see them.

## The 10 invariant rows (measured on `6cb10821`; unchanged from C74du)

The ceilings are hard-coded at `tools/libc-size/src/rules.rs:298` (`INV-S-P2-P0` = 5,120) and
`:300` (`INV-S-P3-P2` = 10,240). Both were `PROPOSED` stub-era values, not frozen measured
budgets.

| # | Invariant | Arch | Set | Measured ΔS | Ceiling | Over | Legitimate contributor |
|---|---|---|---:|---:|---:|---:|---|
| 1 | `INV-S-P2-P0` | aarch64 | tiny | 26,671 | 5,120 | +21,551 | integer printf core |
| 2 | `INV-S-P2-P0` | aarch64 | server | 29,159 | 5,120 | +24,039 | integer printf core |
| 3 | `INV-S-P2-P0` | riscv64 | tiny | 19,075 | 5,120 | +13,955 | integer printf core |
| 4 | `INV-S-P2-P0` | riscv64 | server | 23,421 | 5,120 | +18,301 | integer printf core |
| 5 | `INV-S-P3-P2` | x86_64 | tiny | 17,160 | 10,240 | +6,920 | one dtoa + float layout |
| 6 | `INV-S-P3-P2` | x86_64 | server | 23,104 | 10,240 | +12,864 | one dtoa + float layout |
| 7 | `INV-S-P3-P2` | aarch64 | tiny | 16,964 | 10,240 | +6,724 | one dtoa + float layout |
| 8 | `INV-S-P3-P2` | aarch64 | server | 18,272 | 10,240 | +8,032 | one dtoa + float layout |
| 9 | `INV-S-P3-P2` | riscv64 | tiny | 10,804 | 10,240 | +564 | one dtoa + float layout |
| 10 | `INV-S-P3-P2` | riscv64 | server | 12,400 | 10,240 | +2,160 | one dtoa + float layout |

For context, the x86_64 `INV-S-P2-P0` rows are already `KNOWN` (grandfathered) at 22,048 tiny /
27,552 server vs the same 5,120 ceiling. The policy already accepts the real x86_64 P2-P0 path;
only the aarch64/riscv P2-P0 and the six P3-P2 rows are FAIL.

## Contributing code paths

- **`INV-S-P2-P0` (P2 `%d` minus P0 floor).** P2 links one real stdio core
  (`raven-libc-stdio`: streams/FILE/output), one `raven-libc-fmt` integer `vformat_dyn`
  (3,244 B on aarch64 tiny per C74dl) and the shell vprintf/Va glue; P0 is the bare floor and
  has none of those. C74dl verified exactly one `vformat_dyn` and no dtoa in P2. The resulting
  19-29 KB delta is the legitimate integer-printf stack, not an over-link. The 5,120 ceiling
  predates the real printf path.
- **`INV-S-P3-P2` (P3 `%f` minus P2).** P3 adds exactly one strong float plug-in:
  `__rl_fmt_float_impl` → `raven-libc-dtoa` (8,418 B x86_64, 6,248 B aarch64, 4,906 B riscv64)
  plus `raven-libc-fmt` float layout (`float_layout::emit`/`FloatConv`) and the `vprintf`
  FloatHook glue. C74dl verified the real dtoa path; the 10.8-23.1 KB delta is one plug-in, not
  a duplicate. The 10,240 ceiling is below the legitimate one-dtoa delta on every arch (even
  riscv64 tiny is 564 B over).

## What an over-link regression looks like

- **P2-P0 regression:** P2 pulls a P0-forbidden subsystem (`dtoa*`, `fmt_float*`, `locale*`,
  `malloc*`, a second `stdio*`/`vformat_dyn`, `resolv*`, `regex*`, `iconv*`), or a second copy
  of the stdio/integer engine. The map/member set shows the extra subsystem and the delta jumps
  by its size (a second stdio core or dtoa plug-in is several KB). A reviewed ceiling at
  measured+3% still catches that jump: a new subsystem is larger than the 3% headroom.
- **P3-P2 regression:** P3 pulls a second dtoa/float path (the whole dtoa archive instead of the
  one plug-in, a duplicate `float_layout`/`vformat_dyn`, or locale/time code), or drops the
  `--gc-sections` separation that keeps the integer and float paths single-copy. The map shows
  duplicate `dtoa`/`float_layout`/`vformat_dyn` members or the extra crates; the delta jumps by
  another dtoa-sized block (5-10+ KB), beyond a 3% headroom.

The invariants are **relative** checks (P2 vs P0, P3 vs P2), so they remain useful regression
detectors even after an absolute data/stdio growth: a new copy or subsystem still changes the
difference.

## Dynamic `libc.so.1` rows (C74hp/C74hr)

C74hp found one stale x86_64 `libc.so.1` (built before the ticket, `DT_NEEDED = 0`); C74hr
refreshed x86_64 and aarch64 with `cargo xtask libc-link --arch ...`. The refreshed artifacts are
x86_64 3,009,232 B and aarch64 2,501,104 B, both `SONAME libc.so.1`, `DT_NEEDED = 0`; riscv64
produces no artifact (C74hs). The x86_64 refresh did not clear the C74hp rows: they still fail
for the rule/budget reasons below. With the artifacts present, `libc-size --check` adds five
rows:

| # | Row | Arch | Measured / limit | Class |
|---|---|---|---|---|
| 11 | `INV-LIBC-SO-NEEDED` | x86_64 | 0 / 1 | rule/artifact conflict |
| 12 | `INV-LIBC-SO-NEEDED` | aarch64 | 0 / 1 | rule/artifact conflict |
| 13 | `INV-LIBC-SO-S` | aarch64 | 1,363,768 / 1,300,000 | real size invariant |
| 14 | `libc.so.1` budget | x86_64 | S 1,286,594 / F 3,009,232, budget 0 | missing budget |
| 15 | `libc.so.1` budget | aarch64 | S 1,363,768 / F 2,501,104, budget 0 | missing budget |

- **Rows 11/12 are a rule conflict, not a code regression.** `tools/libc-size/src/rules.rs:382`
  requires `DT_NEEDED == 1` on the `libc.so.1` row, while `xtask/src/libc_link.rs` (`gate_text`,
  lines 663-665) fails the ELF gate if the libc has any `DT_NEEDED`; the link adds no dependency
  library, so the artifact is `NEEDED = 0` by construction. One of the two must change.
- **Rows 14/15 have no budget.** `tests/libc/size/BUDGETS.toml` `[[libc_so]]` entries carry
  `budget_s = 0`, `budget_f = 0` and `status = "skipped: libc.so.1 absent (C62 not landed)"`;
  the check ignores the status, so any present artifact fails the `budget 0` row.
- **Row 13 is a real result.** The fresh aarch64 `libc.so.1` S is 1,363,768 > the 1,300,000
  `INV-LIBC-SO-S` ceiling (x86_64 1,286,594 passes). This needs an approved ceiling/budget or a
  code shrink; it is not artifact staleness.
- **The dynamic `P*` probe rows stay skipped.** `tools/libc-size/src/main.rs:278` tests
  `libc.so.1` in the per-set staging dir, and `prepare_set` copies only the static set (`libc.a`,
  crt objects), so the refreshed artifacts feed only the `libc_so` rows: 30 dynamic-server rows
  are still skipped.

### riscv64 blocker (C74hs, kevin)

`cargo xtask libc-link --arch riscv64` exits 1 with `libc.so.1: FAILED` and `version script
assignment ... symbol not defined`. The complete gap is 151 symbols listed in
`build/riscv64/Symbols.list` but absent from `build/riscv64/libc.a`; all are the `compat` shell.
Root cause: `libc/ravenlibc/crates/raven-libc/src/lib.rs:41` gates the module to
`#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]`, while the symbol map lists the
exports for every arch. C74hs (kevin) owns the fix: enable the generic compat exports for
riscv64 after checking each module for arch-specific asm/syscalls, or explicitly mark riscv64
dynamic libc unsupported with a gate rule and the exact missing primitive. Until then the
riscv64 `libc.so.1` row is correctly skipped; no symbol may be faked.

## Option A: reviewed ceiling update

Two implementation choices; the reviewer/owner picks one.

- **A1 — global measured+3%:** one limit per invariant:
  `INV-S-P2-P0` = **30,034** (max 29,159 + 3%), `INV-S-P3-P2` = **23,798** (max 23,104 + 3%).
  Two constants in `rules.rs`; simple; the x86_64 P2-P0 KNOWN rows fall under the new limit and
  can be cleaned up in the same size-lane action. Less sensitive for the smaller rows (riscv64
  P2-P0 gets ~11 KB headroom).
- **A2 — per arch/set measured+3% (preferred for sensitivity):**

| Invariant | Arch | Set | Measured | Proposed ceiling (+3%) |
|---|---|---|---:|---:|
| `INV-S-P2-P0` | aarch64 | tiny | 26,671 | 27,472 |
| `INV-S-P2-P0` | aarch64 | server | 29,159 | 30,034 |
| `INV-S-P2-P0` | riscv64 | tiny | 19,075 | 19,648 |
| `INV-S-P2-P0` | riscv64 | server | 23,421 | 24,124 |
| `INV-S-P3-P2` | x86_64 | tiny | 17,160 | 17,675 |
| `INV-S-P3-P2` | x86_64 | server | 23,104 | 23,798 |
| `INV-S-P3-P2` | aarch64 | tiny | 16,964 | 17,473 |
| `INV-S-P3-P2` | aarch64 | server | 18,272 | 18,821 |
| `INV-S-P3-P2` | riscv64 | tiny | 10,804 | 11,129 |
| `INV-S-P3-P2` | riscv64 | server | 12,400 | 12,772 |

  A2 needs a per-row limit table in `rules.rs` (the current code has one limit per invariant
  name); it preserves the tight riscv64/aarch64 P3-P2 detection. The x86_64 P2-P0 KNOWN rows can
  remain known or receive their own measured+3% rows (22,710 / 28,379) in the same action.

**Release consequence of A:** the 10 invariant FAIL rows pass, but **3.1 remains NOT MET** while
the dynamic rows (11-15) or the C74hs riscv64 gap are open, and until the other libc-gate rows
(`1.2`, `2.1/2.2`; 1.6/1.7 are MET) close. If the owner also takes D1 (measure the C62 state),
the size row is green for the static set but the riscv64 dynamic libc remains undelivered.
The 0.1.0 release remains **NO-GO** for POSIX, libc, kernel, qtests, images and the Pi 4 board;
the tag is not issued by this decision.

**If A is approved without D2**, the gate still sees 15 fails whenever `build/` holds the linked
artifacts; the owner should therefore state which build state the 3.1 row measures. **If D2 is
approved**, the size lane must add real `libc_so` budgets and settle the `DT_NEEDED` rule in the
same action, and the aarch64 S row needs the approved ceiling or a shrink.

## Option B: keep NOT MET

Leave the hard-coded 5,120/10,240 ceilings and the zero `libc_so` budgets. The 10 rows stay
FAIL; with linked artifacts the five dynamic rows also fail (15 total); 3.1 stays **NOT MET**;
the 0.1.0 tag stays withheld. Passing under the old ceilings would require shrinking the
legitimate integer printf/float plug-in paths by ~1-2× (or adding a special build split), which
is a code project, not a policy decision. The owner/reviewer would explicitly accept the NO-GO.

## Exact approval needed

- **Reviewer** (size rules/allowlist and architecture): approve A1 or A2 and the concrete
  ceiling values, the D1/D2 dynamic-row treatment, the `INV-LIBC-SO-NEEDED` semantic, the
  aarch64 `INV-LIBC-SO-S` row, and the cleanup of the x86_64 P2-P0 KNOWN rows.
- **Owner** (Bo Maryniuk; release/tag policy): accept the ceiling update (A) or the continued
  NO-GO (B), and D1 or D2. The owner's decision is the release-policy decision; the reviewer's
  is the size rule approval.
- **Size lane (lance/C74dl)** implements the approved change in
  `tools/libc-size/src/rules.rs` and `tests/libc/size/BUDGETS.toml` (rules, `libc_so` budgets,
  known rows), then re-runs `cargo xtask libc-size --check`.
- **kevin/C74hs** enables the riscv64 compat shell or explicitly records the unsupported
  dynamic-libc waiver with the exact missing primitive; no faked symbols.
- **3.1 gate owner (carl/C74c/lane)** records the 3.1 row; the release evidence packet is
  regenerated after the gate is MET. This paper edits no rule, BUDGET, source or gate file; no
  minion may treat it as the approval.

## Reproduction

```sh
# C62 state (no build/<arch>/libc.so.1): exactly the ten invariants
cargo xtask libc-size --check       # 63 pass, 10 fail, 62 known, 102 skipped
# after the C74hr refresh (x86_64 + aarch64 artifacts present): adds the five dynamic rows
cargo xtask libc-pack --arch x86_64
cargo xtask libc-pack --arch aarch64
cargo xtask libc-link --arch x86_64
cargo xtask libc-link --arch aarch64
cargo xtask libc-size --check       # 63 pass, 15 fail, 62 known, 100 skipped
# riscv64 still fails until C74hs:
cargo xtask libc-link --arch riscv64   # exit 1: 151 compat symbols
sed -n '288,310p' tools/libc-size/src/rules.rs
```

C74dl's audit (`tickets/lance/reports/C74dl.md`, `tests/libc/size/TRIAGE.md`) has the
`94d35d15` classification and the `P7-R-14` false-positive note. C74hp/C74hr
(`tickets/jerry/reports/C74hp.md`, `C74hr.md`) produced the dynamic rows and the 151-symbol
riscv64 gap; C74hs (kevin) owns that fix. The ten invariant values match the C74du table on
`10d8c9dd`. No QEMU or board run is involved.
