// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::assertions_on_constants
)]

//! `random`: the five state types, D-1 and the Park-Miller discard loop.

use super::*;

fn state(nbytes: usize) -> Vec<u32> {
    vec![0u32; nbytes / 4]
}

#[test]
fn type_selection_matches_the_donor_breaks() {
    let cases = [
        (8usize, TYPE_0),
        (16, TYPE_0),
        (31, TYPE_0),
        (32, TYPE_1),
        (63, TYPE_1),
        (64, TYPE_2),
        (127, TYPE_2),
        (128, TYPE_3),
        (255, TYPE_3),
        (256, TYPE_4),
        (1024, TYPE_4),
    ];
    for (nbytes, expected) in cases {
        let mut ctl = RandomCtl::new();
        let mut st = state(nbytes);
        initstate(&mut ctl, &mut st, 1, nbytes).unwrap();
        assert_eq!(ctl.r#type, expected, "nbytes={nbytes}");
        assert_eq!(st[0], expected);
    }
}

#[test]
fn initstate_rejects_short_buffers() {
    let mut ctl = RandomCtl::new();
    let mut st = state(8);
    assert_eq!(initstate(&mut ctl, &mut st, 1, 4), Err(RandErr::Invalid));
    assert_eq!(initstate(&mut ctl, &mut st, 1, 0), Err(RandErr::Invalid));
}

#[test]
fn type_zero_is_the_donor_lcg() {
    let mut ctl = RandomCtl::new();
    let mut st = state(8);
    initstate(&mut ctl, &mut st, 42, 8).unwrap();
    assert_eq!(st[1], 42);
    let mut expected = 42u32;
    for _ in 0..10 {
        expected = expected.wrapping_mul(1_103_515_245).wrapping_add(12_345) & 0x7fff_ffff;
        assert_eq!(random(&mut ctl, &mut st), expected);
    }
}

#[test]
fn seed_zero_behaves_as_seed_one() {
    let mut a = RandomCtl::new();
    let mut b = RandomCtl::new();
    let mut state_a = state(128);
    let mut state_b = state(128);
    initstate(&mut a, &mut state_a, 0, 128).unwrap();
    initstate(&mut b, &mut state_b, 1, 128).unwrap();
    for _ in 0..1000 {
        assert_eq!(random(&mut a, &mut state_a), random(&mut b, &mut state_b));
    }
}

#[test]
fn seed_zero_outputs_are_not_all_equal() {
    let mut ctl = RandomCtl::new();
    let mut st = state(128);
    initstate(&mut ctl, &mut st, 0, 128).unwrap();
    let first = random(&mut ctl, &mut st);
    let mut differs = false;
    for _ in 0..100 {
        if random(&mut ctl, &mut st) != first {
            differs = true;
            break;
        }
    }
    assert!(differs);
}

#[test]
fn every_type_runs_and_stays_in_range() {
    for nbytes in [8usize, 32, 64, 128, 256] {
        let mut ctl = RandomCtl::new();
        let mut st = state(nbytes);
        initstate(&mut ctl, &mut st, 7, nbytes).unwrap();
        for _ in 0..1000 {
            assert!(random(&mut ctl, &mut st) <= 0x7fff_ffff);
        }
    }
}

#[test]
fn setstate_round_trip_restores_the_stream() {
    let mut ctl = RandomCtl::new();
    let mut st = state(128);
    initstate(&mut ctl, &mut st, 42, 128).unwrap();
    let saved = st.clone();
    let first: Vec<u32> = (0..10).map(|_| random(&mut ctl, &mut st)).collect();
    let mut restored = saved.clone();
    setstate(&mut ctl, &mut restored).unwrap();
    for expected in first {
        assert_eq!(random(&mut ctl, &mut restored), expected);
    }
}

#[test]
fn setstate_rejects_a_bad_type_word() {
    let mut ctl = RandomCtl::new();
    let mut st = state(128);
    st[0] = 99;
    assert_eq!(setstate(&mut ctl, &mut st), Err(RandErr::Invalid));
}

#[test]
fn words_reports_the_buffer_size() {
    let mut ctl = RandomCtl::new();
    assert_eq!(ctl.words(), 32);
    let mut st = state(8);
    initstate(&mut ctl, &mut st, 1, 8).unwrap();
    assert_eq!(ctl.words(), 2);
}
