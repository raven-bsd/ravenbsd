<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# libc size gate 3.1 triage and budget correction (C74w/C74z/C74z-S10)

Status: OBSERVED (C74z-S12, `origin/master` `d97f7c1a`). C74w classified the C74h/C69 data;
C74z corrected the objective budget-data errors; C74z-S10 re-froze the six P9 S/F budgets;
C74z-S11 rebuilt all three arches and fixed the six B-only/layout rows; C74z-S12 rebuilt again
after C47r/K97c, confirmed the same 17/48 residual, and classified the 44 S/F rows with exact
shrink tickets and reviewer-only budget proposals. **3.1 remains NOT MET**: 17 pass / 48 fail /
69 known / 105 skipped / 0 budget-only. No S/F or boot budget is applied by C74z-S12.

## C74zb shrink result (OBSERVED, 2026-11-17)

C74zb collapses the `printf` engine behind one type-erased core and shares the `mntent` line and
arena buffers. Budgets are untouched. Measured with `cargo xtask libc-size --check` on
`origin/master` `e1f252e5` plus C74zb:

| Cluster | Probe | Before | After | Delta |
|---|---|---|---|---|
| `vformat` monomorphizations | P9 x86_64 tiny (sum of the five `vformat` symbols) | 23,246 | 9,818 (`vformat_dyn::<u8>` 4,964 + `::<u32>` 4,854) | -13,428 |
| `vformat` + `emit_common`/`float_layout`/`str` instantiations | P9 x86_64 tiny S | 1,206,381 | 1,151,149 | -55,232 |
| same | P9 x86_64 server S | 1,341,423 | 1,270,815 | -70,608 |
| same | P9 aarch64 tiny/server S | 1,365,588 / 1,414,024 | 1,324,940 / 1,351,496 | -40,648 / -62,528 |
| same | P9 riscv64 tiny/server S | 893,350 / 963,506 | 856,286 / 911,070 | -37,064 / -52,436 |
| compat scratch | P9 x86_64 tiny / server B | 67,600 / 67,840 | 61,384 / 60,632 | -6,216 / -7,208 |
| dtoa | P9 x86_64 tiny `dtoa`/`strtodg`/`strtod` | 8,418 / 10,285 / 7,674 | unchanged | 0 |

No duplicated dtoa instantiation or table was found: `dtoa`, `strtodg` and `strtod` are three
distinct donor algorithms (fast double generator, generic parser, dedicated double parser), not
duplicated instantiations, so C74zb leaves them byte-identical.

The `printf` core is now instantiated once per character unit (`vformat_dyn::<u8>`,
`::<u32>`) instead of once per sink/env/float combination. Probes that previously linked one
instantiation (P2/P3) pay the indirect-call cost: P2/P3 tiny S +1,960 B, P2 server S +200 B.
P3 x86_64 server flips to a B-only row because `.relro_padding` grows from 0xb8 to 0xff8; the
real `.bss` section is 11,424 B before and after, so this is layout padding, not RAM. The gate
count changes 63 fail -> 64 fail for that reason while S/F drop by 37-70 KiB on every P9 row.

## C74zc remaining static shrink (OBSERVED, 2026-11-17)

C74zc moves the remaining per-process scratch arenas from `.bss` to one lazy heap allocation each.
The C APIs already allocate for their parsed database vectors, so the allocation class is
unchanged. Measured on `origin/master` `e414e47d` (C74zb + C39m merged) plus C74zc:

| Static | Location | Before | After | Delta |
|---|---|---|---|---|
| `netdb::records::AREA` | `raven-libc/src/netdb/records.rs` | 8,432 | 8 (pointer in `Cell`) | **-8,424 B** |
| `ffi::tcbbuf::AREA` | `raven-libc/src/ffi/tcbbuf.rs` | 6,776 | 8 (pointer in `Cell`) | **-6,768 B** |
| `ttyent_state::ARENA` | `raven-libc/src/compat/ttyent_state.rs` | 8,192 | 8 (pointer) | **-8,184 B** |
| `usershell_state::SHELL_BUF` | `raven-libc/src/compat/usershell_state.rs` | 4,096 | 8 (pointer) | **-4,088 B** |

| Probe | B before → after | S before → after |
|---|---|---|
| P9 x86_64 tiny | 62,192 → 35,552 (**-26,640**) | 1,138,061 → 1,137,229 (-832) |
| P9 x86_64 server | 61,792 → 33,648 (**-28,144**) | 1,257,367 → 1,258,031 (+664) |

The aarch64/riscv64 B values follow the same removals but stay under budget, so `check` does not
print them. S is neutral (-1,962 … +672 B) because the arenas are untouched at run time. Gate 3.1
stays at 63 fail (the rows removed were already below their B budgets); the x86_64 P9 B headroom
grows from ~8.7 KiB to ~37 KiB.

The lead's rev-95 note explicitly includes `tcbbuf`; the ticket Owns glob
`libc/ravenlibc/src/**` appears to be a typo for the crate `src` tree, so the tcbbuf change is
called out separately in the report.
## C74z-S10 P9 full-link envelope (OBSERVED, `e414e47d`)

Re-measured the six static Rust P9 rows after C39m (ctype range tables) and C74zb (printf/mntent)
merged, compared them against the C69 freeze, and re-froze only the P9 S/F budgets at
`ceil(measured * 103 / 100)`. The P9 B fields stay the C74z Berkeley-bss baseline; no
smaller-profile or boot budget is touched.

### Current P9 rows and budget

| Arch | Set | S measured | S old -> new | F measured | F old -> new | Result |
|---|---|---:|---:|---:|---:|---|
| x86_64 | tiny | 1,138,061 | 1,228,153 -> 1,172,203 (-55,950) | 1,139,528 | 1,229,672 -> 1,173,714 (-55,958) | PASS |
| x86_64 | server | 1,257,367 | 1,365,977 -> 1,295,089 (-70,888) | 1,258,824 | 1,367,478 -> 1,296,589 (-70,889) | PASS |
| aarch64 | tiny | 1,320,240 | 1,038,488 -> 1,359,848 (+321,360) | 1,321,696 | 1,039,987 -> 1,361,347 (+321,360) | PASS |
| aarch64 | server | 1,347,092 | 1,070,261 -> 1,387,505 (+317,244) | 1,348,544 | 1,071,761 -> 1,389,001 (+317,240) | PASS |
| riscv64 | tiny | 843,198 | 741,018 -> 868,494 (+127,476) | 844,784 | 742,655 -> 870,128 (+127,473) | PASS |
| riscv64 | server | 897,904 | 792,159 -> 924,842 (+132,683) | 899,488 | 793,793 -> 926,473 (+132,680) | PASS |

