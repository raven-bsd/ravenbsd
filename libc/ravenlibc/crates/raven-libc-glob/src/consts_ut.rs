// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the flag values and limits.

use super::*;

#[test]
fn fnmatch_values_are_the_freebsd_header() {
    assert_eq!(FNM_NOESCAPE, 0x01);
    assert_eq!(FNM_PATHNAME, 0x02);
    assert_eq!(FNM_PERIOD, 0x04);
    assert_eq!(FNM_LEADING_DIR, 0x08);
    assert_eq!(FNM_CASEFOLD, 0x10);
    assert_eq!(FNM_IGNORECASE, FNM_CASEFOLD);
    assert_eq!(FNM_FILE_NAME, FNM_PATHNAME);
    assert_eq!(FNM_NOMATCH, 1);
    assert_eq!(FNM_NOSYS, -1);
}

#[test]
fn glob_values_are_the_freebsd_header() {
    assert_eq!(GLOB_APPEND, 1);
    assert_eq!(GLOB_DOOFFS, 2);
    assert_eq!(GLOB_ERR, 4);
    assert_eq!(GLOB_MARK, 8);
    assert_eq!(GLOB_NOCHECK, 0x10);
    assert_eq!(GLOB_NOSORT, 0x20);
    assert_eq!(GLOB_ALTDIRFUNC, 0x40);
    assert_eq!(GLOB_BRACE, 0x80);
    assert_eq!(GLOB_MAGCHAR, 0x100);
    assert_eq!(GLOB_NOMAGIC, 0x200);
    assert_eq!(GLOB_QUOTE, 0x400);
    assert_eq!(GLOB_TILDE, 0x800);
    assert_eq!(GLOB_LIMIT, 0x1000);
    assert_eq!(GLOB_NOESCAPE, 0x2000);
    assert_eq!(GLOB_MAXPATH, GLOB_LIMIT);
}

#[test]
fn glob_errors_and_limits() {
    assert_eq!(GLOB_NOSPACE, -1);
    assert_eq!(GLOB_ABORTED, -2);
    assert_eq!(GLOB_NOMATCH, -3);
    assert_eq!(GLOB_NOSYS, -4);
    assert_eq!(GLOB_ABEND, GLOB_ABORTED);
    assert_eq!(GLOB_LIMIT_BRACE, 128);
    assert_eq!(GLOB_LIMIT_PATH, 65536);
    assert_eq!(GLOB_LIMIT_READDIR, 16384);
    assert_eq!(GLOB_LIMIT_STAT, 1024);
    assert_eq!(GLOB_LIMIT_STRING, 131072);
    assert_eq!(MAXPATHLEN, 4096);
}

#[test]
fn wordexp_values_are_the_freebsd_header() {
    assert_eq!(WRDE_APPEND, 0x1);
    assert_eq!(WRDE_DOOFFS, 0x2);
    assert_eq!(WRDE_NOCMD, 0x4);
    assert_eq!(WRDE_REUSE, 0x8);
    assert_eq!(WRDE_SHOWERR, 0x10);
    assert_eq!(WRDE_UNDEF, 0x20);
    assert_eq!(WRDE_BADCHAR, 1);
    assert_eq!(WRDE_BADVAL, 2);
    assert_eq!(WRDE_CMDSUB, 3);
    assert_eq!(WRDE_NOSPACE, 4);
    assert_eq!(WRDE_NOSYS, 5);
    assert_eq!(WRDE_SYNTAX, 6);
}

#[test]
fn wordexp_script_has_four_backslashes() {
    let script = WORDEXP_SCRIPT;
    assert_eq!(script, b"eval \"set --;printf %s\\\\\\\\0 x $1\"");
    assert_eq!(script.iter().filter(|&&b| b == b'\\').count(), 4);
}
