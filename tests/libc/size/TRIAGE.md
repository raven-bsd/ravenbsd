<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# libc size gate 3.1 triage and budget correction (C74w/C74z/C74z-S10)

Status: OBSERVED (C74ia, `origin/master` `fb65ec96`). C74w classified the C74h/C69 data;
C74z corrected the objective budget-data errors; C74z-S10 re-froze the six P9 S/F budgets;
C74z-S11 rebuilt all three arches and fixed the six B-only/layout rows; C74z-S12 classified the
S/F residual; C74z-S14/S15a/S5/S6/S13 landed and C74z-S16 applied the reviewed current+3%
budgets to the 28 legitimate P0/P1/P4/P6 and x86_64/aarch64 P3 rows. The C74dl audit found
**17 legitimate budget/layout rows (0 shrink-first), 1 rule/probe bug (P7-R-14), and 10 stale
invariant ceilings**; C74z-S17 applied the 17 re-freezes and C74cx fixed the P7-R-14 matcher.
C74ht rebuilt the real x86_64/aarch64 `libc.so.1` rows and applied their measured+3% budgets.
C74ia adds the riscv64 `libc.so.1` budget and the third NEEDED known row. C74ib tested aarch64
DSO size reductions; C74id later landed the actual DSO change. C74in's full three-arch rebuild
exposes one more row: P7 riscv64 static server B 15,488 > 12,385, but the growth is
`.relro_padding` (not `.bss`) from C74ij's +246-byte `.text` change crossing a page boundary.
**3.1 remains NOT MET**: 63 pass / 13 fail / 65 known / 99 skipped / 0 budget-only; the 13 are
the 10 static invariants, the P7 riscv64 server B layout row, and the two riscv64 P9 static S/F
rows.

## C74z-S9 boot-row closure (OBSERVED, `6af246ac`)

C74z-S9a (`dab7fe6c`, merged `6862c7ef`) made fd-table/stdio and main-thread TLS page setup
lazy; no budget was raised. A fresh `cargo xtask libc-size --check` on this tree reports:

| boot row | measured / budget | result |
|---|---|---|
| x86_64 syscalls before `main` | 0 / 0 | PASS |
| x86_64 start instructions | 313 / 908 | PASS |
| aarch64 syscalls before `main` | 3 / 3 | PASS |
| aarch64 start instructions | 1235 / 1847 | PASS |
| riscv64 syscalls before `main` | 0 / 0 | PASS |
| riscv64 start instructions | 40 / 41 | PASS |

The three C74z-S9 regressions are closed; the eager fd allocator and TLS page-path subtrees are
outside the static start graph. The check still exits 1 because 61 non-boot rows from the other
C74z clusters remain (15 pass / 61 fail / 65 known / 99 skipped), but no boot row fails.

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
| C74z-S16 | 3.1 reconciliation | Done in C74z-S16: applied the reviewed current+3% budgets to the 28 legitimate P0/P1/P4/P6 and x86_64/aarch64 P3 rows after S14/S15a/S5/S6/S13; shrink-first rows unraised | reviewed budget | lance/C74z-S16 | 46 pass / 28 fail; exact residual and owners in the C74z-S16 section |

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

## C74z-S16 reconciliation (OBSERVED, `1355d85c`)

