// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn empty_text_keeps_defaults() {
    let options = parse(b"");
    assert_eq!(options, DebugOptions::default());
}

#[test]
fn flags_and_numbers_parse() {
    let options = parse(b"redzone,fill,audit=128,guard=4096,leak,fail_every=3");
    assert!(options.redzone);
    assert!(options.fill);
    assert!(options.leak);
    assert_eq!(options.audit, 128);
    assert_eq!(options.guard, 4096);
    assert_eq!(options.fail_every, 3);
}

#[test]
fn audit_is_clamped() {
    assert_eq!(parse(b"audit=0").audit, 1);
    assert_eq!(parse(b"audit=99999").audit, MAX_AUDIT);
}

#[test]
fn unknown_tokens_are_ignored() {
    let options = parse(b"bogus=1,redzone");
    assert!(options.redzone);
    assert_eq!(options.audit, DEFAULT_AUDIT);
}
