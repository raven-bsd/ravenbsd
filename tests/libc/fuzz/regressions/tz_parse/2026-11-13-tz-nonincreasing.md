# 2026-11-13 `tz_parse`: non-increasing POSIX transitions

Target: `tz_parse`
Owner: C44 (`raven-libc-time`)
Input: `2026-11-13-tz-nonincreasing.bin` (15 bytes)

`tzparse("EST5T,0365,J1/2")` returned `Ok` with a transition list that steps backwards
around the `time_t` floor. P-tz-3 requires an accepted state to be strictly increasing, so
the parser now rejects such a rule with `TzError::BadFormat`. Stable regression test:
`libc/ravenlibc/crates/raven-libc-time/tests/fuzz_tz_rule.rs`.