The P9 B values (measured/budget, unchanged C74z baseline) are x86_64 62,192/70,873 and
61,792/72,389; aarch64 60,244/72,673 and 62,188/73,621; riscv64 40,868/43,462 and
39,508/39,771. All six B values fit.

The new S/F budgets are current measured + 3 percent: 25,296-40,457 B of headroom on each row.
The x86_64 rows are **tightened** (the old C74z baseline was before C39m/C74zb); the
aarch64/riscv64 rows are raised because their S/F budgets were still the C69 freeze.

### Per-symbol delta since C69

C69 P9 did not link on x86_64 (`undefined symbol: fgets`), so the x86_64 P9 budget was first
added by C74z. The per-symbol comparison below is the C69 `5b65bcfd` P9 maps against the current
`e414e47d` maps for the four aarch64/riscv64 rows. Values are the nm size-sum deltas by crate;
the executable `S` delta is shown separately because section padding and alignment are not
attributed to a crate.

| Cluster | aarch64 tiny | aarch64 server | riscv64 tiny | riscv64 server |
|---|---:|---:|---:|---:|
| `raven_libc_compat` | +35,416 | +41,628 | 0 | 0 |
| `raven_libc` | +36,999 | +40,999 | +10,257 | +11,951 |
| `raven_libc_m` | +30,772 | +37,500 | +13,792 | +15,732 |
| `raven_libc_time` | +12,572 | +14,508 | +11,076 | +12,128 |
| `raven_libc_fmt` | +8,976 | +11,160 | +7,776 | +9,570 |
| `raven_libc_platform` | +8,944 | +10,104 | +7,660 | +7,886 |
| `raven_spawn` | +8,400 | +9,088 | +8,128 | +8,104 |
| `alloc` | +9,496 | +7,696 | +4,860 | +4,238 |
| `raven_libc_stdio` | +5,264 | +5,884 | +5,118 | +5,062 |
| `raven_libc_iconv` | +4,836 | +5,028 | +4,078 | +4,252 |
| `raven_libc_proc` | +3,312 | +3,672 | +2,986 | +3,122 |
| `raven_elf` | +2,252 | +2,284 | +2,014 | +2,064 |
| `raven_libc_locale` | +1,612 | +2,012 | +1,430 | +1,756 |
| `raven_libc_rand` | +1,760 | +2,032 | +1,584 | +1,778 |
| `raven_proto_fs` | +1,704 | +1,980 | +1,662 | +1,608 |
| `raven_libc_regex` | +1,368 | +1,452 | +1,074 | +1,154 |
| `raven_libc_ctype` | -10,204 | -10,488 | -10,512 | -10,484 |
| **S total (`S_now - S_c69`)** | **+312,000** | **+312,004** | **+123,764** | **+128,818** |

Top individual changes on aarch64 tiny (the other rows follow the same order): growth is
`ttyent_state::ARENA` +8,192, `mntent_state::SCRATCH` +8,192, `PROPS_RANGE_START` +7,024,
`BUILTIN_TLS` +4,096, `usershell_state::SHELL_BUF` +4,096, `tzload` +3,956,
`PROPS_RANGE_LEN` +3,512, `vformat_dyn::<u32>` +3,364, `vformat_dyn::<u8>` +3,148, two `vscan`
instantiations +2,304 each, and `PROPS_RANGE_VALUE` +1,756. The shrink is `PROPS_STAGE2`
-20,096, `PROPS_STAGE1` -4,352, `CASE_STAGE2` -2,688, the three old `vformat` instantiations
-2,140/-2,060/-2,060, three `float_layout::emit_inner` instantiations -1,772/-1,656/-1,656, and
three `emit_common` instantiations -624/-624/-620. The aarch64 server row additionally shows
`vformat_dyn` at 5,596/5,880 and four `vscan` instantiations at 3,760-3,840 each.

### Real feature growth vs duplication

* **Legitimate feature growth.** The delta is spread over the new POSIX/loader/math work:
  `raven_libc_compat` (crypt/vis/setmode), `raven_libc_m` (new fdlibm functions),
  `raven_libc_time` (tz/strptime/strftime), `raven_libc_platform`, `raven_spawn`, `raven_elf`,
  `raven_libc_proc`, `raven_libc_iconv`, `raven_libc_fmt` (scanf), `raven_libc_stdio`,
  `raven_libc_locale`, `raven_libc_rand` and `raven_proto_fs`. Removing the ctype tables
  (-10.2 to -10.5k per row) and the old printf instantiations is visible in the same map.
* **Pre-existing duplication, not growth.** `raven_libc_m::fdlibm::exp2::TBL` 4,096 and
  `compiler_builtins::math::libm_math::exp2::TBL` 4,096 are both present, and
  `rem_pio2_large` exists in both crates (aarch64 tiny 3,468 + 3,380; x86_64 tiny 4,405 + 5,498;
  riscv64 tiny 2,960 + 2,658). The C69 maps contain the same symbols at the same sizes, so this
  is not part of the C74z-S10 growth. It remains C74z-S8; the 3 percent P9 headroom (25-40k) is
  larger than the duplicate, so the new envelope does not depend on keeping it.
* **B-only statics, not S/F.** The compat `ttyent_state::ARENA` 8,192,
  `mntent_state::SCRATCH` 8,192, `usershell_state::SHELL_BUF` 4,096 and `tcbbuf::AREA` 6,776,
  plus `netdb::records::AREA` 8,432, are B (NOBITS), not S/F. C74z-S4 and C74z-S7 still own
  them; all P9 B rows fit their C74z budgets.
* **Not duplication.** `dtoa`/`strtodg`/`strtod` are three distinct donor algorithms (C74zb
  verified this), not instantiations of one function; they are unchanged from C69. The four
  `vscan` instantiations are one per source/char unit (u8/u32 x Slice/File), not identical
  copies; they are new scanf feature code.
* **No new duplicate table or function was found in the growth.** The only exact duplicate pair
  is the pre-existing libm/compiler_builtins one above.

### Result

Re-freezing the six P9 S/F rows is justified by the feature growth and does not hide a new
duplication. `cargo xtask libc-size --check` moves from **7 pass / 56 fail** to **11 pass / 52
fail / 69 known / 105 skipped / 0 budget-only**: the four aarch64/riscv64 P9 S/F rows now pass,
the two x86_64 P9 rows stay passing with tightened budgets, and no row becomes a new fail. The
remaining 52 fails are 44 real S/F rows, 6 B-only rows (x86_64 P3 server, x86_64 P5 tiny/server,
aarch64 P8 tiny/server, riscv64 P8 server) and 2 boot rows. **3.1 remains NOT MET**; the
smaller profiles and boot rows are outside this ticket.

## C74z-S11 residual closure (OBSERVED, `72c630bd`)

Rebuilt all three arches from a clean tree, separated missing/stale artifacts from real rows, and
fixed the objective B-only/layout data. No S/F or boot budget is raised and no libc body is edited.

