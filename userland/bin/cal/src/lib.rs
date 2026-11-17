// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure Gregorian calendar model for `cal`.
//!
//! The model uses the proleptic Gregorian calendar: no Julian/Gregorian reform gap is applied,
//! so every year 1 through 9999 has the normal 365/366-day shape. Month and weekday names are
//! the C-locale English spellings.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

/// C-locale month names in calendar order.
pub const MONTH_NAMES: [&str; 12] = [
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

const MIN_YEAR: i32 = 1;
const MAX_YEAR: i32 = 9999;
const BLOCK_WIDTH: usize = 20;
const WEEKDAY_HEADER: &str = "Su Mo Tu We Th Fr Sa";

/// The calendar view selected by the command line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    /// One month.
    Month,
    /// The previous, selected and next month side by side.
    Three,
    /// All twelve months of one year.
    Year,
}

/// A resolved `cal` request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Options {
    /// Selected view.
    pub view: View,
    /// Gregorian year, 1 through 9999.
    pub year: i32,
    /// Gregorian month, 1 through 12.
    pub month: u32,
}

/// Why an argument list is invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageError {
    /// An unknown option was supplied.
    UnknownOption,
    /// `-3` and `-y` were combined.
    ConflictingOptions,
    /// The month operand is not 1 through 12.
    BadMonth,
    /// The year operand is not 1 through 9999, or a three-month window would leave that range.
    BadYear,
    /// More operands than the selected view accepts were supplied.
    TooManyOperands,
}

/// Parse `cal [-3 | -y] [month [year]]`.
///
/// `today` is the `(year, month)` pair used when the command line has no operands. A single
/// operand is always a year and selects the whole-year view, matching the POSIX form
/// `cal [[month] year]`. The three-month view needs the neighbouring months, so it accepts only
/// years 2 through 9998.
///
/// # Errors
///
/// Returns [`UsageError`] for an unknown option, conflicting options, an invalid month or year,
/// or too many operands.
pub fn parse_args<'a>(
    args: impl IntoIterator<Item = &'a str>,
    today: (i32, u32),
) -> Result<Options, UsageError> {
    let mut three = false;
    let mut yearly = false;
    let mut operands: Vec<&str> = Vec::new();
    let mut options = true;
    for argument in args {
        if options && argument == "--" {
            options = false;
            continue;
        }
        if options && argument.len() > 1 && argument.starts_with('-') {
            match argument {
                "-3" => three = true,
                "-y" => yearly = true,
                _ => return Err(UsageError::UnknownOption),
            }
            continue;
        }
        operands.push(argument);
    }
    if three && yearly {
        return Err(UsageError::ConflictingOptions);
    }
    let (mut year, mut month) = today;
    let view = match operands.len() {
        0 => {
            if yearly {
                View::Year
            } else if three {
                View::Three
            } else {
                View::Month
            }
        }
        1 => {
            year = parse_year(operands[0])?;
            month = 1;
            View::Year
        }
        2 => {
            month = parse_month(operands[0])?;
            year = parse_year(operands[1])?;
            if yearly {
                return Err(UsageError::TooManyOperands);
            }
            if three { View::Three } else { View::Month }
        }
        _ => return Err(UsageError::TooManyOperands),
    };
    if view == View::Year {
        month = 1;
    }
    if view == View::Three && !(MIN_YEAR + 1..=MAX_YEAR - 1).contains(&year) {
        return Err(UsageError::BadYear);
    }
    Ok(Options { view, year, month })
}

fn parse_year(text: &str) -> Result<i32, UsageError> {
    let year: i32 = text.parse().map_err(|_| UsageError::BadYear)?;
    if !(MIN_YEAR..=MAX_YEAR).contains(&year) {
        return Err(UsageError::BadYear);
    }
    Ok(year)
}

fn parse_month(text: &str) -> Result<u32, UsageError> {
    let month: u32 = text.parse().map_err(|_| UsageError::BadMonth)?;
    if !(1..=12).contains(&month) {
        return Err(UsageError::BadMonth);
    }
    Ok(month)
}

/// True when `year` is a proleptic Gregorian leap year.
#[must_use]
pub fn is_leap(year: i32) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

/// Number of days in `month` of `year`.
#[must_use]
pub fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap(year) => 29,
        2 => 28,
        _ => 0,
    }
}

