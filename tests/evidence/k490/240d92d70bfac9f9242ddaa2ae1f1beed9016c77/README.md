<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# K490 pty-privsep-exec determinism evidence: `240d92d70`

Date: 2026-11-22 (UTC). Branch `phil/K490` at `240d92d70bfac9f9242ddaa2ae1f1beed9016c77`.

## Command

```sh
timeout 2400 sh -c 'for i in $(seq 3 50); do
  timeout 120 cargo xtask qtest --case pty-privsep-exec --arch x86_64
  ...
done'
```

Runs 1-2 were measured separately (`55.25 s`, `38.65 s`) and are in `results.tsv`. The measured
per-run cap is **120 s**; the ticket's overall cap is **2400 s**.

## Result

| Item | Value |
|---|---:|
| Runs recorded | 36 of 50 |
| Passed (`exit 0`, `pass`, both round-trips, `allocations=2`) | 32 |
| Failed | 4 |
| Total wall of the recorded 36 | 2468.27 s |
| Overall `timeout 2400` | **expired during run 37** |

Host load at start: `15.91 35.81 46.71` (20 CPUs). Load while the loop ran rose to
`62.25 62.41 59.79`; the 120 s per-run cap is the measured cap and was not raised.

## Failures

| Run | Exit | Wall ms | Observation |
|---:|---:|---:|---|
| 3 | 124 | 120015 | per-run cap; serial stops at `pty-privsep-exec: start uid=0` (QEMU starved under load) |
| 4 | 1 | 16690 | guest boot/QEMU failure; serial empty, no expectations matched |
| 28 | 124 | 120013 | per-run cap; serial stops while booting services under load |
| 29 | 1 | 25617 | guest boot/QEMU failure; serial empty, no expectations matched |

No SCM_RIGHTS `ENOTSOCK`/`EPROTO` boundary failure appears in the 32 completed runs. The four
failures are boot/timeout infrastructure under the measured load, not the K482 control-boundary
row. No run was retried or dropped.

## Files

| File | Contents |
|---|---|
| `results.tsv` | 36 rows: run, exit, wall_ms, pass, roundtrip count, allocations count |
| `logs/` | per-run `cargo xtask qtest` output |
| `serials/` | per-run `build/qtest/pty-privsep-exec.log` copy |
| `SHA256SUMS` | hashes of these files |

This evidence is **BLOCKED**, not a green 50/50 claim: the ticket's overall cap expired with 14
runs unrecorded, and four recorded runs failed on boot/load.
