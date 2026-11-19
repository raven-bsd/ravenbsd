<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# QEMU test harness lifecycle

`cargo xtask qtest` loads the case files under `tests/qtest/cases/`, builds the per-case image
unless `--no-build` is given, launches QEMU, captures the serial stream and writes
`build/qtest/<case>.log` plus `build/qtest/<case>.json`.

## Success path (S137)

A case is complete when every `[[expect]]` has matched in order (or unordered, as declared), no
`[[forbid]]` line has appeared, and every `[[step]]` in the dialogue has finished. From that point
QEMU gets a **2 s completion grace** to exit on its own. If it is still alive after the grace, the
harness:

1. sends `SIGKILL` to the QEMU process group;
2. drops the serial stdin pipe and the QMP socket client;
3. reaps QEMU with `wait`;
4. removes the per-run `<case>.vars.fd` copy and the QMP socket file.

The case image is **released, not deleted**: QEMU no longer holds it, and consecutive
`--no-build` runs reuse it. The process-group kill also removes any child that inherited the
serial pipe, so no orphan can keep the image open and no later run sees an empty serial stream.

The serial stream is also written to `build/qtest/<case>.log` incrementally, so an external
`timeout` that kills the runner still leaves the guest log for diagnosis; a normal finish
rewrites it with the cleaned lines.

The complete guest log is the pass evidence. `expect_exit = "success"` (or `"any"`) is satisfied
by a forced completion because the guest already printed every expected marker; a case that
expects `"failure"` or `"timeout"` keeps its original, stricter classification and is never
turned into a pass by the grace path. A case with no `[[expect]]` also keeps the original
process-driven lifetime, because the harness has no completion marker to trust.

The 1800 s `posix-shell` timeout is only a safety net. A healthy run exits within the grace after
`init exited (0)` and never waits for that bound.

## Timeout and failure paths

`timeout_secs` is unchanged: a run that never reaches its last expectation still hits the case
timeout, the process group is killed, and the JSON result is `timeout`/`fail` with the captured
serial log. Step timeouts and forbidden lines behave as before. The lifecycle fix does not
weaken or skip any expectation.

## Host-side tests

`tests/qtest/src/runner_ut.rs` covers:

- a watched run that prints the final marker and then sleeps: `completed` is set, the process is
  reaped in a few seconds and no process remains;
- a live runner holding a file: `kill_tree` terminates the group, `wait` reaps it and the file
  can be replaced immediately (the image-lock path).

`tests/qtest/src/execute_ut.rs` covers the same complete-log/live-runner path through `execute`
(`ResultKind::Pass`, elapsed time bounded by the grace) and the removal of per-run vars/QMP
artifacts.

## Reproduction

```
timeout 30 cargo xtask qtest --case smp-libc-concurrency-kvm2 --arch x86_64 --no-build
```

Before S137 this could stay alive with a complete guest log while the case waited for
`timeout_secs` (120 s for this case, 1800 s for `posix-shell`). The preserved pre-fix evidence is
in `tickets/bob/reports/S137.md`; the guest-side `start` hang and the page-fault crash are
separate libc/kernel investigations (S136/S138), not harness behaviour.
