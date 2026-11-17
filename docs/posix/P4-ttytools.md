<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# P4x - POSIX more and write

First-party Rust implementations of `more` and `write`, with host tests and executable evidence
rows.

## Tool matrix

| Tool | Covered | Tests |
|---|---|---:|
| `more` | `-n` page height; tty space/enter/q paging; non-tty full streaming; file operands | 4 lib |
| `write` | user/tty parsing; known user, logged-in check, `mesg n` refusal, CRLF normalization and delivery | 3 lib |

## Method

* `more` keeps a pure pager with a `Pager` seam (`is_tty`, `write`, `read_command`). The target
  wrapper reads file operands and drives the serial console; a non-tty output streams the whole
  buffer.
* `write` keeps pure directory/terminal seams. The target wrapper loads the user database, treats
  the console as the logged-in terminal, and sends the normalized message through `debug_write`.
* No ttyd/consd code or other lane's rows were edited.

## Limits

| Item | Detail |
|---|---|
| target tty detection | the target console wrapper treats its output as a tty; non-tty streaming is covered by the host tests |
| target stdin | messages come from the serial console; there is no generic stdin handle |
| `mesg n` | the target console has messages enabled; the refusal path is host-tested |
| `write` sessions | one console terminal; no multi-pty session lookup |
| `talk` | not implemented (explicitly out of scope) |

## Acceptance

```
sh tests/posix/utility/run-ttytools.sh
cargo nextest run -p more -p write
cargo xtask posix-check --profile posix --arch x86_64
make check-fast
```

The runner reports `2 pass, 0 fail`; both rows are registered. No board claim.
