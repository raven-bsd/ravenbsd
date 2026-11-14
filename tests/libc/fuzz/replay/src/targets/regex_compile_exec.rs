// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Target 7: `regcomp`/`regexec`/`regerror`/`regasub` (C45).
//!
//! The body compiles arbitrary patterns in the byte or UTF-8 locale, executes them with three
//! match counts, formats every error code and checks the hardening-limit classes.

use raven_libc_regex::consts::{
    REG_BADPAT, REG_EBRACE, REG_ESPACE, REG_EXTENDED, REG_ICASE, REG_NEWLINE, REG_NOMATCH,
    REG_NOSPEC, REG_NOSUB, REG_NOTBOL, REG_NOTEOL, REG_PEND, REG_STARTEND,
};
use raven_libc_regex::{
    ByteLocale, RegError, RegMatch, Utf8Locale, regasub, regcomp, regerror, regexec,
};

use crate::{Cursor, alloc_probe};

fn hash(parts: &[u64]) -> u64 {
    let mut value = 0xcbf2_9ce4_8422_2325u64;
    for part in parts {
        value ^= *part;
        value = value.wrapping_mul(0x100_0000_01b3);
    }
    value
}

/// Run target 7.
pub fn body(data: &[u8]) {
    let mut cursor = Cursor::new(data);
    let cflags_byte = cursor.u8();
    let eflags_byte = cursor.u8();
    let split = usize::from(cursor.u16()) % 257;
    let pattern = cursor.take(split);
    let subject = cursor.take(1024);

    let mut cflags = 0u32;
    for (bit, flag) in [
        REG_EXTENDED,
        REG_ICASE,
        REG_NOSUB,
        REG_NEWLINE,
        REG_NOSPEC,
        REG_PEND,
    ]
    .iter()
    .enumerate()
    {
        if cflags_byte & (1 << bit) != 0 {
            cflags |= *flag;
        }
    }
    let mut eflags = 0u32;
    for (bit, flag) in [REG_NOTBOL, REG_NOTEOL, REG_STARTEND].iter().enumerate() {
        if eflags_byte & (1 << bit) != 0 {
            eflags |= *flag;
        }
    }
    let locale: &dyn raven_libc_regex::RegexLocale = if cflags_byte & 0x40 != 0 {
        &Utf8Locale
    } else {
        &ByteLocale
    };

    let compiled = regcomp(pattern, cflags, locale);
    let mut matches = 0u64;
    match &compiled {
        Ok(regex) => {
            for count in [0usize, 1, 10] {
                let mut pmatch = vec![
                    RegMatch {
                        rm_so: -1,
                        rm_eo: -1
                    };
                    count
                ];
                if count > 0
                    && eflags & REG_STARTEND != 0
                    && let Some(first) = pmatch.first_mut()
                {
                    first.rm_so = 0;
                    first.rm_eo = i64::try_from(subject.len()).unwrap_or(i64::MAX);
                }
                let call_eflags = if count == 0 {
                    eflags & !REG_STARTEND
                } else {
                    eflags
                };
                match regexec(regex, subject, &mut pmatch, call_eflags, locale) {
                    Ok(()) => {
                        matches = matches.saturating_add(1);
                        for entry in &pmatch {
                            if entry.rm_so >= 0 {
                                let so = usize::try_from(entry.rm_so).unwrap_or(usize::MAX);
                                let eo = usize::try_from(entry.rm_eo).unwrap_or(usize::MAX);
                                assert!(
                                    so <= eo && eo <= subject.len(),
                                    "P-regex-3: a match offset left the subject"
                                );
                            }
                        }
                    }
                    Err(error) => assert_eq!(
                        error.0, REG_NOMATCH,
                        "P-regex-2: regexec returned an undocumented code"
                    ),
                }
            }
            if !subject.is_empty() {
                let pmatch = [RegMatch {
                    rm_so: 0,
                    rm_eo: i64::try_from(subject.len()).unwrap_or(0),
                }];
                let _ = regasub(b"\\0", &pmatch, subject);
            }
        }
        Err(RegError(code)) => {
            assert!(
                (REG_BADPAT..=17).contains(code),
                "P-regex-2: regcomp returned an undocumented code {code}"
            );
        }
    }

    for code in [
        1i32, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 255, 0o401,
    ] {
        let mut large = [0u8; 128];
        let needed = regerror(code, compiled.as_ref().ok(), &mut large);
        assert!(needed >= 1, "P-regex-4: regerror returned no length");
        if let Some(last) = large.get(needed.saturating_sub(1)) {
            assert_eq!(*last, 0, "P-regex-4: regerror left an unterminated message");
        }
        let mut small = [0u8; 4];
        let short = regerror(code, compiled.as_ref().ok(), &mut small);
        assert_eq!(
            short, needed,
            "P-regex-4: the needed length depends on the buffer"
        );
        assert_eq!(
            small.last().copied(),
            Some(0),
            "P-regex-4: regerror overran a small buffer"
        );
    }

    let limits = [b"a{256}".as_slice(), b"a{1,0}", b"^*", b"\\1"];
    for pattern in limits {
        if let Err(RegError(code)) = regcomp(pattern, REG_EXTENDED, locale) {
            assert!(
                code == REG_ESPACE || code == REG_EBRACE || (REG_BADPAT..=17).contains(&code),
                "P-regex-5: a hardening limit surfaced as {code}"
            );
        }
    }

    alloc_probe::note(hash(&[
        u64::from(cflags),
        u64::from(eflags),
        matches,
        pattern.len() as u64,
        subject.len() as u64,
    ]));
}

#[cfg(test)]
#[path = "regex_compile_exec_ut.rs"]
mod tests;
