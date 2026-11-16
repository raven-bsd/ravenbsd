<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# P4d: POSIX system and process utilities

Status: PARTIAL. Two assigned rows are present with executable evidence (`util.id`,
`util.uname`); eight rows remain defects with the exact missing primitive or utility recorded
below. No row is waived and no defect is claimed present.

This is a host/QEMU test result. It is not a board result and it does not change the release
profile or the release gate.

## Method

`tests/qtest/cases/posix-util-system.toml` boots the `base` profile with a purpose-built initfs
from `tests/posix/utility/system/build.sh`. The fixture contains the core services (`logd`,
`sysd`, `randd`, `zerod`, `nulld`, `initfsd`, `ramfsd`, `vfsd`, `consd`), `rsh`, and only the
system tools under test. Keeping the image small avoids the `initfsd` readiness timeout seen with
the full `shell-test` image.

The shell profile installs the uutils `id` at `/bin/id`; the fixture stages that build and the
uutils `env` at `/bin/env`. The rsh builtin `env` shadows the external command, so the POSIX
`env` checks call `/bin/env` explicitly.

`tests/posix/utility/run-system.sh` runs the case and maps the transcript to the matrix in
`tests/posix/utility/system/cases.tsv`. A row with executable evidence is `PASS`; a row with a
defect id is `DEFECT`; a failed qtest is `FAIL`. The runner exits non-zero when the qtest fails.

## Tool matrix

| Row | Tool | Status | Evidence or defect |
|---|---|---|---|
| `util.id` | `id` | **present** | `qtest:posix-util-system` — uutils `id` at `/bin/id` |
| `util.uname` | `uname` | **present** | `qtest:posix-util-system` |
| `util.ps` | `ps` | **defect** | `P4d-D2`: no process-list source; `ps` reads `svc:` only |
| `util.env` | `env` | **defect** | `P4d-D3`: `/bin/env` prints and assigns, but command execution is `ENOSYS` (`execvp`/`execve`, PG-50) |
| `util.getconf` | `getconf` | **defect** | `P4d-D4`: no implementation, source, package or profile row |
| `util.tty` | `tty` | **defect** | `P4d-D5`: `ttyname` PG-21 and `isatty` PG-54 are partial |
| `util.stty` | `stty` | **defect** | `P4d-D6`: `tcgetattr`/`tcsetattr` PG-54 are partial; P3 is not merged |
| `util.kill` | `kill` | **defect** | `P4d-D7`: `kill(2)` PG-5 is TBD (external C58) |
| `util.nice` | `nice` | **defect** | `P4d-D8`: `getpriority`/`setpriority` PG-28 are TBD and return `ENOSYS` |
| `util.nohup` | `nohup` | **defect** | `P4d-D9`: `execvp`/`execve` PG-50 is partial; the exec path returns `ENOSYS` |

The case also exercises two first-party tools that are not POSIX profile rows: `hostname`
(print and `--selftest`) and `whoami` (name plus the exit-2 usage path). They are adjacent
coverage only; `docs/posix/profile.toml` has no `util.hostname` or `util.whoami` row, so no
overlay claim is made for them.

## Behaviour covered

The `posix-util-system` case verifies:

* `id`: default `uid=0(root) gid=0(root) groups=0(root)`, `-u`, `-un`, `-g`, `-gn`, `-G`,
  `-Gn`, `-ru`, `-run`, a named operand (`id root`), and the unknown-user exit status
  (`id nosuchuser` -> `id: 'nosuchuser': no such user`, status 1). The uutils `id` implements
  the POSIX `-r` selector that the first-party `userland/bin/id` lacked; this resolves the old
  `P4d-D1`.
* `ps`: the human `NAME STATE RESTARTS` view, one-line `--json` process records and
  `ps --selftest`. The JSON records deliberately set `pid`, `ppid`, `uid`, `gid`, `command`,
  `threads` and `rss_bytes` to `null`; this is service status, not a process table.
* `uname`: `-s`, `-n`, `-r`, `-v`, `-m`, `-a`, including the kernel version string and machine.
* `hostname`: live `sys:/hostname` value and the set/restore `--selftest` path.
* `/bin/env`: `-i` clears the environment (`/bin/env -i P4D=only` -> `P4D=only`), and an
  assignment without `-i` adds `P4D=outer` to the exported environment.
* `whoami`: effective user name and the `whoami extra` usage failure with status 2.
* Boot/shutdown: `svcd: all services up`, `rsh: ready`, `init exited (0)`, and no panic,
  exception or `command not found` line.

The missing `env` execute path is visible in the case transcript:

```
raven# /bin/env -i P4D=only /bin/whoami
env: '/bin/whoami': Function not implemented
process: child exited (126)
```

That witness is why `util.env` stays a defect instead of being claimed present for the
print/assignment subset.

## Defects

