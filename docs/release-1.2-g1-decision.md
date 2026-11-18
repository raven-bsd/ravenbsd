<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# RavenBSD 0.1.0 libc gate 1.2 (G1) — decision paper

- Date: 2026-11-18
- Product base: `17f3fa95` (C74x round 3 merged)
- Author: jerry (C74ii)
- Status: **G1 NOT MET — 1 of 4 measured rows; C5 MET.** This is a decision paper, not a gate
  claim and not a threshold/test/waiver edit.
- Scope: the G1 donor-suite rows C2-C9, the exact non-pass counts/classes, the closure options and
  the release consequence. No QEMU; no tag.
- Evidence: all counts and rows are **OBSERVED** from the committed `tests/libc/CONFORMANCE.md`/
  `tests/libc/conformance.tsv` and `cargo run -q -p raven-atf-run -- gate --g1` on `17f3fa95`;
  the thresholds in `tests/libc-oracle/g1.toml` are **PROPOSED** and owner-only.

## Decision requested

Two options; the owner picks one:

- **A. Fund a multi-wave libc conformance effort.** The lead issues tickets by failure class and
  reachability (below, no effort estimates); the gate stays NOT MET until each measured row is
  at or above its threshold.
- **B. Explicitly defer/waive 1.2 for 0.1.0.** Keep the gate NOT MET, record the owner-approved
  deferral/waiver in the release packet, and accept the NO-GO consequence. No threshold, test,
  exclusion or waiver file is edited by a minion.

C6 (mlibc vendoring) and the external C7-C9 runs are separate owner/environment decisions.

## Current real rows (OBSERVED on `17f3fa95`)

`cargo run -q -p raven-atf-run -- gate --g1` exits 3:

```
C2 NetBSD ATF libc+libm+libpthread: 51.7 % of 571 eligible (threshold 90.0 %), exclusions 42/250 -> NOT MET
C3 FreeBSD libc+msun: 34.4 % of 93 eligible (threshold 90.0 %) -> NOT MET
C4 OpenBSD regress: 40.0 % of 25 programs (threshold 90.0 %) -> NOT MET
C5 picolibc tiny: 94.7 % of 19 programs (threshold 90.0 %) -> MET
C6 mlibc ansi+posix+rtld: 0.0 % of 0 programs (threshold 85.0 %) -> NOT MET (not vendored (no rows))
C7 LTP: external: not evaluated
C8 pjdfstest: external: not evaluated
C9 differential tests: external: not evaluated
G1: NOT MET (1 of 4 measured rows)
```

| Row | Unit | Eligible | Pass | Rate | Threshold | Result |
|---|---|---:|---:|---:|---:|---|
| C2 NetBSD ATF libc+libm+libpthread | bodies | 571 | 295 | 51.7 % | 90 % | NOT MET |
| C3 FreeBSD libc+msun | bodies | 93 | 32 | 34.4 % | 90 % | NOT MET |
| C4 OpenBSD regress | programs | 25 | 10 | 40.0 % | 90 % | NOT MET |
| C5 picolibc tiny | programs | 19 | 18 | 94.7 % | 90 % | **MET** |
| C6 mlibc | programs | 0 | 0 | 0.0 % | 85 % | NOT MET: not vendored |
| C7 LTP / C8 pjdfstest / C9 differential | external | - | - | - | - | not evaluated |

The C2 exclusion cap is 42/250 (under the cap; the threshold is the blocker). No threshold was
lowered, no host result substituted, and no unexecuted body is counted.

## C2 — NetBSD ATF libc+libm+libpthread (571 eligible, 295 pass, 51.7 %)

Per suite (eligible / pass / non-pass):

| Suite | Eligible | Pass | Non-pass | Composition |
|---|---:|---:|---:|---|
| netbsd-libc | 261 | 94 | 167 | 153 fail + 14 eligible skips (13 shell + 1 fenv); 25 needs-root skips excluded |
| netbsd-libm | 292 | 192 | 100 | 98 fail + 1 skip + 1 not-run |
| netbsd-libpthread | 18 | 9 | 9 | 4 fail + 5 skips |

Top failure clusters (from `tests/libc/CONFORMANCE.md`; ticket owners are the existing Cxx rows):

