<!--
SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
-->

# raven-libc-pledge

The OpenBSD `pledge`/`unveil` models and the FreeBSD Capsicum `cap_rights_*` operations for
ravenlibc.

The crate is `no_std` plus `alloc`, class P (`#![forbid(unsafe_code)]`), layer L1, and depends
only on `raven-libc-abi`; `raven-abi` is a test-only dependency for the rights bit values.

## Contents

* `promise` - the 35 promise names in `pledgenames[]` order, the bit set and the space-only
  parser.
* `reduce` - the monotonic-reduction rule, the `error` masking mode and the unveil-cleanup flag.
* `capmap` - the CLEANROOM promise-to-Raven-capability plan.
* `unveil` - the path table, the longest component-boundary match and the lock.
* `cap_rights`, `consts` - `cap_rights_t` bit operations and the FreeBSD `CAP_*` numbers.

## Checks

```
cargo test -p raven-libc-pledge
cargo clippy -p raven-libc-pledge --all-targets -- -D warnings
cargo build -p raven-libc-pledge --target x86_64-unknown-none
cargo doc -p raven-libc-pledge --no-deps
cargo test -p raven-libc-hdrcheck --release
```
