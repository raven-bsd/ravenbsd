// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The locale seam used by `strftime`, `strptime` and `wcsftime`.
#![allow(clippy::arithmetic_side_effects)] // RavenBSD: the table lookups are index-checked.
//!
//! C48 (locale) is a later ticket; until then this crate owns the trait and the
//! constant C/POSIX table. `C.UTF-8` uses the same time strings, so one table is
//! enough for both.

/// The time strings the formatters read.
pub trait TimeLocale {
    /// Full weekday name, Sunday is 0.
    fn day(&self, index: usize) -> &str;
    /// Abbreviated weekday name, Sunday is 0.
    fn abday(&self, index: usize) -> &str;
    /// Full month name, January is 0.
    fn mon(&self, index: usize) -> &str;
    /// Abbreviated month name, January is 0.
    fn abmon(&self, index: usize) -> &str;
    /// `AM` or `PM`.
    fn am_pm(&self, index: usize) -> &str;
    /// `%c` format.
    fn d_t_fmt(&self) -> &str;
    /// `%x` format.
    fn d_fmt(&self) -> &str;
    /// `%X` format.
    fn t_fmt(&self) -> &str;
    /// `%r` format.
    fn t_fmt_ampm(&self) -> &str;
}

/// The POSIX C locale.
#[derive(Clone, Copy, Debug, Default)]
pub struct CLocale;

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

impl TimeLocale for CLocale {
    fn day(&self, index: usize) -> &str {
        DAY.get(index).copied().unwrap_or("")
    }

    fn abday(&self, index: usize) -> &str {
        ABDAY.get(index).copied().unwrap_or("")
    }

    fn mon(&self, index: usize) -> &str {
        MON.get(index).copied().unwrap_or("")
    }

    fn abmon(&self, index: usize) -> &str {
        ABMON.get(index).copied().unwrap_or("")
    }

    fn am_pm(&self, index: usize) -> &str {
        match index {
            0 => "AM",
            1 => "PM",
            _ => "",
        }
    }

    fn d_t_fmt(&self) -> &str {
        "%a %b %e %H:%M:%S %Y"
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
}

#[cfg(test)]
#[path = "locale_ut.rs"]
mod tests;
