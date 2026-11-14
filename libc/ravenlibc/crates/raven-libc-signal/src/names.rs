// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Signal names, Linux numbering (DR-13).
//!
//! CLEANROOM from the table in ticket C58 section 4.2 and `musl/include/signal.h` numbers; the
//! BSD numbering of `freebsd:lib/libc/gen/siglist.c` is deliberately not used (`SIGUSR1` is 30
//! there, 10 here). `name_of` returns the canonical name, `number_of` also accepts the aliases
//! and an optional `SIG` prefix, case-insensitively.

/// `(number, canonical name, aliases)` sorted by number.
const TABLE: &[(i32, &str, &[&str])] = &[
    (1, "HUP", &[]),
    (2, "INT", &[]),
    (3, "QUIT", &[]),
    (4, "ILL", &[]),
    (5, "TRAP", &[]),
    (6, "ABRT", &["IOT"]),
    (7, "BUS", &[]),
    (8, "FPE", &[]),
    (9, "KILL", &[]),
    (10, "USR1", &[]),
    (11, "SEGV", &[]),
    (12, "USR2", &[]),
    (13, "PIPE", &[]),
    (14, "ALRM", &[]),
    (15, "TERM", &[]),
    (16, "STKFLT", &[]),
    (17, "CHLD", &[]),
    (18, "CONT", &[]),
    (19, "STOP", &[]),
    (20, "TSTP", &[]),
    (21, "TTIN", &[]),
    (22, "TTOU", &[]),
    (23, "URG", &[]),
    (24, "XCPU", &[]),
    (25, "XFSZ", &[]),
    (26, "VTALRM", &[]),
    (27, "PROF", &[]),
    (28, "WINCH", &[]),
    (29, "IO", &["POLL"]),
    (30, "PWR", &[]),
    (31, "SYS", &[]),
];

/// The canonical name of a signal, or `None` for 32..34 and out-of-range numbers.
#[must_use]
pub fn name_of(sig: i32) -> Option<&'static str> {
    TABLE
        .iter()
        .find(|(number, _, _)| *number == sig)
        .map(|(_, name, _)| *name)
}

/// The number of a signal name (case-insensitive, optional `SIG` prefix, aliases accepted).
#[must_use]
pub fn number_of(name: &str) -> Option<i32> {
    let stripped = strip_sig_prefix(name.trim());
    if stripped.is_empty() {
        return None;
    }
    for (number, canonical, aliases) in TABLE {
        if stripped.eq_ignore_ascii_case(canonical) {
            return Some(*number);
        }
        for alias in *aliases {
            if stripped.eq_ignore_ascii_case(alias) {
                return Some(*number);
            }
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

/// Every canonical `(number, name)` pair, for table tests and `strsignal` (C40).
#[must_use]
pub const fn table() -> &'static [(i32, &'static str, &'static [&'static str])] {
    TABLE
}

#[cfg(test)]
#[path = "names_ut.rs"]
mod names_ut;
