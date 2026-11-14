// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Signal names, Linux numbering (DR-13).
//!
//! CLEANROOM from the table in ticket C58 section 4.2 and `musl/include/signal.h` numbers; the
//! BSD numbering of `freebsd:lib/libc/gen/siglist.c` is deliberately not used (`SIGUSR1` is 30
//! there, 10 here). `name_of` returns the canonical name, `number_of` also accepts the aliases
//! and an optional `SIG` prefix, case-insensitively.
//!
//! The names live in one NUL-separated blob and the table stores `u16` offsets (R-12): a table of
//! string pointers would add one relocation per name.

/// All canonical names in number order, then the two aliases (`IOT`, `POLL`).
const NAME_BLOB: &[u8] = b"HUP\0INT\0QUIT\0ILL\0TRAP\0ABRT\0BUS\0FPE\0KILL\0USR1\0SEGV\0USR2\0PIPE\0ALRM\0TERM\0STKFLT\0CHLD\0CONT\0STOP\0TSTP\0TTIN\0TTOU\0URG\0XCPU\0XFSZ\0VTALRM\0PROF\0WINCH\0IO\0PWR\0SYS\0IOT\0POLL\0";

/// Sentinel for a row without an alias.
const NO_ALIAS: u16 = u16::MAX;

/// `(number, canonical offset, alias offset)` sorted by number.
const TABLE: &[(i32, u16, u16)] = &[
    (1, 0, NO_ALIAS),
    (2, 4, NO_ALIAS),
    (3, 8, NO_ALIAS),
    (4, 13, NO_ALIAS),
    (5, 17, NO_ALIAS),
    (6, 22, 150),
    (7, 27, NO_ALIAS),
    (8, 31, NO_ALIAS),
    (9, 35, NO_ALIAS),
    (10, 40, NO_ALIAS),
    (11, 45, NO_ALIAS),
    (12, 50, NO_ALIAS),
    (13, 55, NO_ALIAS),
    (14, 60, NO_ALIAS),
    (15, 65, NO_ALIAS),
    (16, 70, NO_ALIAS),
    (17, 77, NO_ALIAS),
    (18, 82, NO_ALIAS),
    (19, 87, NO_ALIAS),
    (20, 92, NO_ALIAS),
    (21, 97, NO_ALIAS),
    (22, 102, NO_ALIAS),
    (23, 107, NO_ALIAS),
    (24, 111, NO_ALIAS),
    (25, 116, NO_ALIAS),
    (26, 121, NO_ALIAS),
    (27, 128, NO_ALIAS),
    (28, 133, NO_ALIAS),
    (29, 139, 154),
    (30, 142, NO_ALIAS),
    (31, 146, NO_ALIAS),
];

/// The NUL-terminated text at `offset`.
fn text_at(offset: u16) -> Option<&'static str> {
    let rest = NAME_BLOB.get(usize::from(offset)..)?;
    let end = rest.iter().position(|byte| *byte == 0)?;
    core::str::from_utf8(rest.get(..end)?).ok()
}

/// The canonical name of a signal, or `None` for 32..34 and out-of-range numbers.
#[must_use]
pub fn name_of(sig: i32) -> Option<&'static str> {
    TABLE
        .iter()
        .find(|(number, _, _)| *number == sig)
        .and_then(|(_, name, _)| text_at(*name))
}

/// The number of a signal name (case-insensitive, optional `SIG` prefix, aliases accepted).
#[must_use]
pub fn number_of(name: &str) -> Option<i32> {
    let stripped = strip_sig_prefix(name.trim());
    if stripped.is_empty() {
        return None;
    }
    for (number, canonical, alias) in TABLE {
        if text_at(*canonical).is_some_and(|text| stripped.eq_ignore_ascii_case(text)) {
            return Some(*number);
        }
        if *alias != NO_ALIAS
            && text_at(*alias).is_some_and(|text| stripped.eq_ignore_ascii_case(text))
        {
            return Some(*number);
        }
    }
    None
}

/// Remove one leading `SIG` in any case.
#[must_use]
pub fn strip_sig_prefix(name: &str) -> &str {
    let bytes = name.as_bytes();
    if bytes.len() >= 3
        && bytes.first().is_some_and(|b| *b == b'S' || *b == b's')
        && bytes.get(1).is_some_and(|b| *b == b'I' || *b == b'i')
        && bytes.get(2).is_some_and(|b| *b == b'G' || *b == b'g')
    {
        name.get(3..).unwrap_or("")
    } else {
        name
    }
}

/// Every canonical `(number, name, alias)` row, for table tests.
pub fn table() -> impl Iterator<Item = (i32, &'static str, Option<&'static str>)> {
    TABLE.iter().map(|(number, canonical, alias)| {
        (
            *number,
            text_at(*canonical).unwrap_or(""),
            if *alias == NO_ALIAS {
                None
            } else {
                text_at(*alias)
            },
        )
    })
}

#[cfg(test)]
#[path = "names_ut.rs"]
mod names_ut;
