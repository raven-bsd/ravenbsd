// SPDX-License-Identifier: LicenseRef-PublicDomain
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from netbsd:lib/libc/time/asctime.c @
// 36eddebcc36cb0775aa2962005b9c05419a6ff01. The donor file is in the public
// domain, so clarified as of 1996-06-05 by Arthur David Olson.

//! `asctime_r`: the fixed `Www Mmm dd hh:mm:ss yyyy\n` form.
#![allow(clippy::arithmetic_side_effects)] // RavenBSD: donor arithmetic, checked by the vectors.

use alloc::format;
use raven_libc_abi::errno::Errno;
use raven_libc_abi::errno::{EINVAL, EOVERFLOW};

use crate::sys::errno;
use crate::tz::Tm;
use crate::tz::consts::TM_YEAR_BASE;

const WDAY_NAME: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MON_NAME: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Render `tm` into exactly 26 bytes, including the trailing NUL.
///
/// The result is the classic fixed form, not a locale-dependent `strftime`.
pub fn asctime_r(tm: &Tm, out: &mut [u8; 26]) -> Result<(), Errno> {
    let weekday = usize::try_from(tm.tm_wday)
        .ok()
        .and_then(|index| WDAY_NAME.get(index))
        .copied()
        .unwrap_or("???");
    let month = usize::try_from(tm.tm_mon)
        .ok()
        .and_then(|index| MON_NAME.get(index))
        .copied()
        .unwrap_or("???");
    let year = i64::from(tm.tm_year) + i64::from(TM_YEAR_BASE);
    let text = if (-999..=9999).contains(&year) {
        format!(
            "{} {}{:>3} {:02}:{:02}:{:02} {:04}\n",
            weekday, month, tm.tm_mday, tm.tm_hour, tm.tm_min, tm.tm_sec, year
        )
    } else {
        format!(
            "{} {}{:>3} {:02}:{:02}:{:02}     {}\n",
            weekday, month, tm.tm_mday, tm.tm_hour, tm.tm_min, tm.tm_sec, year
        )
    };
    if text.len() >= out.len() {
        return Err(errno(EOVERFLOW));
    }
    out.fill(0);
    if let Some(destination) = out.get_mut(..text.len()) {
        destination.copy_from_slice(text.as_bytes());
    }
    Ok(())
}

/// Render a `Tm` that may be malformed; `None` becomes the donor's `???` text.
pub fn asctime_checked(tm: Option<&Tm>, out: &mut [u8; 26]) -> Result<(), Errno> {
    match tm {
        Some(tm) => asctime_r(tm, out),
        None => {
            let text = b"??? ??? ?? ??:??:?? ????\n";
            out.fill(0);
            if let Some(destination) = out.get_mut(..text.len()) {
                destination.copy_from_slice(text);
            }
            Err(errno(EINVAL))
        }
    }
}

#[cfg(test)]
#[path = "asctime_ut.rs"]
mod tests;
