<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# P4d/P4o: POSIX system and process utilities

Status: PARTIAL. Twelve rows are present with executable evidence; one row remains a defect:

* `util.ps` — `ps -e` exposes pid/ppid/uid/gid, but no pid-to-command source exists, so `cmd`
  is `?`. `P4d-D2`.

The P4u three-row batch is complete: `mesg`, `logger` and `renice` all pass. The combined
system family is 12/13, with only P4o's `util.ps` outstanding. No row is waived and no missing
behavior is claimed present. The docs gate is green.

This is a host/QEMU test result. It is not a board result and it does not change the release
profile or the release gate.

## Method

`tests/qtest/cases/posix-util-system.toml` boots the `base` profile with a purpose-built initfs
from `tests/posix/utility/system/build.sh`. The fixture contains the core services, `rsh`, and
the first-party system utilities `env`, `getconf`, `kill`, `nice`, `nohup`, `mesg`, `logger`,
`renice` and `ps`, plus `dmesg` for the logger readback and the uutils `id` at `/bin/id`, the
first-party `uname`, `hostname`, `whoami`.

The fixture builds user programs with `--release`: the kernel `PROC_EXEC` path caps an image at
1 MiB, and debug builds of `whoami`/`nice` exceed it. Release builds are ~40–60 KiB and let
`env`/`nice`/`nohup` exec commands through the P2k path.

The rev 38 merge is `origin/master` `b75f0497` (includes `104022c7` and the kernel process
model). `PROC_PID_INFO` still writes only eight words (pid, ppid, pgid, sid, uid, euid, gid,
egid), so `ps -e` cannot fill `cmd`.
The tty write-permission operation lives in `raven-proto-tty`, `consd` and `ttyd`; `fstat`
reflects the persistent group-write bit, only root may change it, and `mesg` queries/sets
through the same protocol.

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
| `util.nohup` | `nohup` | **present** | `qtest:posix-util-system` — non-terminal stdout path, exec and exit status |
| `util.mesg` | `mesg` | **present** | `qtest:posix-util-system` — y/n via tty protocol, fstat bit, non-tty error |
| `util.logger` | `logger` | **present** | `qtest:posix-util-system` — priority/tag record through `log:`, dmesg readback |
| `util.renice` | `renice` | **present** | `qtest:posix-util-system` — set/query niceness, kernel permission rules |
| `util.ps` | `ps` | **defect** | `P4d-D2` — pid/ppid/uid/gid work, `cmd` is `?` |

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
* `nohup`: the qtest stdout is not a terminal, so `nohup /bin/whoami` runs the command without a
  `nohup.out` redirect and prints `root`; `nohup /bin/getconf NOPE` propagates exit 1. The
  terminal-only redirect is unavailable (no fd 1/`isatty`/`dup2`) and is recorded below.
* `mesg`: `mesg` prints `y`; `mesg n` persists the cleared group-write bit, the next query
  prints `n` and exits 1; `mesg y` restores it; `mesg -t file:/etc/passwd` reports
  `not a tty` and exits 1.
* `logger`: `logger -t p4u -p user.info hello` writes `p4u: <14>hello` to the `log:` ring;
  `logger -t p4uf -p user.info -f /tmp/p4u.log` logs the file contents; `dmesg` reads both
  records back.
* `renice`: pid, process-group and user targets work; `renice -n 5 -p 1` / `-g 1` / `-u 0`
  print the new value and the matching `-n 0` restores it. An unknown pid reports
  `no such process` with exit 1.
* `ps`: the default service view is unchanged; `ps --json`/`--selftest` remain; `ps -e` scans
  the kernel pid registry and prints `PID PPID UID GID CMD` (pid 1 is `init`, other `cmd`
  values are `?`).
* `tty`/`stty`: the host cases cover the pty success paths and the non-tty failures; P3 supplies
  the target ttyd/ptyd primitives.

## Nohup waiver

POSIX `nohup` redirects stdout only when stdout is a terminal. The Raven runtime has no fd 1,
`isatty` or `dup2` path; output goes through `debug_write` to the kernel console. The qtest
serial is therefore treated as the non-terminal case: the utility execs the command and
preserves its exit status with no redirect. The terminal-only `nohup.out` redirect is the
waived part; the exact follow-up is a libc/tty output-handle API (`isatty`, `dup2`).

