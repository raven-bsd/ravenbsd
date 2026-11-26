// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The FreeBSD wait-status codec and the `wait4` -> `wait6` mapping.
//!
//! The native profile is FreeBSD (`sys/sys/wait.h` at the S700 pin): exits are `code << 8`,
//! signals are `sig | 0x80` with the core bit, stops are `(sig << 8) | 0x7f`, and a continued
//! child is `W_CONTINUED` (`0x13`, the `SIGCONT` number).

use crate::consts::{
    __WALL, KERNEL_WCONTINUED, P_ALL, P_PGID, P_PID, WAIT_OPTION_MASK, WCONTINUED, WEXITED,
};
use crate::errors::{EINVAL, Errno};

/// The FreeBSD `W_CONTINUED` status (`0x13`, the `SIGCONT` number).
pub const CONTINUED_STATUS: i32 = 0x13;

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
        WaitStatus::Continued => CONTINUED_STATUS,
    }
}

/// Decode one raw status. Values that do not name a real class return `None`.
#[must_use]
pub fn decode(raw: i32) -> Option<WaitStatus> {
    if raw == CONTINUED_STATUS {
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

/// Translate the public `sys/wait.h` option word to the raw `PROC_WAIT_EX` option word.
///
/// The C header keeps the FreeBSD values (`WCONTINUED = 4`, `WEXITED = 16`). The kernel's
/// `PROC_WAIT_EX` ABI has no `WEXITED` bit (it always reports exits) and uses
/// [`KERNEL_WCONTINUED`] for continued children, so the two public bits are replaced by the
/// kernel bit. Unsupported bits stay in the word and are rejected by the kernel with `EINVAL`,
/// preserving the existing `WNOWAIT`/`__WALL` behaviour.
#[must_use]
pub fn kernel_wait_options(options: u32) -> u32 {
    let mut kernel = options & !(WCONTINUED | WEXITED);
    if options & WCONTINUED != 0 {
        kernel |= KERNEL_WCONTINUED;
    }
    kernel
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
