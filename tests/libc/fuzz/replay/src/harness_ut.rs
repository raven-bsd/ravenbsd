// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use crate::{PEAK_CAP, ReplayError};

fn boom(_data: &[u8]) {
    panic!("body boom");
}

fn quiet(_data: &[u8]) {}

fn sleepy(_data: &[u8]) {
    std::thread::sleep(Duration::from_secs(3));
}

fn hungry(_data: &[u8]) {
    let buffer = vec![0u8; 2 << 20];
    std::hint::black_box(&buffer);
}

#[test]
fn a_clean_body_passes() {
    assert!(execute_with_timeout("quiet", quiet, b"", PEAK_CAP, 2).is_ok());
}

#[test]
fn a_panicking_body_is_reported_with_the_target() {
    match execute_with_timeout("boom", boom, b"", PEAK_CAP, 2) {
        Err(ReplayError::Panicked { target, message }) => {
            assert_eq!(target, "boom");
            assert!(message.contains("body boom"));
        }
        other => panic!("unexpected result {other:?}"),
    }
}

#[test]
fn a_slow_body_times_out_without_joining() {
    match execute_with_timeout("sleepy", sleepy, b"", PEAK_CAP, 1) {
        Err(ReplayError::Timeout { target, secs }) => {
            assert_eq!(target, "sleepy");
            assert_eq!(secs, 1);
        }
        other => panic!("unexpected result {other:?}"),
    }
}

#[test]
fn a_greedy_body_hits_the_memory_cap() {
    match execute_with_timeout("hungry", hungry, b"", 512 * 1024, 2) {
        Err(ReplayError::MemoryCap { target, peak, cap }) => {
            assert_eq!(target, "hungry");
            assert!(peak > cap);
            assert_eq!(cap, 512 * 1024);
        }
        other => panic!("unexpected result {other:?}"),
    }
}

#[test]
fn input_is_truncated_to_the_maximum() {
    fn length(data: &[u8]) {
        assert_eq!(data.len(), crate::MAX_INPUT);
    }
    let data = vec![0u8; crate::MAX_INPUT + 100];
    assert!(execute_with_timeout("length", length, &data, PEAK_CAP, 2).is_ok());
}

#[test]
fn a_short_input_is_not_padded() {
    fn length(data: &[u8]) {
        assert_eq!(data.len(), 3);
    }
    assert!(execute_with_timeout("length", length, b"abc", PEAK_CAP, 2).is_ok());
}
