<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# P4d: POSIX system and process utilities

Status: PARTIAL. The QEMU behaviour runner is in place and `util.uname` is present with
executable evidence. Nine assigned rows remain defects because the tool is missing or its
current behaviour is not POSIX-conforming; no row is waived and no defect is claimed present.

This is a host/QEMU test result. It is not a board result and it does not change the release
profile or the release gate.

## Method

`tests/qtest/cases/posix-util-system.toml` boots the `base` profile with a purpose-built initfs
from `tests/posix/utility/system/build.sh`. The fixture contains the core services (`logd`,
`sysd`, `randd`, `zerod`, `nulld`, `initfsd`, `ramfsd`, `vfsd`, `consd`), `rsh`, and only the
system tools under test. Keeping the image small avoids the `initfsd` readiness timeout seen with
the full `shell-test` image.

`tests/posix/utility/run-system.sh` runs that case and maps the transcript to the matrix in
`tests/posix/utility/system/cases.tsv`. A row with executable evidence is `PASS`; a row with a
defect id is `DEFECT`; a failed qtest is `FAIL`. The runner exits non-zero when the qtest fails.

## Tool matrix

| Row | Tool | Status | Evidence or defect |
|---|---|---|---|
| `util.id` | `id` | **defect** | `P4d-D1`: core uid/gid output works, `-r` is not implemented |
| `util.ps` | `ps` | **defect** | `P4d-D2`: lists `svc:` services, not POSIX process fields |
| `util.uname` | `uname` | **present** | `qtest:posix-util-system` |
| `util.env` | `env` | **defect** | `P4d-D3`: rsh builtin prints variables only; no `-i`, assignments or command |
| `util.getconf` | `getconf` | **defect** | `P4d-D4`: no implementation |
| `util.tty` | `tty` | **defect** | `P4d-D5`: uutils source exists but is not in the staged slice |
| `util.stty` | `stty` | **defect** | `P4d-D6`: uutils source exists but is not staged; depends on P3 tty/termios |
| `util.kill` | `kill` | **defect** | `P4d-D7`: uutils source exists but is not staged; depends on P2e signals |
| `util.nice` | `nice` | **defect** | `P4d-D8`: uutils source exists but is not staged; depends on P2e scheduling |
| `util.nohup` | `nohup` | **defect** | `P4d-D9`: uutils source exists but is not staged; depends on P2e signals |

The qtest also exercises two first-party tools that are not POSIX profile rows: `hostname`
(print and `--selftest`) and `whoami` (name plus the exit-2 usage path). They are adjacent
coverage only; `docs/posix/profile.toml` has no `util.hostname` or `util.whoami` row, so no
overlay claim is made for them.

## Behaviour covered

The `posix-util-system` case verifies:

* `id`: default `uid=0(root) gid=0(root) groups=0(root)`, `-u`, `-un`, `-g`, `-gn`, `-G`,
  `-Gn`, a named operand (`id root`), and the unknown-user exit status (`id nosuchuser` -> 1).
* `ps`: the human `NAME STATE RESTARTS` view, one-line `--json` process records, and
  `ps --selftest`.
* `uname`: `-s`, `-n`, `-r`, `-v`, `-m`, `-a`, including the kernel version string and machine.
* `hostname`: live `sys:/hostname` value and the set/restore `--selftest` path.
* `env`: the exported `PATH`, `HOME`, `USER` and `SHELL` values, plus an `export P4D=ok`
  round-trip through the shell builtin.
* `whoami`: the effective user name and the `whoami extra` usage failure with status 2.
* Boot and shutdown: `svcd: all services up`, `rsh: ready`, `init exited (0)`, and no panic,
  exception or `command not found` line.

## Defects

