<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Linux trap frame for clone (B21a)

Status: B21a, 2026-11-15. Evidence tags: **OBSERVED** (run on the target or read in the source),
**PROPOSED** (not implemented), **UNVERIFIED** (not exercised).

## 1. Why

`clone` must let a child resume at the instruction after the `syscall` with the parent's registers
and a different stack. B21 could not implement it because the trap packet carried only the number
and six arguments. B21a adds the saved user frame to the trap and lets a handler reply with a frame
that resumes the blocked thread at a chosen RIP/RSP.

## 2. Wire format

The version-1 syscall request already uses 58 of the scheme packet's 64 inline data bytes, so the
frame travels in the packet's **`path` field** (256 bytes) beside the unchanged request:

* `data`: [`LinuxSyscallV1`] — version 1, arch, number, six arguments (58 bytes);
* `path[0..160]`: frame envelope — magic `LFRM`, frame version 1, 3 reserved bytes, then 19
  little-endian u64 words:
  `RIP, RSP, RFLAGS, FS_BASE, RAX, RBX, RCX, RDX, RSI, RDI, RBP, R8, R9, R10, R11, R12, R13,
  R14, R15`.

The reply keeps its 9-byte [`LinuxReplyV1`] in `data`. A handler that wants to resume the thread
writes the same envelope into the reply packet's `path`; the kernel applies it before `sysretq`.
An all-zero path (or no magic) means no resume.

The syscall header stays version 1 on purpose: an older `linuxd` decodes the request, ignores the
path, and echoes it; the kernel compares the reply frame with the captured one and ignores an
unchanged echo, so existing servers keep working. The **frame envelope** is versioned
independently (`FRAME_VERSION`). A future packet ABI that enlarges inline data can move the frame
into `data` without changing the kernel logic.

## 3. Kernel hook

`kernel/src/syscall/mod.rs` (`syscall_dispatch`) sends x86-64 Linux-personality calls to
`linux::trap_x86_64` with the mutable `TrapFrame`; native calls and the aarch64 compact path keep
`dispatch`. `kernel/src/syscall/linux.rs`:

* captures `rip`, `rsp`, `rflags` and the general registers from `TrapFrame`, and `fs_base` from
  `sched::current_thread().tls_base()`;
* encodes the frame into the request packet path without touching `data_len` (the Open path setter
  rewrites `data_len`, so the bytes are written directly);
* waits for the handler and decodes the reply plus an optional frame;
* applies a changed reply frame: validates RIP/RSP as non-zero user addresses, masks RFLAGS to the
  user-visible bits and forces IF/bit 1, restores all GPRs, sets the thread TLS base and installs
  it immediately (`sched::context::install_tls_base`).

An unchanged echoed frame is not applied: an older handler that returns the request packet must not
undo state a syscall already changed (notably `arch_prctl`'s TLS base installed by `linuxd` before
its reply).

## 4. qtest evidence

`tests/qtest/cases/linux-frame.toml` boots `tests/fixtures/linux-frame`:

* the controller spawns the handler and the Linux child and maps a shared resume page into the
  child;
* the child opens `linux:`, sets a nonzero TLS base, sets the Linux personality and issues custom
  syscall 7777 with the target RIP and stack;
* the handler sees the frame, prints it, and replies with a frame whose RIP/RSP are the requested
  target/stack;
* the child resumes at that RIP on the new stack and writes the shared resume magic; the controller
  observes it, terminates and reaps the child.

**OBSERVED** serial output (`build/qtest/linux-frame.log`):

```
linux-frame: handler nr=7777 rip=0x400449 rsp=0x7fffffffe360 fs=0x7000
linux-frame: handler target=0x400000 stack=0x410000
linux-frame: resumed
process: child exited (0)
linux-frame: child exit=0
process: child exited (0)
linux-frame: done
init exited (0)
```

The original RIP/RSP and the nonzero FS base are captured; the target RIP/RSP are applied and the
child proves it continued there.

## 5. Compatibility and limits

* `linux-hello`, `linux-proc` and `linux-root` still pass with the frame extension; the old
  handlers ignore the request path and the kernel skips their echoed frame.
* aarch64 frame capture parity is S56's follow-up; aarch64 keeps the compact trap and sends no
  frame.
* The frame carries the general registers, RIP/RSP/RFLAGS and FS base; segment registers, FP/SIMD
  state and debug registers are not included. The kernel's FPU state is per-thread and unchanged
  across a resume.
* The path field is 256 bytes; the frame uses 160, leaving room for future fields.
* The consumer (linuxd `clone`) is B21b: it will decode the request frame, create/fork the native
  process, and reply with the child's frame (RIP = parent's return address, RSP = child stack,
  RAX = 0, child TLS).

## 6. Provenance

First-party BSD-3-Clause code. No donor code was copied. The register order is the x86-64 Linux
UAPI/GPR convention documented in the ticket and the kernel's `TrapFrame`; the wire values are this
project's protocol, not a copied Linux structure.
