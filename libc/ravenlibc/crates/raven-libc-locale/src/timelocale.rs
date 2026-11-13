// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The C-locale `TimeLocale` table (CLEANROOM; donor-map 06 3.3).

use crate::locale::{Locale, NameTable};

/// The time strings consumed by `strftime`/`strptime`.
pub trait TimeLocale {
    /// Full weekday name.
    fn day(&self, i: usize) -> &str;
    /// Abbreviated weekday name.
    fn abday(&self, i: usize) -> &str;
    /// Full month name.
    fn mon(&self, i: usize) -> &str;
    /// Abbreviated month name.
    fn abmon(&self, i: usize) -> &str;
    /// AM/PM.
    fn am_pm(&self, i: usize) -> &str;
    /// `%c`.
    fn d_t_fmt(&self) -> &str;
    /// `%x`.
    fn d_fmt(&self) -> &str;
    /// `%X`.
    fn t_fmt(&self) -> &str;
    /// `%r`.
    fn t_fmt_ampm(&self) -> &str;
    /// `%Ec`.
    fn era_d_t_fmt(&self) -> &str;
}

/// The POSIX C time table.
#[derive(Clone, Copy, Debug, Default)]
pub struct CTime;

const DAY: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];
const ABDAY: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MON: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const ABMON: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

impl TimeLocale for CTime {
    fn day(&self, i: usize) -> &str {
        DAY.get(i).copied().unwrap_or("")
    }
    fn abday(&self, i: usize) -> &str {
        ABDAY.get(i).copied().unwrap_or("")
    }
    fn mon(&self, i: usize) -> &str {
        MON.get(i).copied().unwrap_or("")
    }
    fn abmon(&self, i: usize) -> &str {
        ABMON.get(i).copied().unwrap_or("")
    }
    fn am_pm(&self, i: usize) -> &str {
        match i {
            0 => "AM",
            1 => "PM",
            _ => "",
        }
    }
    fn d_t_fmt(&self) -> &str {
        "%a %b %e %T %Y"
    }
    fn d_fmt(&self) -> &str {
        "%m/%d/%y"
    }
    fn t_fmt(&self) -> &str {
        "%H:%M:%S"
    }
    fn t_fmt_ampm(&self) -> &str {
        "%I:%M:%S %p"
    }
    fn era_d_t_fmt(&self) -> &str {
        "%a %b %e %T %Y"
    }
}

/// The shared C table.
pub static C_TIME: CTime = CTime;

/// The table for any locale (C and C.UTF-8 share it).
#[must_use]
pub fn time_locale(_loc: &Locale, _table: &NameTable) -> &'static dyn TimeLocale {
    &C_TIME
}

#[cfg(test)]
#[path = "timelocale_ut.rs"]
mod tests;