### Full-build gate and B-only fix

The first full rebuild is **11 pass / 54 fail / 69 known / 105 skipped / 0 budget-only** before
the B-only fix. The lead's 11/62 count is 8 fails higher; the difference is consistent with the
missing/stale artifact rows (the `riscv64 S 755631 > 0` row is gone after the full three-arch
build and no budget-0 row remains). The remaining 54 were 50 budget rows (44 real S/F + 6
B-only), 2 invariants and 2 boot rows.

The six B-only/layout rows are re-measured on `72c630bd` and re-frozen at `measured + 3 percent`
(same rule as the existing B/C69/C74z data). These rows were the only B-only failures; no S/F
field changes.

| Row | Old B measured/budget | New B measured/budget |
|---|---|---|
| x86_64 P5 tiny | 11,944/12,303 | 13,008/13,399 |
| x86_64 P3 server | 14,240/14,668 | 15,512/15,978 |
| x86_64 P5 server | 11,536/11,883 | 13,032/13,423 |
| aarch64 P8 tiny | 11,632/11,981 | 12,032/12,393 |
| aarch64 P8 server | 12,048/12,410 | 13,096/13,489 |
| riscv64 P8 server | 13,768/14,182 | 14,616/15,055 |

After the fix the gate is **17 pass / 48 fail / 69 known / 105 skipped / 0 budget-only**.

### aarch64 P2/P3 measurement correction

The old aarch64 P2/P3 rows were under-measured. A controlled build of `e414e47d` gives P2/P3 tiny
83,988/84,000 S; the current tree gives 115,663/115,675. The `e414e47d` map resolves `printf` to an
8-byte stub in `raven_libc...cgu.1205.rcgu.o`; the current map links the real 0x50-byte `printf`
in `cgu.1206.rcgu.o` and pulls `vformat_dyn::<u8>` 3,148, `float_layout::emit_inner::<u8,...>`
1,772, `pos::analyse` 1,028 and the rest of the printf cluster. The +31,675 is a measurement
correction, not new feature code. The current S/F budgets (C69 78,235/78,248) are therefore under
by 37,428/37,427 S/F and are **not** raised here; the reviewer must decide the S/F envelope.

The correction also makes `INV-S-P2-P0` fail on aarch64 P2 tiny/server (31,707/28,659 vs the
5,120 limit). That is a real invariant failure of the now-honest printf link, not a missing
artifact; it is reported, not silenced.

The P7 growth is separate: a controlled `e414e47d` build already gives aarch64 P7 tiny 120,652
(current 120,652), so the C74z `1bfeaafa` 101,700 → 120,652 growth is earlier real time/tz code
(owner mel/C44), not a C74z-S11 measurement change.

### Final residual

The 48 fails are **44 S/F rows + 2 `INV-S-P2-P0` + 2 boot rows**:

| Arch | Set | S/F rows still over | Also B over | Owner |
|---|---|---|---|---|
| x86_64 | tiny | P0,P1,P2,P3,P4,P6,P7 | P0,P6 | per-probe owners below |
| x86_64 | server | P0,P1,P4,P6,P7 | P0,P4,P6 | per-probe owners below |
| aarch64 | tiny | P0,P1,P2,P3,P4,P5,P6,P7 | P2,P3 | per-probe owners below |
| aarch64 | server | P0,P1,P2,P3,P4,P5,P6,P7 | P5,P7 | per-probe owners below |
| riscv64 | tiny | P0,P1,P2,P3,P4,P5,P6,P7 | P7 | per-probe owners below |
| riscv64 | server | P0,P1,P2,P3,P4,P5,P6,P7 | - | per-probe owners below |

Per-probe owners: P0 raven-libc/dave-C55/C57/C54/kevin-C49; P1 P0+carl-C56; P2/P3
carl-C42/C43/C56; P4 dave-C57/C55; P5 dave-C51; P6 bob-C52/dave-C55; P7 mel-C44/C48. The exact
current/budget numbers are in the C74z-S11 report. `INV-S-P2-P0` is aarch64 P2 tiny/server
(31,707/28,659 vs 5,120; owner carl/C43 fmt). Boot is x86_64 instructions 1,245 > 908 and
aarch64 syscalls 8 > 3 / instructions 3,403 > 1,847 (Mark/C74z-S9a). **3.1 remains NOT MET.**

## C74z-S12 S/F residual classification (OBSERVED, `d97f7c1a`)

C74z-S9a is not merged and C47r/K97c did not change any measured size row. The full three-arch
rebuild on `d97f7c1a` is the same **17 pass / 48 fail / 69 known / 105 skipped / 0 budget-only**
as `72c630bd`: 44 S/F rows + 2 `INV-S-P2-P0` + 2 boot rows. C74z-S12 does not apply a budget; it
classifies each S/F row and proposes exact shrink tickets plus reviewer-only budget changes.

### Per-probe classification

| Probe | S/F rows | Cause | Decision | Owner |
|---|---|---|---|---|
| P0 | 6 | shared floor: `dtoa` 4,906-9,235 plus platform/tls/malloc; the `BUILTIN_TLS`/`STREAMS`/`PLATFORM` statics are in every probe | real feature growth; propose current+3% after C74z-S15 | raven-libc, dave/C55, dave/C57, dave/C54, kevin/C49 |
| P1 | 6 | P0 plus stdio file I/O | real feature growth; propose current+3% | P0 + carl/C56 |
| P2 | 5 | `vformat_dyn` + `float_layout`/`pos`/`emit` linked for `%d`; aarch64 is corrected, riscv still links a printf stub | shrink first (C74z-S14; riscv C74z-S13); no budget | carl/C43 |
| P3 | 3 x86/arm + 2 riscv | `%f` float path is real on x86/aarch64; riscv is still a stub | budget x86/aarch64 after C74z-S14; riscv C74z-S13 | carl/C43 |
| P4 | 6 | thread/tls/platform | real feature growth; propose current+3% | dave/C57, dave/C55 |
| P5 | 4 | resolv/net `lookup_name`/`msend`/`getnameinfo` and name buffers | shrink first (C74z-S5); no budget | dave/C51 |
| P6 | 6 | elf/dynamic loader | real feature growth; propose current+3% | bob/C52 |
| P7 | 6 | real tz (`tzload`/`tzparse`) plus `strptime`/`strftime` duplication | shrink first (C74z-S6), then review the real tz part | mel/C44 |

Counts: P0 6 + P1 6 + P2 5 + P3 5 + P4 6 + P5 4 + P6 6 + P7 6 = 44.

The riscv64 P2/P3 rows are a second measurement defect: P2-P0 is **32 B** on riscv64 versus
25,704 on x86_64 and 31,707 on aarch64, and the top symbols show no `vformat_dyn`/`float_layout`
on riscv64. The probe is measuring a printf stub, not the real path. The riscv64 rows must be
re-measured before any budget decision; their current under-count hides real S.

