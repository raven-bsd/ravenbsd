// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for S16LE to duty conversion.

use super::*;

#[test]
fn signed_samples_map_around_the_midpoint() {
    assert_eq!(sample_to_duty(0, 2083), Ok(1041));
    assert_eq!(sample_to_duty(32767, 2083), Ok(2082));
    assert_eq!(sample_to_duty(-32768, 2083), Ok(0));
}

#[test]
fn a_positive_sample_is_above_the_midpoint() {
    assert!(sample_to_duty(100, 2083).expect("duty") > 1041);
}

#[test]
fn the_left_right_order_is_preserved() {
    let mut pcm = [0u8; 8];
    pcm[0..2].copy_from_slice(&i16::to_le_bytes(1000));
    pcm[2..4].copy_from_slice(&i16::to_le_bytes(-1000));
    pcm[4..6].copy_from_slice(&i16::to_le_bytes(2000));
    pcm[6..8].copy_from_slice(&i16::to_le_bytes(3000));
    let mut out = [0u32; 4];
    let frames = convert_period(&pcm, 2083, SampleOrder::LeftRight, &mut out).expect("convert");
    assert_eq!(frames, 2);
    assert!(out[0] > 1041);
    assert!(out[1] < 1041);
    assert!(out[2] > 1041);
    assert!(out[3] > out[2]);
}

#[test]
fn the_right_left_order_swaps_the_words() {
    let mut pcm = [0u8; 4];
    pcm[0..2].copy_from_slice(&i16::to_le_bytes(1000));
    pcm[2..4].copy_from_slice(&i16::to_le_bytes(-1000));
    let mut left = [0u32; 2];
    let mut right = [0u32; 2];
    convert_period(&pcm, 2083, SampleOrder::LeftRight, &mut left).expect("left");
    convert_period(&pcm, 2083, SampleOrder::RightLeft, &mut right).expect("right");
    assert_eq!(left[0], right[1]);
    assert_eq!(left[1], right[0]);
}

#[test]
fn silence_is_the_range_midpoint() {
    let mut out = [0u32; 8];
    assert_eq!(silence_period(2083, 4, &mut out), Ok(4));
    assert_eq!(out, [1041; 8]);
}

#[test]
fn conversion_rejects_odd_bytes_and_short_destinations() {
    let mut out = [0u32; 2];
    assert_eq!(
        convert_period(&[0u8; 3], 2083, SampleOrder::LeftRight, &mut out),
        Err(DutyError::OddBytes)
    );
    assert_eq!(
        convert_period(&[0u8; 4], 2083, SampleOrder::LeftRight, &mut out[..1]),
        Err(DutyError::BufferTooSmall)
    );
    assert_eq!(
        silence_period(2083, 2, &mut out),
        Err(DutyError::BufferTooSmall)
    );
}

#[test]
fn zero_range_is_rejected() {
    assert_eq!(sample_to_duty(0, 0), Err(DutyError::BadRange));
    let mut out = [0u32; 2];
    assert_eq!(
        convert_period(&[0u8; 4], 0, SampleOrder::LeftRight, &mut out),
        Err(DutyError::BadRange)
    );
    assert_eq!(silence_period(0, 1, &mut out), Err(DutyError::BadRange));
}

#[test]
fn oversized_periods_are_rejected() {
    let mut pcm = [0u8; (MAX_PERIOD_FRAMES + 1) * STEREO_FRAME_BYTES];
    let mut out = [0u32; 2];
    assert_eq!(
        convert_period(&pcm, 2083, SampleOrder::LeftRight, &mut out),
        Err(DutyError::TooManyFrames)
    );
    pcm.fill(0);
    assert_eq!(
        silence_period(2083, MAX_PERIOD_FRAMES + 1, &mut out),
        Err(DutyError::TooManyFrames)
    );
}
