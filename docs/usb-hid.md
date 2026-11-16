<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# USB HID boot keyboard (S60)

OBSERVED on `stuart/S60` (base `origin/master` `b27048e1`). Host tests only; no USB hardware
claim. The real enumeration and the xHCI binding are UNVERIFIED and owned by Phil's follow-up.

## Components

* `crates/raven-usb-hid` — HID class/report descriptor parsing, the eight-byte boot report
  decoder, the HID usage-to-ASCII map and the `BootKeyboard` model behind a `TransferRing` seam.
* `userland/drivers/usb-hid` — the userspace driver skeleton (package `usb-hid`). It binds the
  model to a `StubRing` today and prints that the xHCI ring is pending. It keeps
  `#![forbid(unsafe_code)]`.

## HID class descriptor

`parse_hid_descriptor` validates the 9-byte interface companion descriptor (`bDescriptorType =
0x21`), reads `bcdHID`, `bCountryCode` and the descriptor list, and returns the report descriptor
length from the `0x22` entry. Truncation and a missing report descriptor are errors.

## Report descriptor

`parse_report_descriptor` walks short items (sizes 0/1/2/4), skips long items (`0xFE`) and keeps
the global state (`Usage Page`, `Report Size`, `Report Count`, `Report ID`) with a four-deep
push/pop stack. Local usages reset after every main item. `Input`/`Output` items are recorded in
a fixed 16-entry `Field` table with the bit offset, size, count and usage range; the total input
and output bit counts are exported.

A layout is a boot keyboard when the application usage is Generic Desktop/Keyboard (`0x01`,
`0x06`), there is no report ID and the input payload is at least 64 bits. The host tests use a
standard 63-byte boot keyboard descriptor written from the USB HID specification: three input
fields (8 modifier bits, 8 reserved bits, 6 key bytes) and two LED output fields (5 bits plus 3
padding bits).

## Boot report

`BootReport` is the fixed eight-byte boot report: modifier bitmap, one reserved byte and six key
slots. `Keyboard::update` diffs against the previous report and emits `KeyEvent { usage, pressed,
ascii }`:

* modifier bits become press/release events for usages `0xE0..=0xE7`;
* a key that moves between slots emits no churn;
* the ErrorRollOver report (all six slots `0x01`) is ignored without changing held state;
* Caps Lock toggles on its press before the ASCII of the other keys is computed;
* letters use `shift XOR caps`; punctuation uses the shifted table; control maps letters to
  `0x01..=0x1a`;
* one batch is bounded at 24 events (six releases, six presses, eight modifier changes).

`key_to_ascii` covers US letters, digits, punctuation, Enter, Escape, Backspace, Tab, Space and
keypad `-`/`+`. Arrows and function keys have no ASCII.

## Transfer-ring seam

```rust
pub trait TransferRing {
    fn control_out(&mut self, setup: SetupPacket, data: &[u8]) -> Result<(), Error>;
    fn control_in(&mut self, setup: SetupPacket, data: &mut [u8]) -> Result<usize, Error>;
    fn interrupt_in(&mut self, endpoint: u8, length: usize) -> Result<(), Error>;
    fn next_report(&mut self) -> Option<[u8; 8]>;
}
```

`BootKeyboard::init` sends `SET_PROTOCOL(boot)`, `SET_IDLE(0, 0)`, parses the report descriptor
the caller fetched with `GET_DESCRIPTOR(0x22)` and primes one interrupt-IN transfer.
`poll` takes the next completed report, decodes it and resubmits the interrupt-IN transfer;
`submit_leds` sends the LED output report (`SET_REPORT` type 2).

The xHCI adapter must keep the interrupt transfer buffer alive until completion, copy the event
payload into an eight-byte report and return it from `next_report`. It must not be added to
`crates/raven-rp1-xhci/**` in this ticket.

## Host tests (OBSERVED)

```
$ cargo test -p raven-usb-hid
running 28 tests
test result: ok. 28 passed; 0 failed; 0 ignored
exit 0
```

Coverage: class-descriptor parse/error, boot descriptor layout (offsets, usage ranges, 64 input
bits, 8 output bits), non-keyboard/report-ID/long-item/truncated/stack-overflow cases, setup
packet encoding, every keymap group, press/release/diff/rollover/reorder/modifier-release/control/
caps/shift, and the device model's control sequence, resubmit loop and LED report.

## Limits and follow-ups

* No hardware claim: no USB controller binding, no enumeration, no qtest. The host model is the
  gate the ticket names.
* Boot protocol only; report-ID layouts are parsed but the model rejects them as a boot keyboard.
* US layout, six-key rollover, no key repeat; NKRO and other layouts are follow-ups.
* Phil: implement `TransferRing` over the RP1 xHCI transfer ring (enumeration, endpoint setup,
  control/interrupt transfers, LED output). Do not edit `crates/raven-rp1-xhci/**` here.
