<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# RavenBSD 0.1.0 first 24h soak failure — postmortem

- Date: 2026-11-18
- Run: C74as at product commit `1aa900c3`; soak log `/tmp/opencode/c74as-soak.log`
- Checker: C74dm `tests/scripts/check-soak-24h.sh` (in `27290e61`)
- Status: analysis only, **no 24h PASS claim**; no QEMU was started by this postmortem.

Evidence tags: **OBSERVED** = read/measured in the log, qtest sidecar or reports; **MISSING** =
not preserved; **INFERRED** = conclusion from the available evidence; **UNVERIFIED** = not run.

## Timeline [OBSERVED]

| UTC | Event |
|---|---|
| 2026-11-17T22:03:12Z | C74as preflight `fault-restart-release` passed (79.40 s); soak started at `1aa900c3` with `--profile release --arch x86_64 --soak 86400`; no stale QEMU at start; 1.0 T free |
| 22:03–00:0x | Runs 1-125 pass, `recovery_ms` 32-185 s; cumulative `pass=125 fail=0` |
| 00:0x (exact time not logged) | Run 126 fails: the next line is `fault-injection: run pass=125 fail=1 recovery_ms=117615`; `first_failure` is not printed because the soak never reaches the final summary |
| 00:0x–00:29 | 25 later iterations run and **pass individually** (`pass=126..150 fail=1`); the failing run's qtest artifacts are overwritten by these later passes |
| 2026-11-18T00:29:52Z | Lead sends SIGTERM; `exit 143` after **8665 s (2 h 24 m) of 86400 s**; 151 runs: 150 pass, 1 fail; **no `soak done` line** |
| 00:29Z | One orphan QEMU from the killed iteration (`fault-restart-release.img`), gone by 00:31Z; an unrelated otto QEMU remains |
| 01:29Z | The later `build/qtest/fault-restart-release.{log,json}` is a **pass** at `1aa900c3` (55,458 ms); it is not the failed iteration |

## Evidence available and missing

- **OBSERVED:** the 151-line soak log; the failure line
  `run pass=125 fail=1 recovery_ms=117615`; the 25 later pass lines; the C74as report
  (start, preflight, SIGTERM, 8665 s, 151 runs, orphan QEMU); the fault-injection script's
  behavior; the later passing qtest sidecar at the same old commit.
- **MISSING:** the failed iteration's qtest JSON/serial log; the exact failure UTC; the
  `first_failure` timestamp; the failing run's disk image/serial; the host load/`uptime`
  timeline; any panic, crash or qtest `forbidden_hits` from run 126.
- The soak script discards the qtest output (`run_once >/dev/null 2>&1`) and reuses the same
  `build/qtest/fault-restart-release.*` paths every iteration, so the failing iteration's detail
  was overwritten by run 127 and later passes. The current qtest sidecar is a **later pass**,
  not the failure.

## Verdict [INFERRED]

**Environment-favoured / undetermined; not a confirmed kernel or process bug. The run is
invalid as 24h evidence.**

- The one-off failure at run 126 was followed by 25 individual passes; there is no recurrence,
  no panic/crash, and no preserved failing qtest. A deterministic kernel/process defect would
  normally recur or leave a trace.
- The failure's `recovery_ms=117,615` is about 2× the 8665/151 ≈ 57 s average, but it is below
  the observed **passing** maximum of 185,202 ms; host pressure/timeout is plausible, not
  proven.
- The run was on an **old commit** (`1aa900c3`, before the current `27290e61`), on a shared host
  with concurrent minion activity/another QEMU, and was stopped at 2 h 24 m. The lead's note
  records concurrent host load; the C74as report records the killed-iteration orphan and an
  unrelated otto QEMU.
- The exact root cause cannot be proven: the failing artifacts are missing. The correct
  conclusion is "not a confirmed bug; environmental flake is best supported; clean rerun
  required", not "kernel PASS" or "kernel bug".

## Soak-script and checker behaviour

The C74as driver (`scripts/fault-injection.sh`) does:

```sh
run_once() {
    for case_name in $cases; do
        cargo xtask qtest --case "$case_name" --arch "$arch" || return $?
    done
}
...
while [ $(( $(date +%s) - start )) -lt "$soak" ]; do
    if run_once >/dev/null 2>&1; then pass=...; else fail=...; fi
    printf 'fault-injection: run pass=%s fail=%s recovery_ms=%s\n' ...
done
```

- It **discards qtest output** and does not copy the failing iteration's JSON/log before the
  next run.
- It **continues after a failure**; the cumulative `fail` stays 1 while later runs pass, so the
  log alone cannot tell which run failed beyond the transition line.
