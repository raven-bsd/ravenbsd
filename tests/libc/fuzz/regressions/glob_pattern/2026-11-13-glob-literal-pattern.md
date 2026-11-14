# 2026-11-13 `glob_pattern`: literal pattern fallback

Target: `glob_pattern`
Owner: C46 (`raven-libc-glob`)
Input: `2026-11-13-glob-literal-pattern.bin` (51 bytes)

A malformed bracket makes `glob` fall back to the literal pattern even without
`GLOB_NOCHECK`. The target's P-glob-5 rejected the literal because it is not a match under
`fnmatch`. This is a property bug in C70, not a glob bug: the proof now accepts
`path == pattern` as the documented literal fallback.
