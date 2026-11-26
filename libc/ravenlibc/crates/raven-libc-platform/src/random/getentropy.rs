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
// and the save/restore rule are kept. `getrandom(2)` is the native `GETRANDOM`
// syscall (PG-52) with the cached `rand:` handle and EINTR retry only as the
// fallback; `getentropy` keeps the interim `rand:` source, which is NOT
// cryptographic today (A-sys-14).

//! `getentropy` over the `rand:` scheme; `getrandom` over the native `GETRANDOM` syscall.
//!
//! `getrandom` fills from the kernel's boot-seeded pool (PG-52) and keeps `rand:` only as the
//! documented weak fallback. `getentropy` still reads `rand:`; the interim source is not
//! cryptographic (A-sys-14).

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
    // The kernel CSPRNG is reached through `getrandom` below; this public path keeps the
    // cached `rand:` handle, which is not cryptographic (A-sys-14).
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

/// `getrandom(buf, flags)` over the native `GETRANDOM` syscall.
///
/// The kernel fills the buffer from its boot-seeded pool and owns the error values: unknown
/// flags `EINVAL`, zero length `0`, an unseeded pool `EAGAIN`. The `rand:` scheme is only a
/// fallback for a kernel without the primitive (`ENOSYS`) or before its pool is ready
/// (`EAGAIN`); it never replaces the kernel's own error contract.
pub fn getrandom<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    buf: &mut [u8],
    flags: u32,
) -> Result<usize, Errno> {
    // `kernel/src/syscall/basic.rs:163` accepts only `GRND_NONBLOCK` and `GRND_RANDOM`.
    if flags & !0x3 != 0 {
        return Err(Errno::EINVAL);
    }
    if buf.is_empty() {
        return Ok(0);
    }
    let kernel_error = match raven_libc_sys::pending::getrandom(buf, flags) {
        Ok(count) => return Ok(count),
        Err(error @ (Errno::ENOSYS | Errno::EAGAIN)) => error,
        Err(error) => return Err(error),
    };
    match fill_rand(platform, buf) {
        Ok(()) => Ok(buf.len()),
        Err(_) => Err(kernel_error),
    }
}

/// Fill `buf` from the `rand:` scheme in `GETENTROPY_MAX` chunks.
fn fill_rand<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    buf: &mut [u8],
) -> Result<(), Errno> {
    let mut collected = 0usize;
    while collected < buf.len() {
        let end = collected.saturating_add(GETENTROPY_MAX).min(buf.len());
        let chunk = buf.get_mut(collected..end).ok_or(Errno::EIO)?;
        getentropy(platform, chunk)?;
        collected = end;
    }
    Ok(())
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