### Exact shrink tickets

These are filed in the plan below; the minion rules reserve `ravenbsd-prjmgmt` tickets to the
lead, so the exact rows are recorded here and in the C74z-S12 report.

| ID | Rows | Change | Target | Owner | Acceptance |
|---|---|---|---|---|---|
| C74z-S13 | riscv64 P2/P3 | Link the real printf/variadic path on riscv64 instead of the stub; the P2-P0 32 B is the symptom | honest P2/P3 rows | carl/C43 + platform | P2/P3 riscv link real printf; no stub; re-measure |
| C74z-S14 | P2 all 5 | Split the integer-only vformat path; `%d` must not link `float_layout::emit_inner` (1,772-2,695) and the float hook | -2k S on P2; `INV-S-P2-P0` closer | carl/C43 | P2 S shrink, fmt oracle, invariant re-eval |
| C74z-S15 | P0-P7 floor | Trace `raven_libc_dtoa::dtoa` 4,906-9,235 in P0 and remove the over-link from the floor | -5-9k S on every row | carl/C42 + mel/C48 + dave/C55 | P0 S shrink, dtoa oracle |
| C74z-S6 | P7 all 6 | Share `strptime`/`strftime` parser and append-format; already in the plan | -5k S | mel/C44 | P7 S shrink, time oracle |
| C74z-S5 | P5 aarch64/riscv | Deduplicate resolv/net name buffers and service paths; already in the plan | -5k S | dave/C51 | P5 S shrink, resolv oracle |

C74z-S3 is **not** a dtoa duplication ticket: C74zb verified `dtoa`, `strtodg` and `strtod` are
three distinct algorithms. The P2 dtoa link is the shared floor (C74z-S15) plus the float hook
(C74z-S14), not three copied dtoas. C74z-S8 (libm `exp2`/`rem_pio2`) is not a 3.1 blocker: all
six P9 rows pass; it remains a margin shrink only. C74z-S4/S7 are B-only and pass.

### Reviewer budget proposal (not applied)

| Rows | Proposed | Precondition |
|---|---|---|
| P0/P1/P4/P6, all arch/set (24 rows) | S/F `current + 3%`; B `current + 3%` only on P0 x86 tiny/server, P4 x86 server and P6 x86 tiny/server where B is also over | C74z-S15 floor shrink re-measures; if it lands, re-freeze lower |
| P3 x86 tiny, aarch64 tiny/server (3 rows) | S/F `current + 3%`; B `current + 3%` on aarch64 tiny where B is over | C74z-S14 removes the P2 float duplication; the P3 `%f` path is legitimate |
| P2 (5), P5 (4), P7 (6), riscv P3 (2) | **no budget** | shrink/measurement first (C74z-S13/S14/S5/S6) |

The arithmetic is `ceil(current * 103 / 100)`; the exact current/budget values are in the C74z-S11
table above. The proposal is not applied in this ticket. The aarch64 `INV-S-P2-P0` limit
(5,120) also needs a reviewer decision after C74z-S14: the honest printf-to-stdout path may
legitimately cost more than the old stub-based limit.

**3.1 remains NOT MET** with the same 44 S/F + 2 invariant + 2 boot residual. No boot/S/F budget
is raised to hide duplication.

## Measurement definition

`S` is the Berkeley `text + data` of the stripped probe; `B` is the Berkeley **bss** field of
`llvm-size`, which includes every NOBITS section, so it also counts the linker's `.relro_padding`.
It is not just the `.bss` section. On `1bfeaafa` the `.bss` section is 11,392 bytes on all three
architectures, while Berkeley B is 15,176 (x86, `.relro_padding` 3,784), 13,168 (aarch64, 1,776)
and 13,064 (riscv64, 1,672). C74z re-baselines the arm/riscv B values with this same definition.

The `build/size/libc-size.json`/`.md` pair is a run artifact: `run` writes it before `check`
applies budgets, so its `budget_*` fields stay 0 and `status` stays `ok`. The console output and
the `libc-gate` evidence file are the check evidence.

## Budget data correction (C74z)

The missing x86_64 P9 rows are added from the first `1bfeaafa` measurement (the other arches' P9
rows are also measured + 3 percent). The aarch64/riscv64 B baselines are replaced with the
`1bfeaafa` measurement because the C69 B values were measured from a different/older static
layout: they were 6.9k/7.8k versus 14.3k on x86_64 for the same source. S/F budgets are untouched,
so no real growth is hidden.

| Probe | arch | set | B old measured/budget | B new measured/budget |
|---|---|---|---|---|
| P0 | aarch64 | server | 7640/7870 | 14720/15162 |
| P0 | aarch64 | tiny | 6752/6955 | 13168/13564 |
| P0 | riscv64 | server | 6824/7029 | 15160/15615 |
| P0 | riscv64 | tiny | 7584/7812 | 13064/13456 |
| P1 | aarch64 | server | 7400/7622 | 12624/13003 |
| P1 | aarch64 | tiny | 6448/6642 | 14264/14692 |
| P1 | riscv64 | server | 6672/6873 | 13744/14157 |
| P1 | riscv64 | tiny | 7416/7639 | 15520/15986 |
| P2 | aarch64 | server | 7600/7828 | 14672/15113 |
| P2 | aarch64 | tiny | 6720/6922 | 13136/13531 |
| P2 | riscv64 | server | 6776/6980 | 15128/15582 |
| P2 | riscv64 | tiny | 7544/7771 | 13032/13423 |
| P3 | aarch64 | server | 7584/7812 | 14664/15104 |
| P3 | aarch64 | tiny | 6704/6906 | 13120/13514 |
| P3 | riscv64 | server | 6760/6963 | 15104/15558 |
| P3 | riscv64 | tiny | 7528/7754 | 13008/13399 |
| P4 | aarch64 | server | 11736/12089 | 16112/16596 |
| P4 | aarch64 | tiny | 9896/10193 | 14056/14478 |
| P4 | riscv64 | server | 9880/10177 | 14920/15368 |
| P4 | riscv64 | tiny | 10200/10506 | 16856/17362 |
| P5 | aarch64 | server | 8680/8941 | 11568/11916 |
| P5 | aarch64 | tiny | 8408/8661 | 14576/15014 |
| P5 | riscv64 | server | 6072/6255 | 14376/14808 |
| P5 | riscv64 | tiny | 9384/9666 | 14424/14857 |
| P6 | aarch64 | server | 7392/7614 | 14488/14923 |
| P6 | aarch64 | tiny | 6520/6716 | 12936/13325 |
| P6 | riscv64 | server | 6672/6873 | 15008/15459 |
| P6 | riscv64 | tiny | 7432/7655 | 12912/13300 |
| P7 | aarch64 | server | 9184/9460 | 12184/12550 |
| P7 | aarch64 | tiny | 5840/6016 | 15520/15986 |
| P7 | riscv64 | server | 9144/9419 | 13400/13802 |
| P7 | riscv64 | tiny | 6600/6798 | 11704/12056 |
| P8 | aarch64 | server | 9072/9345 | 12048/12410 |
| P8 | aarch64 | tiny | 9440/9724 | 11632/11981 |
| P8 | riscv64 | server | 9528/9814 | 13768/14182 |
| P8 | riscv64 | tiny | 9688/9979 | 15168/15624 |
| P9 | aarch64 | server | 33176/34172 | 71476/73621 |
| P9 | aarch64 | tiny | 35248/36306 | 70556/72673 |
| P9 | riscv64 | server | 32824/33809 | 38612/39771 |
| P9 | riscv64 | tiny | 33224/34221 | 42196/43462 |

