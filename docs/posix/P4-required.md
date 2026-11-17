<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# P4 required utility acceptance

Status: OBSERVED (P4a10, base `origin/master` `1aa9e959`). P4a7 leaves exactly seven required
utilities. This note is the single acceptance surface for them; P4a10 adds the booted-image target
gate. No utility, profile or evidence-toml code is changed here.

## Host runner

```sh
sh tests/posix/utility/required/runner.sh
sh tests/posix/utility/required/test.sh
```

The runner executes each tool's own per-tool host test and prints one line per row:

* `[PASS] <tool>: <test>` - the test exists and exited 0.
* `[FAIL] <tool>: exit <n>` - the test exists and failed; the log is indented under the line.
* `[DEFECT] <tool>: missing ...` - the tool source or its per-tool test is absent.

Observed on `1aa9e959`: `required-runner: 7 pass, 0 fail, 0 defect`, exit 0.

## Target gate

```sh
cargo xtask qtest --case required-utils --arch x86_64
```

The fixture boots a minimal initfs with the seven tools, `initfsd`/`ramfsd`/`vfsd` (for
`newgrp`'s account files), a `/bin/true` child for `time`, a `/bin/sh` credential probe for
`newgrp`, and `/etc/passwd`, `/etc/group`, `/etc/shadow`, `/etc/locale` and `/etc/charmap`. Each
case is bounded and non-interactive. The fixture exits 1 because `localedef`'s target build is
still a host-only stub, so the qtest declares `expect_exit = "failure"`; the other six rows pass.

Observed summary on `1aa9e959`: `required-utils: 9 cases, 8 pass, 1 fail` (the nine cases are
`time`, `times`, `tput`, three `ulimit` cases, `lp`, `localedef`, `newgrp`).

## Seven required rows

| Utility | Host evidence | Target evidence | Notes |
|---|---|---|---|
| `localedef` | `sh tests/posix/utility/simple3/localedef/test.sh` - pass | **FAIL** - `localedef: target build is host-only` | otto/P4a4; no target evidence registered until the target build compiles the catalog instead of returning the stub |
| `lp` | `sh tests/posix/utility/simple5/lp/test.sh` - pass | `qtest:required-utils` - pass | tim/P4a6; no-destination diagnostic and exit 1 asserted, no printer/network |
| `newgrp` | `sh tests/posix/utility/simple4/newgrp/test.sh` - pass | `qtest:required-utils` - pass | mel/P4a5; `/bin/sh` probe reports `uid=0 gid=100` after the group switch |
| `time` | `sh tests/posix/utility/simple2/time/test.sh` - pass | `qtest:required-utils` - pass | bob/P4a3; `real`/`user`/`sys` output, `real` is the observed monotonic time |
| `times` | `sh tests/posix/utility/simple2/times/test.sh` - pass | `qtest:required-utils` - pass | bob/P4a3; two POSIX `XmY.ZZs` lines from the kernel process counters |
| `tput` | `sh tests/posix/utility/simple2/tput/test.sh` - pass | `qtest:required-utils` - pass | bob/P4a3; `tput cols` prints `80` |
| `ulimit` | `sh tests/posix/utility/simple2/ulimit/test.sh` - pass | `qtest:required-utils` - pass | bob/P4a3; `-n` query prints the default and `-n 128` exits 0 (the limit is per-process, so the next case is a fresh process) |

Six target rows have evidence; `localedef` stays a target FAIL and is not registered as passing.
The `qtest:required-utils` case covers all six in one booted image; the failing row is visible in
the same transcript rather than hidden by a green qtest exit.

## Target transcript excerpt

```
required-utils: services ok
required-utils: time start
real 0.04
user 0.00
sys 0.00
required-utils: time exit=0 PASS
required-utils: times start
0m0.00s 0m0.00s
0m0.00s 0m0.00s
required-utils: times exit=0 PASS
required-utils: tput start
80
required-utils: tput exit=0 PASS
required-utils: ulimit-default start
256
required-utils: ulimit-default exit=0 PASS
required-utils: ulimit-set start
required-utils: ulimit-set exit=0 PASS
required-utils: ulimit-get start
256
required-utils: ulimit-get exit=0 PASS
required-utils: lp start
lp: no print destination configured
required-utils: lp exit=1 PASS
required-utils: localedef start
localedef: target build is host-only
required-utils: localedef exit=1 FAIL
required-utils: newgrp start
required-utils: shell uid=0 gid=100
required-utils: newgrp exit=0 PASS
required-utils: 9 cases, 8 pass, 1 fail
```

The `tput` capability has no trailing newline in the serial log; the qtest matches the `80` prefix.
The `ulimit` set case is accepted per process; a later `ulimit -n` in a new process correctly shows
the inherited default, not the previous process's setting.

## Self-test

`tests/posix/utility/required/test.sh` exercises the host runner with a controlled temporary tree:
`time` passes, `times` fails and the remaining five are missing. It asserts the runner reports
`1 pass, 1 fail, 5 defect`, exits non-zero, and never passes a missing row. It also checks the real
tree has exactly seven row lines and an exit status consistent with its summary.

P4a10 changes no utility, profile, seed or generated POSIX document.
