<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# 2026-11-13 `inet_text`: `inet_pton` drops a leading `::` group

Target: `inet_text`
Owner: C51 (`raven-libc-resolv`)
Input: `2026-11-13-resolv-inet-compression.bin` (16 bytes: selector `0x3a` + 15 text bytes;
the earlier "17-byte" label was off by one)

The input selects `AF_INET6` and the text `0::a:4e:3:e:a:a`. `inet_pton` accepts the
address and `inet_ntop` renders it as `::a:4e:3:e:a:a`; parsing that canonical text returns
`1` but leaves the output all zero. `inet_pton("::a:4e:3:e:a:a")` alone shows the same
result. The compression-expansion loop in `raven-libc-resolv/src/inet.rs::pton6` copies the
post-`::` groups to the wrong destination and zeroes one group too many.

Expected: the 16 address bytes round-trip through `inet_ntop`/`inet_pton`.
Suggested fix location: `raven-libc-resolv/src/inet.rs::pton6`, the `if let Some(break_at)`
expansion block.

## Fixed

C51a (`f2f7508d`, 2026-11-14) replaced the forward element copy with `copy_within`
(memmove semantics), so the expansion no longer overwrites its own source and zeroes the
last group.

Fixed address bytes for `0::a:4e:3:e:a:a`:
`00 00 00 00 00 0a 00 4e 00 03 00 0e 00 0a 00 0a`.

Stable regression tests:
* `libc/ravenlibc/crates/raven-libc-resolv/tests/inet_compression.rs` covers the minimized
  input and the adjacent one/two-group forms.
* `libc/ravenlibc/crates/raven-libc-resolv/tests/fuzz_inet_compression.rs` covers the
  direct `::a:4e:3:e:a:a` form and the exact checked-in input.

2026-11-17 verification (OBSERVED): `replay run inet_text <this .bin>` ok; the 60-second
smoke ok; the one-hour soak completed 3601 s / 261,966,448 runs / 72,748 exec/s / 0 crashes
(SOAK.md).
