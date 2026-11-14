# raven-libc-signal

Everything signal-shaped in ravenlibc except the kernel: the 128-byte `sigset_t` and its name
table, the process disposition table behind `sigaction`/`signal`/the legacy calls, per-thread
masks, the interposed entry path, the illumos-inspired deferral of signals in critical sections,
`raise`/`kill`/`wait`/`sigaltstack`/`psig*`/`sig2str`/`str2sig`, the `setjmp` family and
`getcontext`/`setcontext`/`makecontext`/`swapcontext` for x86-64, AArch64 and RISC-V.

Design: `architecture/raven-libc/donor-map/09-threads-tls-signals.md` sections 3.7, 3.8, 4.1, 4.8;
`architecture/raven-libc/06-solaris-design.md` 3.4 (A-sol-19/20/21/22/23); ticket C58. The illumos
files named by the ticket were read for behaviour only; no illumos code, identifier or structure is
present.

## The twelve-step entry path

1. The per-architecture `__raven_sigentry` asm stub aligns `sp`, reads the shell-installed entry
   function cell and calls it with the kick bits and the optional fault record.
2. `signal_entry` saves `errno` and COPY-INs the kernel-saved `mcontext_t` area into a
   `ucontext_t` on the current stack. The area is free from that moment.
3. Kick bit `CANCEL`: the C57 hook (`CancelHook`) may rewrite `pc` inside the `syscall_cp` window.
4. Kick bit `SYNCCALL`: the C57 `synccall` callback runs.
5. `deliverable = (thread_pending | process_pending) & !mask & APP_MASK`; a synchronous fault maps
   to one signal and ignores the mask.
6. Inside a critical section the lowest deliverable signal is recorded and `SIGRETURN` returns to
   the interrupted code; the handler is never called from `critical_exit`.
7. The lowest signal wins (standard before real-time; procd keeps FIFO per number), its pending bit
   is cleared and the disposition is read through the lock-free seqlock.
8. `SIG_IGN` drops it; `SIG_DFL` applies the table (terminate, core, ignore, stop-as-ignore,
   continue); a function handler is delivered.
9. The handler stack is chosen (`SA_ONSTACK` + an enabled alternate stack, else the interrupted
   `sp` minus the x86-64 red zone), `siginfo_t` and `ucontext_t` are built, `sa_mask` is applied as
   `old | sa_mask | (sig unless SA_NODEFER)`, and `SA_RESETHAND` writes `SIG_DFL` **before** the
   call.
10. The handler runs through `__rl_call_handler` with 16-byte alignment.
11. The mask is restored from the possibly handler-modified `uc_sigmask`, and the loop returns to
    step 5.
12. `errno` is restored, the (possibly modified) machine context is COPY-OUT to the kernel area,
    and `SIGRETURN` (0x82, the labelled PROPOSED PG-7 number) restores the interrupted context.

COPY-IN/COPY-OUT is what makes nested kicks and `siglongjmp` out of a handler safe with one kernel
area: once copied in, the area may be reused; the handler's `ucontext_t` is a live stack object.

## Capability matrix (production today)

`SysKernel::caps()` is all false: no PG-7 (`SIG_SETUP`, `THREAD_KICK`, `SIGRETURN`,
`SIGRESEND`), no PG-8 (`-EINTR`/sticky kick), no PG-10 (stop), no PG-12 (FP in the frame), no
PG-44 (procd kill). Consequences: `sigaction` stores dispositions; `raise` and self-targeted
`pthread_kill` run the handler through the libc-only direct entry when unmasked; `kill` to another
process is `ENOSYS`; asynchronous delivery and `SIGSTOP` are absent; no FP/SIMD state is saved
(PG-12/PG-50). The kick bits `CHECK=1, CANCEL=2, SYNCCALL=4, TIMER=8` are PROPOSED (RD-new 2) and
nothing else is set.

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
`llvm-objdump -d` text of every entry symbol is in `tests/golden/`.

`data/async-signal-safe.txt` carries a `VERIFIED:`/`UNVERIFIED:` mark on every line; the source
for the verified rows is the local `man-pages-5.10 signal-safety(7)` page plus the POSIX.1-2024
additions named by the ticket. `data/not-async-signal-safe.txt` is checked by a grep test over the
signal paths.
