<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# 2026-11-13 `time_format`: `mktime` with `tm_sec = i32::MIN`

Target: `time_format`
Owner: C44 (`raven-libc-time`)
Input: `2026-11-13-time-mktime-min-sec.bin` (23 bytes)

`mktime` panicked with `attempt to subtract with overflow` at
`raven-libc-time/src/tz/mktime.rs:133`: the extreme-seconds branch computes
`saved - 59`. Fixed with `saturating`/wrapping subtraction, matching the donor's
two's-complement behaviour. Stable regression test:
`libc/ravenlibc/crates/raven-libc-time/tests/fuzz_extreme_tm.rs`.
