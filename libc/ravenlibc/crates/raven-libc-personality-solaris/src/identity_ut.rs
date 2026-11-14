// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Tests for `identity.rs` (C67 5.A: `getexecname`/`getisax`).

use crate::identity::{AT_EXECFN, AT_HWCAP, AT_HWCAP2, getexecname, getisax};
use crate::testutil::FakeAux;

#[test]
fn execname_present_returns_the_pointer() {
    let mut aux = FakeAux::new();
    aux.set(AT_EXECFN, 0x1234_5678);
    assert_eq!(getexecname(&aux), Some(0x1234_5678));
}

#[test]
fn execname_absent_returns_null() {
    let aux = FakeAux::new();
    assert_eq!(getexecname(&aux), None);
}

#[test]
fn execname_present_zero_is_still_a_value() {
    let mut aux = FakeAux::new();
    aux.set(AT_EXECFN, 0);
    assert_eq!(getexecname(&aux), Some(0));
}

#[test]
fn execname_does_not_need_other_keys() {
    let mut aux = FakeAux::new();
    aux.set(AT_HWCAP, 1);
    assert_eq!(getexecname(&aux), None);
}

#[test]
fn isax_absent_hwcap_returns_zero_words() {
    let aux = FakeAux::new();
    let mut out = [7u32; 3];
    assert_eq!(getisax(&aux, &mut out), 0);
    assert_eq!(out, [7, 7, 7]);
}

#[test]
fn isax_zero_words_returns_zero() {
    let mut aux = FakeAux::new();
    aux.set(AT_HWCAP, 0xffff_ffff);
    let mut out: [u32; 0] = [];
    assert_eq!(getisax(&aux, &mut out), 0);
}

#[test]
fn isax_one_word_is_the_low_half() {
    let mut aux = FakeAux::new();
    aux.set(AT_HWCAP, 0x1122_3344_5566_7788);
    let mut out = [0u32; 1];
    assert_eq!(getisax(&aux, &mut out), 1);
    assert_eq!(out[0], 0x5566_7788);
}

#[test]
fn isax_three_words_include_the_high_half_and_hwcap2() {
    let mut aux = FakeAux::new();
    aux.set(AT_HWCAP, 0x1122_3344_5566_7788);
    aux.set(AT_HWCAP2, 0x99aa_bbcc);
    let mut out = [0u32; 3];
    assert_eq!(getisax(&aux, &mut out), 3);
    assert_eq!(out, [0x5566_7788, 0x1122_3344, 0x99aa_bbcc]);
}

#[test]
fn isax_ten_words_write_three_and_report_three() {
    let mut aux = FakeAux::new();
    aux.set(AT_HWCAP, 0x1);
    let mut out = [0xdead_beefu32; 10];
    assert_eq!(getisax(&aux, &mut out), 3);
    assert_eq!(out[0], 1);
    assert_eq!(out[1], 0);
    assert_eq!(out[2], 0);
    assert_eq!(&out[3..], &[0xdead_beefu32; 7]);
}

#[test]
fn isax_without_hwcap2_writes_a_zero_word() {
    let mut aux = FakeAux::new();
    aux.set(AT_HWCAP, 0x0000_0001_0000_0002);
    let mut out = [9u32; 3];
    assert_eq!(getisax(&aux, &mut out), 3);
    assert_eq!(out, [2, 1, 0]);
}

#[test]
fn isax_hwcap_present_but_zero_still_reports_words() {
    let mut aux = FakeAux::new();
    aux.set(AT_HWCAP, 0);
    let mut out = [5u32; 3];
    assert_eq!(getisax(&aux, &mut out), 3);
    assert_eq!(out, [0, 0, 0]);
}

#[test]
fn isax_only_low_half() {
    let mut aux = FakeAux::new();
    aux.set(AT_HWCAP, 0x34);
    aux.set(AT_HWCAP2, 0x56);
    let mut out = [0u32; 2];
    assert_eq!(getisax(&aux, &mut out), 2);
    assert_eq!(out, [0x34, 0]);
}

#[test]
fn aux_keys_are_the_pg50_numbers() {
    assert_eq!(AT_HWCAP, 16);
    assert_eq!(AT_HWCAP2, 26);
    assert_eq!(AT_EXECFN, 31);
}