The S14/S15a/S5/S6/S13 merges are in `origin/master` `1355d85c`. A full three-arch rebuild
(different from the directive's 32/50 last run, which lacked a full arch build) first gave
**31 pass / 43 fail** because many rows had become B-only layout rows after the S/F shrink.
`tests/libc/size/reconcile.py` compared the S12 before run with this build and proposed the
current+3% budgets for the 28 legitimate P0/P1/P4/P6 and x86_64/aarch64 P3 rows; it listed
P2/P5/P7/riscv64 P3 as `HOLD`.

The 28 legitimate `BUDGETS.toml` blocks were re-frozen from the helper proposal (S/B/F measured +
3 percent; `members` updated). The 32 shrink-first/held blocks were left byte-identical, including
P2/P5/P7/riscv64 P3 and P8/P9. The gate after the budget edit is **46 pass / 28 fail / 69 known /
105 skipped / 0 budget-only**.

S15a removed the eager `dtoa` floor from P0/P1/P4/P6 (about -17.6k S on x86/aarch64 and -10.7k
to -12.7k on riscv64). S6 and S5 removed most of the P7/P5 S growth; S14 made the x86_64 P2 rows
pass; S13 links the real riscv64 printf path, so the riscv P2/P3 rows now measure the real (and
still over) sizes. The remaining rows are deliberately not budgeted:

| Row | Current/budget | Owner |
|---|---|---|
| P7 x86_64 tiny | B 15080 > 14915 | mel/C44 (S6; B/layout) |
| P5 x86_64 server | B 13656 > 13423 | dave/C51 (S5; B/layout) |
| P7 x86_64 server | B 14880 > 12509 | mel/C44 (S6; B/layout) |
| P2 aarch64 tiny | S 93003 > 78235; F 94464 > 79731 | carl/C43 (S14 rework) |
| P7 aarch64 tiny | S 99836 > 96071; F 101288 > 97570 | mel/C44 (S6; R-14 iconv lane) |
| P8 aarch64 tiny | B 13304 > 12393 | carl/C45/C41/C40 (P8; held) |
| P2 aarch64 server | S 90101 > 73143; F 91560 > 74638 | carl/C43 (S14 rework) |
| P5 aarch64 server | B 14464 > 11916 | dave/C51 (S5; B/layout) |
| P7 aarch64 server | S 94438 > 92659; B 15176 > 12550; F 95896 > 94159 | mel/C44 (S6; R-14 iconv lane) |
| P8 aarch64 server | B 13848 > 13489 | carl/C45/C41/C40 (P8; held) |
| P2 riscv64 tiny | S 67799 > 56282; F 69376 > 57911 | carl/C43 (S14/S13 rework) |
| P3 riscv64 tiny | S 78603 > 56302; B 13984 > 13399; F 80184 > 57928 | carl/C43 (S14/S13 rework) |
| P7 riscv64 tiny | S 72622 > 69979; B 12384 > 12056; F 74200 > 71598 | mel/C44 (S6) |
| P2 riscv64 server | S 68227 > 52879; F 69808 > 54516 | carl/C43 (S14/S13 rework) |
| P3 riscv64 server | S 80627 > 52899; F 82216 > 54533 | carl/C43 (S14/S13 rework) |
| P5 riscv64 server | B 15136 > 14808 | dave/C51 (S5; B/layout) |
| P7 riscv64 server | S 68920 > 67381; F 70496 > 69010 | mel/C44 (S6) |
| rule P7 aarch64 | R-14: `iconv*` member pulled | mel/C44 + iconv lane |
| INV-S-P3-P2 x86 tiny/server | 17,160 / 23,104 > 10,240 | carl/C43 + reviewer (limit) |
| INV-S-P2-P0 aarch64 tiny/server | 26,671 / 29,179 > 5,120 | carl/C43 + reviewer (limit) |
| INV-S-P3-P2 aarch64 tiny/server | 16,964 / 18,272 > 10,240 | carl/C43 + reviewer (limit) |
| INV-S-P2-P0 riscv64 tiny/server | 19,075 / 23,547 > 5,120 | carl/C43 + reviewer (limit) |
| INV-S-P3-P2 riscv64 tiny/server | 10,804 / 12,400 > 10,240 | carl/C43 + reviewer (limit) |

**3.1 remains NOT MET** at 46/28. At S16 the P2/P5/P7/riscv P3 rows were left unraised pending
the C74dl audit (below), and P8 was held. No source-crate edit was made.

## C74dl rule-row audit (OBSERVED, `94d35d15`)

C74dl re-ran the gate on `origin/master` `94d35d15` (the C74z-S14 rework is `7ddebb15`; C74cx
has no branch or commit at this base) and reproduced **46 pass / 28 fail / 69 known / 105
skipped / 0 budget-only**. The audit decomposed every B row with `llvm-size --format=sysv`,
inspected the P2/P3/P7 symbols and the P7 map, and read `rules.toml`/`rules.rs`.

The S16 label "17 budget/rule rows" conflated the rule with the budgets. The exact residual is
**17 budget rows + 1 rule row + 10 invariant rows = 28**:

| Kind | Rows | Classification |
|---|---|---|
| budget rows | 17 | 17 legitimate budget/layout, **0 shrink-first** |
| rule row | 1 | `P7-R-14` false positive (rule/probe bug) |
| invariant rows | 10 | stale `PROPOSED` ceilings; reviewer decision |
| **total** | **28** | **3.1 NOT MET** |

### The 17 budget rows

| Row | Measured / budget | Over | Evidence | Class | Owner |
|---|---|---|---|---|---|
| P7 x86_64 tiny | B 15,080 / 14,915 | B +165 | `.bss` 11,520 + `.relro_padding` 3,560 | budget/layout | mel/C44 (S6) |
| P5 x86_64 server | B 13,656 / 13,423 | B +233 | `.bss` 11,552 + `.relro_padding` 2,104 | budget/layout | dave/C51 (S5) |
| P7 x86_64 server | B 14,880 / 12,509 | B +2,371 | `.bss` 11,552 + `.relro_padding` 3,328 | budget/layout | mel/C44 (S6) |
| P2 aarch64 tiny | S 93,055 / 78,235; F 94,512 / 79,731 | S +14,820; F +14,781 | real `vformat_dyn` printf path; dS(P2-P0)=26,671 | stale budget (stub-era 75,956) | carl/C43 (S14) |
| P7 aarch64 tiny | S 99,888 / 96,071; F 101,344 / 97,570 | S +3,817; F +3,774 | real tz code: `tzload` 3,956, `tzparse` 2,188, `append_format` 2,244 | stale budget | mel/C44 (S6) |
| P8 aarch64 tiny | B 13,256 / 12,393 | B +863 | `.bss` 11,424 + `.relro_padding` 1,832 | budget/layout | P8 lane (carl/C45/C41/C40) |
| P2 aarch64 server | S 90,101 / 73,143; F 91,560 / 74,638 | S +16,958; F +16,922 | same real printf path; dS(P2-P0)=29,179 | stale budget (stub-era 71,012) | carl/C43 (S14) |
| P5 aarch64 server | B 14,464 / 11,916 | B +2,548 | `.bss` 11,552 + `.relro_padding` 2,912 | budget/layout | dave/C51 (S5) |
| P7 aarch64 server | S 94,438 / 92,659; B 15,176 / 12,550; F 95,896 / 94,159 | S +1,779; B +2,626; F +1,737 | real time code; `.bss` 11,552 + `.relro_padding` 3,624 | stale budget/layout | mel/C44 (S6) |
| P8 aarch64 server | B 13,848 / 13,489 | B +359 | `.bss` 11,456 + `.relro_padding` 2,392 | budget/layout | P8 lane (carl/C45/C41/C40) |
| P2 riscv64 tiny | S 67,803 / 56,282; F 69,384 / 57,911 | S +11,521; F +11,473 | real variadic printf after S13; dS(P2-P0)=19,075 | stale budget (stub-era 54,642) | carl/C43 (S13/S14) |
| P3 riscv64 tiny | S 78,607 / 56,302; B 13,984 / 13,399; F 80,184 / 57,928 | S +22,305; B +585; F +22,256 | real `dtoa` path; dS(P3-P2)=10,804; `.bss` 11,408 + `.relro_padding` 2,576 | stale budget/layout (stub-era 54,662) | carl/C43 (S13/S14) |
| P7 riscv64 tiny | S 72,626 / 69,979; B 12,384 / 12,056; F 74,200 / 71,598 | S +2,647; B +328; F +2,602 | real time code; `.bss` 11,520 + `.relro_padding` 864 | stale budget/layout | mel/C44 (S6) |
| P2 riscv64 server | S 68,227 / 52,879; F 69,808 / 54,516 | S +15,348; F +15,292 | real variadic printf; dS(P2-P0)=23,547 | stale budget (stub-era 51,338) | carl/C43 (S13/S14) |
| P3 riscv64 server | S 80,627 / 52,899; F 82,216 / 54,533 | S +27,728; F +27,683 | real `dtoa` path; dS(P3-P2)=12,400 | stale budget (stub-era 51,358) | carl/C43 (S13/S14) |
| P5 riscv64 server | B 15,136 / 14,808 | B +328 | `.bss` 11,552 + `.relro_padding` 3,584 | budget/layout | dave/C51 (S5) |
| P7 riscv64 server | S 68,920 / 67,381; F 70,496 / 69,010 | S +1,539; F +1,486 | real time code; one `append_format` | stale budget | mel/C44 (S6) |

### B is `.bss` + `.relro_padding`, so the B overruns are layout

The B metric is Berkeley bss (all NOBITS), not the `.bss` section alone. Every failing B row
decomposes exactly as `.bss` + `.relro_padding`, and `.bss` is 11,408-11,552 on every arch/set:

| Row | `.bss` | `.relro_padding` | B |
|---|---:|---:|---:|
| P7 x86_64 tiny | 11,520 | 3,560 | 15,080 |
| P5 x86_64 server | 11,552 | 2,104 | 13,656 |
| P7 x86_64 server | 11,552 | 3,328 | 14,880 |
| P8 aarch64 tiny | 11,424 | 1,832 | 13,256 |
| P5 aarch64 server | 11,552 | 2,912 | 14,464 |
| P7 aarch64 server | 11,552 | 3,624 | 15,176 |
| P8 aarch64 server | 11,456 | 2,392 | 13,848 |
| P3 riscv64 tiny | 11,408 | 2,576 | 13,984 |
| P7 riscv64 tiny | 11,520 | 864 | 12,384 |
| P5 riscv64 server | 11,552 | 3,584 | 15,136 |

The overrun is always smaller than the `.relro_padding`; no row has abnormal `.bss` growth. The
B fields are stale layout budgets, not a probe or heap regression, so they are re-frozen (or the
RELRO alignment is changed) by the owner/reviewer, not "shrunk" in code.

### P2/P3 S/F rows are the real printf/dtoa path, not duplication

S14 `7ddebb15` split the integer-only `vformat` path and S13 enabled the real riscv variadic
reader. Each P2 binary contains exactly one `vformat_dyn` (3,244 B on aarch64 tiny) and no
duplicate formatter; P3 adds the `dtoa`/bigint/float helper path (`dtoa` 8,418 B on x86_64,
6,248 B on aarch64, 4,906 B on riscv64). The P2-P0 and P3-P2 deltas are real arch codegen
differences, not a second copy:

| Pair | x86_64 tiny | aarch64 tiny | riscv64 tiny |
|---|---:|---:|---:|
| S(P2)-S(P0) | 22,048 | 26,671 | 19,075 |
| S(P3)-S(P2) | 17,160 | 16,964 | 10,804 |

The failure is stale budget data, not the code. The x86_64 P2 budget block still carries the
pre-S14 measurement (90,100 tiny / 98,722 server), so its current 73,899 / 78,237 passes with
about 19k / 23k of slack. The aarch64 P2 blocks carry the stub-era measurement (75,956 / 71,012)
and the riscv P2/P3 blocks carry 54,642 / 51,338 and 54,662 / 51,358, all before the real printf
path was linked. Those budgets must be re-frozen at the current + 3 percent; no further shrink
ticket is justified by this data.

### P7 S/F rows are the realized time code

S6 removed the shared `strptime`/`strftime` scanner duplication. The aarch64 P7 binaries carry
one `tzload` (3,632-3,956 B), one `tzparse` (2,188-3,144 B), one `strftime::append_format`
(2,244-4,700 B) and one `mktime::time2sub` (1,284-1,604 B); the riscv64 and x86_64 rows carry
the same single set at their own sizes. The S/F overruns are 1.5-3.8k, not a duplicated parser.
They are stale budgets in the mel/C44 lane.

### `P7-R-14` is a rule/probe false positive

The rule matches the archive member name, and the aarch64 P7 link contains the
`raven_libc_iconv` codegen unit because a generic allocator instantiation was emitted there.
The tiny and server maps show that member contributes only `.eh_frame` (0x14),
`<alloc::raw_vec::RawVecInner>::deallocate` (0x50) and its drop glue (0xc); there is no
`iconv_open`/`iconv` symbol and no conversion code. The other `P7-R-14` patterns are clean (the
`raven_libc_time::locale::CLocale` symbols are the time crate's own locale table, not a locale
crate member). This is a matcher false positive: match the called symbols (or exclude
allocator-only instantiations) instead of the codegen-unit name. Owner: size lane + mel/C44.
The iconv rule itself must not be weakened.

### The 10 invariant rows are stale `PROPOSED` ceilings

The limits are hard-coded proposal constants in `tools/libc-size/src/rules.rs`
(`INV-S-P2-P0` 5,120, `INV-S-P3-P2` 10,240), not frozen measurements. They are below the real
printf/dtoa path on every arch:

| Invariant | Rows | Measured delta | Limit | Note |
|---|---|---|---|---|
| `INV-S-P2-P0` | aarch64 tiny/server | 26,671 / 29,179 | 5,120 | x86 P2-P0 22,048 / 27,832 is already `KNOWN` |
| `INV-S-P2-P0` | riscv64 tiny/server | 19,075 / 23,547 | 5,120 | same real printf path after S13 |
| `INV-S-P3-P2` | x86_64 tiny/server | 17,160 / 23,104 | 10,240 | `dtoa` path is required by P3 |
| `INV-S-P3-P2` | aarch64 tiny/server | 16,964 / 18,272 | 10,240 | `dtoa` 6,248 B alone |
| `INV-S-P3-P2` | riscv64 tiny/server | 10,804 / 12,400 | 10,240 | `dtoa` path after S13 |

The x86_64 P2-P0 rows are grandfathered by `[[known-violation]]` entries from the first
measurement; the aarch64/riscv rows fail only because they have no equivalent row. The reviewer
must either raise the proposal ceilings or add the same known-violation rows. This is a
reviewer/limit decision (owner carl/C43 + reviewer), not a shrink-first defect and not a budget
that S16 can apply.

### Exact 3.1 residual and owners

The 28 remaining rows are therefore:

| Owner | Rows | Detail |
|---|---|---|
| carl/C43 | 6 budget + 10 invariant | P2 aarch64 tiny/server, P2 riscv64 tiny/server, P3 riscv64 tiny/server; the 10 invariant ceilings |
| mel/C44 | 6 budget + 1 rule | P7 x86_64 tiny/server, P7 aarch64 tiny/server, P7 riscv64 tiny/server; `P7-R-14` |
| dave/C51 | 3 budget | P5 x86_64/aarch64/riscv64 server B |
| P8 lane (carl/C45/C41/C40) | 2 budget | P8 aarch64 tiny/server B |
| **total** | **17 budget + 1 rule + 10 invariant = 28** | **3.1 NOT MET** |

**0 rows are shrink-first.** The 17 budget rows are legitimate feature growth or linker layout;
C74dl itself applied no budget. C74z-S17 applies the re-freeze at measured + 3 percent and
verifies the C74cx rule fix; the invariants still need a reviewer ceiling decision. The details
are below.

## C74z-S17 budget and rule application (OBSERVED, `10d8c9dd`)

C74z-S17 starts from `origin/master` `10d8c9dd` (C74dl `2eefd5c5` and C74cx `e76b1d01` merged)
and re-runs the full three-arch check. The pre-budget gate is **46 pass / 27 fail / 62 known /
102 skipped / 0 budget-only**: exactly the 17 legitimate budget rows plus the 10 invariants. The
`P7-R-14` false positive is already gone.

### P7-R-14 matcher fix (C74cx `e76b1d01`)

C74cx replaced the P7 `forbid_members` `iconv*` pattern with `forbid_symbols` for the iconv
lifecycle entry points (`iconv_open`, `iconv_close`, `iconvctl`, `__iconv_open`,
`__iconv_close`) and kept `locale*`, `collat*`, `langinfo*`, `catopen*` as member patterns. The
allocator-only `raven_libc_iconv` codegen unit no longer fails the rule. The S17 gate has no
`P7-R-14` FAIL; the x86_64 `locale*` `KNOWN` rows are the pre-existing first-measurement
exceptions, not the iconv false positive. No further `rules.toml` change is made by S17.

### Applied budgets

The 17 blocks are re-frozen at `ceil(current * 103 / 100)` from the S17 measurement. The
measured fields and `members` are updated in the same blocks; no other budget block changes.

| Row | Measured S/B/F | Old budget S/B/F | New budget S/B/F |
|---|---|---|---|
| P7 x86_64 tiny | 82,171 / 15,120 / 83,632 | 84,644 / 14,915 / 86,158 | 84,637 / 15,574 / 86,141 |
| P5 x86_64 server | 124,135 / 13,656 / 125,592 | 155,287 / 13,423 / 156,791 | 127,860 / 14,066 / 129,360 |
| P7 x86_64 server | 82,437 / 14,880 / 83,904 | 91,316 / 12,509 / 92,816 | 84,911 / 15,327 / 86,422 |
| P2 aarch64 tiny | 92,979 / 11,896 / 94,440 | 78,235 / 13,531 / 79,731 | 95,769 / 12,253 / 97,274 |
| P7 aarch64 tiny | 99,812 / 13,872 / 101,264 | 96,071 / 15,986 / 97,570 | 102,807 / 14,289 / 104,302 |
| P8 aarch64 tiny | 153,008 / 13,312 / 154,464 | 180,904 / 12,393 / 182,401 | 157,599 / 13,712 / 159,098 |
| P2 aarch64 server | 90,081 / 14,848 / 91,536 | 73,143 / 15,113 / 74,638 | 92,784 / 15,294 / 94,283 |
| P5 aarch64 server | 139,716 / 14,464 / 141,168 | 160,744 / 11,916 / 162,246 | 143,908 / 14,898 / 145,404 |
| P7 aarch64 server | 94,438 / 15,176 / 95,896 | 92,659 / 12,550 / 94,159 | 97,272 / 15,632 / 98,773 |
| P8 aarch64 server | 148,422 / 13,848 / 149,880 | 177,097 / 13,489 / 178,594 | 152,875 / 14,264 / 154,377 |
| P2 riscv64 tiny | 67,757 / 12,544 / 69,336 | 56,282 / 13,423 / 57,911 | 69,790 / 12,921 / 71,417 |
| P3 riscv64 tiny | 78,561 / 14,024 / 80,144 | 56,302 / 13,399 / 57,928 | 80,918 / 14,445 / 82,549 |
| P7 riscv64 tiny | 72,580 / 12,416 / 74,152 | 69,979 / 12,056 / 71,598 | 74,758 / 12,789 / 76,377 |
| P2 riscv64 server | 68,101 / 12,232 / 69,680 | 52,879 / 15,582 / 54,516 | 70,145 / 12,599 / 71,771 |
| P3 riscv64 server | 80,501 / 12,112 / 82,088 | 52,899 / 15,558 / 54,533 | 82,917 / 12,476 / 84,551 |
| P5 riscv64 server | 106,267 / 15,136 / 107,848 | 121,239 / 14,808 / 122,875 | 109,456 / 15,591 / 111,084 |
| P7 riscv64 server | 68,920 / 12,024 / 70,496 | 67,381 / 13,802 / 69,010 | 70,988 / 12,385 / 72,611 |

The post-budget gate is **63 pass / 10 fail / 62 known / 102 skipped / 0 budget-only**. All 17
budget/layout rows pass; no rule row fails.

### Exact residual: the 10 invariants

The ceilings are unchanged (`INV-S-P2-P0` 5,120, `INV-S-P3-P2` 10,240) by ticket. The current
deltas are:

| Invariant | Rows | Measured delta | Limit |
|---|---|---|---|
| `INV-S-P2-P0` | aarch64 tiny/server | 26,671 / 29,159 | 5,120 |
| `INV-S-P2-P0` | riscv64 tiny/server | 19,075 / 23,421 | 5,120 |
| `INV-S-P3-P2` | x86_64 tiny/server | 17,160 / 23,104 | 10,240 |
| `INV-S-P3-P2` | aarch64 tiny/server | 16,964 / 18,272 | 10,240 |
| `INV-S-P3-P2` | riscv64 tiny/server | 10,804 / 12,400 | 10,240 |

The 10 rows are 4 `INV-S-P2-P0` and 6 `INV-S-P3-P2` invariant failures. They are real P2/P3
relationships, not budgets: the reviewer/owner must raise the hard-coded proposal ceilings or
add the equivalent known-violation rows. **3.1 remains NOT MET** with exactly these 10 rows.

## C74ht dynamic libc.so.1 rules (OBSERVED, `6cb10821`)

C74ht rebuilt the real linked DSOs on `6cb10821` with
`cargo xtask libc-pack --arch x86_64|aarch64` and `cargo xtask libc-link --arch x86_64|aarch64`.
C74hr has no commits on its branch (it is marked done with uncommitted `build/libc-size/**`
artifacts), so the dynamic evidence is reproduced from the tooling on this base.

### DSO evidence

| Arch | S | F | `.data.rel.ro` | `JUMP_SLOT` | `DT_NEEDED` |
|---|---:|---:|---:|---:|---:|
| x86_64 | 1,286,594 | 3,009,232 | 24,600 | 0 | 0 |
| aarch64 | 1,363,768 | 2,501,104 | 24,224 | 2 | 0 |

`readelf -d` on both DSOs shows `SONAME: [libc.so.1]`, `FLAGS: BIND_NOW`, `FLAGS_1: NOW
NODELETE`, and **no `DT_NEEDED` entry**. The DSO does not list itself; the loader resolves it by
SONAME. The riscv64 row stays `skipped: libc.so.1 absent`, blocked by C74hs.

### Zero-budget fix

The two `[[libc_so]]` rows were stub-era zero budgets. They are re-frozen at measured + 3 percent:

| Row | Old budget S/F | Measured S/F | New budget S/F |
|---|---|---|---|
| x86_64 `libc.so.1` | 0 / 0 | 1,286,594 / 3,009,232 | 1,325,192 / 3,099,509 |
| aarch64 `libc.so.1` | 0 / 0 | 1,363,768 / 2,501,104 | 1,404,682 / 2,576,138 |

Both `libc.so.1` budget rows now PASS. The riscv64 row is unchanged and skipped.

### `INV-LIBC-SO-NEEDED` rule bug

`INV-LIBC-SO-NEEDED` is hard-coded in `tools/libc-size/src/rules.rs` as `needed == 1`, but the
linked DSO has 0. That is the self-`NEEDED` expectation the loader does not emit. The check is
outside this ticket's Owns (no source edits), so the gate records a reviewed known exception for
x86_64/aarch64 with the `readelf -d` evidence. Changing the hard-coded expectation to 0 for a
self-contained `libc.so.1` is a follow-up tooling ticket; this ticket does not claim the rule
itself is fixed.

### Exact 3.1 residual after the dynamic fix

| Kind | Rows | Status |
|---|---|---|
| static `INV-S-P2-P0`/`INV-S-P3-P2` invariants | 10 | FAIL (unchanged) |
| aarch64 `INV-LIBC-SO-S` | 1 | FAIL: 1,363,768 > 1,300,000 (real S overrun; ceiling not raised) |
| `INV-LIBC-SO-NEEDED` | 2 | KNOWN (rule bug, `readelf -d` evidence above) |
| `libc.so.1` budgets | 2 | PASS |
| riscv64 `libc.so.1` | 1 | SKIPPED (C74hs) |
| **3.1 total** | | **65 pass / 11 fail / 64 known / 100 skipped / 0 budget-only** |

**3.1 remains NOT MET** with exactly 11 real fails: the 10 static invariants plus the aarch64
`libc.so.1` S ceiling. The two NEEDED rows are KNOWN rule exceptions, not passes.

## C74ia riscv64 libc.so.1 budget (OBSERVED, `fb65ec96`)

C74hs made `libc-link --arch riscv64` succeed. C74ia built the DSO and measured it:

| Arch | S | F | `.data.rel.ro` | `JUMP_SLOT` | `DT_NEEDED` |
|---|---:|---:|---:|---:|---:|
| riscv64 | 1,010,867 | 2,490,256 | 24,776 | 2 | 0 |

`readelf -d` shows `SONAME: [libc.so.1]`, `FLAGS: BIND_NOW`, `FLAGS_1: NOW NODELETE`, and no
`DT_NEEDED`. The riscv64 `libc.so.1` row is re-frozen at measured + 3 percent:

| Row | Old budget S/F | Measured S/F | New budget S/F |
|---|---|---|---|
| riscv64 `libc.so.1` | 0 / 0 (skipped) | 1,010,867 / 2,490,256 | 1,041,194 / 2,564,964 |

The row is now `status = "ok"`; x86_64 and aarch64 rows are unchanged. The riscv64
`INV-LIBC-SO-NEEDED` row joins the C74ht known exception (0 `DT_NEEDED` vs the hard-coded 1), so
all three NEEDED rows are KNOWN, not passes.

### P9 riscv64 static growth from C74hs (not budgeted here)

The full-link P9 rows also grew after C74hs added the 151 compat/IPC exports:

| Row | S measured/budget | F measured/budget | Delta |
|---|---|---|---|
| P9 riscv64 tiny | 904,084 / 868,494 | 905,672 / 870,128 | +35,590 S / +35,544 F |
| P9 riscv64 server | 964,226 / 924,842 | 965,816 / 926,473 | +39,384 S / +39,343 F |

These are real S/F growth from the C74hs source change, not a dynamic-row measurement artifact.
C74ia's contract is the riscv64 `libc_so` budget, so these two rows are **left unbudgeted** and
remain FAIL. They need a C74hs-lane owner decision or a dedicated budget ticket.

### Exact 3.1 residual

| Kind | Rows | Status |
|---|---|---|
| static `INV-S-P2-P0`/`INV-S-P3-P2` invariants | 10 | FAIL (unchanged) |
| aarch64 `INV-LIBC-SO-S` | 1 | FAIL (real 1,363,768 > 1,300,000; ceiling not raised) |
| P9 riscv64 static tiny/server | 2 | FAIL (C74hs growth; not budgeted here) |
| `INV-LIBC-SO-NEEDED` | 3 | KNOWN (rule bug, `readelf -d` evidence) |
| `libc.so.1` budgets | 3 | PASS (x86_64, aarch64, riscv64) |
| **3.1 total** | | **64 pass / 13 fail / 65 known / 99 skipped / 0 budget-only** |

**3.1 remains NOT MET** with 13 real fails. The three dynamic `libc.so.1` budgets pass and the
three NEEDED rows are documented rule exceptions.

## C74ib aarch64 DSO size options (OBSERVED, `07054226`)

C74ib measured the real aarch64 `libc.so.1` (**S 1,363,752 / F 2,501,104**) and tested reductions
on scratch copies with `LD_LLD`/`RUSTC` wrappers. No tracked build file was changed: all candidate
flags are outside this ticket's Owns.

### Section breakdown

S = text 1,331,520 + data 32,232. The largest allocated sections:

| Section | Bytes | Share of S |
|---|---:|---:|
| `.text` | 1,013,616 | 74.3% |
| `.eh_frame` | 167,204 | 12.3% |
| `.rodata` | 74,452 | 5.5% |
| `.dynsym` | 41,088 | 3.0% |
| `.data.rel.ro` | 24,224 | 1.8% |
| `.dynstr` | 18,288 | 1.3% |
| `.gnu.hash` | 12,652 | 0.9% |
| `.data` | 7,000 | 0.5% |
| `.gnu.version` | 3,424 | 0.3% |
| `.relro_padding` | 2,592 | 0.2% |

The remaining allocated sections are `.got` 616, `.relr.dyn` 432, `.dynamic` 352, `.rela.dyn`
168, `.gnu.version_d` 84, `.plt` 64, `.rela.plt` 48 and `.got.plt` 40. The non-allocated debug
sections (F only) are `.debug_str` 66,982, `.debug_info` 53,377, `.debug_line` 42,288,
`.debug_ranges` 20,848, `.debug_abbrev` 7,938 and `.debug_aranges` 3,280.

### Large symbol groups

| Group | Bytes | | Group | Bytes |
|---|---:|---|---|---:|
| `raven_libc_m` (fdlibm) | 129,248 | | `raven_libc_resolv` | 43,136 |
| `raven_libc` | 113,119 | | `raven_libc_compat` | 41,628 |
| `alloc` | 96,236 | | `raven_libc_platform` | 33,576 |
| `raven_libc_fmt` | 66,540 | | `raven_libc_time` | 32,536 |
| `raven_libc_regex` | 48,948 | | `raven_libc_ctype` | 26,728 |
| `raven_libc_dtoa` | 45,676 | | `core` | 24,679 |

The largest single functions are `strtodg` 10,196, `strtod` 8,788, `dtoa` 6,860, regex `p_re`
6,128, and the two `vformat_dyn` monomorphizations 5,868 / 5,616. `.eh_frame` is the only single
section large enough to clear the 63,752-byte gap by itself, but it is the unwind/backtrace table
and cannot be removed without changing behavior.

### Candidate deltas (scratch copies)

| Candidate | S | F | ΔS | Under 1,300,000? | `libc-link` | Assessment |
|---|---:|---:|---:|---|---|---|
| Baseline | 1,363,752 | 2,501,104 | 0 | no (+63,752) | ok | current |
| `--icf=all` | 1,306,012 | 2,443,360 | -57,740 | no (+6,012) | ok | insufficient; ICF may fold address-distinct functions |
| `opt-level=z` alone | 1,330,512 | 2,968,440 | -33,240 | no (+30,512) | ok | insufficient; F grows with debug info |
| `--icf=safe` + `opt-level=z` | 1,330,512 | 2,968,440 | -33,240 | no (+30,512) | ok | safe ICF folded nothing |
| `--icf=all` + `opt-level=z` | 1,255,136 | 2,893,064 | -108,616 | **yes** (-44,864) | ok | F over the 2,576,138 budget by 316,926 |
| `--icf=all` + `opt-level=z` + `--strip-debug` | 1,255,136 | 2,451,200 | -108,616 | **yes** (-44,864) | ok | F under budget by 124,938; needs qtests |
| `force-unwind-tables=no` | 1,200,188 | 2,398,568 | -163,564 | yes (-99,812) | ok | **rejected**: `.eh_frame` 167,204 -> 3,860, breaks backtrace/unwind |

`cargo xtask libc-abi check` is **ok (1713 records)**; it checks the symbol lists, not the DSO
runtime. Qtests were not run (the ticket forbids QEMU), so the `--icf=all` function-pointer
identity and the `opt-level=z` runtime behavior are unverified.

### In-Owns feasibility

None of the S-passing candidates can be enabled from `tests/libc/size/**`: the DSO is built by
`xtask/src/libc_link.rs` (linker flags and strip) and `Cargo.toml` `[profile.libc-so]` /
`xtask/src/libc_pack.rs` (`opt-level=z`). Those paths are outside C74ib's Owns. Editing a size
fixture to wrap the linker would fake the build, not reduce the product DSO. **No safe reduction
is implementable in C74ib.** A follow-up tooling ticket should add `--icf=all` and
`--strip-debug` in `libc_link.rs`, switch the libc-so profile to `opt-level=z`, and then run the
qtests; the measured candidate is S 1,255,136 / F 2,451,200.

### riscv64 P9 growth: permanent parity

The two riscv64 P9 static rows are the full `-u` link; C74hs enabled the same `compat` exports
that x86_64/aarch64 already carry. A static full link necessarily includes every export, so the
+35,590/+39,384 S growth is **permanent parity**, not a C74hs defect. Avoiding it would require
excluding `compat` from the static archive (an API/ABI break) or weakening the P9 `-u` probe (not
allowed). The reviewer budget values at current + 3 percent are:

| Row | S measured/budget | B measured/budget | F measured/budget |
|---|---|---|---|
| P9 riscv64 tiny | 904,084 / **931,207** | 35,268 / **36,327** | 905,672 / **932,843** |
| P9 riscv64 server | 964,226 / **993,153** | 32,596 / **33,574** | 965,816 / **994,791** |

These are reviewer options only; C74ib changes no budget, invariant or ceiling. The aarch64
`INV-LIBC-SO-S` row remains FAIL and **3.1 remains NOT MET** at 64 pass / 13 fail / 65 known /
99 skipped.

## C74in full three-arch rebuild and P7 B attribution (OBSERVED, `d0429e95`)

A full pack/link of all three arches on `d0429e95` yields **63 pass / 13 fail / 65 known / 99
skipped**. The extra row vs the C74ib snapshot is **P7 riscv64 static server B 15,488 > 12,385**.

### Exact 13 fails

| # | Row | Measured / budget | Class |
|---|---|---|---|
| 1 | P9 riscv64 tiny | S 906,278 > 868,494; F 907,864 > 870,128 | real C74hs parity |
| 2 | P7 riscv64 server | B 15,488 > 12,385 | `.relro_padding` layout (C74ij) |
| 3 | P9 riscv64 server | S 966,556 > 924,842; F 968,144 > 926,473 | real C74hs parity |
| 4 | `INV-S-P3-P2` x86_64 tiny | 17,160 > 10,240 | stale ceiling |
| 5 | `INV-S-P3-P2` x86_64 server | 23,088 > 10,240 | stale ceiling |
| 6 | `INV-S-P2-P0` aarch64 tiny | 26,671 > 5,120 | stale ceiling |
| 7 | `INV-S-P3-P2` aarch64 tiny | 16,964 > 10,240 | stale ceiling |
| 8 | `INV-S-P2-P0` aarch64 server | 29,159 > 5,120 | stale ceiling |
| 9 | `INV-S-P3-P2` aarch64 server | 18,272 > 10,240 | stale ceiling |
| 10 | `INV-S-P2-P0` riscv64 tiny | 19,067 > 5,120 | stale ceiling |
| 11 | `INV-S-P3-P2` riscv64 tiny | 10,804 > 10,240 | stale ceiling |
| 12 | `INV-S-P2-P0` riscv64 server | 23,429 > 5,120 | stale ceiling |
| 13 | `INV-S-P3-P2` riscv64 server | 12,392 > 10,240 | stale ceiling |

### P7 riscv64 server B attribution

The growth is **`.relro_padding`**, not `.bss`:

| Build | `.text` | `.data.rel.ro` | `.relro_padding` | `.bss` | B |
|---|---:|---:|---:|---:|---:|
| pre-C74ij `9a8d8695` | 58,110 | 2,000 | 88 | 11,552 | 11,640 |
| current `d0429e95` | 58,356 | 2,000 | 3,936 | 11,552 | 15,488 |
| delta | +246 | 0 | **+3,848** | **0** | **+3,848** |

The p7 map shows no new BSS object: every `.bss` member is the same 11,552-byte set, and the
only large NOBITS contribution is the linker-created `.relro_padding` (`<internal>`, 0xF60).
`readelf -lW` confirms the mechanism:

* pre-C74ij: GNU_RELRO at `0x21698`, filesz `0x910`, memsz `0x968` (ends at page `0x22000`);
* current: GNU_RELRO at `0x21790`, filesz `0x910`, memsz `0x1870` (ends at page `0x23000`).

C74ij's `%s` branch grew `.text` by 246 bytes (plus alignment = 248), moving the RELRO start
across the `0x22000` page boundary. The RELRO segment then extends to the next page, adding one
4,096-byte NOBITS page minus the 248-byte shift = **+3,848** padding. The gate's "+3,103 BSS"
is the over-budget amount (15,488 - 12,385), not a static buffer.

**Owner/file:** the trigger is `libc/ravenlibc/crates/raven-libc-time/src/strftime.rs`, the `%s`
branch (lines ~167-190), owned by **otto/C74ij** (INDEX row 354). It is an intentional code change
with an accidental layout side effect, not a stray static buffer. A fix would either shrink
`.text` back below the page boundary or change the link segment/page alignment; both are outside
C74in's Owns, so no source edit is made.

**Options:** if the layout is accepted, the reviewer budget is `ceil(15,488 x 1.03) = 15,953`
B. Applying it would make P7 pass and 3.1 become **64 pass / 12 fail / 65 known / 99 skipped**
(the 10 invariants + 2 riscv P9 remain). C74in changes no budget.

The current P9 values supersede the earlier C74ib review numbers: tiny S 906,278 / F 907,864
(budget 868,494 / 870,128); server S 966,556 / F 968,144 (budget 924,842 / 926,473). B is
33,076 / 34,364 and stays under the existing P9 B budgets.

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
* C74z-S16 applies that proposal after the S14/S15a/S5/S6/S13 merges: **28 legitimate blocks**
  (P0/P1/P4/P6 all arch/set and x86_64/aarch64 P3) are re-frozen at measured + 3 percent, and
  the 32 shrink-first/held blocks are unchanged. The gate moves to **46 pass / 28 fail / 69
  known / 105 skipped / 0 budget-only**; the exact remaining rows and owners are in the
  C74z-S16 section above. 3.1 remains **NOT MET**.
* C74z-S17 re-freezes the **17 legitimate budget/layout rows** from the C74dl audit at the
  `10d8c9dd` measurement + 3 percent (measured fields and `members` updated); the 10 invariant
  ceilings are unchanged. The `P7-R-14` matcher is fixed by C74cx `e76b1d01`. The gate is
  **63 pass / 10 fail / 62 known / 102 skipped / 0 budget-only**; only the 10 invariants remain.
  3.1 remains **NOT MET**.
* C74ht rebuilds the x86_64/aarch64 `libc.so.1` rows and re-freezes their S/F budgets at measured
  + 3 percent: x86_64 1,286,594/3,009,232 -> 1,325,192/3,099,509; aarch64 1,363,768/2,501,104 ->
  1,404,682/2,576,138. The two `INV-LIBC-SO-NEEDED` rows are KNOWN rule exceptions (the DSO has
  0 `DT_NEEDED`, not a self-`NEEDED`); the aarch64 S ceiling and the riscv64 C74hs block are
  unchanged. The gate is **65 pass / 11 fail / 64 known / 100 skipped / 0 budget-only**.
  3.1 remains **NOT MET**.
* C74ia adds the riscv64 `libc.so.1` row at measured + 3 percent (S 1,010,867 / F 2,490,256 ->
  1,041,194 / 2,564,964) and the riscv64 NEEDED known row. The riscv64 P9 static growth from the
  C74hs compat exports (+35,590/+39,384 S and +35,544/+39,343 F on tiny/server) is **not
  budgeted** and remains FAIL. The gate is **64 pass / 13 fail / 65 known / 99 skipped / 0
  budget-only**. 3.1 remains **NOT MET**.
* C74in changes no budget. The full three-arch rebuild is **63 pass / 13 fail / 65 known / 99
  skipped**; the extra row is P7 riscv64 server B 15,488 > 12,385. The growth is
  `.relro_padding` (88 -> 3,936), not `.bss` (11,552 unchanged): C74ij's +246-byte `.text`
  crossed the `0x22000` page boundary. The measured + 3 percent B option is **15,953**; applying
  it would give **64 pass / 12 fail / 65 known / 99 skipped**. 3.1 remains **NOT MET**.

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
