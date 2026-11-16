# Linux syscall coverage: clone (B21b)

Status: B21b, 2026-11-16. Evidence tags: **OBSERVED** (run on the target), **PROPOSED** (not
implemented), **UNVERIFIED** (not exercised).

## 1. Scope

B21b implements `clone` in `linuxd` from the B21a trap frame. The supporting process/thread calls
needed by the static fixture are served too: `gettid`, `set_tid_address`, `set_robust_list`,
`prctl` (`PR_SET_NAME`/`PR_GET_NAME`) and the existing `arch_prctl`.

`clone` covers two shapes:

| shape | flags | native mechanism | child return |
|---|---|---|---|
| thread | `CLONE_VM\|CLONE_THREAD\|CLONE_SIGHAND` | `THREAD_CREATE_EX` in the same process | `rax = 0`, new rsp |
| fork-like | signal byte only (e.g. `SIGCHLD`) | `PROC_FORK` + `THREAD_CREATE_EX` | `rax = 0`, COW stack |

`CLONE_SETTLS`, `CLONE_PARENT_SETTID`, `CLONE_CHILD_SETTID` and `CLONE_CHILD_CLEARTID` are
honoured. Unknown flags, `CLONE_THREAD` without `CLONE_VM`/`CLONE_SIGHAND`, sharing flags without
`CLONE_VM`, a zero/misaligned thread stack, and a fork-like `CLONE_CHILD_SETTID` are `EINVAL`; a
valid `clone` trap without a B21a frame is `ENOSYS`.

## 2. Child frame and trampoline

The kernel delivers the saved user frame in the scheme packet's path field (B21a). linuxd copies it,
sets `gprs[RAX] = 0` and chooses `rsp`:

* thread: the caller's `child_stack` (already 16-byte aligned);
* fork-like: the captured parent `rsp` (the COW copy contains the same stack), unless the caller
  supplied an explicit stack.

linuxd then builds one executable page containing a RIP-relative trampoline and the frame table.
The code loads all 15 general registers and `rsp` from the page, then jumps through a
`jmp qword ptr [rip+disp32]` to the captured RIP. It never touches the stack, so the eight bytes
below the child's `rsp` (the SysV red zone) survive; the `ret` in the fixture's raw-clone stub sees
exactly the parent's stack.

The child TLS base is installed at thread creation (`THREAD_CREATE_EX.tp`); the child does not need
an `arch_prctl` trap.

## 3. Addresses and the signal page

`linuxd` maps one shared, zero-filled page at `0x0000_4800_0000_0000` before the Linux thread
starts. A clone child cannot be attributed to a process by the trap packet (the packet carries no
sender identity), so the fixture children signal the parent through this page and spin instead of
making a syscall. A fork-like child inherits the shared mapping through `PROC_FORK`; a thread child
shares it directly.

Each clone maps a fresh trampoline page at `0x0000_4800_0000_1000 + n * 4096`, after creating the
memory object and populating it through a shared write mapping in linuxd. The target mapping is
created read/write and changed to read/execute with `PROTECT`, because generic memory objects carry
no `EXEC` right in ABI v1.

## 4. Ownership and cleanup

`LinuxProcess` records every native task in `clones` with its Linux pid/tid and native thread id.
When the parent exits (`exit_group`), linuxd terminates and reaps every fork-like child process,
closes the thread handles, and then terminates the Linux process. A thread child needs no separate
termination: terminating the process kills all of its threads.

`CLONE_CHILD_CLEARTID` is passed to the kernel as the new thread's clear-on-exit word. The main
thread's `set_tid_address` word is cleared by the trap loop at `exit_group`, because the thread was
created before the program called `set_tid_address` and the kernel has no later setter.

## 5. Evidence

**OBSERVED** from `build/qtest/linux-clone.log`:

```
linux-clone: gettid tid=6
linux-clone: set_tid_address=6 same=1
linux-clone: set_robust_list ok=1
linux-clone: prctl name=clone-fixture ok=1
linux-clone: arch_prctl fs=4280664 ok=1
linux-clone: clone bad-flags errno=22
linux-clone: clone thread pid=2 child=0
linux-clone: clone thread flag ok
linux-clone: clone fork pid=3 child=0
linux-clone: clone fork flag ok
linux-clone: done
linuxd: hello exit=0
init exited (0)
```

The thread child and the fork child both resumed at the parent's return RIP with `rax = 0` and
wrote the shared page. The fork child was terminated and reaped when the parent exited.

## 6. Limits

* **Trap attribution**: the scheme packet identifies the handler, not the sending process. A clone
  child that made a syscall would be served as its parent. B21b's children signal through the
  shared page and do not trap; per-task attribution is the B21 follow-up.
* `clone` with `CLONE_VM` but without `CLONE_THREAD` (a shared-VM process), namespaces, `CLONE_VFORK`,
  `CLONE_PIDFD` and the other unsupported bits are `EINVAL`, not silently accepted.
* `wait4` is not part of B21b; a fork-like child is reaped by linuxd at parent exit.
* The task-name attributes beyond `PR_SET_NAME`/`PR_GET_NAME` are `EINVAL`; B21 owns the full
  process/thread batch.
* aarch64 keeps the compact trap without a frame; `clone` there is `ENOSYS` until S56 adds frame
  parity.

## 7. Provenance

First-party BSD-3-Clause code. No donor code was copied. The flag values and `ThreadCreateEx`
layout come from `raven-linux-abi` and `raven-abi`; the frame and trampoline are this project's
protocol and user-space code.
