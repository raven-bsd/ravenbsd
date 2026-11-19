<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# aarch64 release login

Status: **BLOCKED (K159) — the aarch64 kernel has no console input path.** `getty` reads the
username from `tty:console`; the read returns 0 (EOF) before `login` is spawned. The login
start/CRT path, the loader, and `libc` are not reached. This is an aarch64 kernel console-input
gap, outside K159's Owns.

## 1. Reproduction (K159, fresh image)

The image and `login` were rebuilt from `origin/master` `5caaf5ef` (fresh, arch-correct):

```text
$ /usr/bin/time -p timeout 360 cargo xtask image --arch aarch64 --platform generic --profile release \
    --output build/aarch64/login-cold-start-release.img
real 260.29
exit 0
image sha256 19ca70beda7a62e34b0815044296fd7206db6fe92246da9e203de9a681a0bbff
login sha256 0276249510521affc0b37efac18fb6e9ad56b239f1f25a142762c15da89a8d7c
```

`timeout 120 sh scripts/login-cold-start-aarch64.sh --samples 1` exits 3 at sample 1:

```text
login-cold-start-aarch64: commit=5caaf5ef image=... sha256=19ca70be… accel=tcg smp=1 samples=1
login-cold-start-aarch64: BLOCKED stage=login-exit-before-password
RavenBSD 0.1 (aarch64) (tty:console)
login: root
process: child exited (1)
```

The serial log is `build/login-cold-start-aarch64/sample-1.log`; the harness copy is
`/tmp/opencode/k159-repro2.log`.

## 2. Instrumented trace

Temporary markers were added in `userland/rt/src/entry.rs` (start, auxv, main return), the login
`main`/`Console::open`, the getty spawn/wait, and `Console::read_line`; committed only locally
(`14ca52ab`, `ab9d5632`, `bdf25281`), then reverted. The instrumented run
(`/tmp/opencode/k159-marker-trace.log`) shows:

```text
K159 start sp=0x7fffffffee60 argc=1 argv0=Some("/bin/getty")
K159 auxv phdr=Some(…) entry=Some(…) base=Some(…) random=Some(…)
K159 call main
K159 Console::open -> Ok(Handle(33554436))
RavenBSD 0.1 (aarch64) (tty:console)
login: root
K159 read_line count 0
K159 getty read_line -> Err(Errno(5))
K159 getty session ret 1
K159 main ret 1
process: child exited (1)
```

There is **no** `K159 start … argv0=Some("login")`, no `K159 login main enter`, and no
`K159 getty run_login spawn`. The `process: child exited (1)` is **getty**, not login: getty's
`read_line` returns `EIO` and `run_session` returns 1 before it spawns `/bin/login`. The "login:
root" line is the driver's own write to the pty log; the guest never received a byte.

## 3. Root cause: no aarch64 UART receive / DEBUG_READ

The aarch64 kernel only implements polled UART **output**:

* `kernel/src/main.rs:17-18` compiles `mod console` only for `target_arch = "x86_64"`. That module
  owns the 16550 RX ring, the RX ISR/polling fallback, and the `DEBUG_READ` wait queue.
* `kernel/src/syscall/mod.rs:14-15` compiles `debug_read` only for x86-64; the dispatch arm at
  `kernel/src/syscall/mod.rs:267-268` is also `#[cfg(target_arch = "x86_64")]`. On aarch64
  `Syscall::DEBUG_READ` falls through to `basic::dispatch` and returns `ENOSYS`.
* `kernel/src/arch/aarch64/serial.rs` is a PL011 TX driver (`FR_TXFF`, `DR` writes, `CR` RX bit is
  enabled but nothing reads `DR`/`FR_RXFE`); there is no RX IRQ route and no `console::push_byte`.
