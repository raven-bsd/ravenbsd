// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use crate::test_support::{SliceVa, VecSink};
use raven_libc_ctype::MbMode;

#[test]
fn narrow_cstr_size_respects_precision() {
    let mut va = SliceVa::new();
    let p = va.cstr("abcdef");
    assert_eq!(cstr_size(&mut va, p, -1), 6);
    assert_eq!(cstr_size(&mut va, p, 0), 0);
    assert_eq!(cstr_size(&mut va, p, 1), 1);
    assert_eq!(cstr_size(&mut va, p, 3), 3);
    assert_eq!(cstr_size(&mut va, p, 40), 6);
    assert_eq!(va.max_read, p + 6);
}

#[test]
fn narrow_cstr_emit_writes_the_bytes() {
    let mut va = SliceVa::new();
    let p = va.cstr("hello");
    let mut sink = VecSink::<u8>::new();
    emit_cstr::<u8, _, _>(&mut sink, &mut va, p, 5).unwrap();
    assert_eq!(sink.data, b"hello");
}

#[test]
fn wide_to_multibyte_size_and_emit() {
    let mut va = SliceVa::new();
    let p = va.wstr(&[0x00e9, 0x20ac, 0x1f600]);
    assert_eq!(wstr_mb_size(&mut va, p, -1, MbMode::Utf8).unwrap(), 9);
    assert_eq!(wstr_mb_size(&mut va, p, 0, MbMode::Utf8).unwrap(), 0);
    assert_eq!(wstr_mb_size(&mut va, p, 2, MbMode::Utf8).unwrap(), 2);
    assert_eq!(wstr_mb_size(&mut va, p, 3, MbMode::Utf8).unwrap(), 2);
    assert_eq!(wstr_mb_size(&mut va, p, 5, MbMode::Utf8).unwrap(), 5);
    let mut sink = VecSink::<u8>::new();
    emit_wstr_mb::<u8, _, _>(&mut sink, &mut va, p, 5, MbMode::Utf8).unwrap();
    assert_eq!(sink.data, "é€".as_bytes());
}

#[test]
fn multibyte_to_wide_size_and_emit() {
    let mut va = SliceVa::new();
    let p = va.cstr("é€😀");
    assert_eq!(mb_wstr_size(&mut va, p, -1, MbMode::Utf8).unwrap(), 3);
    assert_eq!(mb_wstr_size(&mut va, p, 0, MbMode::Utf8).unwrap(), 0);
    assert_eq!(mb_wstr_size(&mut va, p, 1, MbMode::Utf8).unwrap(), 1);
    assert_eq!(mb_wstr_size(&mut va, p, 2, MbMode::Utf8).unwrap(), 2);
    let mut sink = VecSink::<u32>::new();
    emit_mb_wstr::<u32, _, _>(&mut sink, &mut va, p, 2, MbMode::Utf8).unwrap();
    assert_eq!(sink.data, vec![0x00e9, 0x20ac]);
}

#[test]
fn invalid_multibyte_input_is_encoding() {
    let mut va = SliceVa::new();
    let p = va.arena.len();
    va.arena.extend_from_slice(&[0xff, 0]);
    assert_eq!(
        mb_wstr_size(&mut va, p, -1, MbMode::Utf8),
        Err(FmtErr::Encoding)
    );
}

#[test]
fn wide_character_encoding_modes() {
    assert_eq!(wchar_size(0x41, MbMode::Utf8).unwrap(), 1);
    assert_eq!(wchar_size(0x20ac, MbMode::Utf8).unwrap(), 3);
    assert_eq!(wchar_size(0x20ac, MbMode::Byte), Err(FmtErr::Encoding));
    let mut sink = VecSink::<u8>::new();
    let n = emit_wchar::<u8, _>(&mut sink, 0x20ac, MbMode::Utf8).unwrap();
    assert_eq!(n, 3);
    assert_eq!(sink.data, "€".as_bytes());
}

#[test]
fn wide_string_size_and_emit() {
    let mut va = SliceVa::new();
    let p = va.wstr(&[b'w' as u32, b'i' as u32, b'd' as u32, b'e' as u32]);
    assert_eq!(wstr_size(&mut va, p, -1), 4);
    assert_eq!(wstr_size(&mut va, p, 2), 2);
    let mut sink = VecSink::<u32>::new();
    emit_wstr::<u32, _, _>(&mut sink, &mut va, p, 4).unwrap();
    assert_eq!(
        sink.data,
        vec![b'w' as u32, b'i' as u32, b'd' as u32, b'e' as u32]
    );
}
