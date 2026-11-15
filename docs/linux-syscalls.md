<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# linuxd syscall batches: process and thread surface (B21)

Status: B21, 2026-11-15. Evidence tags: **OBSERVED** (run on the target or read in the source),
**PROPOSED** (not implemented), **UNVERIFIED** (not exercised).

## 1. Scope

`linuxd` is the userspace Linux personality server (`architecture/09-linux-compat.md` section 3).
B20 ran a static musl program, B30 added the synthetic `/proc` and `/sys` files; B21 adds the
process and thread surface every Linux program uses before it does useful work:

| nr (x86-64) | name | state |
|---|---|---|
| 56 | `clone` | served as a documented `-ENOSYS` (blocked, section 4) |
| 59 | `execve` | initfs image replacement (section 5) |
| 61 | `wait4` | option validation and `-ECHILD` (single process; section 6) |
| 231 | `exit_group` | exit status and clear-tid store (section 6) |
| 186 | `gettid` | kernel thread id captured at thread creation (section 3) |
| 218 | `set_tid_address` | records the clear-tid word, returns the tid (section 6) |
| 273 | `set_robust_list` | records the robust-list head (section 6) |
| 157 | `prctl` | name, parent-death signal, dumpable, no-new-privs, THP, subreaper (section 7) |
| 158 | `arch_prctl` | FS base set/get; GS is `-ENOSYS` (section 7) |

The dispatcher survives an `execve`: the image is replaced, the Linux pid and the synthetic file
descriptors stay, and the same personality handler serves the new image.

## 2. Context fields

`LinuxProcess` (in `userland/servers/linuxd/src/table.rs`) is the per-process `LinuxContext`:

| Field | Meaning |
|---|---|
| `pid` | Linux pid shown to the program (`getpid`, `/proc/self/status`) |
| `tid` | kernel tid of the main thread, from `THREAD_CREATE_EX.tid_out` |
| `thread` | native thread handle (`THREAD_SET_TLS`) |
| `clear_tid` | `set_tid_address` word; zeroed at `exit_group` |
| `robust_list`, `robust_list_len` | `set_robust_list` head (24-byte UAPI head) |
| `fs_base` | user FS/TLS base installed with `THREAD_SET_TLS` |
| `pdeathsig`, `dumpable`, `no_new_privs`, `thp_disable`, `subreaper` | `prctl` state |
| `pending_exec` | an `execve` queued for the trap loop |

The trap loop owns all kernel calls: a handler records `pending_tls`, `pending_exec`,
`pending_write` or `exit`, and `main.rs` performs the operation before it replies.

## 3. `gettid`

The server creates the first Linux thread with `THREAD_CREATE_EX` (flag bit 0 of the fifth
argument, `ThreadCreateEx` in `raven-abi`) and reads the 30-bit kernel tid the kernel writes to
`tid_out`. `gettid` returns that value, so it matches the id the kernel uses for futexes, the
clear-tid word and the robust list. A server-local counter would not. **OBSERVED** in the qtest:
`linux-clone: gettid tid=6` and `set_tid_address=6 same=1`.

## 4. `clone`: the missing native primitive

Linux's `clone` child **returns from the same syscall**: its user context is the parent's, except
`rax = 0` and `rsp = child_stack` (the child TID/TLS words may also differ). The personality trap
delivers only a 58-byte `LinuxSyscallV1` — number and six raw arguments — to the handler
(`crates/raven-proto-linux`), and the kernel's two construction calls cannot recreate that context:

* `THREAD_CREATE` starts a thread at an explicit entry with `rsp = stack_top`, `rdi = arg` and every
  other general-purpose register zeroed (`kernel/src/sched/context.rs`, `enter_user`).
* `PROC_FORK` (S50b) clones the address space copy-on-write and the handle table but creates no
  thread; its caller still has to supply an entry and a stack.

A child started that way would not resume after `syscall`, so the handler answers valid requests
with `-ENOSYS` rather than returning a tid for a process that cannot run:

* flags with an unknown bit (outside the UAPI set) or a signal byte above 64: `-EINVAL`;
* `CLONE_THREAD` with a zero or misaligned child stack: `-EINVAL`;
* every valid flag set, including `clone(SIGCHLD, ...)` and the musl `pthread_create` set:
  `-ENOSYS`.

