// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn zero_and_one_do_not_draw() {
    for bound in [0u32, 1] {
        let mut calls = 0usize;
        let value = uniform_with(bound, &mut || {
            calls += 1;
            7
        });
        assert_eq!(value, 0);
        assert_eq!(calls, 0);
    }
}

#[test]
fn values_stay_below_the_bound() {
    for bound in [2u32, 3, 5, 6, 7, 10, 1000, 1 << 31, (1 << 31) + 1, u32::MAX] {
        let mut state = 1u32;
        let value = uniform_with(bound, &mut || {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            state
        });
        assert!(value < bound, "bound {bound} value {value}");
    }
}

#[test]
fn rejection_boundary_is_exact() {
    let bound = 10u32;
    let minimum = bound.wrapping_neg() % bound;
    let mut values = std::vec::Vec::new();
    let mut index = 0usize;
    let scripted = [minimum - 1, minimum, minimum + 1];
    let value = uniform_with(bound, &mut || {
        let value = scripted[index];
        index += 1;
        values.push(value);
        value
    });
    assert_eq!(value, minimum % bound);
    assert_eq!(values, [minimum - 1, minimum]);
}

#[test]
fn high_values_are_reduced() {
    let value = uniform_with(10, &mut || 12345);
    assert_eq!(value, 12345 % 10);
}

#[test]
fn bound_max_never_rejects() {
    let mut calls = 0usize;
    let value = uniform_with(u32::MAX, &mut || {
        calls += 1;
        u32::MAX - 1
    });
    assert_eq!(value, u32::MAX - 1);
    assert_eq!(calls, 1);
}

#[test]
fn deterministic_closure_is_deterministic() {
    let mut first = 99u32;
    let mut second = 99u32;
    let a = uniform_with(1000, &mut || {
        first = first.wrapping_mul(1103515245).wrapping_add(12345);
        first
    });
    let b = uniform_with(1000, &mut || {
        second = second.wrapping_mul(1103515245).wrapping_add(12345);
        second
    });
    assert_eq!(a, b);
}

#[test]
fn bound_two_returns_zero_or_one() {
    let mut values = std::vec::Vec::new();
    for _ in 0..16 {
        values.push(uniform_with(2, &mut || 5));
    }
    assert!(values.iter().all(|value| *value < 2));
}

#[test]
fn scripted_sequence_is_exact() {
    let mut script = [3u32, 9, 4].into_iter();
    assert_eq!(uniform_with(5, &mut || script.next().unwrap_or(0)), 3);
}
