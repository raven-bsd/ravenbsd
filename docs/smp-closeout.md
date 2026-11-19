<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# SMP close-out matrix runner

Date: 2026-11-18. Ticket: S126 (Otto). Depends: S116, S125, S85e.
Status: runner delivered with a **per-cell cap table** (no blanket default); TCG4 10/10, the TCG4
close-out 5/5 and KVM1 5/5 are preserved evidence. KVM2 `posix-shell` is a console/rsh stall under
S135 (FAIL), KVM4 was not run; no partial column is claimed.

## Why

The 0.1.0 SMP gate needs one reproducible matrix, not a pile of ad-hoc `cargo xtask qtest`
invocations. This runner fixes the case list, the mode order, the host preflight, the artifact
layout and the percentile report, so the close-out numbers are produced the same way every time.
It never edits a case, never retries a failure and never relaxes a timeout.

## Matrix

Each cell is one foreground `cargo xtask qtest --case <qtest> --arch x86_64`; each cell runs
exactly once. Order: TCG first, then KVM 1/2/4; cases in the order below.

| Base case | TCG `-smp 4` | KVM `-smp 1` | KVM `-smp 2` | KVM `-smp 4` |
|---|---|---|---|---|
| `posix-shell` | `posix-shell` | `posix-shell-kvm1` | `posix-shell-kvm2` | `posix-shell-kvm4` |
| `posix-util-system` | `posix-util-system` | `posix-util-system-kvm1` | `posix-util-system-kvm2` | `posix-util-system-kvm4` |
| `libc-threads` | `libc-threads` | `libc-threads-kvm1` | `libc-threads-kvm2` | `libc-threads-kvm4` |
| `smp-libc-concurrency` | `smp-libc-concurrency` | `smp-libc-concurrency-kvm1` | `smp-libc-concurrency-kvm2` | `smp-libc-concurrency-kvm4` |
| `posix-fs-smp` | `posix-fs-smp` | `posix-fs-smp-kvm1` | `posix-fs-smp-kvm2` | `posix-fs-smp-kvm4` |