Added x86_64 P9 rows (measured + 3 percent):

| Probe | arch | set | S measured/budget | B measured/budget | F measured/budget |
|---|---|---|---|---|---|
| P9 | x86_64 | tiny | 1192381/1228153 | 68808/70873 | 1193856/1229672 |
| P9 | x86_64 | server | 1326191/1365977 | 70280/72389 | 1327648/1367478 |

Result: the two missing-budget fails and the arm/riscv B overruns are gone. The remaining 56
fails are 52 real S/F rows, 2 x86_64 P5 B-only rows and 2 boot rows.

## C74z baseline failure table (HISTORICAL, `1bfeaafa`)

`cur/budget (delta)`; static Rust. This is the C74z snapshot before C39m/C74zb/C74z-S10; the
current 3.1 state is the C74z-S10 section above. The P9 rows in this table are superseded (all
six P9 rows pass the C74z-S10 envelope). The B column for aarch64/riscv64 was below the corrected
budget at C74z; only the S/F overruns were current then.

### x86_64 / tiny

| Probe | S | B | F | Class | Owner |
|---|---|---|---|---|---|
| P0 | 69203/68355 (+848) | 15176/14272 (+904) | 70664/69859 (+805) | real S/F/B | raven-libc, dave/C55 platform, dave/C57 tls, dave/C54 sys, kevin/C49 malloc |
| P1 | 71035/68545 (+2490) | 13400/14074 (-674) | 72504/70057 (+2447) | real S/F | P0 + carl/C56 stdio |
| P2 | 92947/92803 (+144) | 11912/15129 (-3217) | 94408/94316 (+92) | real S/F | carl/C42 dtoa, carl/C43 fmt, carl/C56 stdio |
| P3 | 92979/92853 (+126) | 11880/15080 (-3200) | 94440/94365 (+75) | real S/F | carl/C42 dtoa, carl/C43 fmt, carl/C56 stdio |
| P4 | 76799/75677 (+1122) | 13456/17222 (-3766) | 78272/77193 (+1079) | real S/F | dave/C57 thread+tls, dave/C55 platform |
| P5 | 149076/150608 (-1532) | 13248/12303 (+945) | 150544/152119 (-1575) | real B | dave/C51 resolv+net |
| P6 | 69363/68504 (+859) | 15032/14140 (+892) | 70824/70008 (+816) | real S/F/B | bob/C52 elf, dave/C55 platform |
| P7 | 84993/84644 (+349) | 11736/14915 (-3179) | 86456/86158 (+298) | real S/F | mel/C44 time, mel/C48 locale |

### x86_64 / server

| Probe | S | B | F | Class | Owner |
|---|---|---|---|---|---|
| P0 | 73643/73347 (+296) | 14856/13531 (+1325) | 75112/74853 (+259) | real S/F/B | raven-libc, dave/C55 platform, dave/C57 tls, dave/C54 sys, kevin/C49 malloc |
| P1 | 75227/73512 (+1715) | 13336/13366 (-30) | 76696/75017 (+1679) | real S/F | P0 + carl/C56 stdio |
| P4 | 81359/80750 (+609) | 17128/16398 (+730) | 82824/82269 (+555) | real S/F/B | dave/C57 thread+tls, dave/C55 platform |
| P5 | 153197/155287 (-2090) | 13256/11883 (+1373) | 154664/156791 (-2127) | real B | dave/C51 resolv+net |
| P6 | 73803/73503 (+300) | 14712/13382 (+1330) | 75272/75017 (+255) | real S/F/B | bob/C52 elf, dave/C55 platform |
| P7 | 91521/91316 (+205) | 13432/12509 (+923) | 92984/92816 (+168) | real S/F/B | mel/C44 time, mel/C48 locale |

### aarch64 / tiny

| Probe | S | B | F | Class | Owner |
|---|---|---|---|---|---|
| P0 | 83508/78202 (+5306) | 13168/13564 (-396) | 84960/79698 (+5262) | real S/F | raven-libc, dave/C55 platform, dave/C57 tls, dave/C54 sys, kevin/C49 malloc |
| P1 | 86572/78511 (+8061) | 14264/14692 (-428) | 88024/80011 (+8013) | real S/F | P0 + carl/C56 stdio |
| P2 | 83540/78235 (+5305) | 13136/13531 (-395) | 84992/79731 (+5261) | real S/F | carl/C42 dtoa, carl/C43 fmt, carl/C56 stdio |
| P3 | 83552/78248 (+5304) | 13120/13514 (-394) | 85008/79747 (+5261) | real S/F | carl/C42 dtoa, carl/C43 fmt, carl/C56 stdio |
| P4 | 92604/90093 (+2511) | 14056/14478 (-422) | 94056/91588 (+2468) | real S/F | dave/C57 thread+tls, dave/C55 platform |
| P5 | 176429/173655 (+2774) | 14576/15014 (-438) | 177888/175158 (+2730) | real S/F | dave/C51 resolv+net |
| P6 | 83752/78445 (+5307) | 12936/13325 (-389) | 85208/79945 (+5263) | real S/F | bob/C52 elf, dave/C55 platform |
| P7 | 101700/96071 (+5629) | 15520/15986 (-466) | 103152/97570 (+5582) | real S/F | mel/C44 time, mel/C48 locale |
| P8 | 183348/180904 (+2444) | 11632/11981 (-349) | 184800/182401 (+2399) | real S/F | carl/C45 regex, carl/C41 stdlib, carl/C40 str |
| P9 | 1350396/1038488 (+311908) | 70556/72673 (-2117) | 1351848/1039987 (+311861) | real S/F | carl/C39 ctype, carl/C42 dtoa, carl/C43 fmt, carl/C56 stdio, carl/C64d compat; dave/C51 net; mel/C44 time; dave/C54+55+57 platform/tls/sys; kevin/C49 malloc; bob/C47 m |

