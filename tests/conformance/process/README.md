<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# S181 process and exec conformance

This table is the application contract for process and exec: exit status decoding, wait and
`WNOHANG`/`ECHILD`, zombie/reparent, signal handler reset on exec, descriptor inheritance and
`O_CLOEXEC`, exact argv/envp/auxv, and a 16-byte aligned entry stack with `AT_RANDOM`.

Run it from the repository root, under the host build slot:

```sh
/home/bo/work/ravenos-workspace/ravenbsd-prjmgmt/scripts/build-slot.sh s181-process -- \
    timeout 1800 python3 tests/conformance/process/run.py --build-image
```

`--build-image` builds the release image only when `build/qtest/ssh-login.img` is absent. The
release image is required for the SSH exit-status row; the host and target rows run regardless.
Each row is reported as PASS, FAIL with its raw exit/hex evidence, or BLOCKED with the exact missing
test. A red row is not skipped: it becomes its own fix ticket (`S168` start layout, `S169` wait,
`S172` descriptor/O_CLOEXEC).

Rows:

* `start-layout-argv-auxv`: host `raven-spawn` `build_stack` tests (argc/argv/NULL/envp/NULL/auxv/
  `(0,0)`, 16-byte `sp0`, `AT_RANDOM`).
* `argv-env-auxv`: target `libc-hello` (exact argv/environ and `getauxval(AT_RANDOM)`).
* `exit-status-wait-echild`: target `libc-spawn` (exit codes, `wait`, second wait `ECHILD`).
* `signal-handler`: target `libc-signals`.
* `spawn-attrs`, `spawn-cmd`, `spawn-std`: target spawn attribute/command/std tests.
* `terminate-wait`: `process-lifecycle` terminate/wait rows.
* `exit-*`, `sigkill-external`, `argv-env-exact`, `fd-inherit`, `handler-reset`: the release-image
  SSH rows. `exit-134` is the status the SSH gate quotes for the failing child.
* `wait-nohang`, `fd-cloexec`, `zombie-reparent`: BLOCKED until S169's wait unit test and S172's
  descriptor harness land; the release shell does not expose `waitpid(WNOHANG)`, `F_SETFD`, or a
  stable zombie census.

No product code lives here; this is test-only.
