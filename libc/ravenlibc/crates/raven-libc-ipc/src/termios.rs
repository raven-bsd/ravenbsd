// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `termios.h` policy: line-speed accessors, queue/flow selectors and the typed unsupported
//! answer for the operations the tty protocol does not carry yet.
//!
//! The speed accessors are real and operate on the ABI `struct termios`; the tty protocol
//! currently reports only one speed (`B38400`), but the full constant range is accepted because
//! the field is a stored attribute that future tty providers can honour. `tcdrain`, flow
//! control, break and window size need tty operations that do not exist, so they report
//! [`unsupported`] with `ENOSYS` rather than pretending the request was applied.

use raven_libc_abi::errno::Errno;
use raven_libc_abi::generated::structs::Termios;

use crate::consts::{EINVAL, ENOSYS};

/// `B0` (hang up).
pub const B0: u32 = 0;
/// `B38400`, the speed the current console reports.
pub const B38400: u32 = 0o17;
/// `B57600`, the first extended speed.
pub const B57600: u32 = 0o10001;
/// `B4000000`, the last extended speed.
pub const B4000000: u32 = 0o10017;

/// `TCIFLUSH`: discard pending input.
pub const TCIFLUSH: i32 = 0;
/// `TCOFLUSH`: discard pending output.
pub const TCOFLUSH: i32 = 1;
/// `TCIOFLUSH`: discard both queues.
pub const TCIOFLUSH: i32 = 2;

/// `TCOOFF`: suspend output.
pub const TCOOFF: i32 = 0;
/// `TCOON`: resume output.
pub const TCOON: i32 = 1;
/// `TCIOFF`: transmit a STOP character.
pub const TCIOFF: i32 = 2;
/// `TCION`: transmit a START character.
pub const TCION: i32 = 3;

/// The answer the unimplemented tty operations give.
#[must_use]
pub const fn unsupported() -> Errno {
    ENOSYS
}

/// Whether `speed` is one of the defined `B*` values (`B0..B38400` or `B57600..B4000000`).
#[must_use]
pub const fn is_supported_speed(speed: u32) -> bool {
    speed <= B38400 || (speed >= B57600 && speed <= B4000000)
}

/// The input speed stored in `termios` (`cfgetispeed`).
#[must_use]
pub const fn cfgetispeed(termios: &Termios) -> u32 {
    termios.__c_ispeed
}

/// The output speed stored in `termios` (`cfgetospeed`).
#[must_use]
pub const fn cfgetospeed(termios: &Termios) -> u32 {
    termios.__c_ospeed
}

/// Set the input speed (`cfsetispeed`).
///
/// # Errors
/// `EINVAL` when `speed` is not a defined `B*` value.
pub fn cfsetispeed(termios: &mut Termios, speed: u32) -> Result<(), Errno> {
    if !is_supported_speed(speed) {
        return Err(EINVAL);
    }
    termios.__c_ispeed = speed;
    Ok(())
}

/// Set the output speed (`cfsetospeed`).
///
/// # Errors
/// `EINVAL` when `speed` is not a defined `B*` value.
pub fn cfsetospeed(termios: &mut Termios, speed: u32) -> Result<(), Errno> {
    if !is_supported_speed(speed) {
        return Err(EINVAL);
    }
    termios.__c_ospeed = speed;
    Ok(())
}

/// Set both speeds (`cfsetspeed`).
///
/// The record is left unchanged when either write would be invalid.
///
/// # Errors
/// `EINVAL` when `speed` is not a defined `B*` value.
pub fn cfsetspeed(termios: &mut Termios, speed: u32) -> Result<(), Errno> {
    if !is_supported_speed(speed) {
        return Err(EINVAL);
    }
    termios.__c_ispeed = speed;
    termios.__c_ospeed = speed;
    Ok(())
}

/// Validate a `tcflush` queue selector.
///
/// # Errors
/// `EINVAL` when `queue` is not `TCIFLUSH`, `TCOFLUSH` or `TCIOFLUSH`.
pub const fn validate_queue(queue: i32) -> Result<(), Errno> {
    if queue == TCIFLUSH || queue == TCOFLUSH || queue == TCIOFLUSH {
        Ok(())
    } else {
        Err(EINVAL)
    }
}

/// Validate a `tcflow` action selector.
///
/// # Errors
/// `EINVAL` when `action` is not `TCOOFF`, `TCOON`, `TCIOFF` or `TCION`.
pub const fn validate_flow(action: i32) -> Result<(), Errno> {
    if action == TCOOFF || action == TCOON || action == TCIOFF || action == TCION {
        Ok(())
    } else {
        Err(EINVAL)
    }
}

#[cfg(test)]
#[path = "termios_ut.rs"]
mod tests;
