// SPDX-License-Identifier: MIT
// RavenBSD: transpiled from the pinned donor; see PROVENANCE.toml.
// Copyright (c) 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: the `langinfo` item table from musl:src/locale/langinfo.c and
// musl:include/langinfo.h @ 7813e9119eb5fe681b6750b8a033a0490bca0941.

//! `nl_langinfo` and the musl item numbering.
#![allow(missing_docs)] // The item constants mirror musl's header.

use crate::locale::{LC_ALL, LC_MESSAGES, LC_MONETARY, LC_NUMERIC, LC_TIME, Locale, NameTable};

/// `CODESET`.
pub const CODESET: u32 = 14;

macro_rules! item {
    ($name:ident = $value:expr) => {
        pub const $name: u32 = $value;
    };
}

item!(ABDAY_1 = 0x20000);
item!(ABDAY_2 = 0x20001);
item!(ABDAY_3 = 0x20002);
item!(ABDAY_4 = 0x20003);
item!(ABDAY_5 = 0x20004);
item!(ABDAY_6 = 0x20005);
item!(ABDAY_7 = 0x20006);
item!(DAY_1 = 0x20007);
item!(DAY_2 = 0x20008);
item!(DAY_3 = 0x20009);
item!(DAY_4 = 0x2000a);
item!(DAY_5 = 0x2000b);
item!(DAY_6 = 0x2000c);
item!(DAY_7 = 0x2000d);
item!(ABMON_1 = 0x2000e);
item!(ABMON_2 = 0x2000f);
item!(ABMON_3 = 0x20010);
item!(ABMON_4 = 0x20011);
item!(ABMON_5 = 0x20012);
item!(ABMON_6 = 0x20013);
item!(ABMON_7 = 0x20014);
item!(ABMON_8 = 0x20015);
item!(ABMON_9 = 0x20016);
item!(ABMON_10 = 0x20017);
item!(ABMON_11 = 0x20018);
item!(ABMON_12 = 0x20019);
item!(MON_1 = 0x2001a);
item!(MON_2 = 0x2001b);
item!(MON_3 = 0x2001c);
item!(MON_4 = 0x2001d);
item!(MON_5 = 0x2001e);
item!(MON_6 = 0x2001f);
item!(MON_7 = 0x20020);
item!(MON_8 = 0x20021);
item!(MON_9 = 0x20022);
item!(MON_10 = 0x20023);
item!(MON_11 = 0x20024);
item!(MON_12 = 0x20025);
item!(AM_STR = 0x20026);
item!(PM_STR = 0x20027);
item!(D_T_FMT = 0x20028);
item!(D_FMT = 0x20029);
item!(T_FMT = 0x2002a);
item!(T_FMT_AMPM = 0x2002b);
item!(ERA = 0x2002c);
item!(ERA_D_FMT = 0x2002e);
item!(ALT_DIGITS = 0x2002f);
item!(ERA_D_T_FMT = 0x20030);
item!(ERA_T_FMT = 0x20031);
item!(CRNCYSTR = 0x4000f);
item!(RADIXCHAR = 0x10000);
item!(THOUSEP = 0x10001);
item!(YESEXPR = 0x50000);
item!(NOEXPR = 0x50001);
item!(YESSTR = 0x50002);
item!(NOSTR = 0x50003);

const TIME: [&[u8]; 0x32] = [
    b"Sun\0",
    b"Mon\0",
    b"Tue\0",
    b"Wed\0",
    b"Thu\0",
    b"Fri\0",
    b"Sat\0",
    b"Sunday\0",
    b"Monday\0",
    b"Tuesday\0",
    b"Wednesday\0",
    b"Thursday\0",
    b"Friday\0",
    b"Saturday\0",
    b"Jan\0",
    b"Feb\0",
    b"Mar\0",
    b"Apr\0",
    b"May\0",
    b"Jun\0",
    b"Jul\0",
    b"Aug\0",
    b"Sep\0",
    b"Oct\0",
    b"Nov\0",
    b"Dec\0",
    b"January\0",
    b"February\0",
    b"March\0",
    b"April\0",
    b"May\0",
    b"June\0",
    b"July\0",
    b"August\0",
    b"September\0",
    b"October\0",
    b"November\0",
    b"December\0",
    b"AM\0",
    b"PM\0",
    b"%a %b %e %T %Y\0",
    b"%m/%d/%y\0",
    b"%H:%M:%S\0",
    b"%I:%M:%S %p\0",
    b"\0",
    b"\0",
    b"%m/%d/%y\0",
    b"0123456789\0",
    b"%a %b %e %T %Y\0",
    b"%H:%M:%S\0",
];

const MESSAGES: [&[u8]; 4] = [b"^[yY]\0", b"^[nN]\0", b"yes\0", b"no\0"];

/// `nl_langinfo`; every returned slice is NUL-terminated static data.
#[must_use]
pub fn nl_langinfo(item: u32, loc: &Locale, table: &NameTable) -> &'static [u8] {
    if item == CODESET {
        return if loc.ctype_is_utf8(table) {
            b"UTF-8\0"
        } else {
            b"ANSI_X3.4-1968\0"
        };
    }
    let cat = (item >> 16) as usize;
    let index = (item & 0xffff) as usize;
    if index == 0xffff && cat < LC_ALL {
        return b"\0";
    }
    match cat {
        LC_NUMERIC => match index {
            0 => b".\0",
            1 => b"\0",
            _ => b"\0",
        },
        LC_TIME => TIME.get(index).copied().unwrap_or(b"\0"),
        LC_MONETARY => b"\0",
        LC_MESSAGES => MESSAGES.get(index).copied().unwrap_or(b"\0"),
        _ => b"\0",
    }
}

/// `_NL_LOCALE_NAME(cat)`; `None` for anything else.
#[must_use]
pub fn nl_langinfo_name<'a>(item: u32, loc: &Locale, table: &'a NameTable) -> Option<&'a [u8]> {
    let cat = (item >> 16) as usize;
    if item & 0xffff == 0xffff
        && cat < LC_ALL
        && let Some(id) = loc.cat.get(cat)
    {
        Some(table.name(*id))
    } else {
        None
    }
}

#[cfg(test)]
#[path = "langinfo_ut.rs"]
mod tests;
