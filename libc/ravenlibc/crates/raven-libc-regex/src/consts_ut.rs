// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests of the constant table.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use super::*;

#[test]
fn cflags_match_the_bsd_headers() {
    assert_eq!(REG_BASIC, 0o0);
    assert_eq!(REG_EXTENDED, 0o1);
    assert_eq!(REG_ICASE, 0o2);
    assert_eq!(REG_NOSUB, 0o4);
    assert_eq!(REG_NEWLINE, 0o10);
    assert_eq!(REG_NOSPEC, 0o20);
    assert_eq!(REG_PEND, 0o40);
    assert_eq!(REG_DUMP, 0o200);
    assert_eq!(REG_GNU, 0o400);
}

#[test]
fn eflags_match_the_bsd_headers() {
    assert_eq!(REG_NOTBOL, 0o1);
    assert_eq!(REG_NOTEOL, 0o2);
    assert_eq!(REG_STARTEND, 0o4);
    assert_eq!(REG_TRACE, 0o400);
    assert_eq!(REG_LARGE, 0o1000);
    assert_eq!(REG_BACKR, 0o2000);
}

#[test]
fn error_codes_are_the_donor_values() {
    assert_eq!(REG_NOMATCH, 1);
    assert_eq!(REG_BADPAT, 2);
    assert_eq!(REG_ECOLLATE, 3);
    assert_eq!(REG_ECTYPE, 4);
    assert_eq!(REG_EESCAPE, 5);
    assert_eq!(REG_ESUBREG, 6);
    assert_eq!(REG_EBRACK, 7);
    assert_eq!(REG_EPAREN, 8);
    assert_eq!(REG_EBRACE, 9);
    assert_eq!(REG_BADBR, 10);
    assert_eq!(REG_ERANGE, 11);
    assert_eq!(REG_ESPACE, 12);
    assert_eq!(REG_BADRPT, 13);
    assert_eq!(REG_EMPTY, 14);
    assert_eq!(REG_ASSERT, 15);
    assert_eq!(REG_INVARG, 16);
    assert_eq!(REG_ILLSEQ, 17);
    assert_eq!(REG_ATOI, 255);
    assert_eq!(REG_ITOA, 0o400);
}

#[test]
fn donor_limits_are_unchanged() {
    assert_eq!(NPAREN, 10);
    assert_eq!(DUPMAX, 255);
    assert_eq!(INFINITY, 256);
    assert_eq!(MAX_RECURSION, 100);
}

#[test]
fn hardening_limits_are_the_ticket_values() {
    assert_eq!(MAX_STRIP, 1 << 21);
    assert_eq!(BACKREF_BUDGET, 1 << 24);
    assert_eq!(MAX_PAREN_DEPTH, 256);
    assert_eq!(BACKREF_DEPTH_MAX, 4096);
}

#[test]
fn magic_words_match_the_donor_expressions() {
    assert_eq!(MAGIC1, 0xf265);
    assert_eq!(MAGIC2, 0xd245);
    assert_ne!(MAGIC1, MAGIC2);
}

#[test]
fn operator_layout_is_the_donor_layout() {
    assert_eq!(OPSHIFT, 27);
    assert_eq!(OPRMASK, 0xf800_0000);
    assert_eq!(OPDMASK, 0x07ff_ffff);
    assert_eq!(USEBOL, 1);
    assert_eq!(USEEOL, 2);
    assert_eq!(BAD, 4);
}
