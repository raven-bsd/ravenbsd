<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# USB HID fuzz and negative tests (S64)

OBSERVED on `stuart/S64` (base `12c0a7af`; queue base `42f64650`). Host only; no USB hardware and
no target run.

## Harness

`tests/fuzz/usb-hid/` is a standalone host package with a deterministic LCG
(`raven-usb-hid-fuzz::Lcg`) and a fixed corpus. Every generated case is reproducible; a panic in
any test fails the run.

```
$ cargo test --manifest-path tests/fuzz/usb-hid/Cargo.toml
running 3 tests   (LCG unit tests)
test result: ok. 3 passed
running 10 tests  (corpus/mutations)
test result: ok. 10 passed
exit 0
```

## Corpus and mutations

| Class | Cases |
|---|---|
| HID class descriptors (hand seeds) | empty, 1-byte, bLength 0/huge, wrong type, missing report descriptor, descriptor count 0/9/255, truncated length/value |
| Report descriptors (hand seeds) | empty, long-item truncations, global push overflow, pop underflow, truncated 2-byte item, unterminated collection, oversized report size/count, report ID, valid boot descriptor |
| Truncation prefixes | every prefix of the 63-byte boot descriptor and a valid class descriptor |
| Descriptor mutations | 50,000 seeded 1–4 byte flips of the boot descriptor |
| Random descriptors | 50,000 random class/report buffers, lengths 0–128/0–64 |
| Boot reports | 100,000 random 8-byte reports, plus every short length 0–7 must be `ShortReport`; event batches stay ≤ 24 |
| Keymap | all 256 usages × shift × caps; modifiers have no ASCII |
| Device model | 10,000 random reports through `BootKeyboard::init`/`poll` with a fake `TransferRing` |
| Rollover | the `0x01` ErrorRollOver report emits no events |

## Bug found and fixed

The fuzz found that `ReportLayout::is_boot_keyboard` accepted **any** input payload of at least
64 bits, so a keyboard usage with a 65,535-bit report was treated as the fixed eight-byte boot
layout. The S64 contract authorizes a decoder fix plus a regression test:

* `crates/raven-usb-hid/src/descriptor.rs`: `input_bits == 64` (was `>= 64`).
* `crates/raven-usb-hid/src/descriptor_ut.rs`: `a_wider_input_report_is_not_a_boot_keyboard`.
* `docs/usb-hid.md`: the wording now says exactly 64 bits.

The S60/S62 64-bit boot descriptor is unchanged and still accepted. No crash (panic/overflow) was
found in the descriptor or report paths after the fix.

## Acceptance

```
$ cargo test -p raven-usb-hid
running 29 tests
test result: ok. 29 passed; 0 failed
exit 0

$ cargo run -q -p raven-provenance -- check --root . --strict
7663 file(s) checked, 0 violation(s)
exit 0

$ make check-fast
check-fast: PASSED
exit 0
```

## Limits

* Deterministic corpus/mutation stress, not coverage-guided libFuzzer; no nightly tooling is
  needed and the run is host-only.
* USB enumeration and the real xHCI transfer ring remain UNVERIFIED (S61 follow-up).
* The parser's logical minimum/maximum items are still descriptive-only; the boot model uses the
  fixed report.
