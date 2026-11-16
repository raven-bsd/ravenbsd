<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# USB HID keyboard integration (S62)

OBSERVED on `stuart/S62` (base `origin/master` `42f64650`). Host + qtest evidence with a fake USB
core; no hardware claim and no real USB enumeration.

## Path

```
USB core (fake in this ticket, S61 xHCI in the follow-up)
  -> TransferRing::next_report()  (8-byte boot report)
  -> raven-usb-hid::BootKeyboard   (S60 descriptor/report decoder)
  -> usb_hid::KeyboardService      (S62 integration)
  -> usb_hid::ConsoleInput         (bounded 256-byte console queue)
  -> TTY/console consumer          (fake in the qtest; consd binding is a follow-up)
```

`userland/drivers/usb-hid` is now a library plus the utility skeleton:

* `ConsoleInput` — a fixed 256-byte single-reader ring with `push`/`pop`/`read`/`clear`.
* `KeyboardService<R: TransferRing>` — `init` selects the boot protocol and validates the report
  descriptor; `poll` decodes one completed report and pushes the pressed-key ASCII; releases,
  modifiers and non-ASCII keys produce no console bytes. `read_console` drains the queue.
* `BOOT_KEYBOARD_DESCRIPTOR` — the standard 63-byte boot descriptor used by the fake core and the
  bring-up tests; a real driver fetches the device's descriptor over `GET_DESCRIPTOR(0x22)`.

`usb-hid` still keeps `#![forbid(unsafe_code)]`; the xHCI/RP1 crates are untouched and
`crates/raven-usb-core/**` is not edited (S61).

## Key-event mapping

The S60 decoder's events feed the queue only on press:

| HID usage | Report | Console byte |
|---|---|---|
| `0x0B` | `h` | `h` |
| `0x0C` | `i` | `i` |
| `0x05` + LShift | shift+b | `B` |
| `0x28` | Enter | `0x0a` |
| `0x2A` | Backspace | `0x08` |
| `0x2B` | Tab | `0x09` |
| `0x2C` | Space | `0x20` |

Caps Lock, Shift and the other modifiers change the ASCII of their sibling key events but are not
queued themselves; releases never queue. ErrorRollOver reports are ignored by the decoder.

## QEMU qtest (fake USB core)

`tests/fixtures/usb-keyboard/` runs the service with a `FakeUsbCore` implementing
`TransferRing`: it counts the two setup requests (`SET_PROTOCOL(boot)`, `SET_IDLE`), hands over
the standard boot descriptor, and completes eight scripted boot reports (h, i, shift+b, Enter,
with releases). The fixture drains the console queue and checks `hiB\n` and the interrupt-IN
resubmit count (init + one per report = 9).

```
$ cargo xtask qtest --case usb-keyboard --arch x86_64
usb-keyboard: pass
exit 0
```

Serial:

```
usb-keyboard: boot protocol ok
usb-keyboard: keys=hiB
usb-keyboard: ok
init exited (0)
```

`build/qtest/usb-keyboard.json`: result `pass`, four matched expectations, no unmatched or
forbidden hits.

## Host evidence

```
$ cargo test -p usb-hid
running 8 tests
test result: ok. 8 passed; 0 failed
```

Coverage: queue FIFO/wrap/full/clear; service init control sequence and first interrupt-IN prime;
report-to-console bytes (`hiB\n`); resubmit count; poll-before-init `NotInitialized`; non-keyboard
descriptor rejection. `cargo test -p raven-usb-hid` (the S60 crate, 28 tests) is unchanged and
still passes.

## S61 follow-up

The real `qemu-xhci` + `usb-kbd` run and Pi 4/5 enumeration need S61's USB core and the xHCI
transfer-ring adapter:

* implement `TransferRing` over the xHCI control/interrupt rings (buffer ownership until the
  completion event);
* enumerate the device, fetch the report descriptor with `GET_DESCRIPTOR(0x22)`, then call
  `KeyboardService::init`;
* feed `ConsoleInput` into `consd`/the TTY (scheme or shared queue) so the bytes appear at the
  console instead of a fixture buffer;
* LEDs (`KeyboardService` resubmit path / `BootKeyboard::submit_leds`) are optional.

Until then the qtest is a fake-core integration and real USB enumeration is UNVERIFIED.
