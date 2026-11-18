<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# RavenBSD 0.1.0 libc gate 3.1 — invariant decision paper

- Date: 2026-11-18 (C74io refresh)
- Product base: `f0420ac7` (origin/master; C74in/C74ij/C74da merged)
- Authors: mark (C74du original), jerry (C74hw/C74ih/C74io refresh)
- Status: **3.1 NOT MET.** This is a decision paper, not a gate claim and not a rule edit.
- Scope: the ten `INV-S-P2-P0`/`INV-S-P3-P2` invariants, the P7 riscv64 server B row (C74in), the
  two riscv64 P9 static rows, and the resolved dynamic `libc.so.1` rows. No QEMU; no tag.
- Evidence: counts and measured values are **OBSERVED** from the commands in Reproduction on
  `f0420ac7`; the A1/A2/C1/P7 ceilings are **PROPOSED** and not applied.

## Decision requested

The dynamic libc rows are resolved: all three DSO budgets PASS, the three `INV-LIBC-SO-NEEDED`
rows are KNOWN rule exceptions, and riscv64 compat is closed. Three decisions remain:

1. **The ten static invariants:** **A.** approve a reviewed ceiling update (A1 global or A2
   per-arch/set measured+3%, below) so the rows pass as regression detectors; the size lane
   implements the rule change in a separate approved action. **B.** keep the current
   5,120/10,240 ceilings: the ten rows stay FAIL and 3.1 stays NOT MET.
2. **The two riscv64 P9 static rows** grown by the C74hs compat exports: **C1.** freeze a
   reviewed measured+3% budget; **C2.** keep them FAIL and require a code shrink.
3. **The P7 riscv64 server B row** (`.relro_padding`, C74in): **D1.** fix the layout (shrink the
   C74ij `%s` `.text` branch or realign RELRO); **D2.** approve the measured+3% B budget
   **15,953**.

`cargo xtask libc-size --check` on `f0420ac7` after the three-arch pack/link:
**63 pass, 13 fail, 65 known, 99 skipped, 0 budget-only**. The 13 fails are the ten invariants,
the P7 riscv64 server B row and the two riscv64 P9 rows. **Approving A1/A2 + C1 + D1/D2 closes all
13 size rows**; the libc gate still needs 1.2 and 2.1/2.2 (1.4/1.6/1.7 are MET).

The historical C74du/C74hw/C74ih counts are superseded: riscv64 now links and is budgeted, the
aarch64 DSO is under the unchanged S ceiling, the NEEDED rows are rule exceptions, and C74ij's
`%s` fix added the one P7 RELRO row.

## The 10 invariant rows (measured on `f0420ac7`; C74du values otherwise unchanged)

The ceilings are hard-coded at `tools/libc-size/src/rules.rs:298` (`INV-S-P2-P0` = 5,120) and
`:300` (`INV-S-P3-P2` = 10,240). Both were `PROPOSED` stub-era values, not frozen measured
budgets.

| # | Invariant | Arch | Set | Measured ΔS | Ceiling | Over | Legitimate contributor |
|---|---|---|---:|---:|---:|---:|---|
| 1 | `INV-S-P2-P0` | aarch64 | tiny | 26,671 | 5,120 | +21,551 | integer printf core |
| 2 | `INV-S-P2-P0` | aarch64 | server | 29,159 | 5,120 | +24,039 | integer printf core |
| 3 | `INV-S-P2-P0` | riscv64 | tiny | 19,067 | 5,120 | +13,947 | integer printf core |
| 4 | `INV-S-P2-P0` | riscv64 | server | 23,429 | 5,120 | +18,309 | integer printf core |
| 5 | `INV-S-P3-P2` | x86_64 | tiny | 17,160 | 10,240 | +6,920 | one dtoa + float layout |
| 6 | `INV-S-P3-P2` | x86_64 | server | 23,088 | 10,240 | +12,848 | one dtoa + float layout |
| 7 | `INV-S-P3-P2` | aarch64 | tiny | 16,964 | 10,240 | +6,724 | one dtoa + float layout |
| 8 | `INV-S-P3-P2` | aarch64 | server | 18,272 | 10,240 | +8,032 | one dtoa + float layout |
| 9 | `INV-S-P3-P2` | riscv64 | tiny | 10,804 | 10,240 | +564 | one dtoa + float layout |
| 10 | `INV-S-P3-P2` | riscv64 | server | 12,392 | 10,240 | +2,152 | one dtoa + float layout |

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