- **libc (167):** `c063/t_faccessat` (10), `stdlib/t_strtod` (9), `sys/t_dup` (8),
  `c063/t_fchownat` (7), `c063/t_fstatat` (7) — the c063 filesystem/ownership family is the
  largest libc class.
- **libm (100):** `t_next` (14), `t_pow` (13), `t_sincos` (12), `t_erf` (8), `t_asin` (7) —
  transcendental/math classification and special cases.
- **libpthread (9):** `t_mtx` (3) plus smaller mutex/thread cases.

## C3 — FreeBSD libc+msun (93 eligible, 32 pass, 34.4 %)

Per suite (eligible / pass / non-pass):

| Suite | Eligible | Pass | Non-pass | Composition |
|---|---:|---:|---:|---|
| freebsd-libc | 52 | 28 | 24 | 24 fail |
| freebsd-msun | 41 | 4 | 37 | 37 fail |

Top failure clusters:

- **libc (24):** `stdio/printfloat_test` (9), `stdio/freopen_test` (6),
  `stdlib/libc_exit_test` (3), `stdio/scanfloat_test` (2) — float formatting/parsing plus
  freopen/exit-fork edges.
- **msun (37):** `fmaximum_fminimum_test` (15), `fmaxmin_test` (7), `next_test` (5),
  `ilogb2_test` (3), `ilogb_test` (3) — min/max/next/ilogb classification and NaN/Inf edges.

## C4 — OpenBSD regress (25 programs, 10 pass, 40.0 %)

15 non-pass: 13 explicit failures plus 2 mixed baseline rows. The 13 explicit failures (C74x
round 3):

| Program | Exit/result | Class |
|---|---|---|
| `basename` | SIGSEGV | C46 |
| `dirname` | 1 | C46 |
| `explicit_bzero` | 134 | C65 |
| `fread` | 1 | C56 |
| `getcap` | 1 | C46 |
| `ldexp` | 134 | C47 |
| `modf` | 134 | C47 |
| `sleep` | 1 | C44 |
| `strlcat` | SIGSEGV | C40 |
| `strlcpy` | SIGSEGV | C40 |
| `telldir` | 1 | C46 |
| `timingsafe` | 134 | C65 |
| `uuid` | 134 | C65 |

The classes are string/secure-memory/uuid (C40/C65), directory/getcap/process (C46/C44) and
math/stdio (C47/C56); none is a QEMU-backend result.

## C5 — picolibc tiny (MET)

The gate counts **18 of 19 programs (94.7 %)**, above the 90 % threshold. C74x round 3 fixed the
four programs that were failing (`test-uchar`, `test-getopt`, `test-strftime`, `test-wcsftime`);
the table has 19 target-pass rows because one program has a mixed oracle-fail/pass history.
**C5 is kept MET** and is not reopened by this paper.

## C6 / external rows

- **C6 mlibc (85 %):** zero programs are vendored, so the row is unmeasured and cannot pass.
  Vendoring the donor suite is a separate owner/reviewer decision (new test material, no gate
  change in this paper).
- **C7 LTP / C8 pjdfstest / C9 differential:** external tools, not evaluated in the gate. Running
  them is an owner/environment decision; they are not counted as passes.

## What each row needs to reach its threshold

At the current eligible counts (owner-approved exclusions could change the denominator; only the
owner edits the cap/waivers):

| Row | Need for 90 % | Currently | Gap |
|---|---:|---:|---:|
| C2 | 514 / 571 | 295 | **219 more passes** (or owner-approved exclusions, cap 250; 42 used) |
| C3 | 84 / 93 | 32 | **52 more passes** |
| C4 | 23 / 25 programs | 10 | **13 more programs** |
| C5 | 18 / 19 (already MET) | 18 | 0 (keep) |
| C6 | 85 % of a vendored suite | 0 rows | vendor mlibc first, then pass 85 % |

## Option A — multi-wave libc conformance effort

Ordered by failure class and reachability, not by effort estimates. Each wave uses the same
committed gate/table evidence and must not regress C5 or the host suites.

