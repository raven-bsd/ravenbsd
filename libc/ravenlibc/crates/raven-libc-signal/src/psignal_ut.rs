// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn message_layout_has_the_musl_colon_and_newline() {
    let mut out = [0u8; 64];
    let length = psignal_text_with(15, b"prog", b"Terminated", &mut out);
    assert_eq!(&out[..length], b"prog: Terminated\n");
    assert_eq!(out[length], 0);
}

#[test]
fn empty_message_prints_only_the_text() {
    let mut out = [0u8; 64];
    let length = psignal_text_with(1, b"", b"Hangup", &mut out);
    assert_eq!(&out[..length], b"Hangup\n");
}

#[test]
fn psignal_text_uses_the_canonical_name() {
    let mut out = [0u8; 64];
    let length = psignal_text(11, b"app", &mut out);
    assert_eq!(&out[..length], b"app: SEGV\n");
}

#[test]
fn psiginfo_uses_si_signo() {
    let mut info = SiginfoT::ZERO;
    info.si_signo = 9;
    let mut out = [0u8; 64];
    let length = psiginfo_text(&info, b"x", &mut out);
    assert_eq!(&out[..length], b"x: KILL\n");
}

#[test]
fn truncation_is_bounded_and_nul_terminated() {
    let mut out = [0u8; 5];
    let length = psignal_text_with(1, b"long", b"message", &mut out);
    assert_eq!(length, 5);
    assert_eq!(out.len(), 5);
}
