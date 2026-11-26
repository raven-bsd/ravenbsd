# Lock stress evidence

SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

Bounded stress runner for `raven-turnstile` and `raven-cyclic`. The product crates are pure
models; the harness supplies process-level parallelism, measured host load, per-iteration logs and
latency/jitter samples. It does not change any lock or timer implementation.

## Invocation

```text
timeout 900 sh scripts/lock-stress.sh --load 64 --iterations 10
```

Optional: `--workers W` (default 4), `--out DIR` (default `/tmp/opencode/lock-stress-<UTC>`).
The script builds the test binaries with `cargo nextest list` and runs the `stress_` tests directly
from those binaries. Each worker gets a separate log and sample file.

## Load control

`--load N` is a **target** one-minute host load. The script reads `/proc/loadavg`; if the current
load is below N, it starts `N - load` CPU burners for the run. If the host is already above N, no
burners are added and the measured load is recorded (adding more load would make the measurement
less meaningful). The burners are killed on exit. Per-iteration rows record 1/5/15-minute load.

## Evidence layout

```text
<out>/binaries.json             nextest binary discovery
<out>/iterations.tsv            iteration, worker, binary, exit, wall_ns, load1/5/15, log
<out>/iter/<n>-<w>-<bin>.log    complete test output for one worker
<out>/samples/*.tsv             label and nanoseconds per sample
<out>/samples/all.tsv           concatenated samples
```

A non-zero test exit or a 120 s per-binary timeout is a **red iteration**: the script stops, keeps
the log, prints `lock-stress: BLOCKED`, and exits non-zero. No retry hides a failure. The global
acceptance cap is 900 s; a cap expiry keeps the partial directory and is reported as TIMEOUT.

## Percentiles

After all iterations pass, the script sorts the sample files and prints nearest-rank
`samples`, `p50_ns`, `p90_ns`, `p99_ns` and `max_ns` per scenario. The scenario labels are:

- `turnstile_uncontended_acquire`, `turnstile_uncontended_release`
- `turnstile_contended_block`, `turnstile_contended_wake`
- `turnstile_timeout_abort`
- `turnstile_wake_all`
- `turnstile_no_lost_wakeup`
- `turnstile_inversion_block`, `turnstile_inversion_restore`
- `cyclic_deadline_jitter`
- `cyclic_order_fire`, `cyclic_cancel`, `cyclic_reprogram`, `cyclic_coalesce`, `cyclic_repeat`

The turnstile samples measure operation latency in the pure model. The cyclic jitter test
busy-waits on a monotonic deadline and reports scheduler delay; it is the host-load-sensitive
metric.

## Observed run

`sh scripts/lock-stress.sh --load 64 --iterations 10` at `2026-11-21T15:32:02Z`:

- 10/10 iterations green; 160 worker runs (0 failures), 4 workers, 22 burners.
- Measured load1: 42.84–53.24; the host was already above the 64 target for part of the run.
- 496,640 samples; 19.0 s from first to last evidence file; max worker wall 2.33 s.
- Evidence: `/tmp/opencode/lock-stress-20261121T153202Z`.

| scenario | samples | p50 ns | p90 ns | p99 ns | max ns |
|---|---:|---:|---:|---:|---:|
| `turnstile_uncontended_acquire` | 10,240 | 19,973 | 34,139 | 73,737 | 24,046,782 |
| `turnstile_uncontended_release` | 10,240 | 18,629 | 32,628 | 47,957 | 31,804,519 |
| `turnstile_contended_block` | 30,720 | 3,208 | 5,068 | 7,296 | 95,728,608 |
| `turnstile_contended_wake` | 30,720 | 1,869 | 2,987 | 4,008 | 24,072,580 |
| `turnstile_timeout_abort` | 10,240 | 1,301 | 1,501 | 1,903 | 11,254,519 |
| `turnstile_wake_all` | 10,240 | 2,461 | 2,895 | 4,414 | 9,031,113 |
| `turnstile_no_lost_wakeup` | 337,920 | 12,596 | 18,356 | 28,943 | 122,935,701 |
| `turnstile_inversion_block` | 10,240 | 2,533 | 3,131 | 4,704 | 137,158,926 |
| `turnstile_inversion_restore` | 10,240 | 1,602 | 2,637 | 5,247 | 7,586,837 |
| `cyclic_deadline_jitter` | 10,240 | 5,912,776 | 24,537,653 | 75,126,118 | 84,547,493 |
| `cyclic_order_fire` | 5,120 | 616 | 731 | 1,564 | 86,755 |
| `cyclic_cancel` | 5,120 | 118 | 147 | 352 | 9,967 |
| `cyclic_reprogram` | 5,120 | 520 | 638 | 1,423 | 32,300 |
| `cyclic_coalesce` | 5,120 | 783 | 967 | 2,333 | 17,055,513 |
| `cyclic_repeat` | 5,120 | 491 | 578 | 1,335 | 22,460 |

## Reading a failure

Open the `log` path from the failed `iterations.tsv` row. The test assertion or timeout is at the
end of that file; the `exit` column is 124 for a per-binary timeout. The iteration row keeps the
load measured at completion. A failing stress assertion is reported to the crate owner with the
exact label and sample; no test or budget is weakened in this harness.
