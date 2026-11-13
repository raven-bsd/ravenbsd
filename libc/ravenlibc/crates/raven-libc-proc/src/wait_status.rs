// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The Linux wait-status codec and the `wait4` -> `wait6` mapping.
//!
//! Raven uses the Linux numeric values (DR-13), so the C header macros and this codec agree:
//! exits are `code << 8`, signals are `sig | 0x80` with the core bit, stops are
//! `(sig << 8) | 0x7f`, and a continued child is `0xffff`.

use crate::consts::{__WALL, P_ALL, P_PGID, P_PID, WAIT_OPTION_MASK, WEXITED};
use crate::errors::{EINVAL, Errno};

/// One decoded wait status.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WaitStatus {
    /// The child exited normally.
    Exited(u8),
    /// The child was killed by a signal.
    Signaled {
        /// Signal number.
        sig: u8,
        /// Whether the signal produced a core dump.
        core: bool,
    },
    /// The child stopped.
    Stopped(u8),
    /// The child continued.
    Continued,
}

/// Encode one status in the Linux `wait` representation.
#[must_use]
pub fn encode(status: WaitStatus) -> i32 {
    match status {
        WaitStatus::Exited(code) => i32::from(code) << 8,
        WaitStatus::Signaled { sig, core } => i32::from(sig) | if core { 0x80 } else { 0 },
        WaitStatus::Stopped(sig) => (i32::from(sig) << 8) | 0x7f,
        WaitStatus::Continued => 0xffff,
    }
}

/// Decode one raw status. Values that do not name a real class return `None`.
#[must_use]
pub fn decode(raw: i32) -> Option<WaitStatus> {
    if raw == 0xffff {
        return Some(WaitStatus::Continued);
    }
    if raw & 0x7f == 0 {
        return Some(WaitStatus::Exited((raw >> 8) as u8));
    }
    if raw & 0xff == 0x7f {
        let sig = (raw >> 8) as u8;
        return if sig == 0 {
            None
        } else {
            Some(WaitStatus::Stopped(sig))
        };
    }
    let sig = (raw & 0x7f) as u8;
    if sig == 0 || (raw as u16).wrapping_sub(1) >= 0xff {
        return None;
    }
    Some(WaitStatus::Signaled {
        sig,
        core: raw & 0x80 != 0,
    })
}

/// The id-selection kind of `wait6`/`waitid`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum IdType {
    /// Any child.
    All = P_ALL,
    /// One pid.
    Pid = P_PID,
    /// One process group.
    Pgid = P_PGID,
    /// One pidfd.
    PidFd = 3,
}

/// Map the BSD/Linux `wait4` arguments to `wait6` arguments.
///
/// `WEXITED` is implied by `wait4`; `__WALL` is accepted and removed.
pub fn wait4_to_wait6(pid: i32, own_pgid: i32, options: u32) -> Result<(IdType, i64, u32), Errno> {
    if options & !WAIT_OPTION_MASK != 0 {
        return Err(EINVAL);
    }
    let options = (options | WEXITED) & !__WALL;
    let (idtype, id) = if pid > 0 {
        (IdType::Pid, i64::from(pid))
    } else if pid == -1 {
        (IdType::All, 0)
    } else if pid == 0 {
        (IdType::Pgid, i64::from(own_pgid))
    } else {
        (IdType::Pgid, i64::from(pid).saturating_neg())
    };
    Ok((idtype, id, options))
}

/// Zero a `siginfo_t`-shaped record when `wait6` reports no ready child.
pub fn waitid_info_on_no_child(info: &mut [u8; 128]) {
    info.fill(0);
}

#[cfg(test)]
#[path = "wait_status_ut.rs"]
mod tests;
