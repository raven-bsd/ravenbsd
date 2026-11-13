// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Thread names: a 16-byte TCB field (15 bytes plus NUL), no kernel call.
//!
//! CLEANROOM (row 24); the contract was read from musl `pthread_setname_np.c` and
//! `pthread_getname_np.c` but no code is copied.

use crate::kernel::ThreadKernel;
use crate::limits::{ENAMETOOLONG, ESRCH, Errno};
use crate::raw;

/// The maximum stored name length (without the NUL).
pub const NAME_MAX_LEN: usize = 15;

/// `pthread_setname_np`.
///
/// # Errors
///
/// `ENAMETOOLONG` when the name does not fit in 15 bytes, `ESRCH` for a null handle.
pub fn setname<K: ThreadKernel>(kernel: &K, thread: usize, name: &[u8]) -> Result<(), Errno> {
    let _ = kernel;
    if name.len() > NAME_MAX_LEN {
        return Err(ENAMETOOLONG);
    }
    if thread == 0 {
        return Err(ESRCH);
    }
    raw::set_name(raw::tcb(thread), name);
    Ok(())
}

/// `pthread_getname_np`: copies at most `buffer.len()` bytes including the NUL; returns the
/// stored length (without the NUL).
#[must_use]
pub fn getname<K: ThreadKernel>(kernel: &K, thread: usize, buffer: &mut [u8]) -> usize {
    let _ = kernel;
    if thread == 0 || buffer.is_empty() {
        return 0;
    }
    raw::get_name(raw::tcb(thread), buffer)
}

/// The stored name of a thread as a fixed 16-byte array (tests).
#[must_use]
pub fn read_name(thread: usize) -> [u8; 16] {
    if thread == 0 {
        return [0u8; 16];
    }
    let pthread = raw::tcb(thread);
    let mut out = [0u8; 16];
    for (index, byte) in pthread.name.iter().enumerate() {
        if let Some(slot) = out.get_mut(index) {
            *slot = *byte;
        }
    }
    out
}

#[cfg(test)]
#[path = "name_ut.rs"]
mod tests;
