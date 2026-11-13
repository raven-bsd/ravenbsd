// SPDX-License-Identifier: BSD-2-Clause
/*
 * Copyright (C) 2018 The Android Open Source Project
 * All rights reserved.
 *
 * Redistribution and use in source and binary forms, with or without
 * modification, are permitted provided that the following conditions
 * are met:
 *  * Redistributions of source code must retain the above copyright
 *    notice, this list of conditions and the following disclaimer.
 *  * Redistributions in binary form must reproduce the above copyright
 *    notice, this list of conditions and the following disclaimer in
 *    the documentation and/or other materials provided with the
 *    distribution.
 *
 * THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
 * "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
 * LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS
 * FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE
 * COPYRIGHT OWNER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT,
 * INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING,
 * BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS
 * OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED
 * AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
 * OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT
 * OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
 * SUCH DAMAGE.
 */
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from bionic:libc/bionic/getentropy.cpp @
// 731631f300090436d7f5df80d50b6275c8c60a93; the 256-byte limit, the fill loop
// and the save/restore rule are kept; `getrandom(2)` and `/dev/urandom` are
// replaced by `S:GETRANDOM*` (PG-52) and, until it lands, a cached `rand:`
// handle with EINTR retry. The source is NOT cryptographic today (A-sys-14):
// callers that need it must wait for PG-52.

//! `getentropy` over the `rand:` scheme until PG-52.

use core::sync::atomic::{AtomicBool, Ordering};

use alloc::vec::Vec;

use raven_abi::errno::Errno;
use raven_abi::types::Handle;

use crate::backend::{PendingCall, SysBackend};
use crate::lock::Lock;
use crate::platform::Platform;

/// Maximum accepted length; anything larger is `EIO` (not `EINVAL`).
pub const GETENTROPY_MAX: usize = 256;

/// Whether the entropy source is still the weak userspace generator (H-5/A-sys-14).
static ENTROPY_WEAK: AtomicBool = AtomicBool::new(true);

/// Whether the current source is weak.
pub fn entropy_weak() -> bool {
    ENTROPY_WEAK.load(Ordering::Acquire)
}

/// Record that PG-52 landed and the source is no longer weak.
pub fn mark_strong() {
    ENTROPY_WEAK.store(false, Ordering::Release);
}

/// `getentropy`.
pub fn getentropy<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    buf: &mut [u8],
) -> Result<(), Errno> {
    if buf.len() > GETENTROPY_MAX {
        return Err(Errno::EIO);
    }
    if buf.is_empty() {
        return Ok(());
    }
    // PG-52 will replace the whole body with one `S:GETRANDOM*` call; until then the cached
    // `rand:` handle is the only source, and it is not cryptographic (A-sys-14).
    let _ = platform.backend().pending(PendingCall::Getrandom);
    let handle = platform.random_handle()?;
    let mut collected = 0usize;
    while collected < buf.len() {
        let target = buf.get_mut(collected..).ok_or(Errno::EIO)?;
        match platform.backend().read(handle, target) {
            Ok(0) => return Err(Errno::EIO),
            Ok(count) => collected = collected.saturating_add(count),
            Err(Errno::EINTR) => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

/// Fill a 32-byte seed for the hardening area: `rand:` mixed with the monotonic clock.
pub fn seed<B: SysBackend, L: Lock>(platform: &Platform<B, L>) -> Result<[u8; 32], Errno> {
    let mut seed = [0u8; 32];
    getentropy(platform, &mut seed)?;
    let now = platform.backend().clock_get(1)?;
    let mixed = (now.tv_nsec as u64) ^ (now.tv_sec as u64).rotate_left(17);
    let bytes = mixed.to_le_bytes();
    for (index, slot) in seed.iter_mut().enumerate() {
        if let Some(byte) = bytes.get(index % bytes.len()) {
            *slot ^= *byte;
        }
    }
    Ok(seed)
}

/// `getrandom(buf, flags)`: `ENOSYS` until PG-52; `GRND_NONBLOCK` would be `EAGAIN` without a
/// seeded kernel source. The interim path reads `rand:` like `getentropy` but does not claim to
/// be cryptographic.
pub fn getrandom<B: SysBackend, L: Lock>(
    _platform: &Platform<B, L>,
    _buf: &mut [u8],
    _flags: u32,
) -> Result<usize, Errno> {
    // A weak userspace source must not be advertised as `getrandom` (A-sys-14).
    Err(Errno::ENOSYS)
}

/// The bytes read from a `rand:` handle; kept for tests that need a deterministic source.
pub fn read_rand<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    len: usize,
) -> Result<Vec<u8>, Errno> {
    let handle = platform.random_handle()?;
    let mut out = alloc::vec![0u8; len];
    platform.backend().read(handle, &mut out)?;
    Ok(out)
}

/// Ensure the `rand:` handle is cached; exported for the shell's early init.
pub fn cached_handle<B: SysBackend, L: Lock>(platform: &Platform<B, L>) -> Result<Handle, Errno> {
    platform.random_handle()
}

#[cfg(test)]
#[path = "getentropy_ut.rs"]
mod tests;
