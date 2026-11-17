<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# P4 required utility acceptance

Status: OBSERVED (P4a11 x86_64, P4a12 aarch64; base `origin/master` `445f78fb`). P4a7 leaves
exactly seven required utilities. This note is the single acceptance surface for them; P4a10 added
the booted-image target gate, P4a11 fixed the localedef target main and P4a12 extended the same
fixture/case to aarch64. No profile, seed or evidence-toml code is changed here.

## Host runner

```sh
sh tests/posix/utility/required/runner.sh
sh tests/posix/utility/required/test.sh
```

The runner executes each tool's own per-tool host test and prints one line per row:

* `[PASS] <tool>: <test>` - the test exists and exited 0.
* `[FAIL] <tool>: exit <n>` - the test exists and failed; the log is indented under the line.
* `[DEFECT] <tool>: missing ...` - the tool source or its per-tool test is absent.

Observed on `1939ae69`: `required-runner: 7 pass, 0 fail, 0 defect`, exit 0.

## Target gate

```sh
cargo xtask qtest --case required-utils --arch x86_64
```

The fixture boots a minimal initfs with the seven tools, `initfsd`/`ramfsd`/`vfsd` (for
`newgrp`'s account files and localedef's `file:` input/output), a `/bin/true` child for `time`, a
`/bin/sh` credential probe for `newgrp`, and `/etc/passwd`, `/etc/group`, `/etc/shadow`,
`/etc/locale` and `/etc/charmap`. Each case is bounded and non-interactive. All seven utilities
pass, so the qtest declares `expect_exit = "success"`.

Observed summary on `1939ae69`: `required-utils: 10 cases, 10 pass, 0 fail` (the ten cases are
`time`, `times`, `tput`, three `ulimit` cases, `lp`, `localedef`, the localedef catalog check and
`newgrp`).

## Seven required rows

| Utility | Host evidence | Target evidence | Notes |
|---|---|---|---|
| `localedef` | `sh tests/posix/utility/simple3/localedef/test.sh` - pass | `qtest:required-utils` - pass | otto/P4a4 + P4a11; target reads `file:` inputs and writes the `ff88ff89` catalog (211 bytes in the fixture); target stub removed |
| `lp` | `sh tests/posix/utility/simple5/lp/test.sh` - pass | `qtest:required-utils` - pass | tim/P4a6; no-destination diagnostic and exit 1 asserted, no printer/network |
| `newgrp` | `sh tests/posix/utility/simple4/newgrp/test.sh` - pass | `qtest:required-utils` - pass | mel/P4a5; `/bin/sh` probe reports `uid=0 gid=100` after the group switch |
| `time` | `sh tests/posix/utility/simple2/time/test.sh` - pass | `qtest:required-utils` - pass | bob/P4a3; `real`/`user`/`sys` output, `real` is the observed monotonic time |
| `times` | `sh tests/posix/utility/simple2/times/test.sh` - pass | `qtest:required-utils` - pass | bob/P4a3; two POSIX `XmY.ZZs` lines from the kernel process counters |
| `tput` | `sh tests/posix/utility/simple2/tput/test.sh` - pass | `qtest:required-utils` - pass | bob/P4a3; `tput cols` prints `80` |
| `ulimit` | `sh tests/posix/utility/simple2/ulimit/test.sh` - pass | `qtest:required-utils` - pass | bob/P4a3; `-n` query prints the default and `-n 128` exits 0 (the limit is per-process, so the next case is a fresh process) |

All seven target rows have evidence. The `qtest:required-utils` case covers them in one booted
image, including the localedef catalog-magic check.

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
required-utils: localedef exit=0 PASS
required-utils: localedef catalog magic=ff88ff89 bytes=211 PASS
required-utils: newgrp start
required-utils: shell uid=0 gid=100
required-utils: newgrp exit=0 PASS
required-utils: 10 cases, 10 pass, 0 fail
```

The `tput` capability has no trailing newline in the serial log; the qtest matches the `80` prefix.
The `ulimit` set case is accepted per process; a later `ulimit -n` in a new process correctly shows
the inherited default, not the previous process's setting. `localedef` reads `/etc/locale` and
`/etc/charmap` through `file:` and writes `/tmp/catalog`; the fixture checks the catalog starts
with `ff88ff89` before it counts the row as PASS.

## aarch64 target gate (P4a12)

```sh
cargo xtask qtest --case required-utils --arch aarch64
```

The case is shared. The qtest `--arch aarch64` override passes `RAVEN_QT_ARCH=aarch64` to the
fixture pre-build, which cross-builds the seven tools, the minimal `required-utils` init/shell/true
and `initfsd`/`ramfsd`/`vfsd` for `aarch64-unknown-none-softfloat` with
`userland/linker/aarch64.ld`. The same bounded expectations then run against the booted aarch64
image.

Observed on `445f78fb`: `required-utils: pass` (54.5 s), with all ten cases passing:

| Utility | aarch64 target | Notes |
|---|---|---|
| `time` | pass | `real`/`user`/`sys` output |
| `times` | pass | two `XmY.ZZs` lines |
| `tput` | pass | prints `80` |
| `ulimit` | pass | default, set and get cases |
| `lp` | pass | no-destination diagnostic, exit 1 |
| `localedef` | pass | catalog magic `ff88ff89`, 211 bytes |
| `newgrp` | pass | shell reports `uid=0 gid=100` |

AArch64 serial excerpt:

```
required-utils: services ok
...
required-utils: localedef start
required-utils: localedef exit=0 PASS
required-utils: localedef catalog magic=ff88ff89 bytes=211 PASS
required-utils: newgrp start
required-utils: shell uid=0 gid=100
required-utils: newgrp exit=0 PASS
required-utils: 10 cases, 10 pass, 0 fail
init exited (0)
```

No utility code was changed; the fixture build script selects the target from `RAVEN_QT_ARCH`.

## Self-test

`tests/posix/utility/required/test.sh` exercises the host runner with a controlled temporary tree:
`time` passes, `times` fails and the remaining five are missing. It asserts the runner reports
`1 pass, 1 fail, 5 defect`, exits non-zero, and never passes a missing row. It also checks the real
tree has exactly seven row lines and an exit status consistent with its summary.

P4a10 added the target gate; P4a11 fixed the localedef target main and the fixture catalog check.
No profile, seed or generated POSIX document is changed.