## Dynamic `libc.so.1` rows (resolved)

The C74hp/C74hr open rows are resolved; the three-arch pack/link on `f0420ac7` produces:

| Row | Arch | Measured | Budget / ceiling | Status |
|---|---|---:|---|---|
| `libc.so.1` | x86_64 | S 1,286,570 / F 3,009,208 | 1,325,192 / 3,099,509 | PASS |
| `libc.so.1` | aarch64 | S 1,270,220 / F 1,929,968 | 1,404,682 / 2,576,138 | PASS |
| `libc.so.1` | riscv64 | S 1,010,867 / F 2,490,256 | 1,041,194 / 2,564,964 | PASS |
| `INV-LIBC-SO-S` | x86_64 / aarch64 / riscv64 | 1,286,570 / 1,270,220 / 1,010,867 | 1,300,000 | PASS |
| `INV-LIBC-SO-NEEDED` | x86_64 / aarch64 / riscv64 | 0 | 1 | KNOWN (rule exception) |

- **The three DSO budget rows are frozen.** x86_64 and riscv64 are measured+3% from the
  C74ht/C74ia measurements (the current x86_64 measurement is 24 B smaller); the aarch64 budget
  is the pre-C74id measured+3% (1,404,682 / 2,576,138) and still carries extra headroom after
  the C74id shrink to S 1,270,220 / F 1,929,968.
- **aarch64 `INV-LIBC-SO-S` passes at S 1,270,220 under the unchanged 1,300,000 ceiling** (C74id:
  a `libc-a-server-z` DSO archive plus `--icf=all --strip-debug --discard-all`; the static
  `libc.a`, x86_64/riscv64 archives, symbols, ABI and ceilings are unchanged).
- **The three NEEDED rows are KNOWN, not passes.** A self-contained `libc.so.1` has no
  `DT_NEEDED` (`readelf -d` shows SONAME/FLAGS only and the loader resolves by SONAME), while
  `tools/libc-size/src/rules.rs:382` hard-codes `needed == 1`. The rows are recorded as reviewed
  known exceptions with readelf evidence (C74ht/C74ia); a follow-up tooling ticket must change
  the expectation to 0.
- **riscv64 compat is closed (C74hs).** `pub mod compat` now builds on riscv64, so the 151 names
  are real exports; `strtold`/`wcstold` are the only waived names (C47m: no riscv64 binary128
  C ABI wrapper), removed from the riscv64 list/version script with an explicit pack message.
  `cargo xtask libc-link --arch riscv64` exits 0 and produces the DSO above.
- **The dynamic `P*` probe rows stay skipped** (30 rows): `tools/libc-size/src/main.rs:278` tests
  `libc.so.1` in the per-set staging dir, and `prepare_set` copies only the static set (`libc.a`,
  crt objects), so the linked DSOs feed only the `libc_so` rows.

## The two riscv64 P9 static rows (new residual)

C74hs added the 151 compat/IPC exports; the full-link P9 probes grew:

| Row | S measured / budget | F measured / budget | Delta |
|---|---:|---:|---:|
| P9 riscv64 static tiny | 906,278 / 868,494 | 907,864 / 870,128 | +37,784 S / +37,736 F |
| P9 riscv64 static server | 966,556 / 924,842 | 968,144 / 926,473 | +41,714 S / +41,671 F |

This is real growth from the C74hs source change, not a measurement artifact. C74ia left the rows
unbudgeted because a budget raise would hide a real full-link regression without the owner's
decision. They are the second owner decision (option C) and are included in the 13 fails.

## The P7 riscv64 server B row (new, C74in)

