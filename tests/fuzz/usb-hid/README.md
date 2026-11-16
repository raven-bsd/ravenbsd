# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

# USB HID fuzz harness (S64)

Deterministic host-only fuzz/negative harness for the `raven-usb-hid` descriptor and boot-report
decoders.

```
cargo test --manifest-path tests/fuzz/usb-hid/Cargo.toml
```

* `tests/seeds.rs` is the hand-written corpus: malformed class descriptors and truncated,
  long-item, stack-overflow, oversized-count and report-ID report descriptors.
* `tests/corpus.rs` adds every truncation prefix, 50k seeded descriptor mutations, 50k random
  class/report descriptors, 100k random boot reports and 10k random device-model reports.
* `raven-usb-hid-fuzz::Lcg` is a fixed LCG, so every failing index is reproducible.

The first run found that the boot-layout check accepted any input payload of at least 64 bits;
it now requires exactly the 64-bit boot report (`a_wider_input_report_is_not_a_boot_keyboard`).
