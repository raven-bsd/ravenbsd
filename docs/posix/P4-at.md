<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# P40 - POSIX at and batch

## Schedule matrix

| Operation | Behaviour | Evidence |
|---|---|---|
| `at now` | queue immediately | `test.sh`; `lib_ut` |
| `at now + N minutes\|hours\|days` | relative queue time; `+ N unit` is accepted too | `test.sh`; `lib_ut` |
| `at HH:MM` | next occurrence of that clock time (today, else tomorrow) | `test.sh`; `lib_ut` |
| `at YYYY-MM-DD HH:MM` | exact UTC date and time; the `T` separator is accepted | `test.sh`; `lib_ut` |
| `at -f file TIME` | read one command line from `file`; `-` means standard input on the host | `test.sh` |
| `at TIME command...` | command-word extension used by the target and the qtest | `test.sh`; qtest |
| `at TIME` with no command | read one command line from standard input on the host | `test.sh` |
| `at -l` | list job id, due Unix time and command, oldest first | `test.sh` |
| `at -r id...` | remove jobs; a missing id is an error | `test.sh` |
| `batch` | queue a command on the idle queue | `test.sh`; qtest |

Every job is one file under `/var/at` named `<due>.<serial>.job`. The host writes a temporary
file plus rename. The target tries the same; ramfs does not implement rename or unlink, so a
failed rename falls back to a direct replace and a failed unlink retires the job as a `due=-1`
tombstone that `at -l` hides. The spool survives a scheduler restart: `atd` rescans it every
second and runs jobs whose due time has passed.

## Scheduler matrix

| Feature | Behaviour |
|---|---|
| queue read | every regular `*.job` file under `/var/at`; a missing spool is empty, not an error |
| granularity | one-second scan; a job is due when `due <= now` |
| command path | kernel `spawn_file`; program plus arguments; no pid/wait/signal ownership |
| logging | `atd: start job=... command=...` and `atd: exit job=... status=...` |
| persistence | jobs stay in the spool until a run attempt removes them; without unlink they become a hidden `due=-1` tombstone |
| batch | runs on a tick with no due regular job; there is no load-average source |
| failure | a spawn failure is logged, then the job is retired to avoid a retry loop |
| overlap | serial; a job never starts twice concurrently |

## Unsupported forms

- `noon`, `midnight`, `teatime`, month names and timezone suffixes.
- POSIX compact date/time `[[CC]YY]MMDDhhmm[.ss]`.
- Multi-line scripts; commands are one program plus arguments.
- Shell metacharacters (`>`, `<`, `|`, `;`, `&`, single and double quotes).
- `at -m`, `-q` and other queue options.
- Target standard input; the target needs `-f file` or command words.
- True load-average-based batch scheduling; an idle scheduler tick is the proxy.
- Exact-once execution across a crash: a job is removed after the attempt, so a crash between
  spawn and removal can repeat it.

## Acceptance

```
$ sh tests/posix/utility/run-at.sh
at: ok
atd: pass

$ cargo xtask qtest --case atd --arch x86_64
atd: pass

$ cargo xtask posix-check --profile posix --arch x86_64
util.at pass (test:tests/posix/utility/at/test.sh)
util.batch pass (test:tests/posix/utility/at/test.sh)

$ make check-fast
check-fast: PASSED
```

Evidence: `docs/posix/evidence/P4-at.toml`. The service manifest is
`etc/services.d/atd.toml`; the image must mount a writable `/var/at`.
