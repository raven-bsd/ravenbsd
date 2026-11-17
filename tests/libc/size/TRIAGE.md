<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# libc size gate 3.1 triage (C74w)

Status: OBSERVED (C74w, `origin/master` `19b6f229`). The 3.1 row remains **NOT MET**; this note
classifies the current failures so shrink and budget tickets can be issued. No libc body, budget
or threshold is changed here.

## Reproduction

```sh
cargo xtask libc-size --check
cargo run -q -p libc-gate -- eval --only 3.1 --out build/libc-gate-c74w
```

Observed on `19b6f229`:

```
libc-size check: 5 pass, 58 fail, 69 known, 105 skipped, 0 budget-only
```

The ticket's older `4 pass / 48 fail / 25 known / 282 skipped / 40 budget-only` is from the C74h
tree and a different matrix run; the current counts above are the ones the gate actually sees.

## Measurement definition

`S` is the Berkeley `text + data` of the stripped probe; `B` is the Berkeley **bss** field of
`llvm-size`, which includes every NOBITS section, so it also counts the linker's `.relro_padding`.
It is not just the `.bss` section. On `19b6f229` the `.bss` section is 11,392 bytes on all three
architectures, while Berkeley B is 15,176 (x86, `.relro_padding` 3,784), 13,168 (aarch64, 1,776)
and 13,064 (riscv64, 1,672). This is the same metric the C69 budgets were frozen with, not a
current regression or a harness change.

The run artifact `build/size/libc-size.json` and `.md` records `budget_* = 0` and `status = ok`
because `run` writes the measurement before `check` applies the budgets. The console output and
the `libc-gate` evidence file are correct; do not read the JSON budget fields as results. A
follow-up harness ticket can annotate the artifact.

## Classification summary

* **54 budget overruns**, **2 missing budgets** and **2 boot regressions** (58 fail rows):
  * 52 overruns are real `S`/`F` growth since the C69 budget (small on x86_64, 3-12 percent on
    aarch64/riscv64); the aarch64/riscv64 P9 rows add +306k/+322k and +141k/+159k `S`.
  * 2 are x86_64 `P5` B-only growth (`+945`, `+1373`); S/F are under budget there.
  * The aarch64/riscv64 B figures are also over their old budgets by 1.9-9.5k. Those B budgets
    are stale: the old aarch64/riscv64 B budget is 6.9k/7.8k while x86_64 is 14.3k for the same
    source, and the current `.bss` section is 11,392 on all three. The arm/riscv baseline
    predates the TLS/static parity that x86 already had; re-freezing it is a budget-owner
    decision, not a code-size fix.
* **2 missing budgets**: x86_64 `P9` tiny and server have no `[[budget]]` row while aarch64 and
  riscv64 P9 do. The harness reports `no budget` as FAIL. Do not add a budget just to hide the
  1.19MB/1.33MB full link; the C69/C74 budget owner should decide the P9 envelope.
* **2 boot regressions**: x86_64 start instructions 1,245 > 908; aarch64 syscalls 8 > 3 and
  instructions 3,403 > 1,847. These are real start/TLS/allocator growth, owed by the P1b/P1c
  start lane (dave/C57 TLS, kevin/C49 malloc).
* **69 known** are the frozen first-measurement rules/invariants; **105 skipped** are the dynamic
  `libc.so.1` rows (C62 not landed) and dynamic tiny cells.

## Full failure table

`cur/budget (delta)`; every row is static Rust. Owners name the crate plus the lane that owns it.

### x86_64 / tiny

