// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The C `struct sigaction` to the raw `SIGACTION` kernel form (C19/S160).
//!
//! The C ABI keeps the FreeBSD 152-byte `Sigaction` (handler, 128-byte mask, flags, restorer).
//! The kernel's 0x87 request is the 40-byte S160 layout: handler, a 128-bit mask, u32 flags and
//! the reserved restorer. These conversions are the only place the two layouts meet; the guest
//! path is a pair of byte copies through `syscall3(SIGACTION, ...)`.

use raven_libc_abi::structs::Sigaction;

/// One raw `SIGACTION` action (40 bytes, S160).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KernelSigAction {
    /// Handler address, `SIG_DFL` or `SIG_IGN`.
    pub handler: usize,
    /// Signals blocked while the handler runs (128 bits).
    pub mask: [u64; 2],
    /// `SA_*` flags.
    pub flags: u32,
    /// Reserved restorer field; zero on Raven.
    pub restorer: usize,
}

/// Convert the C action to the kernel request.
///
/// Only the first 128 mask bits exist in the kernel layout; the remaining C mask words are not
/// part of the 0x87 contract and are dropped. The restorer is reserved zero on Raven.
#[must_use]
pub fn from_c(action: &Sigaction) -> KernelSigAction {
    let low = action.sa_mask.bytes.first().copied().unwrap_or(0);
    let high = action.sa_mask.bytes.get(1).copied().unwrap_or(0);
    KernelSigAction {
        handler: action.sa_handler as usize,
        mask: [low, high],
        flags: action.sa_flags as u32,
        restorer: 0,
    }
}

/// Convert the kernel's previous action back into the C layout.
pub fn to_c(action: &KernelSigAction, out: &mut Sigaction) {
    *out = Sigaction::ZERO;
    out.sa_handler = action.handler as *mut core::ffi::c_void;
    if let Some(slot) = out.sa_mask.bytes.first_mut() {
        *slot = action.mask.first().copied().unwrap_or(0);
    }
    if let Some(slot) = out.sa_mask.bytes.get_mut(1) {
        *slot = action.mask.get(1).copied().unwrap_or(0);
    }
    out.sa_flags = action.flags as i32;
    out.sa_restorer = core::ptr::null_mut();
}

#[cfg(test)]
#[path = "sigaction_abi_ut.rs"]
mod tests;
