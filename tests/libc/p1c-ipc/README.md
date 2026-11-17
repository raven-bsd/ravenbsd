<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# P1c-ipc-tty host tests

Host checks for the POSIX IPC/terminal slice. Run them with:

```sh
cargo nextest run -p raven-libc-p1c-ipc-test
```

`tests/policy.rs` does two things:

* re-derives the 59 interface rows this slice claims and compiles each `&symbol` probe with
  `cc -fsyntax-only -std=c17 -D_POSIX_C_SOURCE=202405L -D_XOPEN_SOURCE=800`, the same feature
  set as `cargo xtask posix-check --profile posix --arch x86_64 --host-only`; a row that is
  claimed `present` must therefore be declared in POSIX/XSI mode, not only under `__BSD_VISIBLE`;
* pins the real policy answers: the `termios` speed accessors, the `ftok` derivation, the
  message-queue name/attribute rules, and the typed `ENOSYS`/`EBADF` results for the primitives
  the kernel does not provide.

The implementation lives in `libc/ravenlibc/crates/raven-libc-ipc` (pure policy) and
`libc/ravenlibc/crates/raven-libc/src/compat/{aio,mqueue,stropts,tty_ipc,sysv_ipc}.rs` (C ABI).
