// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! CMOS real-time clock (MC146818A) on x86-64.
//!
//! The RTC is read through the index/data port pair `0x70`/`0x71`. The date and time are decoded
//! from BCD or binary per status register B, converted to a Unix epoch with the civil-date
//! arithmetic in [`crate::time`], and returned as epoch nanoseconds. The century is not stored by
//! the CMOS chip itself: the year register holds the year within the century, so this driver maps
//! it to `2000 + year` (valid 2000-2099), matching QEMU's default CMOS RTC and a modern BIOS. A
//! board without a usable CMOS RTC (or one whose update never settles) yields `None`, which the
//! caller reports as "no RTC" and `CLOCK_REALTIME` stays at zero.
#![allow(clippy::arithmetic_side_effects)] // BCD decode and civil-date arithmetic are bounded (0-99/0-23/valid dates).

use crate::arch::x86_64::pic;

/// CMOS index register.
const INDEX: u16 = 0x70;
/// CMOS data register.
const DATA: u16 = 0x71;

/// Seconds register.
const REG_SECONDS: u8 = 0x00;
/// Minutes register.
const REG_MINUTES: u8 = 0x02;
/// Hours register.
const REG_HOURS: u8 = 0x04;
/// Day-of-month register.
const REG_DAY: u8 = 0x07;
/// Month register.
const REG_MONTH: u8 = 0x08;
/// Year-within-century register.
const REG_YEAR: u8 = 0x09;
/// Status register A (update-in-progress flag).
const REG_STATUS_A: u8 = 0x0A;
/// Status register B (encoding and hour format).
const REG_STATUS_B: u8 = 0x0B;

/// Update-in-progress bit in status A.
const STATUS_A_UIP: u8 = 0x80;
/// Binary-mode bit in status B (set = binary, clear = BCD).
const STATUS_B_DM: u8 = 0x04;
/// 24-hour-mode bit in status B (set = 24h, clear = 12h).
const STATUS_B_24H: u8 = 0x02;
/// PM bit in the hours register in 12-hour mode.
const HOURS_PM: u8 = 0x80;

/// Read one CMOS register.
fn read_reg(register: u8) -> u8 {
    pic::outb(INDEX, register);
    pic::inb(DATA)
}

/// Decode a BCD or binary byte.
fn decode(byte: u8, bcd: bool) -> u32 {
    if bcd {
        u32::from(byte >> 4) * 10 + u32::from(byte & 0x0F)
    } else {
        u32::from(byte)
    }
}

/// Read the wall clock as epoch nanoseconds, or `None` when the RTC is absent or inconsistent.
pub fn read() -> Option<u64> {
    // A handful of full read attempts, each waiting out the update window.
    for _ in 0..3 {
        let status_b = read_reg(REG_STATUS_B);
        // Wait for the update-in-progress flag to clear (bounded spin).
        let mut spins = 0u32;
        while read_reg(REG_STATUS_A) & STATUS_A_UIP != 0 {
            if spins >= 1_000_000 {
                return None;
            }
            spins = spins.saturating_add(1);
        }
        // The update may have started between the wait and the read; capture the raw register
        // values and validate them, retrying if they are implausible.
        let second = read_reg(REG_SECONDS);
        let minute = read_reg(REG_MINUTES);
        let hour_raw = read_reg(REG_HOURS);
        let day = read_reg(REG_DAY);
        let month = read_reg(REG_MONTH);
        let year = read_reg(REG_YEAR);
        // A fresh update-in-progress after the reads means the values may be torn; retry.
        if read_reg(REG_STATUS_A) & STATUS_A_UIP != 0 {
            continue;
        }

        let bcd = status_b & STATUS_B_DM == 0;
        let second = decode(second, bcd);
        let minute = decode(minute, bcd);
        let hour = if status_b & STATUS_B_24H != 0 {
            decode(hour_raw, bcd)
        } else {
            let pm = hour_raw & HOURS_PM != 0;
            let hour = decode(hour_raw & !HOURS_PM, bcd);
            match (hour, pm) {
                (12, false) => 0,
                (h, true) if h < 12 => h + 12,
                (h, _) => h,
            }
        };
        let day = decode(day, bcd);
        let month = decode(month, bcd);
        let year = 2000u32 + decode(year, bcd);

        if !valid(second, minute, hour, day, month, year) {
            return None;
        }

        let days = crate::time::days_from_civil(i64::from(year), i64::from(month), i64::from(day));
        let seconds = days
            .saturating_mul(86_400)
            .saturating_add(i64::from(hour) * 3600)
            .saturating_add(i64::from(minute) * 60)
            .saturating_add(i64::from(second));
        return Some((seconds as u64).saturating_mul(1_000_000_000));
    }
    None
}

/// Range-check a decoded date/time.
fn valid(second: u32, minute: u32, hour: u32, day: u32, month: u32, year: u32) -> bool {
    (1..=59).contains(&second)
        && minute <= 59
        && hour <= 23
        && (1..=31).contains(&day)
        && (1..=12).contains(&month)
        && (2000..=2099).contains(&year)
}