| Probe | S cur/budget (delta) | B cur/budget (delta) | F cur/budget (delta) | Class | Owner |
|---|---|---|---|---|---|
| P0 | 69203/68355 (+848) | 15176/14272 (+904) | 70664/69859 (+805) | real S/F/B | raven-libc, dave/C55 platform, dave/C57 tls, dave/C54 sys, kevin/C49 malloc |
| P1 | 71035/68545 (+2490) | 13400/14074 (-674) | 72504/70057 (+2447) | real S/F | P0 + carl/C56 stdio |
| P2 | 92947/92803 (+144) | 11912/15129 (-3217) | 94408/94316 (+92) | real S/F | carl/C42 dtoa, carl/C43 fmt, carl/C56 stdio |
| P3 | 92979/92853 (+126) | 11880/15080 (-3200) | 94440/94365 (+75) | real S/F | carl/C42 dtoa, carl/C43 fmt, carl/C56 stdio |
| P4 | 76799/75677 (+1122) | 13456/17222 (-3766) | 78272/77193 (+1079) | real S/F | dave/C57 thread+tls, dave/C55 platform |
| P5 | 149076/150608 (-1532) | 13248/12303 (+945) | 150544/152119 (-1575) | real B | dave/C51 resolv+net |
| P6 | 69363/68504 (+859) | 15032/14140 (+892) | 70824/70008 (+816) | real S/F/B | bob/C52 elf, dave/C55 platform |
| P7 | 84993/84644 (+349) | 11736/14915 (-3179) | 86456/86158 (+298) | real S/F | mel/C44 time, mel/C48 locale |
| P9 | 1192381/— | 68808/— | 1193856/— | missing budget | carl/C39 ctype, carl/C42 dtoa, carl/C43 fmt, carl/C56 stdio, carl/C64d compat; dave/C51 net; mel/C44 time; dave/C54+55+57 platform/tls/sys; kevin/C49 malloc; bob/C47 m |

### x86_64 / server

| Probe | S cur/budget (delta) | B cur/budget (delta) | F cur/budget (delta) | Class | Owner |
|---|---|---|---|---|---|
| P0 | 73643/73347 (+296) | 14856/13531 (+1325) | 75112/74853 (+259) | real S/F/B | raven-libc, dave/C55 platform, dave/C57 tls, dave/C54 sys, kevin/C49 malloc |
| P1 | 75227/73512 (+1715) | 13336/13366 (-30) | 76696/75017 (+1679) | real S/F | P0 + carl/C56 stdio |
| P4 | 81359/80750 (+609) | 17128/16398 (+730) | 82824/82269 (+555) | real S/F/B | dave/C57 thread+tls, dave/C55 platform |
| P5 | 153197/155287 (-2090) | 13256/11883 (+1373) | 154664/156791 (-2127) | real B | dave/C51 resolv+net |
| P6 | 73803/73503 (+300) | 14712/13382 (+1330) | 75272/75017 (+255) | real S/F/B | bob/C52 elf, dave/C55 platform |
| P7 | 91521/91316 (+205) | 13432/12509 (+923) | 92984/92816 (+168) | real S/F/B | mel/C44 time, mel/C48 locale |
| P9 | 1326191/— | 70280/— | 1327648/— | missing budget | carl/C39 ctype, carl/C42 dtoa, carl/C43 fmt, carl/C56 stdio, carl/C64d compat; dave/C51 net; mel/C44 time; dave/C54+55+57 platform/tls/sys; kevin/C49 malloc; bob/C47 m |

### aarch64 / tiny

| Probe | S cur/budget (delta) | B cur/budget (delta) | F cur/budget (delta) | Class | Owner |
|---|---|---|---|---|---|
| P0 | 83508/78202 (+5306) | 13168/6955 (+6213) | 84960/79698 (+5262) | real S/F; stale B budget | raven-libc, dave/C55 platform, dave/C57 tls, dave/C54 sys, kevin/C49 malloc |
| P1 | 86572/78511 (+8061) | 14264/6642 (+7622) | 88024/80011 (+8013) | real S/F; stale B budget | P0 + carl/C56 stdio |
| P2 | 83540/78235 (+5305) | 13136/6922 (+6214) | 84992/79731 (+5261) | real S/F; stale B budget | carl/C42 dtoa, carl/C43 fmt, carl/C56 stdio |
| P3 | 83552/78248 (+5304) | 13120/6906 (+6214) | 85008/79747 (+5261) | real S/F; stale B budget | carl/C42 dtoa, carl/C43 fmt, carl/C56 stdio |
| P4 | 92604/90093 (+2511) | 14056/10193 (+3863) | 94056/91588 (+2468) | real S/F; stale B budget | dave/C57 thread+tls, dave/C55 platform |
| P5 | 176429/173655 (+2774) | 14576/8661 (+5915) | 177888/175158 (+2730) | real S/F; stale B budget | dave/C51 resolv+net |
| P6 | 83752/78445 (+5307) | 12936/6716 (+6220) | 85208/79945 (+5263) | real S/F; stale B budget | bob/C52 elf, dave/C55 platform |
| P7 | 101700/96071 (+5629) | 15520/6016 (+9504) | 103152/97570 (+5582) | real S/F; stale B budget | mel/C44 time, mel/C48 locale |
| P8 | 183348/180904 (+2444) | 11632/9724 (+1908) | 184800/182401 (+2399) | real S/F; stale B budget | carl/C45 regex, carl/C41 stdlib, carl/C40 str |
| P9 | 1345032/1038488 (+306544) | 67724/36306 (+31418) | 1346488/1039987 (+306501) | real S/F; stale B budget | carl/C39 ctype, carl/C42 dtoa, carl/C43 fmt, carl/C56 stdio, carl/C64d compat; dave/C51 net; mel/C44 time; dave/C54+55+57 platform/tls/sys; kevin/C49 malloc; bob/C47 m |

