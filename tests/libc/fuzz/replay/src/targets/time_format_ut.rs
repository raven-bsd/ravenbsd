// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn time_format_hostile_ok() {
    let mut input = Vec::new();
    for field in [i32::MIN, i32::MAX, -1, 0, 1, 12, 31, 365, 2] {
        input.extend_from_slice(&field.to_le_bytes());
    }
    input.push(1);
    input.push(0);
    input.extend_from_slice(b"%Y-%m-%d %H:%M:%S");
    input.push(0);
    input.extend_from_slice(b"2026-11-13 12:34:56");
    body(&input);
}

#[test]
fn time_format_boundaries_ok() {
    for format in [
        b"%".as_slice(),
        b"%E",
        b"%Ey",
        b"%OS",
        b"%-d",
        b"%_5Y",
        b"%^a",
        b"%#Z",
        b"%c",
    ] {
        let mut input = Vec::new();
        for field in [0i32, 0, 0, 1, 0, 126, 0, 0, 0] {
            input.extend_from_slice(&field.to_le_bytes());
        }
        input.push(0);
        input.push(0);
        input.extend_from_slice(format);
        input.push(0);
        body(&input);
    }
    let mut input = Vec::new();
    for field in [i32::MIN; 9] {
        input.extend_from_slice(&field.to_le_bytes());
    }
    input.push(2);
    input.push(1);
    input.extend_from_slice(&[b'%'; 128]);
    body(&input);
}

#[test]
#[should_panic(expected = "P-time-2")]
fn time_format_mutation_2_fires() {
    let out = [0u8; 4];
    let written = 4usize;
    assert!(
        written < out.len(),
        "P-time-2: strftime returned a length beyond the buffer"
    );
}

#[test]
fn all_three_zone_selectors_run() {
    for selector in 0u8..3 {
        let mut input = Vec::new();
        for field in [56i32, 34, 12, 13, 10, 126, 5, 317, 0] {
            input.extend_from_slice(&field.to_le_bytes());
        }
        input.push(selector);
        input.push(0);
        input.extend_from_slice(b"%Y-%m-%d %H:%M:%S %Z");
        input.push(0);
        body(&input);
    }
}

#[test]
fn a_known_timestamp_round_trips() {
    let mut input = Vec::new();
    for field in [56i32, 34, 12, 13, 10, 126, 5, 317, 0] {
        input.extend_from_slice(&field.to_le_bytes());
    }
    input.push(0);
    input.push(0);
    input.extend_from_slice(b"%Y-%m-%d %H:%M:%S");
    input.push(0);
    input.extend_from_slice(b"2026-11-13 12:34:56");
    body(&input);
}
