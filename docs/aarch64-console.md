<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# aarch64 console receive

## Path

1. `kmain` initialises the GIC, then calls `arch::aarch64::console::init(uart_irq)` before it
   unmasks interrupts. `fdt::uart_irq` reports the PL011 SPI from the live DTB (first cell group
   with type `0`: SPI number + 32), or falls back to the PL011 base address: QEMU `virt`
   `0x0900_0000` -> GIC id 33, Pi 4 `0xFE20_1000` -> GIC id 153.
2. `console::init` configures the SPI as level or edge from the DT flags, targets CPU 0 (index
   `0`, converted by `gic::set_target` to the GICv2 `ITARGETSR` mask `1`), sets priority `0xa0`,
   enables it and sets `IMSC.RXIM | IMSC.RTIM`. `irq::set_uart_irq` records the id so the
   dispatcher can route it.
3. `serial::init` selects the 1/8-full RX FIFO trigger (`IFLS.RXIFLSEL = 0`); the receive-timeout
   interrupt covers a FIFO that is non-empty but below the trigger. The IRQ dispatcher
   (`irq::dispatch`) calls `console::handle_irq` for that id. The handler clears `ICR.RXIC |
   ICR.RTIC`, drains `DR` while `FR.RXFE` is clear (bounded to 256 bytes), clears the cause again
   when the FIFO is empty, and wakes one `DEBUG_READ` waiter. If the drain bound is reached the
   cause is left set so the level-triggered GIC re-enters for the remaining bytes.
4. `DEBUG_READ` is enabled on aarch64 and uses the same chunk/block/timeout/wake contract as
   x86-64: it copies up to 256 bytes from the lock-free ring and returns `0` on timeout.
5. `consd` ends the console on a real `EndOfFile`/`Closed`, but a transient `Io` error is retried
   at most 50 times with a 10 ms backoff; a persistent failure is logged and the console is
   closed instead of looking live forever.

## Register semantics (OBSERVED)

| Register | Offset | Use |
|---|---|---|
| `DR` | `0x00` | pop one received byte |
| `FR` | `0x18` | bit 4 `RXFE`: FIFO not empty when clear |
| `IFLS` | `0x34` | bits 5:3 `RXIFLSEL`: 0 = 1/8-full receive trigger |
| `IMSC` | `0x38` | bit 4 `RXIM` receive, bit 6 `RTIM` receive timeout |
| `ICR` | `0x44` | write-one-to-clear of the RX and RT causes |

The BSD bhyve PL011 model in the donor checkout uses the same register indices (`UARTFR 0x06`,
`UARTIMSC 0x0e`, `UARTICR 0x11` at a four-byte stride) and asserts `UARTRXINTR` when the RX FIFO
reaches its trigger level; it clears the state on `ICR`. That is the level-triggered shape
implemented here. No donor code was copied.

## Evidence

`tests/qtest/cases/aarch64-console-rx.toml` boots the release image, matches
`console: PL011 rx irq 33 (level)`, types a 32-byte burst (two FIFO fills) at the login prompt
and matches `Login incorrect`, then types `r` on its own (receive-timeout path) followed by
`oot`, and matches `Password:` plus the `RavenBSD 0.1.0` motd after `raven`. `login-aarch64`
additionally requires the session shell and stays red until K169 links the aarch64 `brush` with
page-aligned segments (see the K166 report for the handoff).