C74ij's `%s` branch grew `.text` by 246 bytes; GNU RELRO crossed the `0x22000` page boundary and
`.relro_padding` grew by 3,848 bytes. `.bss` is **11,552 in both builds** (no new static buffer);
the P7 riscv64 server B is **15,488 > 12,385**.

| Build | `.text` | `.data.rel.ro` | `.relro_padding` | `.bss` | B |
|---|---:|---:|---:|---:|---:|
| pre-C74ij `9a8d8695` | 58,110 | 2,000 | 88 | 11,552 | 11,640 |
| current `f0420ac7` | 58,356 | 2,000 | 3,936 | 11,552 | 15,488 |
| delta | +246 | 0 | **+3,848** | **0** | **+3,848** |

This is a layout side effect of an intentional code change, not an accidental buffer. The two
options are **D1** (fix the layout: shrink the `%s` `.text` branch or realign RELRO, outside any
current size ticket) and **D2** (approve the measured+3% B budget **15,953**). It is the third
owner decision and is included in the 13 fails.

## Option A: reviewed ceiling update

Two implementation choices; the reviewer/owner picks one.

- **A1 — global measured+3%:** one limit per invariant:
  `INV-S-P2-P0` = **30,034** (max 29,159 + 3%), `INV-S-P3-P2` = **23,781** (max 23,088 + 3%).
  Two constants in `rules.rs`; simple; the x86_64 P2-P0 KNOWN rows fall under the new limit and
  can be cleaned up in the same size-lane action. Less sensitive for the smaller rows (riscv64
  P2-P0 gets ~11 KB headroom).
- **A2 — per arch/set measured+3% (preferred for sensitivity):**

| Invariant | Arch | Set | Measured | Proposed ceiling (+3%) |
|---|---|---|---:|---:|
| `INV-S-P2-P0` | aarch64 | tiny | 26,671 | 27,472 |
| `INV-S-P2-P0` | aarch64 | server | 29,159 | 30,034 |
| `INV-S-P2-P0` | riscv64 | tiny | 19,067 | 19,640 |
| `INV-S-P2-P0` | riscv64 | server | 23,429 | 24,132 |
| `INV-S-P3-P2` | x86_64 | tiny | 17,160 | 17,675 |
| `INV-S-P3-P2` | x86_64 | server | 23,088 | 23,781 |
| `INV-S-P3-P2` | aarch64 | tiny | 16,964 | 17,473 |
| `INV-S-P3-P2` | aarch64 | server | 18,272 | 18,821 |
| `INV-S-P3-P2` | riscv64 | tiny | 10,804 | 11,129 |
| `INV-S-P3-P2` | riscv64 | server | 12,392 | 12,764 |

  A2 needs a per-row limit table in `rules.rs` (the current code has one limit per invariant
  name); it preserves the tight riscv64/aarch64 P3-P2 detection. The x86_64 P2-P0 KNOWN rows can
  remain known or receive their own measured+3% rows (22,710 / 28,379) in the same action.

**Release consequence of A:** the ten invariant FAIL rows pass, but **3.1 remains NOT MET** on the
two riscv64 P9 rows and the P7 riscv64 server B row (unless C1 and D1/D2 are approved), and the
other libc-gate rows (`1.2`, `2.1/2.2`; 1.4/1.6/1.7 are MET) close. The 0.1.0 release remains
**NO-GO** for POSIX, libc, kernel, qtests, images and the Pi 4 board; the tag is not issued by
this decision.

## Option B: keep NOT MET

Leave the hard-coded 5,120/10,240 ceilings and the unbudgeted P9/P7 rows. The 10 rows stay FAIL,
the two riscv64 P9 rows and the P7 riscv64 server B row stay FAIL (13 total); 3.1 stays
**NOT MET**; the 0.1.0 tag stays withheld. Passing under the old ceilings would require shrinking
the legitimate integer printf/float plug-in paths by ~1-2× (or adding a special build split),
which is a code project, not a policy decision. The owner/reviewer would explicitly accept the
NO-GO.

## Option C: the two riscv64 P9 rows

