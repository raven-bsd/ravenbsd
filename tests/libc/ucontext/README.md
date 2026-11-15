<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# libc ucontext host regression

C64k. The shell's `ucontext` module is target-only (`#[cfg(target_os = "none")]` in
`libc/ravenlibc/crates/raven-libc/src/lib.rs`), so this standalone crate includes the real
source file `libc/ravenlibc/crates/raven-libc/src/ucontext/context_abi.rs` and supplies only the
`ffi::ret::int` shim. It tests the same `getcontext`/`setcontext`/`swapcontext` code the target
build links.

```
cargo test --manifest-path tests/libc/ucontext/Cargo.toml
```

Tests:

* `the_mcontext_starts_at_ucontext_plus_forty_bytes` — the offset that caused the target fault.
* `a_thousand_makecontext_swapcontext_round_trips` — the C73 `check_ucontext` loop: 1000
  `swapcontext` round trips between the main and worker contexts.
* `null_contexts_report_efault` — NULL arguments return `-1` instead of dereferencing.

The target evidence is `cargo xtask qtest --case libc-signals --arch x86_64`; see
`docs/libc-ucontext.md`. This crate has its own `[workspace]`, like the `tests/fixtures/*`
fixtures, so it is not a product workspace member. `target/` and the generated `Cargo.lock` are
ignored.
