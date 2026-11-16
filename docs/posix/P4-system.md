<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# P4d/P4o: POSIX system and process utilities

Status: PARTIAL. Eight rows are present with executable evidence; two rows remain defects:

* `util.ps` — the process scan exposes pid/ppid/uid/gid, but there is no command-name source
  for a pid, so `cmd` is `?`. `P4d-D2`.
* `util.nohup` — the first-party utility creates `nohup.out`, execs the command and propagates
  its exit status, but the runtime prints through `debug_write` and has no stdout descriptor or
  `isatty`/`dup2` path, so the child's output is not redirected into the file. `P4d-D9`.

The ticket's 10/10 target is not reached. No row is waived and no missing behavior is claimed
present. `make check-fast` passes every step except the docs gate, which requires man pages
under `docs/man1/`; that path is outside this ticket's Owns-globs (see “Docs gate”).

This is a host/QEMU test result. It is not a board result and it does not change the release
profile or the release gate.

## Method

`tests/qtest/cases/posix-util-system.toml` boots the `base` profile with a purpose-built initfs
from `tests/posix/utility/system/build.sh`. The fixture contains the core services, `rsh`, and
the first-party system utilities `env`, `getconf`, `kill`, `nice`, `nohup` and `ps`, plus the
uutils `id` at `/bin/id` and the first-party `uname`, `hostname`, `whoami`.

The fixture builds user programs with `--release`: the kernel's `PROC_EXEC` path caps the image
at 1 MiB, and debug builds of `whoami`/`nice` exceed it. Release builds are ~40–60 KiB and let
`env`/`nice`/`nohup` exec commands through the P2k path.

`tests/posix/utility/run-system.sh` runs the case and maps the transcript to
`tests/posix/utility/system/cases.tsv`; rows with a per-tool host test (`tty`, `stty`) run that
test instead. A row with executable evidence is `PASS`; a row with a defect id is `DEFECT`.

## Tool matrix

| Row | Tool | Status | Evidence |
|---|---|---|---|
| `util.id` | `id` | **present** | `qtest:posix-util-system` — uutils `id`; uid/gid/groups, `-n`, `-r` |
| `util.uname` | `uname` | **present** | `qtest:posix-util-system` — `-s/-n/-r/-v/-m/-a` |
| `util.env` | `env` | **present** | `qtest:posix-util-system` — `-i`, assignments, `execve`, exit status |
| `util.getconf` | `getconf` | **present** | `qtest:posix-util-system` — `ARG_MAX`, `PAGESIZE`, `_POSIX_VERSION`, `-a` |
| `util.tty` | `tty` | **present** | `test:tests/posix/utility/system/tty/test.sh` — pty name, `-s`, non-tty failure |
| `util.stty` | `stty` | **present** | `test:tests/posix/utility/system/stty/test.sh` — `-a`, `size`, `sane`, non-tty failure |
| `util.kill` | `kill` | **present** | `qtest:posix-util-system` — `-l`, names/numbers, pid/group, signal 0, errors |
| `util.nice` | `nice` | **present** | `qtest:posix-util-system` — query, set/clamp, run command |
| `util.ps` | `ps` | **defect** | `P4d-D2` — `ps -e` has pid/ppid/uid/gid but `cmd` is `?` |
| `util.nohup` | `nohup` | **defect** | `P4d-D9` — creates `nohup.out` and execs, but does not redirect child output |

`hostname` and `whoami` are exercised as adjacent first-party tools; they are not POSIX profile
rows and are not claimed in the overlay.

## Behaviour covered

* `env`: `/bin/env -i P4D=only` prints only `P4D=only`; `/bin/env P4D=outer` preserves the base
  environment and adds the assignment; `/bin/env -i FOO=bar /bin/whoami` executes through the
  P2k `execve` path and prints `root`.
* `getconf`: `ARG_MAX` -> 131072, `PAGESIZE` -> 4096, `_POSIX_VERSION` -> 202405, unknown name
  -> `getconf: NOPE: unrecognized variable` exit 1.
* `kill`: `-l` lists the signal names, `-l 15` -> `TERM`, `-l TERM` -> `15`, `-0 1` -> 0
  (existence check without sending), `-s TERM 9999` -> `no such process`, exit 1.
* `nice`: no arguments prints 0; `-n 5 /bin/nice` prints 5; `-n 20` clamps to 19; `-n -5`
  yields -5; `-n 5 /bin/getconf ARG_MAX` runs the command at the new niceness.
* `ps`: the default service view is unchanged; `ps --json`/`--selftest` remain; `ps -e` scans
  the kernel pid registry and prints `PID PPID UID GID CMD` (pid 1 is `init`, other `cmd`
  values are `?` until a path source exists).
* `nohup`: `cd /tmp; nohup /bin/whoami` creates `/tmp/nohup.out`, prints the standard
  `nohup: ignoring input and appending output to '/tmp/nohup.out'` line, execs `whoami`, and
  propagates the command's exit status.
* `tty`/`stty`: the host cases cover the pty success paths and the non-tty failures; P3 supplies
  the target ttyd/ptyd primitives.

