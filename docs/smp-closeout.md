<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# SMP close-out matrix runner

Date: 2026-11-18. Ticket: S126 (Otto). Depends: S116, S125, S85e.
Status: runner implemented and host-self-tested; the release matrix is **not** run yet.

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
sh scripts/smp-closeout.sh --quick --no-preflight --case posix-alarm   # smoke, 1 cell
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
- **Stop on the first failure.** The runner writes `FAILED` (mode, case, rc, artifact paths),
  prints the last 20 lines of the qtest stdout and serial log, and exits 1. Remaining cells are
  not run and no cell is retried.
- **Artifacts.** `<out>/<mode>/<qtest>.json`, `.log`, `.out` for every cell; `results.tsv` for
  the whole run; `host-per-cell.tsv` (load1/QEMU count immediately before each cell);
  `host-idle.txt`, `host-state-before.txt`, `host-state-after.txt`, `libc-pack.log`.
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
