<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# QEMU USB keyboard end-to-end (S63)

Status: **PASS** on QEMU's `qemu-xhci` + `usb-kbd`. The guest enumerates the keyboard through
the S61 USB core, selects the HID boot protocol, receives the QMP key chord `h i shift+b Enter`,
and the xHCI driver drains the S62 console queue. The guest transcript reports four keys and the
`hiB\n` bytes (`[68, 69, 42, 0a]`). No Pi board claim; the DWC3/real-hardware path stays
UNVERIFIED.

## QMP side channel

QEMU keeps using `-serial stdio` for the guest console. The case adds a QMP socket through
`qemu_extra`:

```
-qmp unix:build/qtest/usb-keyboard.qmp,server=on,wait=off
```

A `[[step]]` `send` value beginning with `@qmp:<socket>:<qcode,...>` is intercepted by the qtest
runner and sent as one QMP `send-key` command instead of being written to serial:

```
[[step]]
wait = '^usb-keyboard: boot protocol ok$'
send = '@qmp:build/qtest/usb-keyboard.qmp:h,i,shift,b,ret'
delay_ms = 0
```

The runner connects to QMP immediately after QEMU starts, performs the `qmp_capabilities`
handshake and sends the chord with a 500 ms hold. The socket is removed before a run (`pre` does
`rm -f`), because QEMU's `server=on` refuses a stale socket.

The chord arrives as one HID report per key transition (`h`, then `h i`, then shift, then
`h i b`, then Enter). The driver's bounded drain loop consumes up to eight reports.

## Driver fix

The first end-to-end attempt stalled EP1. QEMU's trace showed:

```
usb_packet_state_change ... ep 1 ... state undef -> setup
usb_xhci_xfer_error ... ret -3
usb_xhci_queue_event ... CC_STALL_ERROR
usb_xhci_ep_state ... running -> halted
```

The input context encoded EP1 with EP Type **3** (Interrupt OUT) even though DCI 3 is EP1 IN.
`crates/raven-xhci/src/context.rs` now encodes EP Type **7** (Interrupt IN) and the context test
asserts the new value.

The driver (`userland/drivers/xhci/src/main.rs`) now:

1. calls `Controller::wait_interrupt()` after the boot protocol primes EP1, acknowledging the
   controller interrupt and advancing the event-ring dequeue pointer;
2. polls the keyboard service until four console bytes are queued (bounded to eight reports);
3. drains `KeyboardService::read_console` and prints the bytes as a hex console/TTY transcript.

The S62 `ConsoleInput` queue remains the seam; no HID or USB-core source was changed.

## Guest transcript

```
xhci: hid interface=0 report=63 ep=0x81
usb-keyboard: boot protocol ok
usb-keyboard: enumerated keys=4
usb-keyboard: console hex=[68, 69, 42, 0a]
usb-keyboard: ok
devd: driver xhci cycle 1 exit=0
svcd: settled
init exited (0)
```

## Commands

```
cargo test -p raven-qtest
cargo nextest run -p raven-xhci
cargo xtask qtest --case usb-keyboard --arch x86_64
make check-fast
```

The case forbids `panic`, `EXCEPTION`, `usb-keyboard: fail` and
`usb-keyboard: enumerated keys=0`, so the zero-key regression cannot pass silently.