- It prints the final `soak done` only after the full budget; SIGTERM leaves the log without a
  summary. The C74dm checker correctly refuses that as incomplete.
- It records no `arch=`, `profile=`, `commit=`, `budget_s=` metadata; the C74dm checker needs a
  qtest sidecar or explicit options.

**Recommended script changes (owner: C74as / stuart, not this ticket):**

1. **Preserve per-iteration failure artifacts:** on a failed iteration, copy
   `build/qtest/<case>.json` and `.log` (and, if practical, the serial log and image) to a unique
   `build/soak/fail-<run>-<UTC>/` **before** the next run; never discard qtest stderr.
2. **Stop on first failure** (or pause and mark the run invalid) so the failure detail is not
   overwritten. The C74as run continued 25 iterations after the failure.
3. **Add metadata** (`arch=`, `profile=`, `commit=`, `budget_s=`) to the log and print an
   explicit interrupted/partial marker on SIGTERM; incomplete remains invalid.
4. Keep the final `[ "$fail" -eq 0 ]`; a cumulative fail must remain invalid. The C74dm checker
   already refuses `fail>0`, incomplete, short and missing-metadata logs; it needs no evidence
   weakening.

**C74dm checker/ticket discrepancy (owner: C74dm / dave or the lead):** the C74fb acceptance
uses `--dry-run`, but the checker has **no `--dry-run` option**:

```
$ sh tests/scripts/check-soak-24h.sh --log /tmp/opencode/c74as-soak.log --dry-run
soak-24h-check: unknown argument: --dry-run
exit 2
```

The checker never starts QEMU, so a dry-run is unnecessary. Either add a no-op `--dry-run` for
the acceptance or correct the C74fb acceptance to the real flags. The correct evidence check on
the C74as log is:

```
$ sh tests/scripts/check-soak-24h.sh \
    --log /tmp/opencode/c74as-soak.log \
    --qtest-log /home/bo/work/ravenos-workspace/minions/stuart/ravenbsd/build/qtest/fault-restart-release.log \
    --profile release
soak-24h-check: FAIL missing final soak summary (incomplete log)
exit 1
```

This is the C74dm refusal for the invalid log: the soak was terminated before the final summary,
and its only sidecar is a later pass. The checker's self-test is green:
`soak-24h-check: self-test PASS (...)` (exit 0).

## Exact clean-rerun conditions

A valid 24h soak must be a **single complete foreground run** on an exclusive host:

1. **Base:** current `origin/master` at the owner's release/freeze commit (at least `27290e61`;
   not `1aa900c3`), clean tree.
2. **Exclusive host:** no other QEMU (`pgrep -a qemu-system-x86_64` empty; kill/verify the
   C74as-style orphan), no concurrent minion builds/qtests/soaks and no other 24h job; free disk
   (the C74as preflight had 1.0 T free); low/known load for the whole window. The 24h soak owns
   the machine.
3. **Patched soak script** with failure-artifact preservation, stop-on-first-failure and
   metadata (the C74as/stuart follow-up) or an equivalent monitor that copies the failing qtest
   JSON/log before the next iteration.
4. **Exact run, foreground:**
   `/usr/bin/time -p sh scripts/fault-injection.sh --profile release --arch x86_64 --soak 86400`
   with a fresh log path (not the C74as log), the qtest sidecar, and a 24h+ timeout. No SIGTERM,
   no backgrounding, no second soak.
5. **Final log:** `fault-injection: soak done pass=N fail=0 first_failure=none`, elapsed
   ≥86,400 s, no crash/panic, and identifiable arch/profile/commit.
6. **C74dm checker:** `sh tests/scripts/check-soak-24h.sh --self-test` then
   `sh tests/scripts/check-soak-24h.sh --log <new log> --qtest-log <last qtest> --profile release`
   (or `--arch x86_64 --commit <sha>` if no sidecar; **no `--dry-run`**). It must exit 0.
7. Only then may the release evidence packet record a 24h soak PASS. **No 24h PASS is claimed
   now**; the C74as log is refused (incomplete, short, old commit, un-preserved failure).

## Follow-up owners

| Item | Owner |
|---|---|
| Preserve failure artifacts, stop on first failure, add metadata in `scripts/fault-injection.sh` | stuart / C74as |
| Add a no-op `--dry-run` or correct the acceptance; keep the checker's fail/incomplete/short refusals | dave / C74dm (checker), lead for the ticket text |
| Clean exclusive-host rerun at the freeze commit and checker PASS | release owner/coordinator |
| Record the checker PASS in the release evidence packet | release lane (C74dd harness) |

The C74as run remains **invalid**; no tag, no 24h PASS, no product change is made by this
postmortem.
