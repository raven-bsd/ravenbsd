<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# P4d/P4o: POSIX system and process utilities

Status: PARTIAL. Eleven rows are present with executable evidence; two rows remain defects:

* `util.ps` — `ps -e` exposes pid/ppid/uid/gid, but no pid-to-command source exists, so `cmd`
  is `?`. `P4d-D2`.
* `util.mesg` — the query reports the console's fixed mode (`y` here), but `mesg n` cannot set
  the group-write bit: `tty:console`/`ttyd` have no chmod/write-permission operation and return
  `ENOENT`/`ENOSYS`. `P4u-D1`.

The P4o 10/10 target is not met and the P4u three-row batch is 2/3; the combined system family
is 11/13. No row is waived and no missing behavior is claimed present. The six P4o and three
P4u man pages are in Owns and the docs gate is green.

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

The rev 36 merge is `origin/master` `e9d91b57`. It includes the kernel process model, but
`PROC_PID_INFO` still writes only eight words (pid, ppid, pgid, sid, uid, euid, gid, egid) and
The kernel keeps the process record but `PROC_PID_INFO` writes only identity, not the command. `ps -e` therefore
cannot fill `cmd`.

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
| `util.mesg` | `mesg` | **defect** | `P4u-D1` — query `y`; set has no tty chmod/write-permission operation |
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
* `mesg`: the console mode has the group-write bit, so `mesg` prints `y`; `mesg n` attempts
  `chmod` on `tty:console` and fails because the tty server has no chmod operation.
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

## Remaining defects

| Defect | Row | Exact missing piece | Follow-up |
|---|---|---|---|
| `P4d-D2` | `util.ps` | no command-name/path source for a pid. The kernel keeps `Process.path`, but `PROC_PID_INFO` writes only identity, not the command. `svc:` status exposes a service-local handle, not a pid. `ps -e` therefore prints `?` for `cmd` | add a kernel pid-path query (P2o), then print the real `cmd` |
| `P4u-D1` | `util.mesg` | `tty:console` (consd) and `ttyd` have no chmod/write-permission operation; `fstat` returns a fixed mode (`0o620` for consd, `0o666` for ttyd). The query reads the group-write bit, but a set request fails `ENOENT`/`ENOSYS` | add a tty write-permission op (or a chmod handler) to the tty server, then test both `mesg y` and `mesg n` |

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

`docs/posix/evidence/P4-system.toml` registers the eleven present rows:

* `util.id`, `util.uname`, `util.env`, `util.getconf`, `util.kill`, `util.nice`,
  `util.nohup`, `util.logger`, `util.renice` with `qtest:posix-util-system`.
* `util.tty`, `util.stty` with their host test scripts.

`util.ps` and `util.mesg` are not in the overlay.

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
util.mesg	mesg	DEFECT	mesg	P4u-D1
util.logger	logger	PASS	logger -t p4u hello	qtest
util.renice	renice	PASS	renice -n 5 -p 1	qtest
```

The QEMU case passed in 42888 ms with `exit_status: 33` and no unmatched expectation.

### `cargo xtask qtest --case posix-util-system --arch x86_64`

Exit code: **0** — `posix-util-system: pass`.

### `cargo xtask posix-check --profile posix --arch x86_64`

Exit code: **1** (global gate still release-blocked outside P4d/P4o)

```
posix-check: 918 pass, 881 fail, 10 skip; FAIL (release blocked)
```

The eleven P4d/P4o/P4u present rows pass; `util.ps` and `util.mesg` stay fail as defects.

### `make check-fast`

Exit code: **0** — **check-fast: PASSED**

```
check-fast summary:
  ok       guard (exit 0, 4.67s)
  ok       fmt (exit 0, 5.28s)
  ok       clippy (host) (exit 0, 0.59s)
  ok       tests (host, parallel) (exit 0, 46.56s)
  ok       tests (doc) (exit 0, 10.64s)
  ok       profiles (exit 0, 0.25s)
  ok       provenance (exit 0, 50.62s)
  skipped  clippy kernel x86_64 (--fast)
  skipped  clippy kernel aarch64 (--fast)
  skipped  clippy user programs x86_64 (--fast)
  skipped  clippy loader x86_64 (--fast)
  ok       docs (exit 0, 0.05s)
check-fast: PASSED
```

## Limitations

* The target is QEMU x86_64 only. No board log was produced and no release claim is made.
* Eleven rows pass; `util.ps` and `util.mesg` remain exact defects. The system family is 11/13,
  not 13/13.
* `env`, `nice` and `nohup` copy a bounded set of well-known environment variables (`PATH`,
  `HOME`, `USER`, `LOGNAME`, `SHELL`, `PWD`, `TERM`) because `raven_rt::Args` exposes only
  `env_get`, not an environment iterator. Arbitrary exported variables are not inherited by
  the exec'd command; that needs a safe runtime enumeration API outside P4o Owns.
* The `cargo xtask build uutils` target build still fails in vendored rustix; the fixture uses
  the existing K25a uutils `id` slice and does not edit the uutils tree.
