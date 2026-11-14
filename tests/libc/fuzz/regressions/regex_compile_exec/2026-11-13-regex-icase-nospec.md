# 2026-11-13 `regex_compile_exec`: REG_ICASE + REG_NOSPEC recursion

Target: `regex_compile_exec`
Owner: C45 (`raven-libc-regex`)
Input: `2026-11-13-regex-icase-nospec.bin` (39 bytes)

The target passes `cflags = REG_ICASE | REG_NOSUB | REG_NOSPEC` and a literal pattern
containing the alphabetic byte `0xff`. `p_str` calls `ordinary`, which under `REG_ICASE`
calls `bothcases`; `bothcases` re-enters `p_bracket` with the same character, and
`p_bracket` collapses the singleton set back into `ordinary`, recursing until the stack
overflows. The process aborts, so the watchdog cannot report it.

Expected behaviour: compile the literal pattern, matching both cases, without recursion.
Fix: `ravens-libc-regex/src/regcomp.rs::p_bracket` keeps the singleton as an `OANYOF` set
when `REG_ICASE` is set and the character has an opposite case. Stable regression test:
`libc/ravenlibc/crates/raven-libc-regex/tests/fuzz_icase_nospec.rs`.
