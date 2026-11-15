// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host regression for the libc `ucontext_t *` ABI (C64k).
//!
//! The module below is the shell's real
//! `libc/ravenlibc/crates/raven-libc/src/ucontext/context_abi.rs`. The shell
//! module is target-only (`#[cfg(target_os = "none")]` in `lib.rs`), so this
//! standalone crate includes the same file and supplies only the tiny
//! `ffi::ret::int` mapping it expects. `raven-libc-signal` and
//! `raven-libc-abi` are the product dependencies.
//!
//! Run with:
//!
//! ```text
//! cargo test --manifest-path tests/libc/ucontext/Cargo.toml
//! ```

#[path = "../../../../libc/ravenlibc/crates/raven-libc/src/ucontext/context_abi.rs"]
pub mod context_abi;

/// The shell's `ffi::ret::int` contract, reduced to what this crate observes.
pub(crate) mod ffi {
    /// `Errno` to C return conversion.
    pub(crate) mod ret {
        use core::ffi::c_int;

        use raven_libc_abi::errno::Errno;

        /// `Ok(v)` becomes `v`; `Err(_)` becomes `-1`.
        pub(crate) fn int(result: Result<c_int, Errno>) -> c_int {
            result.unwrap_or(-1)
        }
    }
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
