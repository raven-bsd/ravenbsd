// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The getrandom 0.4 custom backend symbol.
//!
//! `getrandom` declares `__getrandom_v03_custom(dest, len) -> Result<(), Error>` with the Rust
//! ABI. `Error` is a `NonZeroRawOsError` (a `u32`), so the return is 0 for success and the
//! error code otherwise. The kernel entropy primitive is not available yet (G-7/PG-52), so the
//! slot reads the documented weak `rand:` source. Returning success here keeps a std collection
//! or a `uuid` caller working; `entropy_weak()` stays true until the kernel CSPRNG lands.

/// The getrandom 0.4 custom backend entry point.
///
/// The `Error::UNSUPPORTED` internal code is `1 << 16` (the internal error range starts at
/// `2^16`). The C ABI for two pointer scalars and a `u32` return is the same ABI the Rust
/// declaration uses on x86-64.
#[unsafe(no_mangle)]
pub extern "C" fn __getrandom_v03_custom(dest: *mut u8, len: usize) -> u32 {
    if len == 0 {
        return 0;
    }
    // SAFETY: the caller passes a writable buffer of `len` bytes, as the getrandom contract
    // requires.
    let buffer = unsafe { core::slice::from_raw_parts_mut(dest, len) };
    let platform = crate::ffi::platform::global();
    for chunk in buffer.chunks_mut(256) {
        if platform.getentropy(chunk).is_err() {
            return 1 << 16;
        }
    }
    0
}