### aarch64 / server

| Probe | S | B | F | Class | Owner |
|---|---|---|---|---|---|
| P0 | 77882/73102 (+4780) | 14720/15162 (-442) | 79344/74597 (+4747) | real S/F | raven-libc, dave/C55 platform, dave/C57 tls, dave/C54 sys, kevin/C49 malloc |
| P1 | 80042/73349 (+6693) | 12624/13003 (-379) | 81504/74844 (+6660) | real S/F | P0 + carl/C56 stdio |
| P2 | 77930/73143 (+4787) | 14672/15113 (-441) | 79392/74638 (+4754) | real S/F | carl/C42 dtoa, carl/C43 fmt, carl/C56 stdio |
| P3 | 77942/73155 (+4787) | 14664/15104 (-440) | 79400/74655 (+4745) | real S/F | carl/C42 dtoa, carl/C43 fmt, carl/C56 stdio |
| P4 | 86474/84007 (+2467) | 16112/16596 (-484) | 87936/85507 (+2429) | real S/F | dave/C57 thread+tls, dave/C55 platform |
| P5 | 163088/160744 (+2344) | 11568/11916 (-348) | 164544/162246 (+2298) | real S/F | dave/C51 resolv+net |
| P6 | 78134/73361 (+4773) | 14488/14923 (-435) | 79592/74861 (+4731) | real S/F | bob/C52 elf, dave/C55 platform |
| P7 | 96870/92659 (+4211) | 12184/12550 (-366) | 98328/94159 (+4169) | real S/F | mel/C44 time, mel/C48 locale |
| P8 | 178858/177097 (+1761) | 12048/12410 (-362) | 180320/178594 (+1726) | real S/F | carl/C45 regex, carl/C41 stdlib, carl/C40 str |
| P9 | 1398744/1070261 (+328483) | 71476/73621 (-2145) | 1400200/1071761 (+328439) | real S/F | carl/C39 ctype, carl/C42 dtoa, carl/C43 fmt, carl/C56 stdio, carl/C64d compat; dave/C51 net; mel/C44 time; dave/C54+55+57 platform/tls/sys; kevin/C49 malloc; bob/C47 m |

### riscv64 / tiny

| Probe | S | B | F | Class | Owner |
|---|---|---|---|---|---|
| P0 | 59034/56241 (+2793) | 13064/13456 (-392) | 60608/57870 (+2738) | real S/F | raven-libc, dave/C55 platform, dave/C57 tls, dave/C54 sys, kevin/C49 malloc |
| P1 | 60736/56414 (+4322) | 15520/15986 (-466) | 62312/58043 (+4269) | real S/F | P0 + carl/C56 stdio |
| P2 | 59066/56282 (+2784) | 13032/13423 (-391) | 60640/57911 (+2729) | real S/F | carl/C42 dtoa, carl/C43 fmt, carl/C56 stdio |
| P3 | 59086/56302 (+2784) | 13008/13399 (-391) | 60664/57928 (+2736) | real S/F | carl/C42 dtoa, carl/C43 fmt, carl/C56 stdio |
| P4 | 65222/64466 (+756) | 16856/17362 (-506) | 66800/66085 (+715) | real S/F | dave/C57 thread+tls, dave/C55 platform |
| P5 | 127429/126240 (+1189) | 14424/14857 (-433) | 129008/127869 (+1139) | real S/F | dave/C51 resolv+net |
| P6 | 59198/56409 (+2789) | 12912/13300 (-388) | 60776/58035 (+2741) | real S/F | bob/C52 elf, dave/C55 platform |
| P7 | 72746/69979 (+2767) | 11704/12056 (-352) | 74320/71598 (+2722) | real S/F | mel/C44 time, mel/C48 locale |
| P8 | 142946/142674 (+272) | 15168/15624 (-456) | 144520/144299 (+221) | real S/F | carl/C45 regex, carl/C41 stdlib, carl/C40 str |
| P9 | 882336/741018 (+141318) | 42196/43462 (-1266) | 883920/742655 (+141265) | real S/F | carl/C39 ctype, carl/C42 dtoa, carl/C43 fmt, carl/C56 stdio, carl/C64d compat; dave/C51 net; mel/C44 time; dave/C54+55+57 platform/tls/sys; kevin/C49 malloc; bob/C47 m |

### riscv64 / server

| Probe | S | B | F | Class | Owner |
|---|---|---|---|---|---|
| P0 | 56964/52829 (+4135) | 15160/15615 (-455) | 58544/54467 (+4077) | real S/F | raven-libc, dave/C55 platform, dave/C57 tls, dave/C54 sys, kevin/C49 malloc |
| P1 | 58442/52992 (+5450) | 13744/14157 (-413) | 60024/54623 (+5401) | real S/F | P0 + carl/C56 stdio |
| P2 | 56996/52879 (+4117) | 15128/15582 (-454) | 58576/54516 (+4060) | real S/F | carl/C42 dtoa, carl/C43 fmt, carl/C56 stdio |
| P3 | 57016/52899 (+4117) | 15104/15558 (-454) | 58600/54533 (+4067) | real S/F | carl/C42 dtoa, carl/C43 fmt, carl/C56 stdio |
| P4 | 63090/60597 (+2493) | 14920/15368 (-448) | 64672/62229 (+2443) | real S/F | dave/C57 thread+tls, dave/C55 platform |
| P5 | 123405/121239 (+2166) | 14376/14808 (-432) | 124992/122875 (+2117) | real S/F | dave/C51 resolv+net |
| P6 | 57128/52998 (+4130) | 15008/15459 (-451) | 58712/54632 (+4080) | real S/F | bob/C52 elf, dave/C55 platform |
| P7 | 71076/67381 (+3695) | 13400/13802 (-402) | 72656/69010 (+3646) | real S/F | mel/C44 time, mel/C48 locale |
| P8 | 144366/142866 (+1500) | 13768/14182 (-414) | 145952/144497 (+1455) | real S/F | carl/C45 regex, carl/C41 stdlib, carl/C40 str |
| P9 | 951548/792159 (+159389) | 38612/39771 (-1159) | 953136/793793 (+159343) | real S/F | carl/C39 ctype, carl/C42 dtoa, carl/C43 fmt, carl/C56 stdio, carl/C64d compat; dave/C51 net; mel/C44 time; dave/C54+55+57 platform/tls/sys; kevin/C49 malloc; bob/C47 m |

## Boot regressions

| Probe | Measured | Budget | Delta | Owner |
|---|---|---|---|---|
| boot x86_64 instructions | 1245 | 908 | +337 (+37%) | start/TLS/allocator: dave/C57, kevin/C49 |
| boot aarch64 syscalls | 8 | 3 | +5 | start/TLS/allocator: dave/C57, kevin/C49 |
| boot aarch64 instructions | 3403 | 1847 | +1556 (+84%) | start/TLS/allocator: dave/C57, kevin/C49 |

