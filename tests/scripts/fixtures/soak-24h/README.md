<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# 24-hour soak evidence fixtures

`tests/scripts/check-soak-24h.sh` checks the foreground output of the real fault-injection
soak:

```
/usr/bin/time -p sh scripts/fault-injection.sh \
    --profile release --arch x86_64 --soak 86400
```

It refuses a short run, an incomplete or interrupted log, a missing log, any failed iteration,
a crash count above zero, and a log without an identifiable arch/profile/commit. It never starts
QEMU.

## Running on the final C74as log

The C74as run writes `/tmp/opencode/c74as-soak.log` via `/usr/bin/time -p`. The soak itself does
not print arch/profile/commit, so pass the last qtest serial log as the metadata sidecar:

```
sh tests/scripts/check-soak-24h.sh \
    --log /tmp/opencode/c74as-soak.log \
    --qtest-log <stuart-clone>/build/qtest/fault-restart-release.log \
    --profile release
```

The qtest banner supplies `arch` and the commit; `--profile release` supplies the profile the
soak was invoked with. If the qtest log is unavailable, pass
`--arch x86_64 --profile release --commit <sha>` explicitly.
The checker also accepts metadata already present in the log (`arch=`, `profile=`, `commit=`,
`elapsed_s=`, `budget_s=`, `crashes=`) and a captured `# command:` line.

Exit codes: `0` complete clean 24 h evidence; `1` evidence refused; `2` usage or unreadable input.

## Fixtures

| Fixture | Expected | Reason |
|---|---|---|
| `good.log` + `qtest.log` | PASS | elapsed 86400 s, 3/3 iterations, 0 failures, 0 crashes |
| `real-only.log` + `qtest.log` | PASS | the C74as shape: only `time -p real` plus the qtest sidecar |
| `short.log` | refused | elapsed 1801 s |
| `missing.log` (absent) | refused | no log file |
| `crash.log` | refused | `crashes=1` |
| `first-failure.log` | refused | `fail=1` with the first-failure timestamp |
| `incomplete.log` | refused | run lines but no final `soak done` summary |
| `no-elapsed.log` | refused | no elapsed seconds, `time -p real`, or budget |
| `iteration-mismatch.log` | refused | summary says 2 iterations, the log has 1 run line |

`--self-test` runs all fixtures plus arch/profile/commit mismatch, argument and syntax checks.

## Log fields

* `fault-injection: run pass=N fail=M recovery_ms=T` — one completed iteration.
* `fault-injection: soak done pass=N fail=M first_failure=X` — the final summary; `X` is `none`
  exactly when `fail=0`.
* `real N.NN` — the `/usr/bin/time -p` wall-clock line; the checker takes the integer part and
  requires at least 86400.
* A captured command line or `budget_s=86400` also proves the script's loop budget. A complete
  summary with a 86400 s budget is accepted when no `real` line exists, because the loop cannot
  print the summary before the budget elapses.
* The recovery milliseconds are a lower bound only; they are used when no wall-clock or budget
  evidence exists.
* The soak driver prints only cumulative pass/fail counters. A run with `fail=0` has no crashed
  iteration; any `fail>0` is refused before crash classification. An explicit `crashes=` token
  (or a crash/panic marker) is also rejected above zero.