* `userland/servers/consd/src/main.rs:60-69` `UartInput::read` calls `raven_rt::debug_read(out, 0)`;
  `ENOSYS` maps to `InputError::Io`. The `uart_reader` thread (`:116-137`) calls `mark_eof()` and
  exits, so `ConsoleHandler::read` (`:208-231`) returns `Ok(0)` on `console.eof()`.
* `userland/bin/login/src/lib.rs:261-268` `Console::read_line` maps a 0-byte read to `EIO`;
  `userland/bin/getty/src/lib.rs:112-135` `run_session` returns 1 on the read error. The login
  child is never spawned.

x86-64 has the complete path (`kernel/src/console.rs` + `kernel/src/syscall/debug_read.rs`), which
is why the x86 release login works. This is an aarch64 console-parity gap, not a `raven_rt`, login,
loader, or libc defect. K151/K154's "login exits before main" inference came from the kernel's
`process: child exited (1)` line; the instrumented trace shows that child is getty.

## 4. Handoff

* **dave (aarch64 kernel)**: implement PL011 RX for QEMU `virt` and Pi 4 — drain `DR` on the PL011
  RX interrupt, route it through the aarch64 GIC (FDT-provided IRQ), feed the shared console ring,
  and expose `DEBUG_READ` on aarch64 with the same block/timeout/wake semantics as
  `kernel/src/console.rs` + `kernel/src/syscall/debug_read.rs`.
* **kevin (kernel console/consd integration)**: wire the aarch64 `DEBUG_READ` dispatch and verify
  consd's `uart_reader` no longer marks EOF.
* No login start/CRT change is needed. `tests/qtest/cases/login-aarch64.toml` stays on
  `origin/phil/K151` and must not land until the 5-sample cold start passes.

## 5. Prior staging evidence (K153/K154, unchanged)

The aarch64 staging and loader work remains valid:

* `profiles/shell.d/72-uutils.toml` resolves `build/{arch}/uutils/*` (K153).
* `build/aarch64/user/login` is AArch64 DYN/PIE with `PT_PHDR`, an `INTERP`, `NEEDED libc.so.1`,
  and the R/R E/RW segment order of x86-64; `build/aarch64/user/raven-getty` runs dynamically.
* K154 traced the loader stages (`selfrel`, `main`, `premap-ok`, `needed`, `tls`, `reloc`, `enter`)
  and found no loader failure. The failing stage is after the loader enters the user image, in
  the console read path documented above.

The image build is deterministic: the fresh login hash is
`0276249510521affc0b37efac18fb6e9ad56b239f1f25a142762c15da89a8d7c`, matching K154's build.
K159's marker image had login `97332f6a32b1f52a4dc4acb7fd32491dbae2a7fb11cc723190c3758487abaef7`
and getty `6690004a0a33434197a79bba5d64e3ea75107f0ea67e5bb51b2feb663c230d3e`; both markers are
reverted in the branch.

## 6. Acceptance

```text
timeout 120 sh scripts/login-cold-start-aarch64.sh --samples 1
  exit 3: BLOCKED stage=login-exit-before-password (getty read_line EIO)

timeout 120 cargo xtask qtest --case login-retry --arch x86_64
  (recorded in the K159 report; x86 path unchanged)

timeout 240 make check-fast
  (recorded in the K159 report)
```

The 5-sample aarch64 gate cannot pass until the kernel RX path lands.

## 7. Donor reconciliation (DR-52)

| Topic | Illumos finding | BSD file/licence | Linux finding | Decision |
|---|---|---|---|---|
| aarch64 console input | Illumos console input is a kernel tty layer fed by the UART driver; behaviour only | NetBSD/FreeBSD PL011 drivers own RX FIFO drain and tty input; permissive BSD references, no code copied | Linux PL011 amba-pl011 owns RX IRQ/FIFO and tty flip buffer; read-only | Implement an original Rust PL011 RX path in the aarch64 kernel; no donor code copied or translated |

Prior build/staging work: first-party RavenBSD build code; no donor code read or copied, no C, no
CDDL/GPL.