## C74z shrink plan (follow-up list)

The rows are the same clusters repeated across P0-P9 and the three arches. Each follow-up names
the concrete change and the acceptance it must satisfy; none may raise an S/F or boot budget.

| ID | Cluster | Concrete change | Target | Owner | Acceptance |
|---|---|---|---|---|---|
| C74z-S1 | ctype Unicode tables | Pack `PROPS_STAGE2`/`CASE_STAGE2`/`PROPS_STAGE1` into shared range/bit tables; drop duplicate stage arrays | -10k S/B | carl/C39 | done in C39m (-13,308 bytes; P8 riscv64 tiny now passes; P8 aarch64 tiny is B-only fail); host nextest, `libc-size --check` |
| C74z-S2 | fmt monomorphization | Collapse the six `vformat` instantiations to one sink/va-reader erased path; keep the typed fast paths | -15k S | carl/C43 | done in C74zb: `vformat_dyn::<u8>` + `::<u32>` replace the six instantiations; P9 x86_64 tiny S -55,232 from the fmt cluster; fmt oracle + nextest |
| C74z-S3 | dtoa | Share `strtod`/`strtodg`/`dtoa` bigint code and remove duplicate conversion tails | -5k S | carl/C42 | P2/P3/P9 S shrink, dtoa oracle |
| C74z-S4 | compat static buffers | Replace the 8k `mntent`/`ttyent` ARENAs and `tcbbuf` with one bounded pool or lazy allocation | -10k B/S | carl/C64d | partial in C74zb: `mntent` line/arena merged into one 8,192 SCRATCH; `ttyent` ARENA 8,192, `usershell` SHELL_BUF 4,096 and `tcbbuf` 6,776 remain (B-only); P9 B still fits C74z; compat host tests |
| C74z-S5 | resolv/net | Deduplicate `lookup_name`/`getnameinfo`/`msend` name buffers and service paths | -5k S | dave/C51 | P5/P9 S shrink, resolv oracle |
| C74z-S6 | time | Share `strptime`/`strftime` parser and append-format code | -5k S | mel/C44 | P7/P9 S shrink, time oracle |
| C74z-S7 | TLS/pages/streams statics | Review the 4,096 `BUILTIN_TLS`, 3,080 page registry, 2,808 `STREAMS`, 1,024 `MAIN_TSD` and 1,064 exit registry; make the registry slots demand-paged/lazy | -3k B | dave/C57, dave/C54, carl/C64d, kevin/C49 | P0/P9 B shrink, boot row stays within budget |
| C74z-S8 | libm tables | Share `exp2`/`rem_pio2` tables between `raven-libc-m` and `compiler_builtins` where possible; drop duplicate constants | -4k S/B | bob/C47 | P9 S/B shrink, libm oracle |
| C74z-S9 | boot start path | Trim the start/TLS path from 1,245 to <=908 x86 instructions, aarch64 3,403 to <=1,847 and 8 syscalls to <=3 | boot budgets | dave/C57, kevin/C49 | boot x86/aarch64 pass, `libc-size --check` |
| C74z-S10 | P9 full-link envelope | Done in C74z-S10: re-measured the six P9 rows on `e414e47d` and re-froze only the P9 S/F budget at measured + 3 percent; x86 tightened, aarch64/riscv feature growth accepted; B, smaller profiles and boot untouched | reviewed budget | budget owner + carl | done: all six P9 rows pass; 3.1 11 pass / 52 fail (44 S/F + 6 B-only + 2 boot); remaining shrink work is S3/S5/S6/S7/S8 |
| C74z-S11 | 3.1 residual closure | Done in C74z-S11: full three-arch rebuild, six B-only/layout rows re-frozen at measured + 3 percent, aarch64 P2/P3 printf-stub measurement corrected; final residual reported with owners | no S/F or boot raise | lance/C74z-S11 | 17 pass / 48 fail (44 S/F + 2 INV-S-P2-P0 + 2 boot); rows.toml evidence updated |
| C74z-S12 | S/F residual classification | Done in C74z-S12: rebuild on `d97f7c1a`, classify the 44 S/F rows, exact shrink tickets and a reviewer-only budget proposal | no budget applied | lance/C74z-S12 | 17/48 residual unchanged; exact table in the C74z-S12 report |
| C74z-S13 | riscv64 P2/P3 measurement | Link the real riscv64 printf/variadic path instead of the stub; P2-P0 is 32 B versus 25,704 x86 and 31,707 aarch64 | honest P2/P3 rows | carl/C43 + platform | P2/P3 riscv link real printf; no stub; re-measure |
| C74z-S14 | P2 `%d` float-path duplication | Split an integer-only vformat path; `%d` must not link `float_layout::emit_inner` 1,772-2,695 and the float hook | -2k S on P2; `INV-S-P2-P0` closer | carl/C43 | P2 S shrink, fmt oracle, invariant re-eval |
| C74z-S15 | P0 floor dtoa over-link | Trace `raven_libc_dtoa::dtoa` 4,906-9,235 in P0 and remove the reference chain from start/platform/locale | -5-9k S on every row | carl/C42 + mel/C48 + dave/C55 | P0 S shrink, dtoa oracle |

### C74z-S14 result (OBSERVED; split on `265b996a`, current base `1355d85c`)

The tiny profile (`raven-libc` feature `tiny` -> `raven-libc-fmt/tiny`) compiles the float call
out of `vformat`: `%f` emits the R-13 `*float*` stub and never references
`float_layout::emit_inner`/the float hook. The float module stays in the archive but is
unreferenced, so the other tiny probes keep their link set. The float/server build keeps real
digits. C74z-S14 is merged; the pre-split -> split delta on `265b996a` was:

| Probe | S before -> after | State |
|---|---|---|
| P2 x86_64 tiny | 95,195 -> 91,027 (**-4,168**) | FAIL -> PASS |
| P2 x86_64 server | 101,699 -> 101,293 (**-406**) | FAIL -> PASS |
| P3 x86_64 tiny | 95,227 -> 91,059 (**-4,168**) | FAIL -> PASS |
| P2 aarch64 tiny | 116,007 -> 109,891 (**-6,116**) | FAIL |
| P2 aarch64 server | 107,213 -> 108,313 (+1,100, old-base codegen) | FAIL |
| INV-S-P2-P0 aarch64 | 31,707 -> 26,523 | FAIL |

Current `1355d85c` (S14 + S13 real riscv variadic + S15a dtoa floor + S6) is the S16 input:

