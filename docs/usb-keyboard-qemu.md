<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# QEMU USB keyboard end-to-end (S63)

Status: the qtest harness now has a QMP side channel and the `usb-keyboard` case injects a key
chord. The S67 xHCI driver still polls once and exits, so the current transcript reports
`usb-keyboard: enumerated keys=0`; the exact driver follow-up is recorded below. No Pi board claim.

Rev 39 merged `origin/master` `95e72780` (S61/S67/S68); the QMP harness and this limit are
unchanged. `cargo test -p raven-qtest` passes 173 tests, and the host gate is green, but the
end-to-end case is BLOCKED on the driver's single report poll and its missing console/TTY drain.

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
wait = '^xhci: hid interface='
send = '@qmp:build/qtest/usb-keyboard.qmp:h,i,shift,b,ret'
delay_ms = 0
```

The runner connects to QMP immediately after QEMU starts, performs the `qmp_capabilities`
handshake and sends the chord with a 500 ms hold. QEMU's trace (`input_event_key_qcode`) confirms
the key event reaches the input subsystem. The QMP socket path must be removed before a
run (`pre` does `rm -f`), because QEMU's `server=on` refuses a stale socket.

## Current limit

The S67 driver (`userland/drivers/xhci/src/main.rs`, `run`) does:

```
let queued = service.poll().map_err(hid_text)?;
println!("usb-keyboard: enumerated keys={queued}");
```

`KeyboardService::poll` performs one `next_report`; if the QMP key arrives after that call, or the
driver exits before the interrupt-IN completion, the queue stays empty. The observed transcript is
`keys=0`. The driver must be changed (outside S63's Owns) to poll for a bounded interval, drain
`service.read_console` and print the received bytes (`hiB\n`). Once that lands, the case can keep
its `keys=4` expectation and the report can include the key transcript.

## Commands

```
cargo test -p raven-qtest
cargo xtask qtest --case usb-keyboard --arch x86_64
make check-fast
```

The qtest currently fails only the `keys=4` expectation; the QMP step itself completes and the
QEMU trace shows the key event. The host gate is independent of the qtest key assertion.

Current guest transcript:

```
xhci: hid interface=0 report=63 ep=0x81
usb-keyboard: boot protocol ok
usb-keyboard: enumerated keys=0
usb-keyboard: ok
...
--- unmatched expectations ---
^usb-keyboard: enumerated keys=4$
```
