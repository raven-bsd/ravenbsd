// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Native (FreeBSD-valued) to Linux error-number conversion for the personality boundary.
//!
//! The two directions are the donor's tables from `freebsd:sys/compat/linux/linux_errno.inc`
//! (BSD-2-Clause) at `13334440`: `linux_errtbl` (native error index to positive Linux number) and
//! `linux_to_bsd_errtbl` (Linux number to native error). They are transcribed as plain numbers
//! so this crate keeps no licence barrier with the native contract crate.
//!
//! An error outside the defined ranges is rejected (`None`), never passed through unchanged:
//! a raw equality of numbers is exactly the aliasing this conversion exists to prevent.

/// Native errors the conversion covers (`ELAST`).
const NATIVE_LAST: u16 = 97;
/// Linux errors the reverse conversion covers (`LINUX_ELAST`).
const LINUX_LAST: u16 = 133;

/// `linux_errtbl`: index is the native error, value is the positive Linux number.
///
/// FreeBSD-only errors map to the donor's explicit destination, including the donor's documented
/// approximations (for example the RPC errors to `ENXIO` and `EPROCLIM` to `EAGAIN`).
const NATIVE_TO_LINUX: [u16; NATIVE_LAST as usize + 1] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 35, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25,
    26, 27, 28, 29, 30, 31, 32, 33, 34, 11, 115, 114, 88, 89, 90, 91, 92, 93, 94, 95, 96, 97, 98,
    99, 100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111, 40, 36, 112, 113, 39, 11, 87,
    122, 116, 66, 6, 6, 6, 6, 6, 37, 38, 9, 6, 6, 43, 42, 75, 125, 84, 61, 22, 74, 72, 67, 71, 1,
    1, 131, 130, 22,
];

/// `linux_to_bsd_errtbl`: index is the Linux error, value is the native error (0 = undefined).
///
/// The donor maps the Linux-only and unused numbers to an explicit native error (`EINVAL` for the
/// reserved 41/58 and the stream/library classes, `EIO` for the key-management class), so the
/// reverse conversion never invents a value.
const LINUX_TO_NATIVE: [u16; LINUX_LAST as usize + 1] = [
    0, 1, 2, 3, 4, 5, 6, 7, 2, 9, 10, 35, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25,
    26, 27, 28, 29, 30, 31, 32, 33, 34, 11, 63, 77, 78, 66, 62, 22, 83, 82, 22, 22, 22, 22, 22, 22,
    22, 22, 22, 22, 22, 22, 22, 22, 22, 22, 22, 87, 22, 22, 22, 22, 71, 91, 22, 22, 22, 92, 90, 22,
    89, 84, 22, 9, 22, 22, 22, 22, 22, 22, 86, 35, 22, 68, 38, 39, 40, 41, 42, 43, 43, 45, 46, 47,
    48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 64, 65, 37, 36, 70, 22, 22, 22, 22, 22,
    69, 5, 5, 85, 5, 5, 5, 5, 96, 95, 50, 22,
];

/// Convert a native error number to the Linux number a personality process must observe.
///
/// Returns `None` for `0` and for values above `ELAST`; the caller decides the fail-closed
/// action instead of receiving a guessed number.
#[must_use]
pub const fn linux_from_native(native: u16) -> Option<u16> {
    if native == 0 || native > NATIVE_LAST {
        return None;
    }
    let linux = NATIVE_TO_LINUX[native as usize];
    if linux == 0 { None } else { Some(linux) }
}

/// Convert a Linux error number to the native error the kernel/libc must carry.
///
/// Returns `None` for `0` and for values above `LINUX_ELAST`.
#[must_use]
pub const fn native_from_linux(linux: u16) -> Option<u16> {
    if linux == 0 || linux > LINUX_LAST {
        return None;
    }
    let native = LINUX_TO_NATIVE[linux as usize];
    if native == 0 { None } else { Some(native) }
}

#[cfg(test)]
#[path = "xlat_ut.rs"]
mod tests;