| Defect | Row | Detail | Follow-up |
|---|---|---|---|
| `P4d-D1` | `util.id` | `userland/bin/id` parses `-u -g -G -n` but not the required `-r`; `id -r` returns usage and status 2 | extend `id` argument parsing and add the real-credential test |
| `P4d-D2` | `util.ps` | `userland/bin/ps` formats `svc:` service status; the JSON records deliberately set `pid`, `ppid`, `uid`, `gid`, `command`, `threads` and `rss_bytes` to `null`. It is not a POSIX process table | feed process data from procd/P2e or a kernel process source, then test the POSIX fields |
| `P4d-D3` | `util.env` | the only `env` is the rsh builtin; it ignores its arguments and only prints exported variables. No `/bin/env` is staged | stage a POSIX `env` implementation and test `-i`, `NAME=VALUE` and command execution |
| `P4d-D4` | `util.getconf` | no `getconf` source, package or staged binary exists | add the utility and test the required system variables |
| `P4d-D5` | `util.tty` | `userland/uutils/src/uu/tty` exists but is not in `xtask/src/uutils.rs` `SELECTED`, so no `/bin/tty` is staged | add it to the build slice and stage `/bin/tty` |
| `P4d-D6` | `util.stty` | `userland/uutils/src/uu/stty` exists but is not staged; the tty/termios work is P3 | stage `stty` after P3 and test `/dev/tty` behaviour |
| `P4d-D7` | `util.kill` | `userland/uutils/src/uu/kill` exists but is not staged; the process-signal path is P2e | stage `kill` after P2e and test signal names and exit status |
| `P4d-D8` | `util.nice` | `userland/uutils/src/uu/nice` exists but is not staged; scheduling is P2e | stage `nice` after P2e and test the niceness change |
| `P4d-D9` | `util.nohup` | `userland/uutils/src/uu/nohup` exists but is not staged; `SIGHUP` handling is P2e | stage `nohup` after P2e and test output/exit status |

`stuart/P2e` (`procd`, commit `15d47e18`) is DONE but is not merged into the P4d base
`origin/master` `14908851`. That is the dependency behind `P4d-D7`..`P4d-D9` and the process
source needed by `P4d-D2`.

## Evidence overlay

`docs/posix/evidence/P4-system.toml` registers the one row with executable evidence:

```toml
[[row]]
id = "util.uname"
status = "present"
evidence = "qtest:posix-util-system"
notes = "P4d: QEMU x86_64 case checks uname -s/-n/-r/-v/-m/-a against the kernel uname"
```

The gate executes the case and applies the row. It does not execute any claim for the nine
defect rows.

## Acceptance

### `sh tests/posix/utility/run-system.sh`

Exit code: **0**

```
posix-util-system: qtest PASS (log build/posix/utility/system/qtest.log)
row	tool	status	command	detail
util.id	id	DEFECT	id	P4d-D1
util.ps	ps	DEFECT	ps	P4d-D2
util.uname	uname	PASS	uname -a	qtest
util.env	env	DEFECT	env	P4d-D3
util.getconf	getconf	DEFECT	getconf ARG_MAX	P4d-D4
util.tty	tty	DEFECT	tty	P4d-D5
util.stty	stty	DEFECT	stty -a	P4d-D6
util.kill	kill	DEFECT	kill -l	P4d-D7
util.nice	nice	DEFECT	nice true	P4d-D8
util.nohup	nohup	DEFECT	nohup true	P4d-D9
```

The QEMU case itself passed in 150802 ms with `exit_status: 33` and no unmatched expectation.

### `cargo xtask posix-check --profile posix --arch x86_64`

Exit code: **1** (the global gate is still release-blocked outside P4d)

```
posix-check: 737 pass, 1022 fail, 10 skip; FAIL (release blocked)
```

The only P4d row that moved is `util.uname`: the utility area went from 0 pass / 177 fail to
1 pass / 176 fail, and the total went from 736 pass / 1023 fail to 737 pass / 1022 fail. The
remaining nine system rows stay defects. `docs/posix/CONFORMANCE.md` was regenerated for
inspection and restored; it is outside P4d Owns.

### `make check-fast`

Exit code: **0**

```
==> guard
==> fmt
==> clippy (host)
==> tests (host, parallel)
==> tests (doc)
==> profiles
==> provenance
==> docs
check-fast summary:
  ok       guard (exit 0, 6.33s)
  ok       fmt (exit 0, 4.74s)
  ok       clippy (host) (exit 0, 10.60s)
  ok       tests (host, parallel) (exit 0, 46.26s)
  ok       tests (doc) (exit 0, 3.24s)
  ok       profiles (exit 0, 0.06s)
  ok       provenance (exit 0, 21.44s)
  skipped  clippy kernel x86_64 (--fast)
  skipped  clippy kernel aarch64 (--fast)
  skipped  clippy user programs x86_64 (--fast)
  skipped  clippy loader x86_64 (--fast)
  ok       docs (exit 0, 0.06s)
check-fast: PASSED
```

## Limitations

* The target is QEMU x86_64 only. No board log was produced and no release claim is made.
* The image is a test fixture. It does not change any release or board profile.
* Nine rows are defects; the ticket's "0 failing rows" target is not met at profile level. The
  runner reports zero `FAIL` rows because missing or partial tools are recorded as `DEFECT`
  with a defect id, not as passes.
* `util.hostname` and `util.whoami` are verified as adjacent tools but are not POSIX profile
  rows and are not claimed in the overlay.
