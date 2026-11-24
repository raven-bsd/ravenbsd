// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the `PROC_GETRUSAGE` to `struct rusage` mapping.

use super::{rusage_from_words, timeval_from_ns};

#[test]
fn timeval_splits_seconds_and_microseconds() {
    let value = timeval_from_ns(1_234_567_890);
    assert_eq!(value.tv_sec, 1);
    assert_eq!(value.tv_usec, 234_567);
}

#[test]
fn timeval_truncates_below_a_microsecond() {
    let value = timeval_from_ns(1_999);
    assert_eq!(value.tv_sec, 0);
    assert_eq!(value.tv_usec, 1);
}

#[test]
fn rusage_maps_all_kernel_words() {
    let words = [1_500_000_000, 2_250_000, 5_242_880, 3, 4, 5, 6, 7];
    let usage = rusage_from_words(words);
    assert_eq!(usage.ru_utime.tv_sec, 1);
    assert_eq!(usage.ru_utime.tv_usec, 500_000);
    assert_eq!(usage.ru_stime.tv_sec, 0);
    assert_eq!(usage.ru_stime.tv_usec, 2_250);
    assert_eq!(usage.ru_maxrss, 5_242_880);
    assert_eq!(usage.ru_minflt, 3);
    assert_eq!(usage.ru_majflt, 4);
    assert_eq!(usage.ru_nvcsw, 5);
    assert_eq!(usage.ru_nivcsw, 6);
}

#[test]
fn zero_words_are_the_zero_record() {
    assert_eq!(rusage_from_words([0; 8]), raven_libc_abi::structs::Rusage::ZERO);
}