### aarch64 / server

| Probe | S cur/budget (delta) | B cur/budget (delta) | F cur/budget (delta) | Class | Owner |
|---|---|---|---|---|---|
| P0 | 77882/73102 (+4780) | 14720/7870 (+6850) | 79344/74597 (+4747) | real S/F; stale B budget | raven-libc, dave/C55 platform, dave/C57 tls, dave/C54 sys, kevin/C49 malloc |
| P1 | 80042/73349 (+6693) | 12624/7622 (+5002) | 81504/74844 (+6660) | real S/F; stale B budget | P0 + carl/C56 stdio |
| P2 | 77930/73143 (+4787) | 14672/7828 (+6844) | 79392/74638 (+4754) | real S/F; stale B budget | carl/C42 dtoa, carl/C43 fmt, carl/C56 stdio |
| P3 | 77942/73155 (+4787) | 14664/7812 (+6852) | 79400/74655 (+4745) | real S/F; stale B budget | carl/C42 dtoa, carl/C43 fmt, carl/C56 stdio |
| P4 | 86474/84007 (+2467) | 16112/12089 (+4023) | 87936/85507 (+2429) | real S/F; stale B budget | dave/C57 thread+tls, dave/C55 platform |
| P5 | 163088/160744 (+2344) | 11568/8941 (+2627) | 164544/162246 (+2298) | real S/F; stale B budget | dave/C51 resolv+net |
| P6 | 78134/73361 (+4773) | 14488/7614 (+6874) | 79592/74861 (+4731) | real S/F; stale B budget | bob/C52 elf, dave/C55 platform |
| P7 | 96870/92659 (+4211) | 12184/9460 (+2724) | 98328/94159 (+4169) | real S/F; stale B budget | mel/C44 time, mel/C48 locale |
| P8 | 178858/177097 (+1761) | 12048/9345 (+2703) | 180320/178594 (+1726) | real S/F; stale B budget | carl/C45 regex, carl/C41 stdlib, carl/C40 str |
| P9 | 1392892/1070261 (+322631) | 69140/34172 (+34968) | 1394344/1071761 (+322583) | real S/F; stale B budget | carl/C39 ctype, carl/C42 dtoa, carl/C43 fmt, carl/C56 stdio, carl/C64d compat; dave/C51 net; mel/C44 time; dave/C54+55+57 platform/tls/sys; kevin/C49 malloc; bob/C47 m |

### riscv64 / tiny

