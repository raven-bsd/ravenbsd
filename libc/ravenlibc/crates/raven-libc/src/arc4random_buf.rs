// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The `arc4random_buf` C-ABI export over the C50 state machine.
//!
//! The entropy comes from the platform `getentropy` (the cached `rand:` scheme until PG-52);
//! a failure is fatal, never a weak fallback (OpenBSD `arc4random.c`).

use core::ffi::c_void;

use raven_libc_malloc::Entropy;
use raven_libc_rand::{Arc4Random, RandPlatform};

use crate::personality_common::Global;

/// The platform entropy source.
struct ShellEntropy;

impl Entropy for ShellEntropy {
    fn fill(&self, buffer: &mut [u8]) -> bool {
        crate::ffi::platform::global().getentropy(buffer).is_ok()
    }
}

/// The fatal no-entropy path.
struct ShellRandPlatform;

impl RandPlatform for ShellRandPlatform {
    fn no_entropy(&self) -> ! {
        let _ = raven_libc_sys::sys::debug_write(b"arc4random: no entropy\n");
        raven_libc_sys::sys::exit(134)
    }
}

static STATE: Global<Arc4Random> = Global::new(Arc4Random::new());

/// `arc4random_buf`.
///
/// # Safety
/// `buffer` is NULL or writable for `length` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn arc4random_buf(buffer: *mut c_void, length: usize) {
    if buffer.is_null() || length == 0 {
        return;
    }
    // SAFETY: the caller granted `length` writable bytes.
    let bytes = unsafe { core::slice::from_raw_parts_mut(buffer.cast::<u8>(), length) };
    STATE.with(|state| state.random_buf(&ShellEntropy, &ShellRandPlatform, bytes));
}
