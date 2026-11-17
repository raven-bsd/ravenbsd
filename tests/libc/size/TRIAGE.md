<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# libc size gate 3.1 triage and budget correction (C74w/C74z)

Status: OBSERVED (C74z, `origin/master` `1bfeaafa`). C74w classified the C74h/C69 data; C74z
corrects the objective budget-data errors (x86_64 P9 budgets, aarch64/riscv64 B baselines) and
records the shrink plan. **3.1 remains NOT MET**: 7 pass / 56 fail / 69 known / 105 skipped / 0
budget-only. No libc body, S/F budget or boot budget is changed.

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

## Remaining failure table

`cur/budget (delta)`; static Rust. The B column for aarch64/riscv64 is now below its corrected
budget; only the S/F overruns remain.

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
| C74z-S2 | fmt monomorphization | Collapse the six `vformat` instantiations to one sink/va-reader erased path; keep the typed fast paths | -15k S | carl/C43 | P2/P3/P9 S shrink, fmt oracle, nextest |
| C74z-S3 | dtoa | Share `strtod`/`strtodg`/`dtoa` bigint code and remove duplicate conversion tails | -5k S | carl/C42 | P2/P3/P9 S shrink, dtoa oracle |
| C74z-S4 | compat static buffers | Replace the 8k `mntent`/`ttyent` ARENAs and `tcbbuf` with one bounded pool or lazy allocation | -10k B/S | carl/C64d | P9 B/S shrink, compat host tests |
| C74z-S5 | resolv/net | Deduplicate `lookup_name`/`getnameinfo`/`msend` name buffers and service paths | -5k S | dave/C51 | P5/P9 S shrink, resolv oracle |
| C74z-S6 | time | Share `strptime`/`strftime` parser and append-format code | -5k S | mel/C44 | P7/P9 S shrink, time oracle |
| C74z-S7 | TLS/pages/streams statics | Review the 4,096 `BUILTIN_TLS`, 3,080 page registry, 2,808 `STREAMS`, 1,024 `MAIN_TSD` and 1,064 exit registry; make the registry slots demand-paged/lazy | -3k B | dave/C57, dave/C54, carl/C64d, kevin/C49 | P0/P9 B shrink, boot row stays within budget |
| C74z-S8 | libm tables | Share `exp2`/`rem_pio2` tables between `raven-libc-m` and `compiler_builtins` where possible; drop duplicate constants | -4k S/B | bob/C47 | P9 S/B shrink, libm oracle |
| C74z-S9 | boot start path | Trim the start/TLS path from 1,245 to <=908 x86 instructions, aarch64 3,403 to <=1,847 and 8 syscalls to <=3 | boot budgets | dave/C57, kevin/C49 | boot x86/aarch64 pass, `libc-size --check` |
| C74z-S10 | P9 full-link envelope | After S1-S9, re-measure P9 and re-freeze only the reviewed S/F budget; do not treat the current 1.19MB/1.33MB x86 and 1.35MB/1.39MB aarch64 sizes as a target | reviewed budget | budget owner + carl | gate review, 3.1 can be MET only if all rows fit |

The first three clusters are the largest current symbols: ctype tables 31,360 bytes, the six
`vformat` instantiations about 30k, dtoa about 26k. The remaining x86_64 P5 B-only and boot rows
are separate follow-ups; the P5 B overruns are 945/1,373 bytes and do not justify a budget raise.

## Budget actions

* Added x86_64 P9 tiny/server budgets from the `1bfeaafa` first measurement + 3 percent (the same
  rule as the other arches).
* Replaced the aarch64/riscv64 B measured/budget fields with the `1bfeaafa` Berkeley-bss
  measurement + 3 percent. S/F fields remain C69.
* No S/F or boot budget changed. 3.1 remains **NOT MET** at 7 pass / 56 fail / 69 known / 105
  skipped / 0 budget-only.
* `BUDGETS.toml` is normally generated by `libc-size freeze`; C74z uses a targeted data edit
  because a full freeze would raise every S/F budget, which the ticket forbids. The header comment
  records the reason.

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
