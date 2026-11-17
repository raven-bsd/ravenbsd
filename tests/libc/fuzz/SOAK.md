<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->
# ravenlibc fuzz soak (G1: 1 h per target, 0 crashes)

Date: 2026-11-14; inet_text rerun 2026-11-17   Host CPU: 12th Gen Intel(R) Core(TM) i7-12700H   rustc +nightly: 1.102.0-nightly (5b7b37aef 2026-11-08)   cargo-fuzz: 0.13.2   Commit: 18acebf; inet_text rerun d351a239

| target | seconds | executions | exec/s | corpus files after | new coverage edges | crashes | regression files |
|---|---|---|---|---|---|---|---|
| elf_dynamic | 3601 | 91,074,272 | 25,291 | 360 | 582 | 0 | 0 |
| getopt_args | 3601 | 74,432,525 | 20,669 | 460 | 448 | 0 | 0 |
| glob_pattern | 3601 | 7,477,018 | 2,076 | 1,872 | 1,089 | 0 | 0 |
| inet_text | 3601 | 261,966,448 | 72,748 | 277 | 348 | 0 | 1 |
| printf_format | 3604 | 983,286 | 272 | 1,186 | 1,517 | 0 | 0 |
| regex_compile_exec | 3601 | 4,167,504 | 1,157 | 3,140 | 2,688 | 0 | 0 |
| resolver_conf | 3601 | 57,241,967 | 15,896 | 3,949 | 1,827 | 0 | 0 |
| resolver_message | 3601 | 96,505,870 | 26,799 | 138 | 327 | 0 | 0 |
| scanf_format | 3601 | 119,000,541 | 33,046 | 2,042 | 1,485 | 0 | 0 |
| strtod_input | 3601 | 33,441,356 | 9,286 | 1,121 | 1,962 | 0 | 0 |
| strtol_input | 3601 | 177,332,005 | 49,245 | 182 | 255 | 0 | 0 |
| time_format | 3603 | 96,266 | 26 | 433 | 1,080 | 0 | 0 |
| tz_parse | 3601 | 42,331,785 | 11,755 | 562 | 837 | 0 | 0 |
| utf8_decode | 3601 | 34,698,132 | 9,635 | 358 | 412 | 0 | 0 |

`inet_text` was the one failed row in the 2026-11-14 run: the smoke and the 1-hour soak both
hit the C51 `inet_pton` compression finding (`FINDING 1` in the report) within seconds. C51a
(`f2f7508d`) fixed the `pton6` expansion on 2026-11-14. The 2026-11-17 rerun on `d351a239`
completed the full hour with 0 crashes; its coverage started at 55 and ended at 403 (348 new
edges), and the soak corpus ended at 277 files. The other thirteen targets completed the full
hour with zero crashes in the 2026-11-14 run. The `tz_parse` row is from the rerun after the
C44 transition-order fix; the first run stopped on that finding.

The `cargo-fuzz` `-jobs=1` mode writes `fuzz-0.log` in the current directory; `soak.sh`
runs each target in its own `$TMPDIR` subdirectory, so the per-target logs are not interleaved.

G2 (24 CPU-h per target) is the owner's weekly job and is not claimed here.
