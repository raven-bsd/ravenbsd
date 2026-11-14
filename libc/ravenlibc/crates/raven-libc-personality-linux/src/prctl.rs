// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `prctl` (`10` 3.8, `X85`).
//!
//! The native subset is exactly the one of the design document: thread/process name,
//! parent-death signal, dumpable, child subreaper, no-new-privs and timer slack. Every other
//! option is `EINVAL`; there is no Linux-syscall pass-through.
//!
//! The C entry is variadic and pointer-carrying. The pure function handles the value options
//! and returns the integer a getter should store; the shell owns the `va_list` read, the
//! `PR_SET_NAME` pointer read and the `PR_GET_NAME`/`PR_GET_PDEATHSIG` writes (see
//! [`set_name`] and [`get_name`]). This split keeps every byte access in the class-U shell.

use crate::seams::{Errno, ProcCtl};

/// `PR_SET_PDEATHSIG`.
pub const PR_SET_PDEATHSIG: i32 = 1;
/// `PR_GET_PDEATHSIG`.
pub const PR_GET_PDEATHSIG: i32 = 2;
/// `PR_GET_DUMPABLE`.
pub const PR_GET_DUMPABLE: i32 = 3;
/// `PR_SET_DUMPABLE`.
pub const PR_SET_DUMPABLE: i32 = 4;
/// `PR_SET_NAME`.
pub const PR_SET_NAME: i32 = 15;
/// `PR_GET_NAME`.
pub const PR_GET_NAME: i32 = 16;
/// `PR_SET_TIMERSLACK`.
pub const PR_SET_TIMERSLACK: i32 = 29;
/// `PR_GET_TIMERSLACK`.
pub const PR_GET_TIMERSLACK: i32 = 30;
/// `PR_SET_CHILD_SUBREAPER`.
pub const PR_SET_CHILD_SUBREAPER: i32 = 36;
/// `PR_GET_CHILD_SUBREAPER`.
pub const PR_GET_CHILD_SUBREAPER: i32 = 37;
/// `PR_SET_NO_NEW_PRIVS`.
pub const PR_SET_NO_NEW_PRIVS: i32 = 38;
/// `PR_GET_NO_NEW_PRIVS`.
pub const PR_GET_NO_NEW_PRIVS: i32 = 39;

/// The Linux default timer slack: 50000 nanoseconds (PROPOSED, `10` 3.8).
pub const TIMERSLACK_DEFAULT: u64 = 50_000;

/// The highest valid signal number.
pub const MAX_SIGNAL: u64 = 64;

/// The shared `prctl` value dispatcher.
///
/// `PR_SET_NAME` and `PR_GET_NAME` are handled by [`set_name`]/[`get_name`] because they carry
/// a 16-byte buffer; calling them here is `EINVAL`.
pub fn prctl<P: ProcCtl>(
    p: &mut P,
    option: i32,
    a2: u64,
    a3: u64,
    a4: u64,
    a5: u64,
) -> Result<i64, Errno> {
    match option {
        PR_SET_PDEATHSIG => {
            if a2 > MAX_SIGNAL {
                return Err(Errno::EINVAL);
            }
            let signal = i32::try_from(a2).map_err(|_| Errno::EINVAL)?;
            p.prctl_pdeathsig_set(signal)?;
            Ok(0)
        }
        PR_GET_PDEATHSIG => Ok(i64::from(p.prctl_pdeathsig_get()?)),
        PR_SET_DUMPABLE => {
            if a2 > 1 {
                return Err(Errno::EINVAL);
            }
            let value = i32::try_from(a2).map_err(|_| Errno::EINVAL)?;
            p.prctl_dumpable_set(value)?;
            Ok(0)
        }
        PR_GET_DUMPABLE => Ok(i64::from(p.prctl_dumpable_get()?)),
        PR_SET_CHILD_SUBREAPER => {
            if a2 > 1 {
                return Err(Errno::EINVAL);
            }
            let value = i32::try_from(a2).map_err(|_| Errno::EINVAL)?;
            p.prctl_subreaper_set(value)?;
            Ok(0)
        }
        PR_GET_CHILD_SUBREAPER => Ok(i64::from(p.prctl_subreaper_get()?)),
        PR_SET_NO_NEW_PRIVS => {
            if a2 != 1 || a3 != 0 || a4 != 0 || a5 != 0 {
                return Err(Errno::EINVAL);
            }
            p.prctl_no_new_privs_set()?;
            Ok(0)
        }
        PR_GET_NO_NEW_PRIVS => Ok(i64::from(p.prctl_no_new_privs_get()?)),
        PR_SET_TIMERSLACK => {
            p.prctl_timerslack_set(a2)?;
            Ok(0)
        }
        PR_GET_TIMERSLACK => {
            let value = p.prctl_timerslack_get()?;
            i64::try_from(value).map_err(|_| Errno::EINVAL)
        }
        PR_SET_NAME | PR_GET_NAME => Err(Errno::EINVAL),
        _ => Err(Errno::EINVAL),
    }
}

/// `PR_SET_NAME`: copy at most 15 bytes and NUL-terminate.
pub fn set_name<P: ProcCtl>(p: &mut P, name: &[u8]) -> Result<i64, Errno> {
    let mut field = [0u8; 16];
    let copied = name.len().min(15);
    if let (Some(dst), Some(src)) = (field.get_mut(..copied), name.get(..copied)) {
        dst.copy_from_slice(src);
    }
    p.prctl_name_set(&field)?;
    Ok(0)
}

/// `PR_GET_NAME`: the stored 16-byte name.
pub fn get_name<P: ProcCtl>(p: &mut P) -> Result<[u8; 16], Errno> {
    p.prctl_name_get()
}

#[cfg(test)]
#[path = "prctl_ut.rs"]
mod tests;
