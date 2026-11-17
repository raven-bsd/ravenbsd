<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# P4y - POSIX crontab and minimal crond

## Spool matrix

| Operation | Behaviour | Evidence |
|---|---|---|
| install file | validate five fields + command, atomic temp+rename to `/var/cron/<user>` | `test.sh`; `lib_ut` |
| install stdin | host `-` / no operand; target requires a file | `test.sh`; target usage |
| list `-l` | print the spool or fail when absent | `test.sh` |
| remove `-r` | remove the spool or fail when absent | `test.sh` |
| invalid field | reject, leave the previous spool untouched | `test.sh`; `lib_ut` |
| `@reboot` | explicit unsupported error | `test.sh`; `lib_ut` |
| `CRON_SPOOL` | host-test spool override | `test.sh`; `lib_ut` |

Field syntax: `*`, a number, `a-b`, comma lists, and `/step`. Day-of-month and day-of-week are
ORed when both are restricted, as POSIX requires. Sunday matches 0 or 7.

## Scheduler matrix

| Feature | Behaviour |
|---|---|
| spool read | every regular file under `/var/cron`; missing/empty is not an error |
| granularity | once per minute; the first loop runs the current minute immediately |
| command path | kernel `spawn_file`; simple program + arguments; no pid/wait/signal ownership |
| logging | `crond: start` and `crond: exit` with user/command/status |
| overlap policy | serial wait; overlap detection is an explicit defect |
| `@reboot` | parse error; logged and skipped |
| shell metacharacters | rejected; use a script |

The qtest fixture mounts ramfs at `/var/cron`, installs one spool
(`* * * * * echo cron-ok`), starts crond, and observes `crond: start ...` and
`crond: exit ... status=0`. The child's stdout is not captured on the serial line, so the qtest
asserts the scheduler's start/exit records rather than the echo text.

## Acceptance

```
sh tests/posix/utility/run-crontab.sh
crontab: ok
crond: pass

cargo xtask qtest --case crond --arch x86_64
crond: pass

cargo xtask posix-check --profile posix --arch x86_64
util.crontab pass (test:tests/posix/utility/crontab/test.sh)
total 914 pass / 885 fail / 10 skip

make check-fast
check-fast: PASSED
```

Evidence: `docs/posix/evidence/P4-crontab.toml`. The service manifest is
`etc/services.d/crond.toml`; the image must mount a writable `/var/cron`.