- **C1 — freeze measured+3%:** add the two P9 riscv64 rows to the size lane's frozen budgets at
  tiny **933,467 S / 935,100 F** and server **995,553 S / 997,189 F** (measured+3%), recording
  the C74hs compat growth as reviewed. The rows remain regression detectors.
- **C2 — keep them FAIL:** require a riscv64 full-link shrink (a code project) and accept the
  continued NO-GO.

C1 does not change the 10 invariant ceilings; the gate still needs A/B, D1/D2 and the other rows.

## Option D: the P7 riscv64 server B row

- **D1 — fix the layout:** shrink the C74ij `%s` branch or realign RELRO so the 3,848-byte
  `.relro_padding` disappears and B returns to ~11,640; otto/tooling owns the code/link change.
- **D2 — freeze the B budget at 15,953** (`ceil(15,488 × 1.03)`) in the size lane; the row then
  passes as a regression detector.

**Approving A1/A2 + C1 + D1/D2 closes all 13 size rows.** The libc gate then still needs 1.2 and
2.1/2.2 (1.4/1.6/1.7 are MET).

## Exact approval needed

- **Reviewer** (size rules/allowlist and architecture): approve A1 or A2 and the concrete
  invariant ceilings, C1 or C2 for the two riscv64 P9 rows, D1 or D2 for the P7 riscv64 server B
  row, the `INV-LIBC-SO-NEEDED` follow-up (expected 0), and the cleanup of the x86_64 P2-P0
  KNOWN rows.
- **Owner** (Bo Maryniuk; release/tag policy): accept A or B, C1 or C2, and D1 or D2. The owner's
  decision is the release-policy decision; the reviewer's is the size rule approval.
- **Size lane (lance/C74dl)** implements the approved change in
  `tools/libc-size/src/rules.rs` and `tests/libc/size/BUDGETS.toml` (invariant/P9/P7 budgets,
  known rows), then re-runs `cargo xtask libc-size --check`.
- **Tooling follow-up** changes `INV-LIBC-SO-NEEDED` to expect 0 for a self-contained
  `libc.so.1`; until then the three rows remain KNOWN with readelf evidence.
- **3.1 gate owner (carl/C74c/lane)** records the 3.1 row; the release evidence packet is
  regenerated after the gate is MET. This paper edits no rule, BUDGET, source or gate file; no
  minion may treat it as the approval. Approving all three decisions closes the 13 size rows.

## Reproduction

```sh
cargo xtask libc-pack --arch x86_64
cargo xtask libc-pack --arch aarch64
cargo xtask libc-pack --arch riscv64
cargo xtask libc-link --arch x86_64
cargo xtask libc-link --arch aarch64
cargo xtask libc-link --arch riscv64
cargo xtask libc-size --check       # 63 pass, 13 fail, 65 known, 99 skipped
# libc.so.1 rows: x86_64 S 1286570/1325192 F 3009208/3099509
#                 aarch64 S 1270220/1404682 F 1929968/2576138
#                 riscv64 S 1010867/1041194 F 2490256/2564964
# 13 fails: the 10 INV-S rows + P7 riscv64 server B + P9 riscv64 tiny/server
sed -n '288,310p' tools/libc-size/src/rules.rs
```

C74dl's audit (`tickets/lance/reports/C74dl.md`, `tests/libc/size/TRIAGE.md`) has the
`94d35d15` classification and the `P7-R-14` false-positive note. C74hp/C74hr produced the dynamic
rows; C74hs closed the riscv64 compat gap (151 exports plus the C47m `strtold`/`wcstold` waiver);
C74ht/C74ia froze the DSO budgets and recorded the NEEDED rule exceptions; C74id shrank the
aarch64 DSO below the unchanged S ceiling; C74ij fixed `strftime %s` (1.4 MET) and added the
246-byte `.text` growth that C74in attributed to the P7 `.relro_padding`. The ten invariant values
on `f0420ac7` are the C74in table (riscv64 server 23,429 and 12,392; x86_64 server P3 23,088;
riscv64 tiny P2 19,067). No QEMU or board run is involved.
