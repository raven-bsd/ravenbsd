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

//! `qsort` correctness, the antiqsort adversary, stability switch and element sizes.

use super::{STABLE_QSORT, qsort, qsort_with};

fn value(elem: &[u8]) -> u64 {
    let mut value = 0u64;
    for (index, byte) in elem.iter().take(8).enumerate() {
        value |= u64::from(*byte) << (index * 8);
    }
    value
}

fn cmp(a: &[u8], b: &[u8]) -> i32 {
    let left = value(a);
    let right = value(b);
    if left < right {
        -1
    } else if left > right {
        1
    } else {
        0
    }
}

fn encode(values: &[u64], es: usize) -> Vec<u8> {
    let mut buffer = vec![0u8; values.len() * es];
    for (index, value) in values.iter().enumerate() {
        let bytes = value.to_le_bytes();
        let take = core::cmp::min(es, 8);
        buffer[index * es..index * es + take].copy_from_slice(&bytes[..take]);
    }
    buffer
}

fn decode(buffer: &[u8], es: usize) -> Vec<u64> {
    (0..buffer.len() / es)
        .map(|index| value(&buffer[index * es..index * es + core::cmp::min(es, 8)]))
        .collect()
}

fn truncate(value: u64, es: usize) -> u64 {
    if es >= 8 {
        value
    } else {
        value & ((1u64 << (es * 8)) - 1)
    }
}

fn lcg(state: &mut u64) -> u64 {
    *state = state
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1);
    *state
}

fn distribution(kind: usize, n: usize, seed: u64) -> Vec<u64> {
    let mut state = seed | 1;
    let mut values = Vec::with_capacity(n);
    for index in 0..n {
        let value = match kind {
            0 => (index % 16) as u64,
            1 => lcg(&mut state) % 1000,
            2 => {
                if index % 2 == 0 {
                    index as u64
                } else {
                    (n - index) as u64
                }
            }
            3 => ((index / 5) * 5) as u64,
            _ => lcg(&mut state),
        };
        values.push(value);
    }
    values
}

#[test]
fn sorts_every_configuration() {
    let sizes = [0usize, 1, 2, 3, 6, 7, 8, 40, 41, 100];
    let element_sizes = [1usize, 2, 3, 4, 7, 8, 16, 24, 33];
    for &n in &sizes {
        for &es in &element_sizes {
            for kind in 0..5 {
                let values = distribution(kind, n, 0x9e37_79b9 + n as u64)
                    .into_iter()
                    .map(|value| truncate(value, es))
                    .collect::<Vec<u64>>();
                let mut buffer = encode(&values, es);
                qsort(&mut buffer, es, cmp);
                let mut sorted = values.clone();
                sorted.sort_unstable();
                assert_eq!(decode(&buffer, es), sorted, "n={n} es={es} kind={kind}");
            }
        }
    }
}

#[test]
fn all_equal_elements() {
    for n in [2usize, 7, 8, 41, 1000] {
        let mut buffer = encode(&vec![42u64; n], 8);
        qsort(&mut buffer, 8, cmp);
        assert!(decode(&buffer, 8).iter().all(|value| *value == 42));
    }
}

#[test]
fn es_zero_and_short_inputs() {
    let mut empty: [u8; 0] = [];
    qsort(&mut empty, 0, cmp);
    super::swap_elems(&mut empty, 0, 0, 0);
    let mut one = encode(&[1], 8);
    qsort(&mut one, 8, cmp);
    assert_eq!(decode(&one, 8), vec![1]);
}

#[test]
fn every_permutation_of_nine_or_less() {
    for n in 0..=9usize {
        let mut items: Vec<u64> = (0..n as u64).collect();
        permute(&mut items, 0, &mut |values| {
            let mut buffer = encode(values, 8);
            qsort(&mut buffer, 8, cmp);
            let sorted = decode(&buffer, 8);
            let mut expected = values.to_vec();
            expected.sort_unstable();
            assert_eq!(sorted, expected);
        });
    }
}

fn permute<F: FnMut(&mut [u64])>(items: &mut [u64], start: usize, action: &mut F) {
    if start == items.len() {
        action(items);
        return;
    }
    for index in start..items.len() {
        items.swap(start, index);
        permute(items, start + 1, action);
        items.swap(start, index);
    }
}

#[test]
fn antiqsort_comparison_bound() {
    let n = 100_000usize;
    let gas = (n - 1) as u64;
    let mut val = vec![gas; n];
    let mut state = (0u64, 0u64, 0usize, gas);
    let mut buffer = encode(&(0..n as u64).collect::<Vec<_>>(), 8);
    qsort(&mut buffer, 8, |a, b| {
        let x = value(a) as usize;
        let y = value(b) as usize;
        state.0 += 1;
        if val[x] == state.3 && val[y] == state.3 {
            if x == state.2 {
                val[x] = state.1;
                state.1 += 1;
            } else {
                val[y] = state.1;
                state.1 += 1;
            }
        }
        if val[x] == state.3 {
            state.2 = x;
        } else if val[y] == state.3 {
            state.2 = y;
        }
        match val[x].cmp(&val[y]) {
            core::cmp::Ordering::Less => -1,
            core::cmp::Ordering::Equal => 0,
            core::cmp::Ordering::Greater => 1,
        }
    });
    let bound = 12.0 * (n as f64) * ((n as f64).log2());
    println!("antiqsort n={n} comparisons={}", state.0);
    assert!(
        (state.0 as f64) <= bound,
        "comparisons {} exceed {bound}",
        state.0
    );
}

#[test]
fn inconsistent_comparator_terminates() {
    let mut state = 0x1234_5678u64;
    for n in [3usize, 7, 40, 1000] {
        let values = distribution(4, n, 7);
        let mut buffer = encode(&values, 8);
        qsort(&mut buffer, 8, |a, b| {
            state = lcg(&mut state);
            let left = value(a);
            let right = value(b);
            if left == right {
                return 0;
            }
            if state & 1 == 0 { -1 } else { 1 }
        });
        assert_eq!(buffer.len(), n * 8);
    }
}

#[test]
fn stable_switch_and_fallback() {
    assert!(!STABLE_QSORT);
    let values = (0..33u64).rev().collect::<Vec<_>>();
    let mut buffer = encode(&values, 8);
    qsort_with(true, &mut buffer, 8, cmp);
    assert_eq!(decode(&buffer, 8), {
        let mut expected = values.clone();
        expected.sort_unstable();
        expected
    });
    // es < PSIZE/2 makes the stable path return Inval, exercising the introsort fallback.
    let mut buffer = encode(&values, 1);
    qsort_with(true, &mut buffer, 1, cmp);
    assert_eq!(decode(&buffer, 1).len(), 33);
}
