<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# POSIX xargs utility (P4q)

Status: **DONE for the bounded argument-batching core.** `raven-xargs` reads whitespace or
NUL-separated items, batches them with `-n`/`-s`, replaces `-I` markers, runs real child commands
through the host process API and the landed target `spawn` path, and maps child status to the
POSIX xargs exit values. `util.xargs` is registered as present only through the host behaviour
script.

## Method

`userland/bin/xargs` is a first-party Rust crate (`raven-xargs`), `no_std` on the target and
`#![forbid(unsafe_code)]`. The library owns parsing, input splitting, batching, replacement and
status aggregation. A small `Runner` trait isolates execution: the host binary uses
`std::process::Command`, and the target binary resolves the command in the initfs and uses
`raven_rt::spawn` plus `RtSys::proc_wait`.

`tests/posix/utility/xargs/test.sh` is self-contained: it builds the host binary and runs 21
behaviour cases with real child processes. `userland/bin/xargs/tests/host.rs` adds five real
process end-to-end tests; `src/lib_ut.rs` covers the parser and batching with a recording runner.

```sh
sh tests/posix/utility/run-xargs.sh
sh tests/posix/utility/xargs/test.sh
```

## Behaviour matrix

| Area | Implemented | Cases |
|---|---|---|
| Input | whitespace splitting with quotes/backslashes, `-0` NUL splitting, `-0` with `-I` | 5 |
| Batching | `-n`, `-s`, `-n` plus `-s` splitting | 3 |
| Replacement | `-I` marker, `-I` without a marker, empty line items | 3 |
| Empty input | default runs the command once; `-r` and `-I` do not run | 2 |
| Trace | `-t` writes each command to standard error | 2 |
| Errors/status | `-x`, size overflow, child 1/255/signal, missing command | 6 |

Exit status follows POSIX xargs: 0 success, 123 for a child exit 1-125, 124 for 255, 125 for a
signal, 126 for a command that cannot be invoked, 127 for a missing command, 1 for input/size
errors, and 2 for usage errors.

## Known limits and defects

* The target command resolver searches `/bin` and `/sbin` in the initfs, or accepts a command that
  contains `/`; the host uses the normal `PATH`.
* The target standard input path reads `tty:console`; the image has no general process-stdin or
  pipeline handle. The target binary is not staged in the image, so the host suite is the
  executable evidence.
* Host child stdin is `/dev/null`; the target child inherits the console because `SpawnOptions` has
  no stdio redirection. This is documented rather than silently emulated.
* `-s` counts the command and argument bytes plus one separator each, not the environment. The
  default bound is 4096 bytes. A single item that cannot fit is always an error; with `-n` and
  `-x`, an incomplete `-n` group that would exceed `-s` is an error instead of being split.
* `-I` implies one item per invocation and strict size checking. If the marker is absent from the
  command, the command runs unchanged once per item (the GNU-compatible behaviour).
* GNU extensions such as `-L`, `-P`, `-o`, `-e`, `-d`, `-a` and `-p` are not implemented.
  Interactive prompting, replacement of `{}` as a special case, and environment-size accounting
  are not implemented.
* The trace format is a simple space-joined command line, not a shell-quoted rendering.

## Evidence overlay

`docs/posix/evidence/P4-xargs.toml` contains the `util.xargs` row with
`test:tests/posix/utility/xargs/test.sh`. The P0b loader accepts the `P4-xargs` owner batch file and
the gate executes the script; the row is present only while every case passes.