1. **Wave 1 — C4 (13 programs).** Smallest denominator and each program is isolated: fix
   `strlcpy`/`strlcat` (SIGSEGV), `explicit_bzero`/`timingsafe`/`uuid` (exit 134),
   `ldexp`/`modf` (134), `basename`/`dirname`/`getcap`/`telldir` (C46) and
   `fread`/`sleep` (1). Closing all 13 reaches 23/25 (the two mixed baseline rows can remain
   non-pass).
2. **Wave 2 — C3 (52 more bodies).** Float stdio first (`printfloat`, `scanfloat`, `freopen`),
   then `libc_exit`, then msun min/max/next/ilogb classification. The 61 non-pass split
   cleanly into 24 libc + 37 msun.
3. **Wave 3+ — C2 (219 more bodies).** Start with the c063 filesystem/ownership family
   (`faccessat`, `fchownat`, `fstatat`, `dup`), then `strtod`, then the libm transcendentals
   (`next`, `pow`, `sincos`, `erf`, `asin`), then pthread mutexes. This is the largest row and
   will need several cycles; the gate stays NOT MET until it crosses 90 %.
4. **C6 vendoring (parallel, separate ticket):** vendor the mlibc suite so the 85 % row is
   measurable; no threshold change.
5. **External C7-C9:** owner/environment runs; not a libc implementation wave.

Out of scope for every wave: threshold/exclusion/waiver edits, host-result substitution, new
non-donor suites, QEMU-backend changes, the separate 3.1 size decision (C74ih paper), POSIX
(1700/8/105), Pi 4 and the 24 h soak.

## Option B — defer/waive 1.2 for 0.1.0

Keep the gate and thresholds unchanged. 1.2 stays **NOT MET**, G1 stays **1 of 4**, the libc gate
stays **NOT MET**, and 0.1.0 stays **NO-GO**. The owner records the explicit deferral/waiver in
the release packet; the notes remain DRAFT/NO-GO and claim no libc/POSIX PASS. C5 remains MET.
The consequence is that 0.1.0 ships without a libc-conformance claim and the next release inherits
the same C2/C3/C4 effort; no row is reclassified as a pass.

## Release consequence

- **G1 1/4 measured rows; 1.2 NOT MET.** The libc gate stays NOT MET (1.2; 3.1 also NOT MET with
  the C74ih paper; 1.6/1.7 MET; 2.1/2.2 BLOCKED).
- **0.1.0 remains NO-GO** under the current freeze checklist. Option A does not change the gate
  until each row crosses its threshold; Option B is an explicit owner NO-GO/waiver, not a PASS.
- No tag, no libc/POSIX PASS, no threshold/test/waiver change is claimed by this paper.

## Exact approval needed

- **Owner (Bo Maryniuk; release policy):** choose **A** or **B**. If B, approve the exact 1.2
  deferral/waiver text and the NO-GO consequence for 0.1.0. Decide C6 mlibc vendoring and the
  C7-C9 external runs separately.
- **Lead/reviewer:** for A, issue the wave tickets by the classes above (starting with C4) and
  keep each wave evidenced by `gate --g1` + `CONFORMANCE.md`; for B, record the owner's waiver in
  the release packet and notes. No minion edits `tests/libc-oracle/g1.toml`, the donor tests,
  the exclusion list or any waiver.
- **3.1 owner:** the size decision remains the separate C74ih package; 1.2 and 3.1 are
  independent blockers.

## Evidence and reproduction

Committed evidence on `17f3fa95`:

- `tests/libc/CONFORMANCE.md` — the suite table and every non-pass row/reason.
- `tests/libc/conformance.tsv` — the machine-readable rows used by the gate.
- `tests/libc-oracle/g1.toml` — PROPOSED thresholds and the C2 exclusion cap; owner-only.
- `tickets/otto/reports/C74x.md` — round 3 fixes, the real-run table and the target commands.

```sh
cargo run -q -p raven-atf-run -- gate --g1          # exit 3; the block above
cargo run -q -p raven-atf-run -- table --write      # regenerates CONFORMANCE.md/TSV
cargo run -q -p raven-atf-run -- suite --suite picolibc --mode target \
    --sysroot build/x86_64/sysroot --run-backend qemu --max-timeout 90   # C5 target run
```

No QEMU was run for this paper; the target rows are the committed C74x runs. The G1 verdict is
the gate output above, not a hand count.
