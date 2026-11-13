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

//! The model test: 100,000 random interleavings against a `BTreeMap`, plus the D-2 invariants.

use std::collections::BTreeMap;

use super::*;

fn leak(bytes: &[u8]) -> &'static [u8] {
    Box::leak(bytes.to_vec().into_boxed_slice())
}

fn entry(name: &[u8], value: &[u8]) -> &'static [u8] {
    let mut buffer = Vec::new();
    buffer.extend_from_slice(name);
    buffer.push(b'=');
    buffer.extend_from_slice(value);
    buffer.push(0);
    leak(&buffer)
}

fn lcg(state: &mut u64) -> u64 {
    *state = state
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1);
    *state
}

fn model_entries(model: &BTreeMap<Vec<u8>, Vec<u8>>) -> Vec<(Vec<u8>, Vec<u8>)> {
    model.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
}

#[test]
fn one_hundred_thousand_interleavings_match_the_model() {
    let names: [&[u8]; 8] = [b"A", b"B", b"C", b"D", b"E", b"F", b"G", b"H"];
    let values: [&[u8]; 4] = [b"", b"v1", b"value-2", b"longer-value-three"];
    let initial: Vec<&'static [u8]> = names
        .iter()
        .enumerate()
        .map(|(index, name)| entry(name, format!("init{index}").as_bytes()))
        .collect();
    let initial_copy = initial.clone();
    let mut env = EnvTable::new();
    env.adopt_initial(&initial);
    let mut model: BTreeMap<Vec<u8>, Vec<u8>> = names
        .iter()
        .enumerate()
        .map(|(index, name)| (name.to_vec(), format!("init{index}").into_bytes()))
        .collect();
    let mut earlier: Vec<(&'static [u8], Vec<u8>)> = Vec::new();
    let mut state = 0x5eed_1234_9876_abcd;
    for step in 0..100_000u64 {
        let name = names[(lcg(&mut state) as usize) % names.len()];
        let value = values[(lcg(&mut state) as usize) % values.len()];
        match lcg(&mut state) % 100 {
            0..=34 => {
                let overwrite = lcg(&mut state) & 1 == 0;
                env.setenv(name, value, overwrite).unwrap();
                if overwrite || !model.contains_key(name) {
                    model.insert(name.to_vec(), value.to_vec());
                }
            }
            35..=54 => {
                env.unsetenv(name).unwrap();
                model.remove(name);
            }
            55..=69 => {
                if lcg(&mut state) & 3 == 0 {
                    // putenv("NAME") removes (RD-26).
                    let mut text = name.to_vec();
                    text.push(0);
                    env.putenv(leak(&text)).unwrap();
                    model.remove(name);
                } else {
                    let mut text = Vec::new();
                    text.extend_from_slice(name);
                    text.push(b'=');
                    text.extend_from_slice(value);
                    text.push(0);
                    env.putenv(leak(&text)).unwrap();
                    model.insert(name.to_vec(), value.to_vec());
                }
            }
            70..=92 => {
                let got = env.getenv(name);
                match (got, model.get(name)) {
                    (Some(got), Some(want)) => {
                        assert_eq!(got, want.as_slice(), "step {step}");
                        earlier.push((got, want.clone()));
                    }
                    (None, None) => {}
                    _ => panic!("getenv mismatch at step {step}"),
                }
            }
            93..=96 => {
                let mut buf = [0u8; 64];
                match env.getenv_r(name, &mut buf) {
                    Ok(length) => {
                        assert_eq!(&buf[..length], model.get(name).unwrap().as_slice());
                        assert_eq!(buf[length], 0);
                    }
                    Err(_) => assert!(!model.contains_key(name)),
                }
            }
            _ => {
                if lcg(&mut state).is_multiple_of(20) {
                    env.clearenv();
                    model.clear();
                }
            }
        }
        let mut actual: Vec<(Vec<u8>, Vec<u8>)> = env
            .as_slice()
            .iter()
            .filter_map(|entry| {
                let equal = entry.iter().position(|byte| *byte == b'=')?;
                let name = entry.get(..equal)?.to_vec();
                let value = entry.get(equal + 1..)?.to_vec();
                let value = match value.last() {
                    Some(0) => value[..value.len() - 1].to_vec(),
                    _ => value,
                };
                Some((name, value))
            })
            .collect();
        let mut expected = model_entries(&model);
        actual.sort();
        expected.sort();
        assert_eq!(actual, expected, "step {step}");
    }
    for (slice, bytes) in earlier {
        assert_eq!(slice, bytes.as_slice());
    }
    for (index, original) in initial_copy.iter().enumerate() {
        assert_eq!(*original, initial[index]);
    }
}

#[test]
fn name_validation() {
    let mut env = EnvTable::new();
    assert!(env.setenv(b"", b"x", true).is_err());
    assert!(env.setenv(b"A=B", b"x", true).is_err());
    assert!(env.unsetenv(b"").is_err());
    assert!(env.unsetenv(b"A=B").is_err());
    assert_eq!(env.getenv(b""), None);
    assert_eq!(env.getenv(b"A=B"), None);
}

#[test]
fn generation_bumps_only_on_change() {
    let mut env = EnvTable::new();
    let start = env.generation();
    env.setenv(b"A", b"1", true).unwrap();
    assert!(env.generation() > start);
    let after = env.generation();
    env.setenv(b"A", b"2", false).unwrap();
    assert_eq!(env.generation(), after);
    env.unsetenv(b"missing").unwrap();
    assert_eq!(env.generation(), after);
    env.setenv(b"A", b"2", true).unwrap();
    assert!(env.generation() > after);
}
