<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# P4a3 - required POSIX time/tty utilities

First-party Rust implementations of the four required rows: `time`, `times`, `tput` and `ulimit`.
`tabs`, `unget` and `what` are optional and waived by P4a2; they are not in this batch.

## Tool matrix

| Tool | Covered | Tests | Evidence |
|---|---|---:|---|
| `time` | `-p`, command operands, real time from `clock_get`, child user/sys from `PROC_TIMES`, stderr statistics, child status | 5 lib | present |
| `times` | POSIX `XmY.ZZs` two-line rendering from `PROC_TIMES`; operand rejection | 5 lib | present |
| `tput` | `-T`, ANSI table (`clear`, `cup`, `bold`, `sgr0`, `smso`/`rmso`, `smul`/`rmul`, `el`, `ed`, `bel`, `cols`, `lines`), typed errors | 6 lib | present |
| `ulimit` | `-H`/`-S`, `-a`, resource selectors, `unlimited`, soft/hard query and set over `PROC_GETRLIMIT`/`PROC_SETRLIMIT`, POSIX display units | 8 lib | present |

## Method

* `time` spawns the command through `raven_rt::spawn`, waits with `RtSys::proc_wait`, reads the
  monotonic clock for real time and `PROC_TIMES` for the reaped child's user/sys counters, then
  writes the POSIX statistics to stderr and returns the child status.
* `times` calls `PROC_TIMES` (nanoseconds) and renders the POSIX two-line form.
* `tput` and `tabs` were the P4g ANSI subset; this batch keeps `tput` only because `tabs` is
  optional under P4a2.
* `ulimit` maps the POSIX selectors to the kernel's resource table, reads and writes the limit pair
  through the raw `PROC_GETRLIMIT`/`PROC_SETRLIMIT` numbers, and reports `-a` in POSIX units
  (512-byte blocks for file/core, kbytes for data/stack/address, seconds/count otherwise).

## Limits

| Item | Detail |
|---|---|
| `time`/`times` counters | `PROC_TIMES` reports the caller's process and reaped-child nanoseconds; the utility does not fabricate ticks |
| `ulimit` | the `-c..-v` selectors are the XSI optional form; the required `-a` and soft/hard report/set are implemented |
| `tput` | `-S` is not supported (usage error); the capability table is the ANSI subset |
| `tput -T` | accepts a terminal type but the output is the same ANSI table |

## Acceptance

```
sh tests/posix/utility/run-simple2.sh
cargo nextest run -p raven-time -p times -p tput -p ulimit
cargo xtask posix-check --profile posix --arch x86_64 --host-only
make check-fast
```

All four rows are registered in `docs/posix/evidence/P4-simple2.toml`.
