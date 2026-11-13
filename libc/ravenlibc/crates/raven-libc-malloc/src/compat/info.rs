// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `mallinfo` clamping and field helpers.

use crate::compat::{Mallinfo, Mallinfo2};

/// Clamp a counter to `i32::MAX`.
#[must_use]
pub fn clamp_i32(value: usize) -> i32 {
    if value > i32::MAX as usize {
        i32::MAX
    } else {
        value as i32
    }
}

/// Convert the wide statistics to the ten-`int` structure.
#[must_use]
pub fn narrow(info: Mallinfo2) -> Mallinfo {
    Mallinfo {
        arena: clamp_i32(info.arena),
        ordblks: clamp_i32(info.ordblks),
        smblks: clamp_i32(info.smblks),
        hblks: clamp_i32(info.hblks),
        hblkhd: clamp_i32(info.hblkhd),
        usmblks: clamp_i32(info.usmblks),
        fsmblks: clamp_i32(info.fsmblks),
        uordblks: clamp_i32(info.uordblks),
        fordblks: clamp_i32(info.fordblks),
        keepcost: clamp_i32(info.keepcost),
    }
}

#[cfg(test)]
#[path = "info_ut.rs"]
mod tests;
