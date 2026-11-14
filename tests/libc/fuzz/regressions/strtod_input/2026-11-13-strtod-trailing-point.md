# 2026-11-13 `strtod_input`: a trailing decimal point

Target: `strtod_input`
Owner: C42 (`raven-libc-dtoa`)
Input: `2026-11-13-strtod-trailing-point.bin` (4 bytes)

`strtod("65.")` consumed 4 bytes of a 3-byte slice and returned 85.8: the fraction loop
treated the slice end (`at` returns 0) as a fractional digit. `strtof` had the same parser
bug through `strtodg`. Fixed in `strtod.rs` and `strtodg.rs` (only an actual digit enters
the fraction loop) and in the `strtod`/`strtof` wrappers (`consumed <= s.len()`).
Stable regression test: `libc/ravenlibc/crates/raven-libc-dtoa/tests/fuzz_consumed.rs`.
