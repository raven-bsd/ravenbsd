<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# libc kernel triage (S54)

OBSERVED on `stuart/S54` (base `origin/master` `dcaf3f92`). The kernel-side reproducer is
`tests/qtest/cases/libc-kernel.toml` with the standalone fixture
`tests/fixtures/libc-kernel/`; it does not use the libc harness or a C toolchain.

The seven libc cases still carry the stale `skip` lines and the old large `libc-test` profile on
`origin/master`. The classification below was produced in a temporary worktree from
`stuart/S54` merged with `origin/dave/C73c` (the C73c skip removal, the minimal `libc-test`
profile and the C73 build harness), commit `e822300b`. That merge is not part of the S54
branch; it is the C73c prerequisite, reported to the lead.

## Kernel reproducer (`libc-kernel`): PASS

```
libc-kernel: thread create ex ok
libc-kernel: futex wait/wake ok
libc-kernel: futex eagain ok
libc-kernel: signal delivery ok
libc-kernel: ok
init exited (0)
```

The fixture covers `THREAD_SET_TLS` followed immediately by `%fs:0x31`, `THREAD_CREATE_EX` with a
per-thread `tp`/`tid_out`/`clear_tid_addr`, private `FUTEX_WAIT`/`FUTEX_WAKE` including
`-EAGAIN`/`-ETIMEDOUT` and the clear-tid join wait, and `SIG_SETUP`/self `THREAD_KICK`/entry/
`SIGRETURN` delivery. The kernel syscalls are not the remaining blocker.

## Seven libc target cases

| case | result | class | serial evidence | follow-up owner |
|---|---|---|---|---|
| libc-hello | FAIL | libc | `FAIL auxv` (startup block/PG-50 content) and `FAIL string`; `FAILED 2` | kevin (libc startup/string) |
| libc-stdio | build link fail | libc | undefined `rewind remove stdin stdout stderr __ctype_get_mb_cur_max fwprintf fgetwc getline` | kevin (stdio) |
| libc-time | build link fail | libc | undefined `usleep sleep tzset tzname` (the S22 hard-float build now reaches the link) | kevin (time) |
| libc-threads | page fault `0x31` | libc | `libc-threads: start` then fault; the kernel `libc-kernel` TLS/THREAD_CREATE_EX probe passes, so the fault is the libc TLS init/`set_thread_pointer` path, not the S23 syscall | kevin (K25a/C57 TLS init) |
| libc-signals | page fault `0x0` | libc | `sigset sigaction raise mask setjmp` ok then fault; the kernel signal delivery probe passes | carl (C64k ucontext) |
| libc-dynamic | build link fail | libc | undefined `tcgetattr tcsetattr fgetln umask arc4random_buf posix_openpt grantpt unlockpt` | kevin (pty/termios/random; C64h adds tc*/umask) |
| libc-spawn | FAIL 7 | libc | `spawn wait wait twice enoent spawnp exit codes getpid` fail; `sigdefault`/`fork enosys` ok | kevin (spawn/libc proc) |

Observed commands (temporary worktree `e822300b`, sysroot from `cargo xtask libc-pack
--arch x86_64` + `libc-link --arch x86_64`):

```
cargo xtask qtest --case libc-hello   --arch x86_64   # fail: auxv, string
cargo xtask qtest --case libc-stdio   --arch x86_64   # error at pre: link
cargo xtask qtest --case libc-time    --arch x86_64   # error at pre: link
cargo xtask qtest --case libc-threads --arch x86_64   # fail: page fault 0x31
cargo xtask qtest --case libc-signals --arch x86_64   # fail: page fault 0x0
cargo xtask qtest --case libc-dynamic --arch x86_64   # error at pre: link
cargo xtask qtest --case libc-spawn   --arch x86_64   # fail: 7 checks
```

## Conclusion

No remaining libc-case failure is kernel-side. S23 fixed the kernel `THREAD_SET_TLS` install and
the S54 fixture proves the kernel path; the `libc-threads` 0x31 fault now belongs to libc's TLS
initialisation (`set_thread_pointer`/`SysKernel::caps`), the signals 0x0 fault to C64k ucontext,
and the other five to missing libc surfaces or libc routines. The kernel lane has a passing
regression probe; the libc owners above own the next fixes.