The blocker is the trap frame, not the process model: zcore's `sys_clone` clones
`thread.context_cloned()`, sets the stack, TLS and return value, and starts the thread from that
context (`zcore/linux-syscall/src/task.rs`); gVisor's `Clone` runs inside a kernel that owns the
`Task` register frame. A userspace personality needs the trap protocol to carry the user frame, or
a native "create a thread from this saved frame" call; neither exists in this tree.

**OBSERVED** in the qtest: `clone bad-flags errno=22`, `clone fork-like errno=38`,
`clone thread errno=38`, `clone misaligned errno=22`.

## 5. `execve`

`execve` replaces the Linux image:

1. The handler validates the prefetched path (absolute, bounded, NUL-terminated) and queues
   `ExecRequest { path, argv, envp }`.
2. The trap loop reads the `argv`/`envp` vectors from the caller (64 entries, 4 KiB per string,
   64 KiB total; a NULL vector is empty), loads the image from the initfs, plans it with
   `linux-elf`, maps it into a fresh native process and marks that process Linux with the same
   handler.
3. It starts the new first thread (`THREAD_CREATE_EX`), replaces the server's session, then
   terminates and reaps the old native process. The Linux pid stays the same, and the synthetic
   descriptor table moves to the new context.
4. If any step fails, the old process continues and the syscall returns `-ENOENT`, `-ENOEXEC`,
   `-E2BIG` or the native errno; nothing is torn down.

Limits: only static images present in the initfs (there is no filesystem or dynamic loader yet);
`argv[0]` is whatever the caller passed; no credential, signal or rlimit reset (none exist yet).
The synthetic descriptors move to the new context unconditionally (there is no `FD_CLOEXEC` flag
yet). The Linux pid is preserved, but the native thread is new, so `gettid` after `execve` is a new
id; Linux keeps the calling thread's tid. `execve` with a relative path is `-ENOENT` because the
server has no cwd. **OBSERVED**: `linux-clone: exec start`,
`linuxd: exec path=/exec pid=1 tid=8`, then the target's
`linux-exec: target ok pid=1 tid=8 argv1=arg-one env=CLONE_TEST=1` and `linuxd: hello exit=7`.

## 6. `exit_group`, `set_tid_address`, `set_robust_list`, `wait4`

* `set_tid_address(ptr)` records `ptr` and returns the tid. The kernel fixes a thread's clear-tid
  word at `THREAD_CREATE` and has no call to change it; the server therefore writes zero to the
  recorded word when `exit_group` runs. It cannot wake a futex waiter on that word: `FUTEX` acts
  on the calling process's address space and the server cannot run it in the child's. **PROPOSED**
  follow-up: a native clear-and-wake call.
* `set_robust_list(head, len)` records the head; `len` must be the 24-byte
  `sizeof(struct robust_list_head)`, otherwise `-EINVAL`. There is no robust-list walk at exit:
  the kernel walks the native thread's list, and this personality never installed one for the
  Linux thread. **PROPOSED** follow-up: walk the recorded head and mark owner-died futexes.
* `exit_group(code)` queues the clear-tid zero store, records the status and makes the trap loop
  terminate the native process. The server reaps it and prints the status.
* `wait4` accepts `WNOHANG` (other option bits are `-EINVAL`) and returns `-ECHILD` because no
  child can exist while `clone` is blocked. The status word and `rusage` paths arrive with a later
  clone ticket.

## 7. `prctl` and `arch_prctl`

`prctl` serves the attributes a static Linux program sets at startup:

* `PR_SET_NAME`/`PR_GET_NAME` (15/16): the 15-byte `comm`, also shown by `/proc/self/status` and
  `/proc/self/stat`; the trap loop prefetches only the `SET_NAME` pointer window.
* `PR_SET_PDEATHSIG`/`PR_GET_PDEATHSIG` (1/2): signal 0..=64; the signal is not delivered yet
  (signals are B28).