| Probe | S cur/budget (delta) | B cur/budget (delta) | F cur/budget (delta) | Class | Owner |
|---|---|---|---|---|---|
| P0 | 59034/56241 (+2793) | 13064/7812 (+5252) | 60608/57870 (+2738) | real S/F; stale B budget | raven-libc, dave/C55 platform, dave/C57 tls, dave/C54 sys, kevin/C49 malloc |
| P1 | 60736/56414 (+4322) | 15520/7639 (+7881) | 62312/58043 (+4269) | real S/F; stale B budget | P0 + carl/C56 stdio |
| P2 | 59066/56282 (+2784) | 13032/7771 (+5261) | 60640/57911 (+2729) | real S/F; stale B budget | carl/C42 dtoa, carl/C43 fmt, carl/C56 stdio |
| P3 | 59086/56302 (+2784) | 13008/7754 (+5254) | 60664/57928 (+2736) | real S/F; stale B budget | carl/C42 dtoa, carl/C43 fmt, carl/C56 stdio |
| P4 | 65222/64466 (+756) | 16856/10506 (+6350) | 66800/66085 (+715) | real S/F; stale B budget | dave/C57 thread+tls, dave/C55 platform |
| P5 | 127429/126240 (+1189) | 14424/9666 (+4758) | 129008/127869 (+1139) | real S/F; stale B budget | dave/C51 resolv+net |
| P6 | 59198/56409 (+2789) | 12912/7655 (+5257) | 60776/58035 (+2741) | real S/F; stale B budget | bob/C52 elf, dave/C55 platform |
| P7 | 72746/69979 (+2767) | 11704/6798 (+4906) | 74320/71598 (+2722) | real S/F; stale B budget | mel/C44 time, mel/C48 locale |
| P8 | 142946/142674 (+272) | 15168/9979 (+5189) | 144520/144299 (+221) | real S/F; stale B budget | carl/C45 regex, carl/C41 stdlib, carl/C40 str |
| P9 | 882336/741018 (+141318) | 42196/34221 (+7975) | 883920/742655 (+141265) | real S/F; stale B budget | carl/C39 ctype, carl/C42 dtoa, carl/C43 fmt, carl/C56 stdio, carl/C64d compat; dave/C51 net; mel/C44 time; dave/C54+55+57 platform/tls/sys; kevin/C49 malloc; bob/C47 m |

### riscv64 / server

| Probe | S cur/budget (delta) | B cur/budget (delta) | F cur/budget (delta) | Class | Owner |
|---|---|---|---|---|---|
| P0 | 56964/52829 (+4135) | 15160/7029 (+8131) | 58544/54467 (+4077) | real S/F; stale B budget | raven-libc, dave/C55 platform, dave/C57 tls, dave/C54 sys, kevin/C49 malloc |
| P1 | 58442/52992 (+5450) | 13744/6873 (+6871) | 60024/54623 (+5401) | real S/F; stale B budget | P0 + carl/C56 stdio |
| P2 | 56996/52879 (+4117) | 15128/6980 (+8148) | 58576/54516 (+4060) | real S/F; stale B budget | carl/C42 dtoa, carl/C43 fmt, carl/C56 stdio |
| P3 | 57016/52899 (+4117) | 15104/6963 (+8141) | 58600/54533 (+4067) | real S/F; stale B budget | carl/C42 dtoa, carl/C43 fmt, carl/C56 stdio |
| P4 | 63090/60597 (+2493) | 14920/10177 (+4743) | 64672/62229 (+2443) | real S/F; stale B budget | dave/C57 thread+tls, dave/C55 platform |
| P5 | 123405/121239 (+2166) | 14376/6255 (+8121) | 124992/122875 (+2117) | real S/F; stale B budget | dave/C51 resolv+net |
| P6 | 57128/52998 (+4130) | 15008/6873 (+8135) | 58712/54632 (+4080) | real S/F; stale B budget | bob/C52 elf, dave/C55 platform |
| P7 | 71076/67381 (+3695) | 13400/9419 (+3981) | 72656/69010 (+3646) | real S/F; stale B budget | mel/C44 time, mel/C48 locale |
| P8 | 144366/142866 (+1500) | 13768/9814 (+3954) | 145952/144497 (+1455) | real S/F; stale B budget | carl/C45 regex, carl/C41 stdlib, carl/C40 str |
| P9 | 951548/792159 (+159389) | 38612/33809 (+4803) | 953136/793793 (+159343) | real S/F; stale B budget | carl/C39 ctype, carl/C42 dtoa, carl/C43 fmt, carl/C56 stdio, carl/C64d compat; dave/C51 net; mel/C44 time; dave/C54+55+57 platform/tls/sys; kevin/C49 malloc; bob/C47 m |

## Real offenders (largest current symbols)

