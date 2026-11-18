<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# RavenBSD 0.1.0 libc gate 3.1 — invariant decision paper

- Date: 2026-11-18
- Product base: `10d8c9dd` (merge of `origin/lance/C74dl`)
- Author: mark (size/3.1 audit)
- Status: **3.1 NOT MET.** This is a decision paper, not a gate claim and not a rule edit.
- Scope: the 10 `INV-S-P2-P0`/`INV-S-P3-P2` rows. No QEMU; no tag.

## Decision requested

The reviewer and owner must choose one of:

- **A. Approve a reviewed ceiling update** with the measured values below (global measured+3% or
  per-arch/set measured+3%), so the 10 invariant rows pass while the invariants remain regression
  detectors; the size lane implements the rule/BUDGETS change in a separate approved action.
- **B. Keep the current 5,120/10,240 ceilings**: the 10 rows stay FAIL, 3.1 stays NOT MET, and
  0.1.0 remains NO-GO unless the P2/P3 paths are shrunk below the old ceilings.

`cargo xtask libc-size --check` on `10d8c9dd`: **46 pass, 30 fail, 62 known, 101 skipped, 0
budget-only**. The 10 invariants are 10 of the 30 fails. The C74dl audit classified the
`94d35d15` residual as 17 budget + 1 rule + 10 invariants; the current base has two additional
non-invariant rows from later merges, outside this paper's decision.

## The 10 invariant rows (measured on `10d8c9dd`)

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

**Release consequence of A:** the 10 invariant FAIL rows pass, but **3.1 remains NOT MET** until
the other current size rows (17 budget + `P7-R-14` + the two extra rows on this base) are
re-frozen/matcher-fixed, and until the other libc-gate rows (`1.2`, `1.6`, `2.1/2.2`) close.
The 0.1.0 release remains **NO-GO** for POSIX, libc, kernel, qtests, images and the Pi 4 board;
the tag is not issued by this decision.

## Option B: keep NOT MET

Leave the hard-coded 5,120/10,240 ceilings. The 10 rows stay FAIL; 3.1 stays **NOT MET**; the
0.1.0 tag stays withheld. Passing under the old ceilings would require shrinking the legitimate
integer printf/float plug-in paths by ~1-2× (or adding a special build split), which is a code
project, not a policy decision. The owner/reviewer would explicitly accept the NO-GO.

## Exact approval needed

- **Reviewer** (size rules/allowlist and architecture): approve A1 or A2 and the concrete
  ceiling values, and the cleanup of the x86_64 P2-P0 KNOWN rows.
- **Owner** (Bo Maryniuk; release/tag policy): accept the ceiling update (A) or the continued
  NO-GO (B). The owner's decision is the release-policy decision; the reviewer's is the size
  rule approval.
- **Size lane (lance/C74dl)** implements the approved change in
  `tools/libc-size/src/rules.rs` (and the BUDGETS/known rows if A1), then re-runs
  `cargo xtask libc-size --check`.
- **3.1 gate owner (carl/C74c/lane)** records the 3.1 row; the release evidence packet is
  regenerated after the gate is MET. This paper edits no rule, BUDGET, source or gate file; no
  minion may treat it as the approval.

## Reproduction

```sh
cargo xtask libc-size --check       # 46 pass, 30 fail, 62 known, 101 skipped; the 10 rows above
sed -n '288,310p' tools/libc-size/src/rules.rs
```

C74dl's audit (`tickets/lance/reports/C74dl.md`, `tests/libc/size/TRIAGE.md`) has the
`94d35d15` classification and the `P7-R-14` false-positive note; the current measured values
differ by at most ~200 B on the aarch64/riscv server rows. No QEMU or board run is involved.