20 cells. The KVM cases append `-enable-kvm -cpu host` (and the mode's `-smp N`); the TCG case
uses the harness's fixed `-smp 4`.

## Usage

```
sh scripts/smp-closeout.sh                 # exclusive: preflight, then 20 cells
sh scripts/smp-closeout.sh --max-load 15 --wait-for-idle 1800   # close-out window
sh scripts/smp-closeout.sh --mode tcg4 --max-load 15 --wait-for-idle 600   # one column
sh scripts/smp-closeout.sh --quick --no-preflight --case posix-alarm --cell-timeout 20   # smoke, 1 cell
sh scripts/smp-closeout.sh --case posix-shell,posix-fs-smp             # subset, all modes
sh scripts/smp-closeout.sh --out /tmp/opencode/s126                    # explicit output root
```

| Option | Effect |
|---|---|
| `--case NAME` | base case name, repeatable/comma-separated; defaults to the five release cases |
| `--mode NAME` | `tcg4`, `kvm1`, `kvm2`, `kvm4`; repeatable/comma-separated; defaults to all four |
| `--quick` | TCG `-smp 4` column only; smoke, **not** release evidence |
| `--no-preflight` | skip `host-idle.sh`; records the host state but does not gate; smoke only |
| `--max-load LOAD` | preflight maximum 1-minute load average; unset uses the `host-idle.sh` default |
| `--wait-for-idle S` | poll the preflight in the foreground for up to S seconds (default 0: refuse) |
| `--cell-timeout S` | cap override for a **single-cell** invocation (e.g., a smoke); the table below applies otherwise |
| `--out DIR` | output root; default `build/smp-closeout/<UTC>` |
| `-h`, `--help` | usage |

Exit codes: `0` all requested cells passed; `1` first failure preserved; `2` host refused or bad
invocation (including a missing case file) — zero samples.

## Behaviour

- **Exclusive preflight.** The default runs `tests/smp/boot/host-idle.sh` (S107) before any
  build or sample. Any other QEMU, `xtask`/`cargo`/`rustc` job, or load1 above the bound
  (`--max-load`, otherwise the host-idle default) refuses the host. `--wait-for-idle S` polls in
  the foreground instead of refusing immediately. A refusal is **zero samples**, not a retry and
  not a partial pass. The close-out window itself is provided by S116/S107; this runner never
  waits forever (the timeout is explicit).
- **Pinned libc.** `cargo xtask libc-pack --arch x86_64` runs once before the first cell, so the
  matrix cannot silently use a stale `libc.a` (the S119 lesson).
- **Per-cell cap.** Every matrix cell has an entry in the table below (expected runtime plus a
  proportional margin); there is no blanket default. A 1 s-tick watchdog kills only that cell's
  QEMU (matched by `file=build/qtest/<case>.img`), so the qtest harness still exits normally and
  writes its serial log/JSON; a hard `timeout` kills the harness at cap+5 s if it does not. A
  cell over its cap is recorded as `result=timeout`, `cell_timeout=1`, the last 20 serial lines
  are printed, and the run stops. `--cell-timeout S` may override one cell only (a single-cell
  invocation); any other use is rejected. This bounds a console stall without changing the case
  or its assertions.
- **Stop on the first failure.** The runner writes `FAILED` (mode, case, rc, artifact paths,
  `cell_timeout`), prints the last 20 lines of the qtest stdout and serial log, and exits 1.
  Remaining cells are not run and no cell is retried.
- **Artifacts.** `<out>/<mode>/<qtest>.json`, `.log`, `.out` for every cell; `results.tsv` for
  the whole run; `host-per-cell.tsv` (load1/QEMU count immediately before each cell);
  `<out>/<mode>/<qtest>.telemetry.tsv` (UTC, load1, QEMU/xtask/cargo/rustc counts every 5 s while
  the cell runs); `<qtest>.cap-fired`/`<qtest>.cap.log` when the cell cap fired;
  `host-idle.txt`, `host-state-before.txt`, `host-state-after.txt`, `libc-pack.log`. The
  telemetry counts include the cell's own qtest/xtask, so another lane's overlap shows as
  QEMU>=2 or xtask>=2 during the cell.
- **Statistics.** `summary.txt` prints `n/min/p50/p90/max` per mode and overall, using the qtest
  JSON `duration_ms` and nearest-rank percentiles (the S120 method). A failure contributes no
  duration.

## Self-test (host-only, 2026-11-18, `3a8fd623`)

```
$ sh -n scripts/smp-closeout.sh                              # exit 0
$ sh scripts/smp-closeout.sh --quick --no-preflight --case posix-alarm
smp-closeout: [1] mode=tcg4 case=posix-alarm
tcg4: n=1 min=8911 p50=8911 p90=8911 max=8911 (ms)
overall: n=1 min=8911 p50=8911 p90=8911 max=8911 (ms)
exit 0

$ sh scripts/smp-closeout.sh --quick --case posix-alarm      # preflight on
host-idle: REFUSE context=smp-closeout busy=load(10.55>2.0) qemu xtask/cargo/rustc
smp-closeout: exclusive preflight refused the host; zero samples run
exit 2

$ sh scripts/smp-closeout.sh --quick --no-preflight --case posix-fs-smp --out /tmp/opencode/s126-selftest
smp-closeout: FAIL mode=tcg4 case=posix-fs-smp rc=1 result=fail
smp-closeout: stopped on the first failure; no retry
exit 1; FAILED + tcg4/posix-fs-smp.{json,log,out} preserved
```

The failure-path self-test used the known-red S125/Mel `posix-fs-smp` case: it still fails with
`EBADF/OOM/EIO` and 7 worker failures on `3a8fd623`, which is expected while S125/S85e are open.
That run is a self-test of the runner, not the close-out matrix.

## Host telemetry

While a cell runs, the runner samples the host every 5 s into
`<out>/<mode>/<qtest>.telemetry.tsv`: `utc`, `load1`, and the QEMU, `xtask`, `cargo` and `rustc`
process counts. The counts include the cell's own qtest/`xtask`, so another lane's overlap shows
as QEMU>=2 or xtask>=2 during the cell (or `cargo`/`rustc` outside the cell's build phase).
`host-per-cell.tsv` keeps the pre-cell snapshot for the same cells.

## Exclusive-window result (2026-11-18, rev 55)

The rework ran only after a genuine window: `host-idle` PASS at load1 **0.19** with no QEMU or
xtask, and the release image built from `c1e85d4c` (product = `c4f7d748`).

```
sh scripts/smp-boot-soak.sh --kvm1 0 --kvm2 0 --kvm4 0 --tcg4 10 --wait-for-idle 1200 --max-load 2
```

- TCG4 **run 1/10 failed**: no `login:` within 30 s. The serial reaches `smp: 4 cpus online`,
  `/init`, all services and the network, then:

  ```
  svcd: getty failed: bad ELF: SegmentOutsideUserRange
  ```

- The soak stopped at the first failure (no retry), so the 5-case TCG4 close-out column was not
  run and no pass is claimed. Raw log: `build/smp-boot-soak/20261118T220332Z/tcg4/run-1.log`.

Cause (product, outside S126): the release `getty` is dynamically linked (`PT_INTERP
/libexec/ld-elf.so.1`), and `ld-elf.so.1`'s first `PT_LOAD` is at vaddr `0x0`.
`crates/raven-spawn/src/lib.rs:1483` (P9a) parses the **interpreter** with `USER_MIN..USER_END`,
so it rejects the loader (`USER_MIN` is `0x1000`); the kernel's `exec.rs:106` uses `0..USER_END`
for the interpreter. Trigger: SESSION1's dynamic getty; owner: P9a/carl (`raven-spawn`), with
SESSION1/phil and K97/K97a (loader) as the other touch points. This is a deterministic release
boot blocker, reproduced on a quiet host.

This section is the accurate close-out state: runner and telemetry delivered, release gate
blocked by the getty dynamic-ELF defect.

## Rework round 2 (rev 56): S129 and the 6/10 run

Branch `otto/S126` merged `origin/master` `4d3e1576` (no rebase): S129's vaddr-0 interpreter parse
is in `crates/raven-spawn` (`0..USER_END`), with S127's readiness timing and S107's host
telemetry.

**Image.** The first clean-tree build packed a **stale** `ld-elf.so.1`: the Nov 16 02:28 stub was
still in `build/x86_64/` and `xtask initfs` skips `libc-link` when the dynamic outputs already
exist. That image (sha256 `20396d1a...`) failed run 1 with
`ld-elf.so.1: no direct target start yet`. `cargo xtask libc-link --arch x86_64` rebuilt the
runtime (`ld-elf.so.1` 4352 -> 107128 bytes), and the valid image is:

- commit `5aec5fc502816e7a2011abf343ea3bd37c42b179`
- sha256 `90fa029d863c4ca780fee51637f7da231fdadad7aae696674de3997ecc31b893`
- size 335544320 bytes

**Window.** Waited for S128's aarch64 matrix to finish (23:21:59Z: fast=50 soak=5 cross=5,
timed_out=0 failed=0). The soak preflight passed at load1 4.76.

```
sh scripts/smp-boot-soak.sh --kvm1 0 --kvm2 0 --kvm4 0 --tcg4 10 --wait-for-idle 1200 --max-load 5
```

| Run | login | host before (load/qemu/jobs) | host after |
|---|---|---|---|
| 1 | 11.673 s | 4.76 / 0 / 0 | 4.34 / 0 / 0 |
| 2 | 10.753 s | 4.34 / 0 / 0 | 4.04 / 0 / 0 |
| 3 | 10.098 s | 4.04 / 0 / 0 | 3.96 / 0 / 0 |
| 4 | 10.180 s | 3.96 / 0 / 0 | 4.04 / 0 / 1 |
| 5 | 10.970 s | 4.04 / 0 / 1 | 4.17 / 0 / 2 |
| 6 | 13.258 s | 4.17 / 0 / 2 | 5.65 / 0 / 4 |
| 7 | **FAIL 30 s timeout** | 5.65 / 0 / 4 | **28.32 / 0 / 5** |

Run 7's serial reaches `svcd: all services up` at 27.8 s (clean runs: ~10-13 s to `login:`), with
the S127 timing lines showing every service ready (getty `ready_at=6123`, netd `5495`, sysd
`4831`), then no `login:` before the 30 s timeout. Other lanes' jobs appeared during the soak
(jobs 0 -> 5, load 28.32): the guest was starved after all services were up. Raw log:
`build/smp-boot-soak/20261118T233016Z/tcg4/run-7.{log,stamped.log}`; telemetry:
`build/smp-boot-soak/20261118T233016Z/host.tsv`.

**Classification:** host-contention window violation, not a runner or case defect. The first six
boots are clean with S129; run 7 failed only after competing jobs started. Per the directive the
soak stopped on the first failure; the 5-case TCG4 column and the KVM modes were not run, and no
partial pass is claimed. Owner: the S107/S116 exclusive window (other lanes), with S127's login
banner timing affected by the 30 s bound under starvation.

## Rework round 3 (rev 57): S132/S130, 10/10 and the KVM2 console stall

Branch merged `origin/master` `6f6cba1e` (no rebase): S132 thread-list unlink (`b79c2056`) and
S130 release-login expectation (`6679570b`). Merge commit `8af9dd93`; clean tree.

**Image.** `cargo xtask libc-link --arch x86_64` refreshed the dynamic runtime, then
`cargo xtask image --arch x86_64 --profile release`:

- commit `8af9dd93be08630846d83b5848e703f7e5721880`
- sha256 `225bd44e89e3436325de82f9e7b5604a4f815d5abc2d654d7cdeb9fefec32d41`
- size 335544320 bytes; `ld-elf.so.1` 107128 bytes

**TCG4 boot gate (10/10 PASS).**

```
sh scripts/smp-boot-soak.sh --kvm1 0 --kvm2 0 --kvm4 0 --tcg4 10 --wait-for-idle 1200 --max-load 5
```

`login:` p50 **7.990 s**, p90 **8.169 s**, worst **9.155 s**; host telemetry jobs=0 for all ten
boots, load 3-4, QEMU=0 apart from the boot itself. Logs:
`build/smp-boot-soak/20261119T003239Z/` (`host.tsv`, `summary.txt`).

**TCG4 close-out column (5/5 PASS).** `--max-load 5 --wait-for-idle 300`; host load 1.6-1.8,
QEMU/xtask only the cell's own.

| Case | Duration |
|---|---|
| `posix-shell` | 24.7 s |
| `posix-util-system` | 22.1 s |
| `libc-threads` | 135.3 s |
| `smp-libc-concurrency` | **48.0 s (S132 confirms the fix)** |
| `posix-fs-smp` | 111.3 s |

n=5, min 22.076 s, p50 47.974 s, p90 135.262 s, max 135.262 s. Artifacts:
`build/smp-closeout/r3-tcg4/`.

**KVM columns — stopped on the first failure.**

- KVM `-smp 1`: **5/5 pass** (shell 8.3 s, util 12.5 s, threads 13.4 s, concurrency 13.7 s,
  fs-smp 23.7 s), including `smp-libc-concurrency-kvm1`.
- KVM `-smp 2`, `posix-shell-kvm2`: **FAIL**, 379.1 s. The serial stops mid-line at the first
  brush command (`raven# /bin/brush --posix -c '... case a`) with no shell output, the qtest step
  for `^until:2$` reports "the process exited before this step finished", and the guest exits 0.
  Host telemetry for that cell: load 2.26 -> 1.03, QEMU/xtask 0-1 (the cell's own), no external
  jobs. The same case passes on TCG4 and KVM1. Artifacts:
  `build/smp-closeout/r3-kvm/kvm2/posix-shell-kvm2.{log,json,out,telemetry.tsv}`.
- KVM `-smp 2` remaining cells and all KVM `-smp 4` were not run (stop rule); no partial column
  is claimed.

**Classification:** the KVM2 `posix-shell` stall is in the console/rsh path (the command echo
never completes and no brush output appears), not the S126 runner, not the shell assertions and
not S132. S130 only changed `tests/qtest/cases/release-login.toml`, so it is not the code cause.
Owner: console/rsh (phil/TTY1 or the consd/rsh lane) with the KVM2 console timing. The `smp-libc-concurrency`
cells prove S132 is fixed (TCG4 48.0 s, KVM1 13.7 s).

## Rework round 4 (rev 58): per-cell cap

*(Superseded by rev 59's per-cell table; kept as history.)*

Round 3's KVM2 `posix-shell` cell stalled 379 s while the runner waited on the qtest case timeout
(1800 s). The runner now bounds every cell; the case and its assertions are unchanged.

**Implementation.**

- `--cell-timeout S` (default **300 s**, `0` disables; non-negative integer only).
- A 5-second-tick watchdog kills only the cell's QEMU, matched by
  `file=build/qtest/<case>.img`, so the harness sees the exit and still writes its serial log and
  JSON. A hard `timeout --kill-after=10` around the qtest kills the harness itself at cap+30 s if
  it does not exit.
- A capped cell is recorded as `result=timeout`, `cell_timeout=1`, `cell_timeout_s=S`; the last
  20 serial lines are printed, `FAILED` is written, and the run stops. `results.tsv` gained a
  `cell_timeout` column; `<qtest>.cap-fired`/`<qtest>.cap.log` preserve the kill evidence.
- Stale `build/qtest/<case>.{log,json}` are removed before each cell, so a killed cell cannot be
  reported from an earlier run's JSON.
- No case, timeout, expectation or fixture was edited; no retry.

**Shell test without QEMU.** A fake `cargo` under `/tmp/opencode/s126-cap-fake` handles
`xtask libc-pack` (exit 0) and `xtask qtest` (writes a 30-line fake serial + JSON, spawns a fake
QEMU whose cmdline contains the image path, then waits). With `--cell-timeout 2`:

```
$ PATH=/tmp/opencode/s126-cap-fake:$PATH timeout 120 sh scripts/smp-closeout.sh \
      --quick --no-preflight --case posix-alarm --cell-timeout 2 --out /tmp/opencode/s126-cap-test
smp-closeout: FAIL mode=tcg4 case=posix-alarm rc=1 result=timeout cell_timeout=1
--- serial (last 20 lines) ---
fake serial line 11
...
fake serial line 30
smp-closeout: stopped on the first failure; no retry
exit 1
```

`FAILED` has `cell_timeout=1`/`cell_timeout_s=2`; `results.tsv` has `timeout ... 1`; the cap log
names the killed fake QEMU pid. No QEMU was started by this test.

**Preserved round-3 evidence (unchanged, no close-out claim).**

| Gate | Result |
|---|---|
| TCG4 boot loop 10/10 | **PASS** (login p50 7.990 s, p90 8.169 s, worst 9.155 s) |
| TCG4 close-out column | **5/5 PASS** (shell 24.7, util 22.1, threads 135.3, concurrency 48.0, fs-smp 111.3 s) |
| KVM1 column | **5/5 PASS** |
| KVM2 `posix-shell` | **FAIL** — console/rsh input stall, now under S135/Stuart; 379 s |
| KVM2 rest / KVM4 | not run (stop rule); no partial column |

`make check-fast` (host clear: qemu=0 xtask=0 cargo=0 rustc=0): `check-fast: PASSED`.

## Per-cell cap table (rev 59)

Caps are seconds: expected runtime + a proportional margin, from the round-3 measurements. There
is no blanket default. A cell over its cap is FAILED/TIMEOUT with the last 20 serial lines; it is
never retried.

| Case | TCG4 | KVM1 | KVM2 | KVM4 | Measured (TCG4) |
|---|---:|---:|---:|---:|---|
| `posix-shell` | 40 | 20 | 20 | 20 | 24.7 s (KVM 8.3-15 s) |
| `posix-util-system` | 35 | 25 | 25 | 25 | 22.1 s (KVM 12.5-20 s) |
| `libc-threads` | 170 | 30 | 30 | 30 | 135.3 s (KVM 13.4-20 s) |
| `smp-libc-concurrency` | 70 | 30 | 30 | 30 | 48.0 s (KVM 13.7-20 s) |
| `posix-fs-smp` | 140 | 40 | 40 | 40 | 111.3 s (KVM 23.7-30 s) |

`--cell-timeout S` overrides the cap for a single-cell invocation only; a multi-cell selection
with an override is rejected, and a case with no table entry (e.g., a custom smoke) requires the
override. The watchdog ticks every second and kills only the cell's QEMU; the hard `timeout` is
cap+5 s.

## Rework round 5 (rev 59): proportional caps

- Replaced the blanket 300 s default with the 20-cell table above; the watchdog ticks at 1 s and
  the hard harness cap is **cap+5 s**, not cap+30 s.
- Negative shell tests (no QEMU): `--quick --no-preflight --case posix-alarm` (no table entry, no
  override) exits 2 with "no cap table entry ... use --cell-timeout N for a single-cell run";
  `--quick --no-preflight --cell-timeout 2` (5 cells) exits 2 with "overrides one cell; this
  invocation selects 5".
- Cap shell test with the fake cargo (no QEMU): `--case posix-alarm --cell-timeout 2` exits 1
  with `result=timeout`, `cell_cap_s=2`, `cell_timeout=1`, serial lines 11-30 printed, `FAILED`
  preserved.
- Preserved evidence is unchanged: TCG4 10/10, TCG4 close-out 5/5, KVM1 5/5; KVM2 `posix-shell`
  FAIL (S135 console/rsh), KVM4 not run. No close-out claim.
- Session caps are proportional: `timeout 150 make check-fast` (measured ~80 s) and
  `timeout 30 git commit/push`.

## When the full matrix runs

Only after **S125 and Mel/S85e land** and **S116/S107 provide the exclusive window**. Until then
this document and the runner are the deliverable; a shared-host run is not release evidence. If a
cell fails at that point, the preserved `FAILED` artifact and the first failing serial go to the
case owner (S125/Mel for `posix-fs-smp`, S119/Phil for stdio, S113/Stuart for kernel SMP).

## Donor reconciliation (DR-52)

No donor code was copied. The runner reuses in-tree harness pieces and a statistical method.

| Topic | Illumos finding | BSD file/licence | Linux finding | Decision |
|---|---|---|---|---|
| exclusive host gate for timing tests | no donor gate; fakes/dedicated CI (CDDL, behaviour only) | no permissive donor gate to port; in-tree S107 `tests/smp/boot/host-idle.sh` (BSD-3) | CI practice, read-only | reuse the in-tree preflight as-is; no copy |
| deterministic case matrix + artifacts | no donor harness | in-tree `scripts/tlb-stress.sh` (S120, BSD-3) is the closest pattern | CI matrix practice, read-only | same in-tree conventions (ordering, results.tsv, stop on first failure); no donor code |
| p50/p90/max | no donor | in-tree S120 nearest-rank implementation (BSD-3) | read-only | reuse the nearest-rank method; no donor code |
| QEMU SMP case variants | CDDL behaviour only | in-tree `posix-*-kvm*` / `libc-threads-kvm*` cases (BSD-3) | read-only | use the merged in-tree cases; no edits |

Donor revisions: `netbsd 36eddebcc36cb0775aa2962005b9c05419a6ff01`,
`freebsd 133344405f532bd54ff3629a77d4c4e062f01fdb`,
`illumos-gate 586678ac1bcbd77644a6ec4dac7f79d676cfb4ec`,
`musl 7813e9119eb5fe681b6750b8a033a0490bca0941`.