* `PR_SET_DUMPABLE`/`PR_GET_DUMPABLE` (4/3): 0, 1 or 2.
* `PR_SET_NO_NEW_PRIVS`/`PR_GET_NO_NEW_PRIVS` (38/39): only the value 1 is accepted.
* `PR_SET_THP_DISABLE`/`PR_GET_THP_DISABLE` (41/42): 0 or 1.
* `PR_SET_CHILD_SUBREAPER`/`PR_GET_CHILD_SUBREAPER` (36/37): 0 or 1.
* Any other option: `-EINVAL`.

`arch_prctl` sets and reads the x86-64 FS base through `THREAD_SET_TLS` (S42): `ARCH_SET_FS`
queues the base and the trap loop installs it in the blocked thread before the reply;
`ARCH_GET_FS` copies the stored value. `ARCH_SET_GS`/`ARCH_GET_GS` are `-ENOSYS` because the
kernel exposes no user GS base; any other code is `-EINVAL`.

## 8. Trap copy-in fix

B20/B30 prefetched every path from argument 1. That is correct for the `*at` family but wrong for
`open(2)`, `stat(2)`, `readlink(2)` and `execve(2)`, whose path is argument 0. `trap::copy_in`
now returns `(argument index, length)`; tests cover both groups. The procfs fixture used only
`openat`, so its qtest never exposed the defect.

## 9. Tests and evidence

`tests/qtest/cases/linux-clone.toml` boots the `linux-hello` profile with
`tests/fixtures/linuxd/build-clone.sh`: `/init` is `linuxd`, `/hello` is `clone.c` and `/exec` is
the `execve` target. **OBSERVED** serial evidence (`build/qtest/linux-clone.log`):

```
linux-clone: start
linux-clone: gettid tid=6
linux-clone: set_tid_address=6 same=1
linux-clone: set_robust_list ok=1
linux-clone: prctl name=clone-fixture ok=1
linux-clone: arch_prctl fs=4215128 ok=1
linux-clone: clone bad-flags errno=22
linux-clone: clone fork-like errno=38
linux-clone: clone thread errno=38
linux-clone: clone misaligned errno=22
linux-clone: wait4 child errno=10
linux-clone: wait4 wnochang errno=10
linux-clone: exec start
linuxd: exec path=/exec pid=1 tid=8
linux-exec: target ok pid=1 tid=8 argv1=arg-one env=CLONE_TEST=1
linuxd: hello exit=7
init exited (0)
```

The B20 `linux-hello` and B30 `linux-proc` qtests still pass. Host tests: 63 in `linuxd`.

### LTP cases (skipped, binary not vendored)

The LTP tree is in `ravenbsd-resources/ltp` as GPL source; no LTP binary is built for the target
and `raven-conform` (B09) only parses LTP logs. The following cases are therefore **skipped** and
are not evidence:

| Syscall | LTP cases |
|---|---|
| `clone` | `clone01`-`clone11` |
| `execve` | `execve01`-`execve06` |
| `wait4` | `wait401`-`wait403` |
| `exit_group` | `exit_group01` |
| `gettid` | `gettid01`, `gettid02` |
| `set_tid_address` | `set_tid_address01` |
| `prctl` | `prctl01`, `prctl02`, `prctl03`, `prctl05`-`prctl10` |
| `arch_prctl` | `arch_prctl01` |

The static fixture is the actual coverage for this batch.

## 10. Donors read (behaviour only)

Read for semantics, copied from none: `kerla/kernel/syscalls/{gettid,set_tid_address,arch_prctl,
wait4}.rs` (MIT/Apache-2.0), `gvisor/pkg/sentry/syscalls/linux/{sys_clone_amd64,sys_prctl}.go`
(Apache-2.0), `zcore/linux-syscall/src/task.rs` (`sys_clone`, `sys_wait4`; MIT) and
`fuchsia/src/starnix/kernel/core/task/syscalls.rs` (`sys_execve`, `sys_prctl`; BSD-3 per
`donors.list`; read only). The `clone` blocker cites zcore's `thread.context_cloned()` as the
capability the personality trap does not carry.

## 11. Provenance

First-party BSD-3-Clause code. The syscall numbers and flags come from `raven-linux-abi`
(generated from `linux-raw-sys`, Apache-2.0 OR MIT); the `prctl` option numbers were checked
against the same donor and `kerla`/`starnix` only to confirm the documented values. No donor code
was copied or translated. `PROVENANCE.toml` is unchanged.