## Remaining defects

| Defect | Row | Exact missing piece | Follow-up |
|---|---|---|---|
| `P4d-D2` | `util.ps` | no command-name/path source for a pid. The kernel keeps `Process.path`, but `PROC_PID_INFO` exposes only pid/ppid/pgid/sid/uid/euid/gid/egid; `procd` keeps only its own children and the v1 protocol has no list operation; `svc:` status exposes a service-local handle, not a pid. `ps -e` therefore prints `?` for `cmd` | add a kernel pid-path query or a procd list operation, then print the real `cmd` |
| `P4d-D9` | `util.nohup` | the runtime prints through `debug_write`; there is no fd 1, `isatty` or `dup2` path. The utility creates/appends `nohup.out` and execs the command, but cannot redirect the child's output into the file | add a console/fd redirection path (or a runtime output handle), then assert the file content |

`P4d-D1` (`id -r`) remains resolved by the uutils `id`. `P4d-D3`..`D8` are resolved by the
first-party `env`, `getconf`, `kill` and `nice` implementations.

## Docs gate (outside Owns)

`make check-fast` passes guard, fmt, clippy, host tests, doc tests, profiles and provenance, but
the docs gate fails:

```
missing man page: docs/man1/env.1
missing man page: docs/man1/getconf.1
missing man page: docs/man1/kill.1
missing man page: docs/man1/nice.1
missing man page: docs/man1/nohup.1
```

`docs/man1/**` is not in P4o's Owns-globs (`userland/bin/{env,getconf,kill,nice,nohup,ps}/**`,
tests, `docs/posix/P4-system.md`, `docs/posix/evidence/P4-system.toml`, `Cargo.toml`,
`Cargo.lock`). The man-page stubs are the only remaining check-fast blocker; the ticket needs an
Owns extension or the lead must add the five pages.

## Evidence overlay

`docs/posix/evidence/P4-system.toml` registers the eight present rows:

* `util.id`, `util.uname`, `util.env`, `util.getconf`, `util.kill`, `util.nice` with
  `qtest:posix-util-system`.
* `util.tty`, `util.stty` with their host test scripts.

`util.ps` and `util.nohup` are not in the overlay.

## Acceptance

### `sh tests/posix/utility/run-system.sh`

Exit code: **0**

```
posix-util-system: qtest PASS (log build/posix/utility/system/qtest.log)
row	tool	status	command	detail
util.id	id	PASS	id	qtest
util.ps	ps	DEFECT	ps -e	P4d-D2
util.uname	uname	PASS	uname -a	qtest
util.env	env	PASS	env	qtest
util.getconf	getconf	PASS	getconf ARG_MAX	qtest
util.tty	tty	PASS	tty	host test
util.stty	stty	PASS	stty -a	host test
util.kill	kill	PASS	kill -l	qtest
util.nice	nice	PASS	nice -n 5 true	qtest
util.nohup	nohup	DEFECT	nohup true	P4d-D9
```

The QEMU case passed in 21656 ms with `exit_status: 33` and no unmatched expectation.

### `cargo xtask qtest --case posix-util-system --arch x86_64`

Exit code: **0**; `posix-util-system: pass`.

### `cargo xtask posix-check --profile posix --arch x86_64`

Exit code: **1** (the global gate is still release-blocked outside P4d/P4o)

```
posix-check: 901 pass, 898 fail, 10 skip; FAIL (release blocked)
```

The eight P4d/P4o present rows pass; `util.ps` and `util.nohup` stay fail as defects.

### `make check-fast`

Exit code: **2** — **check-fast: FAILED** only at the docs gate

```
check-fast summary:
  ok       guard (exit 0, 8.34s)
  ok       fmt (exit 0, 5.66s)
  ok       clippy (host) (exit 0, 0.73s)
  ok       tests (host, parallel) (exit 0, 115.32s)
  ok       tests (doc) (exit 0, 5.49s)
  ok       profiles (exit 0, 0.07s)
  ok       provenance (exit 0, 40.44s)
  skipped  clippy kernel x86_64 (--fast)
  skipped  clippy kernel aarch64 (--fast)
  skipped  clippy user programs x86_64 (--fast)
  skipped  clippy loader x86_64 (--fast)
  FAILED   docs (exit 1, 0.11s)
```

The five missing man pages are outside Owns; see “Docs gate”.

## Limitations

* The target is QEMU x86_64 only. No board log was produced and no release claim is made.
* Eight rows pass; `util.ps` and `util.nohup` remain exact defects. The ticket's 10/10 target is
  not met.
* The `cargo xtask build uutils` target build still fails in vendored rustix; the fixture uses
  the existing K25a uutils `id` slice and does not edit the uutils tree.
* `env`, `nice` and `nohup` copy a bounded set of well-known environment variables (`PATH`,
  `HOME`, `USER`, `LOGNAME`, `SHELL`, `PWD`, `TERM`) because `raven_rt::Args` exposes only
  `env_get`, not an environment iterator. Arbitrary exported variables are not inherited by
  the exec'd command; that needs a safe runtime enumeration API outside P4o Owns.
