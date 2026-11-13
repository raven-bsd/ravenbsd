// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use crate::test_support::RecordingSource;

#[test]
fn a_limit_stops_the_cursor() {
    let mut src = RecordingSource::new(b"abcdef".to_vec());
    let mut f = Limited::new(&mut src);
    f.shlim(3);
    assert_eq!(f.getc(), Some(b'a'));
    assert_eq!(f.getc(), Some(b'b'));
    assert_eq!(f.getc(), Some(b'c'));
    assert_eq!(f.getc(), None);
    assert_eq!(f.count(), 3);
}

#[test]
fn zero_limit_is_unlimited() {
    let mut src = RecordingSource::new(b"abcdef".to_vec());
    let mut f = Limited::new(&mut src);
    f.shlim(0);
    for _ in 0..6 {
        assert!(f.getc().is_some());
    }
    assert_eq!(f.getc(), None);
    assert_eq!(f.count(), 6);
}

#[test]
fn ungetc_restores_one_unit_and_the_count() {
    let mut src = RecordingSource::new(b"xy".to_vec());
    let mut f = Limited::new(&mut src);
    f.shlim(0);
    let c = f.getc().unwrap();
    assert_eq!(c, b'x');
    f.ungetc(c);
    assert_eq!(f.count(), 0);
    assert_eq!(f.getc(), Some(b'x'));
    assert_eq!(f.getc(), Some(b'y'));
}

#[test]
fn finish_pushes_the_lookahead_back_to_the_source() {
    let mut src = RecordingSource::new(b"abcd".to_vec());
    {
        let mut f = Limited::new(&mut src);
        f.shlim(0);
        assert_eq!(f.getc(), Some(b'a'));
        assert_eq!(f.getc(), Some(b'b'));
        f.ungetc(b'b');
        f.finish();
    }
    assert_eq!(src.ungets, vec![b'b']);
    assert_eq!(src.getc(), Some(b'b'));
    assert_eq!(src.getc(), Some(b'c'));
}

#[test]
fn the_no_unget_flag_matches_musl_after_eof() {
    let mut src = RecordingSource::new(b"a".to_vec());
    let mut f = Limited::new(&mut src);
    f.shlim(0);
    assert_eq!(f.getc(), Some(b'a'));
    assert_eq!(f.getc(), None);
    f.ungetc(b'a');
    assert_eq!(f.count(), 1);
}
