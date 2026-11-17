<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# P43 - POSIX uucp, uustat and uux

First-party Rust implementations of the bounded POSIX `uucp`, `uustat` and `uux` surface. The
local spool queue is the tested surface; there is no modem, network transport or remote execution.

## Method

* `uucp` copies local files or queues a copy/fetch job for a configured peer. A destination with
  `system!` is always queued because no transport exists.
* `uustat` lists, summarizes and cancels jobs from the same queue.
* `uux` parses a `system!command` string, reads local `< file` inputs and stdin (`< -`), records
  `>`/`>>` output names, and queues the command job.
* The target wrappers use `raven_rt::fs` through `uucp::runtime::TargetSpool`; the host tests use
  an in-memory spool and the same public models. No network daemon or other lane's row is touched.
* All three crates are `no_std` libraries with host unit tests in `src/lib_ut.rs`; the two host
  programs are the target wrappers.

## Spool and queue format

```
/var/spool/uucp/seq              next sequence number, decimal text
/var/spool/uucp/queue/<id>.job   one first-party job record
```

The record starts with `uucp1\n`; all integers are little-endian and every string/byte field is a
`u32` length followed by its bytes. The fields are:

```
id u64 | user text | system text | grade u8 | kind u8
kind 1 (copy):    source text | destination text | mode u32 | data bytes
kind 2 (fetch):   source text | destination text
kind 3 (command): command text | stdin bytes | file count u32 |
                  (name text | data bytes)* | output count u32 | output text*
```

The format is bounded at 4 MiB per job and is not a wire protocol. The queue directory is the
only durable state; no `work/` or lock file is used.

## Peer configuration

`/etc/uucp/config` is optional and line-oriented:

```
# local system name, spool root and known peers
local raven
spool /var/spool/uucp
peer alice
peer bob
```

Blank lines and `#` comments are ignored. A `system!` operand must name a configured peer; an
unknown peer is an error. Every configured peer is treated as unavailable because no transport is
implemented, so a remote request is queued rather than sent.

## Tool matrix

| Tool | Supported surface | Tests |
|---|---|---:|
| `uucp` | local copy with mode preservation; `-d`/`-f` directory policy; `-j`; `-g`; `-n`; `-c`/`-C`, `-r`/`-R` accepted; remote copy and fetch queueing | 15 lib + 2 host |
| `uustat` | `-a`, `-q`, `-s system`, `-u user`, `-k job`; default current user | 7 lib |
| `uux` | `-a`, `-b`/`-c`/`-C`, `-g`, `-j`, `-n`, `-z`, `-x`; quoted command words; `< file`, `< -`, `>`, `>>` | 9 lib |

The cases run `cargo nextest run -p uucp`, `-p uustat` and `-p uux` and are registered in
`docs/posix/evidence/P4-uucp.toml`.

## Runner

```
sh tests/posix/utility/uucp/run-uucp.sh
sh tests/posix/utility/uucp/run-uucp.sh uustat
sh tests/posix/utility/uucp/run-uucp.sh --list
```

The ticket named `tests/posix/utility/run-uucp.sh`; the runner lives under
`tests/posix/utility/uucp/` so it stays inside the ticket's Owns-globs, matching the P4z rework
pattern.

## Unsupported cases

| Item | Detail |
|---|---|
| Transport | no modem, network, peer daemon or actual remote execution; every remote job remains queued |
| `uucp` remote-to-remote | rejected; a transport between two peers does not exist |
| `uucp` `-m` | accepted but no mail transport; no notification is sent |
| `uucp` `-x` | parsed and ignored; no debug transcript |
| `uustat` | `-e`, `-i`, `-o`, `-r`, `-W`, `-y`, `-z` are not implemented and are usage errors |
| `uux` | `-p`, `-r`, `-s` are usage errors; command execution, pipes, `&&`, globbing and shell expansion are not implemented |
| `uux` outputs | `>`/`>>` names are stored in the job; no remote file is created |
| stdin | the target only has the serial console; `< -` reads `tty:console` |
| clock/accounting | no wall-clock timestamps or accounting files; sequence ids order the queue |
| target runtime | host tests exercise the models; the target wrappers and `TargetSpool` are build-checked but not board-tested |

## Acceptance

```
sh tests/posix/utility/uucp/run-uucp.sh
cargo nextest run -p uucp -p uustat -p uux
cargo xtask posix-check --profile posix --arch x86_64
make check-fast
```

No board claim.
