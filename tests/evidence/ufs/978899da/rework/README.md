<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# K378 rework round 1 — failed-case re-runs

Re-ran only the five failed cases from the original matrix on tree `978899da`
(branch `mel/K378`, no source change). Classification was written first from
`serial/*.full.log`; the re-runs confirm it.

| Case | Re-run | Result | First failing line |
|---|---|---|---|
| ufs-root | `--no-build`, cap 120 s | FAIL, 112 s | `step 23 (^init exited \(0\)$) failed` |
| ufs-update | `--no-build`, cap 300 s | FAIL, 65 s | `step 15 (^init exited \(0\)$) failed` |
| ufs-update-persist | `--no-build`, cap 150 s | FAIL, 61 s | `step 10 (^init exited \(0\)$) failed` |
| disk-tools | `--no-build`, cap 200 s | FAIL, 135 s | `step 7 (^blockd running [0-9]+$) failed`; serial `blockd: waiting for disk:0` |
| ufs-full | build, cap 480 s | FAIL, 124 s | `step 32 (^init exited \(0\)$) failed` |

No case shows a UFS read, write, mount, bitmap, fsck, or pivot error in the
confirming runs. The four `qemu-ufs` cases match every product expectation and
stop only at the post-`exit 0` `login:` prompt (K234's restarting getty).
`disk-tools` stops at `devd`'s second legacy-IRQ claim (`irq: resource busy`).

## Transient run kept for honesty

`ufs-update-r2.log` (cap 300 s) failed after the root pivot with
`login: unable to start the session shell` / `svcd: getty failed: input/output
error`. It ran while a QEMU left over from an earlier cap-expired attempt was
still alive. After killing that process and rebuilding the baseline root disk
(`image-baseline.log`), `ufs-update-r3.log` passed `k185-first`, `ufs on /` and
stopped only at `init exited (0)`. The EIO is recorded, not hidden; it did not
reproduce on the clean boot.

`ufs-update` cap 100 s and `ufs-full` cap 300 s expired on the first re-run
attempt (their QEMU children were then killed by PID); the measured case
runtimes are 61-118 s and 124-222 s, and the confirming caps were 300 s / 480 s.

## Logs

`k378r1-<case>*.log` are the qtest runs; `<case>.json` are the final harness
JSONs; `k378r1-image-baseline.log` is the baseline root-disk rebuild.
