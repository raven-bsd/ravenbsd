<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# USB HID boot mouse and pointer events (S66)

OBSERVED on `stuart/S66` (base `origin/master` `ce2f2f91` plus S65 `7a6c875b`). Host tests only;
no board claim.

## Path

```
boot mouse report -> raven-usb-hid::Mouse (buttons, deltas)
                  -> PointerEvent { buttons, pressed, released, dx, dy, wheel }
                  -> usb_hid::MouseService (PointerQueue)
                  -> PointerQueue::read / drain_into(PointerConsumer)
```

The USB core/xHCI are untouched. The real enumeration and transfer-ring binding remain S61's
follow-up.

## Report

The HID boot mouse report is three bytes; the wheel extension adds a fourth. Both lengths are
accepted by the report descriptor check.

| Byte | Field | Meaning |
|---|---|---|
| 0 | buttons | bit 0 left, bit 1 right, bit 2 middle, bit 3 back, bit 4 forward; bits 5-7 reserved and cleared |
| 1 | X | signed relative movement (`-128..=127`) |
| 2 | Y | signed relative movement (`-128..=127`) |
| 3 | wheel | optional signed wheel movement; zero when the report is three bytes |

`MouseReport::from_bytes` rejects a report shorter than three bytes with
`Error::ShortReport`. `MouseReport::button(BUTTON_*)` tests one button.

## Descriptor check

`ReportLayout::is_boot_mouse()` requires a Generic Desktop (`0x01`) Mouse (`0x02`) application
with no report ID and a 24-bit (three-byte) or 32-bit (four-byte) input payload.
`boot_mouse_report_len()` returns the matching length. The descriptor parser now records the
application usage only from an Application collection, so the nested Physical collection in a
standard mouse descriptor cannot overwrite the Mouse usage.

`BootMouse::init` sends `SET_PROTOCOL(0)` and `SET_IDLE(0, 0)`, validates the descriptor, then
primes `interrupt_in(REPORT_ENDPOINT, report_len)` with the descriptor's length. A non-mouse
descriptor returns `MouseError::NotAMouse` before priming. `poll` decodes one completed report,
updates the pointer state and resubmits. Before `init` it returns `MouseError::Hid(NotInitialized)`.

## Pointer events and state

`Mouse` keeps the held button bitmap and accumulates X/Y/wheel with saturating `i32` arithmetic.
Each `update` returns:

| Field | Meaning |
|---|---|
| `buttons` | button bitmap after the report |
| `pressed` | buttons set by this report (`new & !old`) |
| `released` | buttons cleared by this report (`old & !new`) |
| `dx`, `dy` | relative movement from the report |
| `wheel` | relative wheel movement |

`PointerEvent::is_empty()` is true when there is no button change and no movement.
`Mouse::position()` is the accumulated relative position; screen mapping, acceleration and
clamping belong to the pointer consumer.

## Queue and consumer seam

`PointerQueue` is a bounded FIFO of `POINTER_QUEUE` (64) events:

* `push` returns false when full;
* `pop` / `read(&mut [PointerEvent])` drain in order;
* `drain_into(&mut impl PointerConsumer)` hands every waiting event to a consumer;
* `len`, `is_empty`, `clear`.

`PointerConsumer` is the console/pointer seam:

```rust
pub trait PointerConsumer {
    fn pointer_event(&mut self, event: PointerEvent);
}
```

`MouseService::poll()` queues at most one event and returns the queued count (zero when no
report completed); `read_pointer`, `drain_pointer`, `pointer_len` and `pointer()` are the consumer
accessors. The keyboard `ConsoleInput` queue is unchanged.

## Host evidence

```
$ cargo test -p raven-usb-hid
running 50 tests
test result: ok. 50 passed; 0 failed

$ cargo test -p usb-hid
running 25 tests
test result: ok. 25 passed; 0 failed

$ make check-fast
check-fast: PASSED
```

Coverage: button bits and reserved-bit clearing, signed X/Y/wheel decode, three- and four-byte
reports, short reports, press/release transitions, accumulated position, descriptor recognition
and report-ID/wider-payload rejection, init control requests and report length selection,
resubmit behavior, non-mouse rejection before priming, queue FIFO/full/read/clear, and the
`PointerConsumer` seam.

## Limits

* No board claim; the S61 xHCI binding and real enumeration remain UNVERIFIED.
* Movement is relative and unaccelerated; absolute tablets, double-click detection and screen
  clamping are consumer follow-ups.
* The wheel is a signed relative byte; horizontal wheel/tilt is not decoded.