## Remaining defect

| Defect | Row | Exact missing piece | Follow-up |
|---|---|---|---|
| `P4d-D2` | `util.ps` | no command-name/path source for a pid. The kernel keeps `Process.path`, but `PROC_PID_INFO` writes only identity, not the command. `svc:` status exposes a service-local handle, not a pid. `ps -e` therefore prints `?` for `cmd` | add a kernel pid-path query (P2o), then print the real `cmd` |

`P4u-D1` is resolved by the tty write-permission operation in `raven-proto-tty`, `consd` and
`ttyd`: the setting is persistent, `fstat` reflects the group-write bit, root is required to
change it, and `mesg` queries/sets through the same protocol.

`P4d-D1` (`id -r`) remains resolved by the uutils `id`. `P4d-D3`..`D8` are resolved by the
first-party `env`, `getconf`, `kill` and `nice` implementations. `P4d-D9` is reduced to the
terminal-only nohup redirect waiver above.

## Man pages

The six pages are now present and the docs gate is green:

```
docs/man1/env.1
docs/man1/getconf.1
docs/man1/kill.1
docs/man1/nice.1
docs/man1/nohup.1
docs/man1/ps.1       (updated with ps -e and the CMD limitation)
docs/man1/mesg.1
docs/man1/logger.1
docs/man1/renice.1
```

## Evidence overlay

`docs/posix/evidence/P4-system.toml` registers the twelve present rows:

* `util.id`, `util.uname`, `util.env`, `util.getconf`, `util.kill`, `util.nice`,
  `util.nohup`, `util.mesg`, `util.logger`, `util.renice` with `qtest:posix-util-system`.
* `util.tty`, `util.stty` with their host test scripts.

`util.ps` is not in the overlay.

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
util.nohup	nohup	PASS	nohup true	qtest
util.mesg	mesg	PASS	mesg	qtest
util.logger	logger	PASS	logger -t p4u hello	qtest
util.renice	renice	PASS	renice -n 5 -p 1	qtest
```

The QEMU case passed in 24378 ms with `exit_status: 33` and no unmatched expectation.

### `cargo xtask qtest --case posix-util-system --arch x86_64`

Exit code: **0** — `posix-util-system: pass`.

### `cargo xtask posix-check --profile posix --arch x86_64`

Exit code: **1** (global gate still release-blocked outside P4d/P4o)

```
posix-check: 923 pass, 876 fail, 10 skip; FAIL (release blocked)
```

The twelve P4d/P4o/P4u present rows pass; `util.ps` stays fail as a defect.

### `make check-fast`

Exit code: **0** — **check-fast: PASSED**

```
check-fast summary:
  ok       guard (exit 0, 19.41s)
  ok       fmt (exit 0, 31.80s)
  ok       clippy (host) (exit 0, 2.09s)
  ok       tests (host, parallel) (exit 0, 78.89s)
  ok       tests (doc) (exit 0, 18.07s)
  ok       profiles (exit 0, 0.13s)
  ok       provenance (exit 0, 84.72s)
  skipped  clippy kernel x86_64 (--fast)
  skipped  clippy kernel aarch64 (--fast)
  skipped  clippy user programs x86_64 (--fast)
  skipped  clippy loader x86_64 (--fast)
  ok       docs (exit 0, 0.13s)
check-fast: PASSED
```

## Limitations

* The target is QEMU x86_64 only. No board log was produced and no release claim is made.
* Twelve rows pass; `util.ps` remains the only exact defect. The system family is 12/13.
* `env`, `nice` and `nohup` copy a bounded set of well-known environment variables (`PATH`,
  `HOME`, `USER`, `LOGNAME`, `SHELL`, `PWD`, `TERM`) because `raven_rt::Args` exposes only
  `env_get`, not an environment iterator. Arbitrary exported variables are not inherited by
  the exec'd command; that needs a safe runtime enumeration API outside P4o Owns.
* The `cargo xtask build uutils` target build still fails in vendored rustix; the fixture uses
  the existing K25a uutils `id` slice and does not edit the uutils tree.
