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
pub const B38400: u32 = 38_400;
/// `B57600`, the first extended speed.
pub const B57600: u32 = 57_600;
/// `B4000000`, the last extended speed.
pub const B4000000: u32 = 4_000_000;

/// `TCIFLUSH`: discard pending input.
pub const TCIFLUSH: i32 = 1;
/// `TCOFLUSH`: discard pending output.
pub const TCOFLUSH: i32 = 2;
/// `TCIOFLUSH`: discard both queues.
pub const TCIOFLUSH: i32 = 3;

/// `TCOOFF`: suspend output.
pub const TCOOFF: i32 = 1;
/// `TCOON`: resume output.
pub const TCOON: i32 = 2;
/// `TCIOFF`: transmit a STOP character.
pub const TCIOFF: i32 = 3;
/// `TCION`: transmit a START character.
pub const TCION: i32 = 4;

/// The answer the unimplemented tty operations give.
#[must_use]
pub const fn unsupported() -> Errno {
    ENOSYS
}

/// Whether `speed` is one of the defined FreeBSD numeric `B*` values.
#[must_use]
pub const fn is_supported_speed(speed: u32) -> bool {
    matches!(
        speed,
        0 | 50
            | 75
            | 110
            | 134
            | 150
            | 200
            | 300
            | 600
            | 1200
            | 1800
            | 2400
            | 4800
            | 7200
            | 9600
            | 14_400
            | 19_200
            | 28_800
            | 38_400
            | 57_600
            | 76_800
            | 115_200
            | 230_400
            | 460_800
            | 500_000
            | 921_600
            | 1_000_000
            | 1_500_000
            | 2_000_000
            | 2_500_000
            | 3_000_000
            | 3_500_000
            | 4_000_000
    )
}

/// The input speed stored in `termios` (`cfgetispeed`).
#[must_use]
pub const fn cfgetispeed(termios: &Termios) -> u32 {
    termios.c_ispeed
}

/// The output speed stored in `termios` (`cfgetospeed`).
#[must_use]
pub const fn cfgetospeed(termios: &Termios) -> u32 {
    termios.c_ospeed
}

/// Set the input speed (`cfsetispeed`).
///
/// # Errors
/// `EINVAL` when `speed` is not a defined `B*` value.
pub fn cfsetispeed(termios: &mut Termios, speed: u32) -> Result<(), Errno> {
    if !is_supported_speed(speed) {
        return Err(EINVAL);
    }
    termios.c_ispeed = speed;
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
    termios.c_ospeed = speed;
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
    termios.c_ispeed = speed;
    termios.c_ospeed = speed;
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
