<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# P4d: POSIX system and process utilities

Status: PARTIAL. Four assigned rows are present with executable evidence (`util.id`,
`util.uname`, `util.tty`, `util.stty`); six rows remain defects with the exact missing
primitive or utility recorded below. No row is waived and no defect is claimed present.

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

The rev 34 merge is `origin/master` `f44ebb9b`: P2e/procd and P3/ttyd+ptyd were merged at
`6957cb8b`, and P2l priority at `e3b394c5`. P3 supplies the target tty/termios primitives; the
`tty` and `stty` rows use host behaviour tests of the same vendored uutils source on a pty.
P2l's kernel/runtime priority path exists, but the libc `getpriority`/`setpriority` wrapper is
still the `ENOSYS` stub. P2k's kernel/runtime exec path exists, but the libc `execvp` wrapper
is still `ENOSYS` (the P9c blocker). `ps` still reads `svc:` only, and `getconf` has no
implementation.

`tests/posix/utility/run-system.sh` runs the case and maps the transcript to the matrix in
`tests/posix/utility/system/cases.tsv`. A row with executable evidence is `PASS`; a row with a
defect id is `DEFECT`; a failed qtest is `FAIL`. The runner exits non-zero when the qtest fails.

## Tool matrix

| Row | Tool | Status | Evidence or defect |
|---|---|---|---|
| `util.id` | `id` | **present** | `qtest:posix-util-system` — uutils `id` at `/bin/id` |
| `util.uname` | `uname` | **present** | `qtest:posix-util-system` |
| `util.ps` | `ps` | **defect** | `P4d-D2`: no process-list source; `ps` reads `svc:` only |
| `util.env` | `env` | **defect** | `P4d-D3`: `/bin/env` prints and assigns; libc `execvp` wrapper still returns `ENOSYS` (P9c, PG-50) |
| `util.getconf` | `getconf` | **defect** | `P4d-D4`: no implementation, source, package or profile row |
| `util.tty` | `tty` | **present** | `test:tests/posix/utility/system/tty/test.sh` — host uutils on a pty; P3 supplies target tty primitives |
| `util.stty` | `stty` | **present** | `test:tests/posix/utility/system/stty/test.sh` — host uutils on a pty; P3 supplies target termios primitives |
| `util.kill` | `kill` | **defect** | `P4d-D7`: `kill(2)` PG-5 is TBD (external C58) |
| `util.nice` | `nice` | **defect** | `P4d-D8`: P2l kernel priority exists, but libc `getpriority`/`setpriority` PG-28 are still `ENOSYS` |
| `util.nohup` | `nohup` | **defect** | `P4d-D9`: libc `execvp`/`execve` still `ENOSYS` (P9c, PG-50); no target `nohup` staged |

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
* `tty` (host): a pty name (`/dev/pts/N`), `-s` success on a pty, and the `not a tty`/exit-1
  path on a pipe.
* `stty` (host): `-a` reports `speed`/`rows`, `size` is two numbers, `sane` succeeds on a pty,
  and a pipe fails with `Inappropriate ioctl`.
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
| `P4d-D2` | `util.ps` | P2e/procd is merged, but `userland/bin/ps` still formats `svc:` service status and its JSON process fields are `null`; no code consumes the process registry | add a `ps` process-table path over procd and test POSIX fields |
| `P4d-D3` | `util.env` | P2k kernel `PROC_EXEC`/`raven_rt::exec` exists, but the libc symbol `execvp` (and the underlying `execve` wrapper) still returns `ENOSYS` (38); PG-50 remains partial. This is the P9c/libc blocker | P9c: wire the libc `execvp`/`execve` wrapper to the P2k runtime, then test command execution |
| `P4d-D4` | `util.getconf` | no `getconf` implementation: no source, package, profile row or staged binary; the libc `sysconf(_SC_ARG_MAX)` already returns 131072 | add the utility; test `getconf ARG_MAX` |
| `P4d-D7` | `util.kill` | `kill(2)` PG-5 (TBD, external C58); no target `kill` binary is staged | implement the signal contract, stage `kill`, then test signal names/delivery |
| `P4d-D8` | `util.nice` | P2l adds the kernel/runtime priority calls (`raven_rt::priority`), but the libc `getpriority`/`setpriority` wrapper (PG-28) is still the `ENOSYS` stub | wire the libc priority wrappers, stage `nice`, then test the niceness change |
| `P4d-D9` | `util.nohup` | the libc `execvp`/`execve` wrapper still returns `ENOSYS` (P9c, PG-50), so the final exec fails; no target `nohup` binary is staged | P9c: wire the libc exec wrapper; stage `nohup`; test output/exit status |