| Probe | S current / budget | Delta vs P0 | State |
|---|---|---|---|
| P2 x86_64 tiny | 73,899 / 92,803 | 22,048 / 5,120 (KNOWN) | PASS |
| P2 x86_64 server | 78,237 / 101,684 | 27,832 / 5,120 (KNOWN) | PASS |
| P3 x86_64 tiny | 91,059 / 92,853 | - | PASS |
| P3 x86_64 server | 101,341 / 101,734 | - | PASS |
| P2 aarch64 tiny | 93,003 / 78,235 | 26,671 / 5,120 | FAIL |
| P2 aarch64 server | 90,101 / 73,143 | 29,179 / 5,120 | FAIL |
| P3 aarch64 tiny | 109,967 / 78,248 | - | FAIL |
| P3 aarch64 server | 108,373 / 73,155 | - | FAIL |
| P2 riscv64 tiny / server | 67,799 / 68,227 (56,282 / 52,879) | 19,075 / 23,547 | FAIL |
| P3 riscv64 tiny / server | 78,603 / 80,627 (56,302 / 52,899) | - | FAIL |

The aarch64 P2/P3 residual is **real growth after the float shrink**, not printf duplication:
P2 has no `float_layout`/`dtoa` symbol; its delta (26,671) is the aarch64 floor (P0 66,332,
`BUILTIN_TLS`/`PLATFORM`/`streams`/allocator) plus the same stdio core x86 carries (22,048).
The aarch64 P2/P3 budgets (78,235/78,248) are below P0+stdio; a reviewer budget decision is
needed (not taken here). riscv P2/P3 carry the C74z-S13 real variadic path and are over too.

3.1 on `1355d85c` has **43 static fails** with the dynamic `libc.so.1` rows skipped in a clean
worktree (50 including the dynamic rows when `build/` has `libc.so.1`); the S14 collateral is
still present in the merged base: P5 x86_64 server B, P8 aarch64 B (tiny/server), and the P7
aarch64 R-14 `iconv` allocator-CGU pull. Those are S16 reconcile items, not new in this rework.

The current largest clusters are the shared P0 floor `dtoa` (4,906-9,235 per probe), the
P2/P3 printf core (`vformat_dyn` 3,148-5,646 plus float/pos/emit), the P7 time code
(`tzload`/`tzparse` plus `strftime::append_format`), and the P5 resolv code
(`lookup_name`/`msend`). The old ctype/vformat/dtoa cluster sizes in the C74z section were the
`1bfeaafa` baseline; C39m and C74zb removed the ctype and six-vformat duplication.

## Budget actions

* C74z added x86_64 P9 tiny/server budgets from the `1bfeaafa` first measurement + 3 percent
  (the same rule as the other arches).
* C74z replaced the aarch64/riscv64 B measured/budget fields with the `1bfeaafa` Berkeley-bss
  measurement + 3 percent. At C74z the S/F fields remained the C69 values.
* C74z-S10 re-measured the six static Rust P9 rows on `e414e47d` after C39m/C74zb and re-froze
  only their S/F budgets at measured + 3 percent: x86_64 1,172,203/1,173,714 and
  1,295,089/1,296,589; aarch64 1,359,848/1,361,347 and 1,387,505/1,389,001; riscv64
  868,494/870,128 and 924,842/926,473. The P9 B budgets and all smaller-profile/boot budgets are
  unchanged.
* C74z-S11 re-froze the six B-only/layout rows on `72c630bd`: x86_64 P5 tiny 13,399, P3 server
  15,978, P5 server 13,423; aarch64 P8 tiny 12,393, P8 server 13,489; riscv64 P8 server 15,055
  (all B budgets, measured + 3 percent). No S/F or boot budget is changed. The aarch64 P2/P3 S
  measurement correction is reported, not budgeted.
* The 3.1 count is now **17 pass / 48 fail / 69 known / 105 skipped / 0 budget-only**; the
  remaining 48 are 44 real S/F, 2 `INV-S-P2-P0` and 2 boot rows. 3.1 remains **NOT MET**.
* `BUDGETS.toml` is normally generated by `libc-size freeze`; C74z/C74z-S10/C74z-S11 use
  targeted data edits because a full freeze would touch every row (and, for C74z, raise every
  S/F budget). The header comment records the reason and the measurement base.
* C74z-S12 does **not** change any budget. It classifies the 44 S/F rows and proposes (a)
  current+3% for P0/P1/P4/P6 and the x86/aarch64 P3 rows, and (b) shrink-first for P2/P5/P7 and
  riscv P3. The proposal is in the C74z-S12 section; the 3.1 count remains 17/48.

## C39m ctype table shrink (OBSERVED, `e1f252e5`)

`libc/ravenlibc/crates/raven-libc-ctype` replaces the two-level property table
(`PROPS_STAGE1` + `PROPS_STAGE2`) with non-zero range encoding and shrinks the case blocks from
256 to 64 code points. The generated table total is **33,304 → 19,996 bytes (-13,308, -40.0%)**;
`PROPS_STAGE2`/`PROPS_STAGE1` are gone, `CASE_STAGE2` is 6,912 → 4,224 and `CASE_STAGE1`
512 → 2,048, with the 179 case pairs unchanged. The range arrays are 1,756 ranges (starts u32,
lengths u16, values u8) and the lookup is a binary search plus a bounds check.

Before/after measured with the same `libc-size` build on a clean `origin/master` `e1f252e5`
worktree and on the C39m branch:

| Cell | before S/B/F | after S/B/F | delta S |
|---|---|---|---|
| x86_64 tiny P8 | 167024/11560/168488 | 153824/12480/155280 | -13,200 |
| x86_64 tiny P9 | 1206381/67600/1207848 | 1193197/68488/1194672 | -13,184 |
| aarch64 tiny P8 | 183796/15280/185248 | 170656/12032/172112 | -13,140 |
| aarch64 tiny P9 | 1365588/68148/1367040 | 1352436/69012/1353888 | -13,152 |
| riscv64 tiny P8 | 143148/14968/144720 | 129982/11744/131560 | -13,166 |
| riscv64 tiny P9 | 893350/39868/894936 | 880178/40748/881768 | -13,172 |

The 3.1 row improves from **7 pass / 56 fail** to **8 pass / 55 fail** (69 known, 105 skipped, 0
budget-only). The new pass is `P8 riscv64 tiny`; no row becomes a new fail. `P8 aarch64 tiny` now
fits S/F but its B is 12,032 > 11,981 (the code-layout/relro padding moved 96 bytes), so that row
stays B-only fail. The same table layout shifts B slightly upward on `aarch64 P8 server` and
`riscv64 P9 server`; those rows already fail S/F, so the fail count is unchanged. No budget is
changed by C39m.

The exhaustive `ucd_oracle::every_code_point_agrees_with_the_oracle` test compares the new
lookup against the independent UCD-derived oracle for every code point in 0..=0x10FFFF (137
ctype tests pass, including the generator-freshness and table-budget checks).
