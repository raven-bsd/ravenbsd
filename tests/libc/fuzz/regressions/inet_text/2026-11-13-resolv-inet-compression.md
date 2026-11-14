# 2026-11-13 `inet_text`: `inet_pton` drops a leading `::` group

Target: `inet_text`
Owner: C51 (`raven-libc-resolv`)
Input: `2026-11-13-resolv-inet-compression.bin` (17 bytes)

The input selects `AF_INET6` and the text `0::a:4e:3:e:a:a`. `inet_pton` accepts the
address and `inet_ntop` renders it as `::a:4e:3:e:a:a`; parsing that canonical text returns
`1` but leaves the output all zero. `inet_pton("::a:4e:3:e:a:a")` alone shows the same
result. The compression-expansion loop in `raven-libc-resolv/src/inet.rs::pton6` copies the
post-`::` groups to the wrong destination and zeroes one group too many.

Expected: the 16 address bytes round-trip through `inet_ntop`/`inet_pton`.
Suggested fix location: `raven-libc-resolv/src/inet.rs::pton6`, the `if let Some(break_at)`
expansion block. This is more than a one-line change, so it is reported rather than fixed.
Stable regression test: `libc/ravenlibc/crates/raven-libc-resolv/tests/fuzz_inet_compression.rs`.
