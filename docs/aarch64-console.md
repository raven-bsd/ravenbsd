<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# aarch64 console receive

## Path

1. `kmain` initialises the GIC, then calls `arch::aarch64::console::init(uart_irq)` before it
   unmasks interrupts. `fdt::uart_irq` reports the PL011 SPI from the live DTB (first cell group
   with type `0`: SPI number + 32), or falls back to the PL011 base address: QEMU `virt`
   `0x0900_0000` -> GIC id 33, Pi 4 `0xFE20_1000` -> GIC id 153.
2. `console::init` configures the SPI as level or edge from the DT flags, targets CPU 0, sets
   priority `0xa0`, enables it and sets `IMSC.RXIM`. `irq::set_uart_irq` records the id so the
   dispatcher can route it.
3. The IRQ dispatcher (`irq::dispatch`) calls `console::handle_irq` for that id. The handler drains
   `DR` while `FR.RXFE` is clear (bounded to 256 bytes), clears `ICR.RXIM`, and wakes one
   `DEBUG_READ` waiter. The dispatcher then sends EOI and runs the normal tick/preemption path.
4. `DEBUG_READ` is enabled on aarch64 and uses the same chunk/block/timeout/wake contract as
   x86-64: it copies up to 256 bytes from the lock-free ring and returns `0` on timeout.
5. `consd` no longer marks the console EOF on a transient backend error; it backs off 10 ms and
   keeps the reader alive, so getty can wait for the next line.

## Register semantics (OBSERVED)

| Register | Offset | Use |
|---|---|---|
| `DR` | `0x00` | pop one received byte |
| `FR` | `0x18` | bit 4 `RXFE`: FIFO not empty when clear |
| `IMSC` | `0x38` | bit 4 `RXIM`: receive interrupt mask |
| `ICR` | `0x44` | write-one-to-clear of the receive interrupt |

The BSD bhyve PL011 model in the donor checkout uses the same register indices (`UARTFR 0x06`,
`UARTIMSC 0x0e`, `UARTICR 0x11` at a four-byte stride) and asserts `UARTRXINTR` when the RX FIFO
reaches its trigger level; it clears the state on `ICR`. That is the level-triggered shape
implemented here. No donor code was copied.

## Evidence

`tests/qtest/cases/aarch64-console-rx.toml` boots the release image, matches
`console: PL011 rx irq 33 (level)`, types `root` and `raven`, and matches `Password:` plus the
`RavenBSD 0.1.0` motd. `login-aarch64` additionally requires the session shell and currently
fails at `login: unable to start the session shell` because the staged aarch64 `brush` is not
page-aligned for `spawn(Strict)` (see the K166 report for the handoff).