| Defect | Row | Exact missing primitive or utility | Follow-up |
|---|---|---|---|
| `P4d-D2` | `util.ps` | no userland process-list source; `userland/bin/ps` formats `svc:` service status and its JSON process fields are `null`. `procd` is on `stuart/P2e` `15d47e18` and P3 is on `stuart/P3`; neither is in `origin/master` `f10bbbd7` | merge P2e/P3, expose the process registry, then test POSIX `ps` fields |
| `P4d-D3` | `util.env` | `execvp`/`execve` PG-50 (partial); the target exec path returns `ENOSYS` (38), as the QEMU witness shows | wire the exec/spawn path and test command execution |
| `P4d-D4` | `util.getconf` | no `getconf` implementation: no source, package, profile row or staged binary | add the utility; test the required system variables |
| `P4d-D5` | `util.tty` | `ttyname` PG-21 (partial) and `isatty` PG-54 (partial); uutils `tty` uses `rustix::termios::ttyname` | implement the OFD-path/ttyname primitive, then stage `tty` |
| `P4d-D6` | `util.stty` | `tcgetattr`/`tcsetattr` PG-54 (partial, tty `OP_GET_MODE`/`OP_SET_MODE`); P3 (ptyd/ttyd) is not merged | merge P3, complete termios, then stage `stty` |
| `P4d-D7` | `util.kill` | `kill(2)` PG-5 (TBD, external C58) | implement the signal contract, then stage `kill` and test signal names/delivery |
| `P4d-D8` | `util.nice` | `getpriority`/`setpriority` PG-28 (TBD; stubs return `ENOSYS` 38) | implement the procd priority primitive, then stage `nice` |
| `P4d-D9` | `util.nohup` | `execvp`/`execve` PG-50 (partial); `nohup` opens `nohup.out`, ignores `SIGHUP` and then execs, so the exec failure blocks it | wire the exec path, then stage `nohup` and test output/exit status |

`P4d-D1` (the old `id -r` defect) is resolved by staging the shell-profile uutils `id`; it is
not a defect row anymore.

### Build follow-up

`cargo xtask build uutils --arch x86_64` currently exits 101 in the vendored
`rustix-ravenbsd` crate (38 errors: unresolved `crate::fs::Mode`, `crate::process::WaitOptions`,
`crate::termios::InputModes` and related items; several build scripts also fail to find
`cfg_aliases`/`quote`). The fixture therefore uses the existing K25a staging output
`build/x86_64/uutils/id` and `build/x86_64/uutils/env`; `build.sh` fails with a clear message if
those artifacts are absent. This is a build-infrastructure follow-up outside P4d Owns.

## Evidence overlay

`docs/posix/evidence/P4-system.toml` registers the two rows with executable evidence:

```toml
[[row]]
id = "util.id"
status = "present"
evidence = "qtest:posix-util-system"

[[row]]
id = "util.uname"
status = "present"
evidence = "qtest:posix-util-system"
```

The gate executes the case for each row and applies the claims. It does not execute any claim for
the eight defect rows.

## Acceptance

### `sh tests/posix/utility/run-system.sh`

Exit code: **0**

```
posix-util-system: qtest PASS (log build/posix/utility/system/qtest.log)
row	tool	status	command	detail
util.id	id	PASS	id	qtest
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

The QEMU case passed in 222155 ms with `exit_status: 33` and no unmatched expectation.

### `cargo xtask posix-check --profile posix --arch x86_64`

Exit code: **1** (the global gate is still release-blocked outside P4d)

```
posix-check: 814 pass, 949 fail, 10 skip; FAIL (release blocked)
```

The P4d rows in the generated report:

```
| `util.id` | utility | pass | `qtest:posix-util-system` |  |
| `util.uname` | utility | pass | `qtest:posix-util-system` |  |
```

The eight defect rows stay fail in the gate, as expected; they are not present claims.
`docs/posix/CONFORMANCE.md` was regenerated for inspection and restored; it is outside P4d
Owns.

### `make check-fast`

Exit code: **0** — **check-fast: PASSED**

```
==> guard
==> fmt
==> clippy (changed)
==> tests (changed, parallel)
    Starting 172 tests across 5 binaries
    Summary [   5.844s] 172 tests run: 172 passed, 0 skipped
==> profiles
check-fast summary:
  ok       guard (exit 0, 14.50s)
  ok       fmt (exit 0, 12.28s)
  ok       clippy (changed) (exit 0, 0.44s)
  ok       tests (changed, parallel) (exit 0, 7.17s)
  ok       profiles (exit 0, 0.10s)
check-fast: PASSED
```

## Limitations

* The target is QEMU x86_64 only. No board log was produced and no release claim is made.
* The image is a test fixture. It does not change any release or board profile.
* Eight rows are defects; the ticket's profile-level "0 failing rows" target is not met. The
  runner reports zero `FAIL` rows because missing or partial tools are recorded as `DEFECT`
  with an exact defect id, not as passes.
* `util.hostname` and `util.whoami` are verified as adjacent tools but are not POSIX profile
  rows and are not claimed in the overlay.
* The directive says P2e/P3 are merged into master. The fetched `origin/master` `f10bbbd7`
  does not contain `userland/servers/procd` or `userland/servers/ptyd`; only `stuart/P2e` and
  `stuart/P3` contain them. That is why `ps`, `tty`, `stty` and the exec-dependent rows remain
  defects.
