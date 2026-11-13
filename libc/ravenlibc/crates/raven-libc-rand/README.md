<!--
SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
-->

# raven-libc-rand

The OpenBSD `arc4random` design (area 15, 3.A.1) as pure, allocation-free Rust: the ChaCha20
keystream, the `rs`/`rsx` state machine with "zero what you hand out" and rekeying, the
`arc4random_uniform` rejection loop, the fork wipe hook, the `getentropy`/`getrandom` flag logic
and the `ENTROPY_WEAK` build/runtime gate (H-5). Entropy is the `Entropy` trait of
`raven-libc-malloc::provider`; failure never falls back to weak data.

Build and test: `cargo test -p raven-libc-rand`; the release gate is
`cargo run -q -p raven-libc-rand --example entropy_gate` (exit 1 while the interim feature is on).
