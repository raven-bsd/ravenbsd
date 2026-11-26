<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# raven-libc-signal

Everything signal-shaped in ravenlibc except the kernel: the 128-byte `sigset_t` and its name
table, the process disposition mirror behind `sigaction`/`signal`/the legacy calls, per-thread
masks, the illumos-inspired deferral of signals in critical sections,
`raise`/`kill`/`wait`/`sigaltstack`/`psig*`/`sig2str`/`str2sig`, the `setjmp` family and
`getcontext`/`setcontext`/`makecontext`/`swapcontext` for x86-64, AArch64 and RISC-V.

Design: `architecture/raven-libc/donor-map/09-threads-tls-signals.md` sections 3.7, 3.8, 4.1, 4.8;
`architecture/raven-libc/06-solaris-design.md` 3.4 (A-sol-19/20/21/22/23); ticket C58. The illumos
files named by the ticket were read for behaviour only; no illumos code, identifier or structure is
present.

## Delivery

C19 owns the caught-signal frame, the sigcode trampoline and `SIGRETURN`; libc registers no user
entry. `sigaction`, `sigprocmask` and `sigaltstack` are thin wrappers over the kernel
`SIGACTION`/`SIGPROCMASK`/`SIGALTSTACK` provider, and the libc disposition table is only a mirror
for the legacy getters. The kernel derives the handler arguments, applies the mask and default
action, and resumes the interrupted context through the sigcode.

The libc direct path remains only for a synchronous wait that cannot use the kernel entry
(`ppoll` with a temporary mask): it saves `errno`, computes the deliverable set, honours the
critical-section deferral, applies the default action or calls the handler on the selected stack,
and restores the mask. The handler stack is the alternate stack for `SA_ONSTACK`, else the caller's
stack below the x86-64 red zone. `call_handler` enters the function with 16-byte alignment.

## Capability matrix (production today)

On the Raven target (`cfg!(target_os = "none")`) `SysKernel::caps()` reports the committed PG-8
interruptible wait; the host build keeps every kernel bit false because the unit tests have no
Raven kernel behind them.

| Capability | Target | Host | Meaning |
|---|---|---|---|
| `kick` | yes | no | `THREAD_KICK` sets the sticky word and wakes an interruptible wait |
| `eintr` | yes | no | an interruptible wait returns `-EINTR` for a deliverable signal |
| `proc_kill` | yes | no | `kill`/`killpg`/`sigqueue` through the process signal queue |
| `resend` | no | no | no `SIGRESEND`; a deferred replay uses restore-mask + pending bit + self kick |
| `stop` | no | no | no PG-10 stop/continue |
| `fp_in_frame` | no | no | no PG-12 FP/SIMD state in the signal frame |

A blocked signal stays in the kernel queue; `sigprocmask` re-checks with a fresh kick after an
unblock. The kick bits `CHECK=1, CANCEL=2, SYNCCALL=4, TIMER=8` are PROPOSED (RD-new 2) and
nothing else is set.

The on-target `libc-signal-chld` case (`tests/libc/c/libc-signal-chld.c`, qtest
`tests/qtest/cases/libc-signal-chld.toml`) proves the entry: a blocked SIGCHLD is delivered when
the mask is restored, and a child exit interrupts a blocking `waitpid` with `EINTR` while the
handler runs and the retried wait reaps.

## Soft-float

`raven_va_softfloat` is the one soft-float profile cfg (H-20), and the target-feature check makes
`aarch64-unknown-none-softfloat` select it automatically. `jmp_buf` stays 392 bytes and
`ucontext_t` keeps the Linux sizes; the FP slots (`d8-d15`, `FPCR/FPSR`, `fs0-fs11`, `fcsr`,
MXCSR/x87 CW) are written only in the hard-float profile and stay zero otherwise.

## Tests

```
cargo test -p raven-libc-signal                       # 359 tests: 160 unit + 199 integration
cargo test -p raven-libc-signal --test asm_x86_64     # the x86-64 asm, RUN on the host
cargo build -p raven-libc-signal --target aarch64-unknown-none-softfloat
cargo build -p raven-libc-signal --target riscv64gc-unknown-none-elf
cargo xtask libc-oracle diff signal-musl              # layer A, C, D
cargo xtask libc-oracle diff signal-freebsd           # layer B
```

The x86-64 assembly is executed by `tests/asm_x86_64.rs`: setjmp/longjmp with all callee-saved
registers, nested levels, `sigsetjmp` with/without the mask, a getcontext/setcontext loop,
makecontext with 0..6 arguments and both `uc_link` cases, a 1,000-iteration `swapcontext`
ping-pong and `call_handler` on a separate stack. **AArch64 and RISC-V were only built**, not
executed: the reference machine has no `qemu-aarch64`/`qemu-riscv64` (only `qemu-system-*`); the
`llvm-objdump -d` text of every signal assembly symbol is in `tests/golden/`.

`data/async-signal-safe.txt` carries a `VERIFIED:`/`UNVERIFIED:` mark on every line; the source
for the verified rows is the local `man-pages-5.10 signal-safety(7)` page plus the POSIX.1-2024
additions named by the ticket. `data/not-async-signal-safe.txt` is checked by a grep test over the
signal paths.
