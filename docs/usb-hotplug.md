<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# USB HID input hotplug and removal (S68)

OBSERVED on `stuart/S68` (base `origin/master` `0f171ab3`). Host tests plus a fake-core qtest;
no board claim and no real USB enumeration in this ticket.

## Path

```
USB core / fake core -> TransferRing
                     -> KeyboardService / MouseService
                     -> ConsoleInput / PointerQueue
                     -> detach() releases stuck input, clears repeat
                     -> init(descriptor) re-enumerates and primes again
```

The USB core and xHCI crates are unchanged. The xHCI port-change event that would call these
methods is still the S61/S67 integration follow-up.

## Service state

Both services start absent. `init` validates the descriptor, sends `SET_PROTOCOL(0)` and
`SET_IDLE(0,0)` (through the core model), primes the interrupt-IN transfer and marks the link
attached. `detach` marks it absent; `poll`/`poll_at` return `NotInitialized` until the next
`init`. `init` can be called again on the same service to re-enumerate; the report descriptor is
validated again and the transport is primed again.

The keyboard detach path:

* stops the auto-repeat (`Repeat::clear`);
* resets modifiers, held keys, Caps Lock and Num Lock while preserving the selected layout;
* returns synthetic `KeyEvent` releases for every usage held at detach (keys and modifiers, all
  `pressed: false`, `ascii: None`);
* does not clear bytes already queued in `ConsoleInput`.

The mouse detach path:

* queues a synthetic `PointerEvent` release for every held button (zero movement) and returns it;
* clears the service-level button state;
* does not clear pointer events already queued;
* normalizes later reports against the service state, so a stale button in the core mouse model
  cannot produce a duplicate transition after re-enumeration.

## Hotplug matrix

| Step | Keyboard | Mouse |
|---|---|---|
| Attach | 2 setup controls, 1 interrupt-IN prime | 2 setup controls, 1 interrupt-IN prime |
| Input held | Shift+A queues `A`; repeat is active | left button press is queued |
| Detach | 2 releases (LShift `0xe1`, `a` `0x04`); repeat stops; modifiers/locks reset | left release event is queued; buttons reset |
| Poll while absent | `Error::NotInitialized` | `MouseError::Hid(Error::NotInitialized)` |
| Re-enumerate while absent | `Error::NoDevice`; stays detached | `MouseError::Hid(Error::NoDevice)`; stays detached |
| Re-attach | 2 setup controls, 1 prime; layout preserved | 2 setup controls, 1 prime; buttons reset |
| First report on new device | `b` queues unshifted `b`; no stale Shift | right press queues `pressed=RIGHT`, `released=0`; no stale left |

Counts in the fixture include the report-driven resubmit: initial `init` + first `poll` gives two
interrupt-IN submissions, and the successful re-attach adds one more.

## APIs

* `KeyboardService::detach() -> KeyEvents` — release stuck usages.
* `KeyboardService::init()` — initial attach and re-enumeration.
* `KeyboardService::is_attached()`, `ring_mut()` — link state and transport reset seam.
* `MouseService::detach() -> Option<PointerEvent>` — release held buttons.
* `MouseService::init()` — initial attach and re-enumeration.
* `MouseService::is_attached()`, `buttons()`, `ring_mut()` — link and button state.
* `KeyEvent`/`KeyEvents` are re-exported by `usb-hid` for release consumers.

## Host evidence

```
$ cargo test -p usb-hid
running 29 tests
test result: ok. 29 passed; 0 failed

$ cargo test -p raven-usb-hid
running 50 tests
test result: ok. 50 passed; 0 failed
```

Coverage: keyboard detach releases held Shift+A, repeat stops and `tick` is silent, modifiers and
locks reset, poll is rejected while absent, failed re-enumeration stays detached, successful
re-enumeration re-primes and decodes an unshifted key, layout is preserved; mouse detach releases
the held button through the queue, poll is rejected while absent, re-enumeration resets the button
state and suppresses the core's stale left-button transition.

## QEMU qtest (fake USB core)

```
$ cargo xtask qtest --case usb-hotplug --arch x86_64
usb-hotplug: pass
exit 0
```

Serial:

```
usb-hotplug: keyboard attach ok
usb-hotplug: stuck releases=2
usb-hotplug: repeat stopped
usb-hotplug: keyboard reconnect=b
usb-hotplug: mouse release ok
usb-hotplug: mouse reconnect=right
usb-hotplug: ok
init exited (0)
```

`build/qtest/usb-hotplug.json`: result `pass`, eight matched expectations, no unmatched or
forbidden hits.

## Limits

* No board claim; the real xHCI port-change event and RP1 DWC3 binding remain UNVERIFIED.
* The fixture's fake core models presence, not electrical debounce/hotplug timing.
* `init` assumes the transport has been quiesced/reset by the caller; it does not abort an
  in-flight transfer owned by the real xHCI controller.
* Console/pointer queues keep events queued before the detach; consumers see them in order.
