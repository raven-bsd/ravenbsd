// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `USER_COPY` (`0x8E`): fault-safe copy between two ranges of the caller's address space.
//!
//! FreeBSD `kern_poll` copies the caller's `pollfd` array with `copyin` and copies the
//! `revents` out with `pollout` (`sys/kern/sys_generic.c`), so an unmapped in-window pointer
//! is `EFAULT` rather than a fault. Raven's fd table lives in libc, so the array is
//! libc-owned; `USER_COPY` is the minimal primitive that lets the libc snapshot it through
//! the same fault-safe user-copy fixups the kernel already uses.
//!
//! The contract (`tickets/evidence/artifacts/S173b/user-copy-independent-contract-2026-11-27.md`):
//! only the current address space, `len == 0` succeeds without touching either pointer,
//! `len > 64 KiB` is `EINVAL` before any copy, overlapping ranges are `EINVAL` (a copy, not a
//! `memmove`), and any user fault is `EFAULT`. No kernel lock is held across the copy.

use raven_abi::errno::Errno;
use raven_usercopy::{UserSlice, validate_user_copy};

use super::SyscallFrame;

/// Bounce-buffer size for one fault-safe round trip.
const CHUNK: usize = 512;

/// Handle one `USER_COPY(dst, src, len)` frame.
pub fn user_copy(frame: &SyscallFrame) -> Result<usize, Errno> {
    let destination = frame.rdi;
    let source = frame.rsi;
    let length = usize::try_from(frame.rdx).map_err(|_| Errno::EFAULT)?;
    validate_user_copy(destination, source, length as u64)?;
    if length == 0 {
        return Ok(0);
    }
    // A user-to-user copy needs a kernel bounce page because `copy_from_user` writes a kernel
    // buffer and `copy_to_user` reads one. The chunk loop keeps the stack use bounded and lets
    // the second half of a two-page source fault mid-copy (the destination prefix is then
    // unspecified, as the contract allows).
    let mut buffer = [0u8; CHUNK];
    let mut offset = 0usize;
    while offset < length {
        let chunk = length.saturating_sub(offset).min(CHUNK);
        let from = UserSlice::new(source.saturating_add(offset as u64), chunk as u64)?;
        crate::usercopy::copy_from_user(buffer.get_mut(..chunk).ok_or(Errno::EFAULT)?, from)?;
        let to = UserSlice::new(destination.saturating_add(offset as u64), chunk as u64)?;
        crate::usercopy::copy_to_user(to, buffer.get(..chunk).ok_or(Errno::EFAULT)?)?;
        offset = offset.saturating_add(chunk);
    }
    Ok(0)
}

#[cfg(test)]
#[path = "user_copy_ut.rs"]
mod tests;
