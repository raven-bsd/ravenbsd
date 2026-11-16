<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# PROC_HANDLE_INFO and wait-status encoding (P9b)

OBSERVED on `stuart/P9b` (base `origin/master` `5cea29d5`). Kernel check and qtest; no board
claim.

## Syscall numbers

| Raw | Name | Signature | Result |
|---|---|---|---|
| `0x3E` | `PROC_HANDLE_INFO` | `(handle) -> pid` | pid of an owned process handle |
| `0x3F` | `PROC_EXEC_CRED` | `(proc, file, flags)` | U3b native exec credentials |

The frozen `crates/raven-abi` is not edited. `0x3E` follows the P9/P9a contract; U3b's exec
credential helper moved from `0x3E` to `0x3F` so the two raw calls do not collide. The userland
`raven-rt` exec wrapper uses the new `0x3F`.

`PROC_HANDLE_INFO` looks only in the caller's handle table (no ambient search); a non-process or
stale handle returns `EBADF`. It works after the child exits because the parent's handle remains
valid until it closes it.

## Wait status

`PROC_WAIT` writes the **raw exit code** as a little-endian `i32`. The libc layer encodes it in
the POSIX/Linux wait-status representation; the qtest checks the 42 -> `42 << 8 = 10752` case.

| POSIX class | Encoding | Decode |
|---|---|---|
| exited `code` | `code << 8` | `WIFEXITED`: low 7 bits zero; `WEXITSTATUS = (status >> 8) & 0xff` |
| signaled `sig`, core | `sig | (core ? 0x80 : 0)` | `WIFSIGNALED`; `WTERMSIG = status & 0x7f`, `WCOREDUMP = status & 0x80` |
| stopped `sig` | `(sig << 8) | 0x7f` | `WIFSTOPPED`; `WSTOPSIG = (status >> 8) & 0xff` |
| continued | `0xffff` | `WIFCONTINUED` |

The current kernel only produces normal exit/terminate codes; signal and stopped classes are
defined for the libc codec and future signal-aware wait, and are not fabricated by this ticket.

## QEMU evidence

```
$ cargo xtask qtest --case proc-handle-info --arch x86_64
proc-handle-info: pid=2 info=ok
proc-handle-info: wait=42 status=10752
proc-handle-info: exited info=ok
proc-handle-info: stale=badf
proc-handle-info: nonprocess=badf
proc-handle-info: ok
init exited (0)
```

`build/qtest/proc-handle-info.json`: result `pass`, no unmatched or forbidden hits. The fixture
creates an empty child, reads its pid, terminates it with code 42, waits, checks the encoded
status, keeps the pid readable after exit, closes the handle and checks stale/non-process
rejection.

## Host tests

`cargo nextest run -p raven-kernel-core` is named by the ticket, but this tree has **no**
`raven-kernel-core` package (the kernel is a binary-only `raven-kernel` package), so the command
returns “package ID specification did not match any packages”. The kernel checks and the qtest
cover the syscall; the report records the unavailable command.

## Limits

* `PROC_WAIT` does not yet return signal/core/stopped classes; only normal exit/terminate codes.
* No `pidfd`, job control or multi-child wait options.
* The raw syscall numbers live in the kernel until the next ABI revision; no ABI contract edit.