`P4d-D1` (`id -r`) is resolved by the shell-profile uutils `id`. `P4d-D5` (`tty`) and
`P4d-D6` (`stty`) are resolved in this round: P3 supplies the ttyd/ptyd primitives, and the
host behaviour tests above exercise the same uutils source on a pty.

### Build follow-up

`cargo xtask build uutils --arch x86_64` currently exits 101 in the vendored
`rustix-ravenbsd` crate (25 errors: missing `backend::io::types::DupFlags`/`FdFlags`,
`backend::pipe::types::PipeFlags`, `crate::fs::Mode` and related items). The fixture therefore
uses the existing K25a staging output `build/x86_64/uutils/id` and
`build/x86_64/uutils/env`; `build.sh` fails with a clear message if those artifacts are absent.
The `tty`/`stty` host tests build their own host binaries. This is a build-infrastructure
follow-up outside P4d Owns.

## Evidence overlay

`docs/posix/evidence/P4-system.toml` registers the four present rows with executable evidence:

```toml
[[row]]
id = "util.id"
status = "present"
evidence = "qtest:posix-util-system"

[[row]]
id = "util.uname"
status = "present"
evidence = "qtest:posix-util-system"

[[row]]
id = "util.tty"
status = "present"
evidence = "test:tests/posix/utility/system/tty/test.sh"

[[row]]
id = "util.stty"
status = "present"
evidence = "test:tests/posix/utility/system/stty/test.sh"
```

The gate executes the qtest for `util.id`/`util.uname` and the host test scripts for
`util.tty`/`util.stty`. It does not execute any claim for the six defect rows.

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
util.tty	tty	PASS	tty	host test
util.stty	stty	PASS	stty -a	host test
util.kill	kill	DEFECT	kill -l	P4d-D7
util.nice	nice	DEFECT	nice true	P4d-D8
util.nohup	nohup	DEFECT	nohup true	P4d-D9
```

The QEMU case passed in 127829 ms with `exit_status: 33` and no unmatched expectation. The
`tty`/`stty` host cases passed on a pty; the host build is the same vendored uutils source.

### `cargo xtask posix-check --profile posix --arch x86_64`

Exit code: **1** (the global gate is still release-blocked outside P4d)

```
posix-check: 878 pass, 920 fail, 10 skip; FAIL (release blocked)
```

The four P4d present rows in the generated report:

```
| `util.id` | utility | pass | `qtest:posix-util-system` |  |
| `util.uname` | utility | pass | `qtest:posix-util-system` |  |
| `util.tty` | utility | pass | `test:tests/posix/utility/system/tty/test.sh` |  |
| `util.stty` | utility | pass | `test:tests/posix/utility/system/stty/test.sh` |  |
```

The six defect rows stay fail in the gate, as expected; they are not present claims.
`docs/posix/CONFORMANCE.md` was regenerated for inspection and restored; it is outside P4d
Owns.

### `make check-fast`

Exit code: **0** — **check-fast: PASSED**

```
==> guard
==> fmt
==> clippy (changed)
==> tests (changed, parallel)
    Starting 173 tests across 5 binaries
    Summary [   3.103s] 173 tests run: 173 passed, 0 skipped
==> profiles
check-fast summary:
  ok       guard (exit 0, 8.33s)
  ok       fmt (exit 0, 6.37s)
  ok       clippy (changed) (exit 0, 0.27s)
  ok       tests (changed, parallel) (exit 0, 3.50s)
  ok       profiles (exit 0, 0.05s)
check-fast: PASSED
```

## Limitations

* The target is QEMU x86_64 only. No board log was produced and no release claim is made.
* The image is a test fixture. It does not change any release or board profile.
* Six rows are defects; the ticket's profile-level "0 failing rows" target is not met. The
  runner reports zero `FAIL` rows because the missing or partially wired tools are recorded as
  `DEFECT` with an exact defect id, not as passes.
* `util.hostname` and `util.whoami` are verified as adjacent tools but are not POSIX profile
  rows and are not claimed in the overlay.
* P2e/procd and P3/ttyd+ptyd are merged in `f44ebb9b`, which is why `tty`/`stty` close against
  the P3 tty primitives. P2l's kernel/runtime priority and P2k's kernel/runtime exec exist, but
  the libc `getpriority`/`setpriority` and `execvp`/`execve` wrappers are still `ENOSYS`.
  `ps` still reads `svc:` rather than procd, and `getconf`/`kill` have no implementation or
  staged binary. Those are the six remaining defects.