From `build/size/libc-size.json` `top_symbols`; sizes are bytes in the stripped P9/P2 links.

| Symbol/cluster | Size | File | Owner |
|---|---|---|---|
| `PROPS_STAGE2`, `CASE_STAGE2`, `PROPS_STAGE1` | 20,096 + 6,912 + 4,352 | `libc/ravenlibc/crates/raven-libc-ctype/src/unicode/tables.rs` | carl/C39 |
| `mntent_state::ARENA` ×2, `mntent_state::LINE` ×2, `ttyent_state::ARENA` | 8,192 ×3 + 8,192 | `libc/ravenlibc/crates/raven-libc/src/compat/{getmntent,addmntent,getttynam}.rs` | carl/C64d |
| `netdb::records::AREA` | 8,432 | `libc/ravenlibc/crates/raven-libc/src/netdb/` | carl/C64d |
| `tcbbuf::AREA` | 6,776 | `libc/ravenlibc/crates/raven-libc/src/netdb/inet_ntoa.rs` and pwd/netdb users | carl/C64d |
| `dtoa`, `strtodg`, `strtod` | 8,418 + 10,285 + 7,674 | `libc/ravenlibc/crates/raven-libc-dtoa/src/` | carl/C42 |
| `fmt::printf::vformat` instantiations | six × 4,641-6,616 | `libc/ravenlibc/crates/raven-libc-fmt/src/printf/` | carl/C43 |
| `resolv::lookup::lookup_name`, `resolv::engine::msend`, `net::getnameinfo` | 4,735/5,810 + 4,838 + 5,166 | `libc/ravenlibc/crates/raven-libc-resolv/src/{lookup,engine}.rs`, `raven-libc-net/src/netdb.rs` | dave/C51 |
| `time::strptime::parse`, `time::strftime::append_format` | 7,773/5,132 + 5,460/5,524 | `libc/ravenlibc/crates/raven-libc-time/src/{strptime,strftime}.rs` | mel/C44 |
| `regex::regcomp::p_re` | 4,603/6,727 | `libc/ravenlibc/crates/raven-libc-regex/src/regcomp.rs` | carl/C45 |
| `tls::raw::BUILTIN_TLS`, `sys::pages::REGISTRY`, `stdio::streams::STREAMS`, `ffi::platform::PLATFORM`, `exit_registry::REGISTRY`, `thread::tsd::MAIN_TSD` | 4,096 + 3,080 + 2,808 + 2,336 + 1,064 + 1,024 | `raven-libc-tls/src/raw.rs`, `raven-libc-sys/src/pages.rs`, `raven-libc/src/{stdio/streams,ffi/platform,exit_registry}.rs`, `raven-libc-thread/src/thread/tsd.rs` | dave/C57, dave/C54, carl/C64d, kevin/C49 |
| `raven_libc_m::fdlibm::exp2::TBL`, `rem_pio2_large`, `pow` | 4,096 + 5,498 + 4,405 | `libc/ravenlibc/crates/raven-libc-m/src/` | bob/C47 |
| `compiler_builtins::math::libm_math::exp2::TBL` | 4,096 | toolchain `compiler_builtins` | not a RavenBSD crate |

The P9 full-link growth is distributed across these clusters, not one file. The largest data
clusters are the ctype tables and the compat static buffers; the largest code clusters are the fmt
monomorphizations, dtoa, time and resolv/net. Shrink tickets should start with ctype tables and
fmt monomorphization, then the compat static buffers.

## Budget actions

No budget is edited by C74w. Specifically:

* x86_64 P9: **missing budget** (a data gap versus aarch64/riscv64). Add only after the budget
  owner sets a justified full-link envelope; the current 1.19MB/1.33MB is not a green target.
* aarch64/riscv64 B: **stale budget**; the old baseline omits the static parity now present on all
  arches (`.bss` 11,392 everywhere). A re-freeze is a budget-owner decision, not a C74w edit.
* All other rows are real S/F growth since the C69 freeze and need shrink tickets, not larger
  budgets.
* The 3.1 row stays **NOT MET**; `cargo run -q -p libc-gate -- eval --only 3.1` records exit 1 in
  `build/libc-gate-c74w/3.1.json`.