/// Day of the week for a Gregorian date, with Sunday as 0 and Saturday as 6.
#[must_use]
pub fn weekday(year: i32, month: u32, day: u32) -> u32 {
    const SHIFT: [i32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let mut adjusted = year;
    if month < 3 {
        adjusted -= 1;
    }
    let index = usize::try_from(month.saturating_sub(1)).unwrap_or(0);
    let offset = SHIFT.get(index).copied().unwrap_or(0);
    let value = adjusted + adjusted / 4 - adjusted / 100
        + adjusted / 400
        + offset
        + i32::try_from(day).unwrap_or(0);
    value.rem_euclid(7) as u32
}

/// Convert Unix seconds to a proleptic Gregorian `(year, month, day)` tuple.
///
/// The conversion is exact for negative seconds too; it does not depend on a wall clock.
#[must_use]
pub fn civil_from_unix(seconds: i64) -> (i32, u32, u32) {
    let mut days = seconds.div_euclid(86_400);
    let mut year = 1970i32;
    while days < 0 {
        year -= 1;
        days += i64::from(days_in_year(year));
    }
    while days >= i64::from(days_in_year(year)) {
        days -= i64::from(days_in_year(year));
        year += 1;
    }
    let mut month = 1u32;
    while month < 12 && days >= i64::from(days_in_month(year, month)) {
        days -= i64::from(days_in_month(year, month));
        month += 1;
    }
    (year, month, u32::try_from(days + 1).unwrap_or(1))
}

fn days_in_year(year: i32) -> u32 {
    if is_leap(year) { 366 } else { 365 }
}

/// Render the calendar selected by `options`.
#[must_use]
pub fn render(options: &Options) -> String {
    let lines = match options.view {
        View::Month => join_blocks(&[month_lines(options.year, options.month)]),
        View::Three => {
            let blocks: Vec<Vec<String>> = [-1i32, 0, 1]
                .iter()
                .map(|delta| {
                    let (year, month) = shift(options.year, options.month, *delta);
                    month_lines(year, month)
                })
                .collect();
            join_blocks(&blocks)
        }
        View::Year => year_lines(options.year),
    };
    to_output(&lines)
}

fn shift(year: i32, month: u32, delta: i32) -> (i32, u32) {
    let index = (year - 1) * 12 + i32::try_from(month).unwrap_or(1) - 1 + delta;
    let shifted_year = index.div_euclid(12) + 1;
    let shifted_month = index.rem_euclid(12) + 1;
    (shifted_year, u32::try_from(shifted_month).unwrap_or(1))
}

fn year_lines(year: i32) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push(center(&format!("{year}"), BLOCK_WIDTH * 3 + 4));
    lines.push(String::new());
    for quarter in 0u32..4 {
        let start = quarter * 3 + 1;
        let blocks: Vec<Vec<String>> = (start..start + 3)
            .map(|month| month_lines(year, month))
            .collect();
        lines.extend(join_blocks(&blocks));
        if quarter < 3 {
            lines.push(String::new());
        }
    }
    lines
}

fn month_lines(year: i32, month: u32) -> Vec<String> {
    let name = MONTH_NAMES
        .get(usize::try_from(month.saturating_sub(1)).unwrap_or(0))
        .copied()
        .unwrap_or("?");
    let title = format!("{name} {year}");
    let mut lines = Vec::new();
    lines.push(center(&title, BLOCK_WIDTH));
    lines.push(String::from(WEEKDAY_HEADER));
    let mut cells: Vec<Option<u32>> = Vec::new();
    for _ in 0..weekday(year, month, 1) {
        cells.push(None);
    }
    for day in 1..=days_in_month(year, month) {
        cells.push(Some(day));
    }
    while !cells.len().is_multiple_of(7) {
        cells.push(None);
    }
    for week in cells.chunks(7) {
        let mut line = String::new();
        for (index, cell) in week.iter().enumerate() {
            if index > 0 {
                line.push(' ');
            }
            match cell {
                Some(day) => line.push_str(&format!("{day:>2}")),
                None => line.push_str("  "),
            }
        }
        while line.ends_with(' ') {
            line.pop();
        }
        lines.push(line);
    }
    lines
}

fn join_blocks(blocks: &[Vec<String>]) -> Vec<String> {
    let height = blocks.iter().map(Vec::len).max().unwrap_or(0);
    let mut lines = Vec::with_capacity(height);
    for row in 0..height {
        let mut line = String::new();
        for (index, block) in blocks.iter().enumerate() {
            if index > 0 {
                line.push_str("  ");
            }
            let text = block.get(row).map_or("", String::as_str);
            line.push_str(text);
            for _ in text.chars().count()..BLOCK_WIDTH {
                line.push(' ');
            }
        }
        while line.ends_with(' ') {
            line.pop();
        }
        lines.push(line);
    }
    lines
}

fn center(text: &str, width: usize) -> String {
    let length = text.chars().count();
    if length >= width {
        return String::from(text);
    }
    let mut line = String::new();
    for _ in 0..(width - length) / 2 {
        line.push(' ');
    }
    line.push_str(text);
    line
}

fn to_output(lines: &[String]) -> String {
    let mut output = String::new();
    for line in lines {
        output.push_str(line);
        output.push('\n');
    }
    output
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
