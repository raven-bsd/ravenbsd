<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# P4g - POSIX time/tty utilities

First-party Rust implementations of `time`, `times`, `tput`, `tabs`, `ulimit`, `unget` and
`what`, with host tests, a runner and evidence rows. `ulimit` is a documented gap: the ABI has no
rlimit syscall, so no row is claimed.

## Tool matrix

| Tool | Covered | Tests | Evidence |
|---|---|---:|---|
| `time` | `-p`, command operands, real-time report, child spawn/wait, usage errors | 5 lib | present |
| `times` | POSIX `XmY.ZZs` two-line rendering, zero-tick limit, operand rejection | 5 lib | present |
| `tput` | `-T`, ANSI table (`clear`, `cup`, `bold`, `sgr0`, `smso`/`rmso`, `smul`/`rmul`, `el`, `ed`, `bel`, `cols`, `lines`), typed errors | 6 lib | present |
| `tabs` | default stops, explicit stops, `-c` cursor motion, `-T`, unsupported historical styles | 9 lib | present |
| `ulimit` | `-H`/`-S`, resource selectors, `unlimited`, typed parser errors | 6 lib | **none** (no syscall) |
| `unget` | `-n`/`-s`/`-r`, `s.*` to `p.*`, usage errors | 6 lib | present |
| `what` | `@(#)` extraction to `"`/`>`/newline/NUL, `-s`, multiple files, usage errors | 7 lib | present |

## Method

* Every package is a workspace member with a host-testable library and a target-only binary; the
  host binary is a no-op, matching `cal` and `asa`.
* `time` spawns the command through `raven_rt::spawn`, waits with `RtSys::proc_wait`, and reports
  the monotonic real time. User and sys stay `0.00` because the kernel exposes no process CPU
  clock.
* `times` renders the POSIX shape from clock ticks; with no accounting clock the target writes
  zero ticks.
* `tput` and `tabs` emit the ANSI subset the console uses; unknown capabilities and unsupported
  styles are typed usage/exit-1 failures, not silent success.
* `what` scans bytes, and `unget` maps SCCS `s.*` names to `p.*` and restores the p-file.

## Limits

| Item | Detail |
|---|---|
| `ulimit` | no `getrlimit`/`setrlimit` in `raven-abi`; the utility parses the POSIX surface and exits 1 with an explicit message. It is not claimed in the evidence overlay |
| `time` user/sys | no process CPU-time clock; only real time is measured |
| `times` values | same limit; the renderer is exact but the target supplies zeros |
| `tput -S` | not supported (usage error); the capability table is the ANSI subset |
| `tabs` styles | only default and `-c`; `-a/-f/-p/-s/-u` and `+m` return usage |
| `unget` | p-file restore only; no SCCS delta reconstruction |
| `what` terminator | stops at `"`, `>`, newline or NUL; other historical delimiters are not special |

## Acceptance

```
sh tests/posix/utility/run-simple2.sh
cargo nextest run -p time -p times -p tput -p tabs -p ulimit -p unget -p what
cargo xtask posix-check --profile posix --arch x86_64 --host-only
make check-fast
```

The runner reports `7 pass, 0 fail` (the `ulimit` test covers the parser and the explicit
unavailable-limit path). Six rows are registered in `docs/posix/evidence/P4-simple2.toml`;
`util.ulimit` stays missing pending an rlimit syscall or a P4f waiver.
