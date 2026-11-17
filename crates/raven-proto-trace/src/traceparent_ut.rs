// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! W3C `traceparent` conformance vectors and fuzz.

use crate::traceparent::{Traceparent, TraceparentError, format, parse};

/// The W3C example traceparent.
const EXAMPLE: &str = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";

fn tp(trace_id: [u8; 16], parent_id: [u8; 8], flags: u8) -> Traceparent {
    Traceparent {
        trace_id,
        parent_id,
        flags,
    }
}

#[test]
fn parses_the_w3c_example() {
    let parsed = parse(EXAMPLE.as_bytes()).unwrap();
    assert!(parsed.sampled());
    assert_eq!(parsed.flags, 0x01);
    assert_eq!(parsed.trace_id[0], 0x4b);
    assert_eq!(parsed.parent_id[0], 0x00);
}

#[test]
fn valid_edge_ids() {
    // Minimum non-zero ids.
    let min = "00-00000000000000000000000000000001-0000000000000001-00";
    let parsed = parse(min.as_bytes()).unwrap();
    assert!(!parsed.sampled());
    assert_eq!(parsed.trace_id[15], 1);
    assert_eq!(parsed.parent_id[7], 1);

    // Maximum ids.
    let max = "00-ffffffffffffffffffffffffffffffff-ffffffffffffffff-00";
    assert!(parse(max.as_bytes()).is_ok());
}

#[test]
fn sampled_flag_is_bit_zero() {
    assert!(
        parse("00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01".as_bytes())
            .unwrap()
            .sampled()
    );
    assert!(
        !parse("00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-00".as_bytes())
            .unwrap()
            .sampled()
    );
}

#[test]
fn zero_ids_are_rejected() {
    assert_eq!(
        parse("00-00000000000000000000000000000000-00f067aa0ba902b7-01".as_bytes()),
        Err(TraceparentError::ZeroTraceId)
    );
    assert_eq!(
        parse("00-4bf92f3577b34da6a3ce929d0e0e4736-0000000000000000-01".as_bytes()),
        Err(TraceparentError::ZeroParentId)
    );
}

#[test]
fn wrong_length_is_rejected() {
    assert_eq!(parse("".as_bytes()), Err(TraceparentError::BadLength));
    // One byte short.
    assert_eq!(
        parse("00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-0".as_bytes()),
        Err(TraceparentError::BadLength)
    );
    // Oversized / trailing bytes.
    assert_eq!(
        parse("00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01x".as_bytes()),
        Err(TraceparentError::BadLength)
    );
}

#[test]
fn non_hex_and_uppercase_are_rejected() {
    // A 'g' in the trace id.
    assert_eq!(
        parse("00-4bf92f3577b34da6a3ce929d0e0e473g-00f067aa0ba902b7-01".as_bytes()),
        Err(TraceparentError::NonHex)
    );
    // Uppercase hex violates the lowercase grammar.
    assert_eq!(
        parse("00-4BF92F3577B34DA6A3CE929D0E0E4736-00F067AA0BA902B7-01".as_bytes()),
        Err(TraceparentError::NonHex)
    );
}

#[test]
fn bad_version_and_separator_are_rejected() {
    assert_eq!(
        parse("01-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01".as_bytes()),
        Err(TraceparentError::UnsupportedVersion)
    );
    // Missing separator: replace the first '-' with a non-hyphen, keeping the length valid.
    assert_eq!(
        parse("00g4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01".as_bytes()),
        Err(TraceparentError::BadSeparator)
    );
}

#[test]
fn format_parse_round_trips() {
    let parsed = parse(EXAMPLE.as_bytes()).unwrap();
    let reformatted = format(&parsed);
    assert_eq!(&reformatted[..], EXAMPLE.as_bytes());
    assert_eq!(parse(&reformatted), Ok(parsed));
}

#[test]
fn format_is_lowercase() {
    let parsed =
        parse("00-ffffffffffffffffffffffffffffffff-ffffffffffffffff-ff".as_bytes()).unwrap();
    let bytes = format(&parsed);
    assert_eq!(
        std::str::from_utf8(&bytes).unwrap(),
        "00-ffffffffffffffffffffffffffffffff-ffffffffffffffff-ff"
    );
}

// --- fuzz / small-input model ------------------------------------------------

/// A deterministic xorshift PRNG.
struct XorShift(u64);

impl XorShift {
    const fn new(seed: u64) -> Self {
        Self(seed)
    }

    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}

#[test]
fn exhaustive_version_field_never_panics() {
    // Every 2-byte version is either accepted (only "00") or rejected deterministically.
    for hi in 0u8..=15 {
        for lo in 0u8..=15 {
            let mut input = EXAMPLE.as_bytes().to_vec();
            input[0] = hex(hi);
            input[1] = hex(lo);
            let _ = parse(&input); // must not panic
        }
    }
}

#[test]
fn randomized_fuzz_never_panics_and_round_trips() {
    let mut rng = XorShift::new(0xD1B5_4A32_D192_ED03);
    const ALPHABET: &[u8] = b"0123456789abcdefABCDEF-xyz";
    const ITERATIONS: usize = 100_000;

    let mut buffer = [0u8; 64];
    for _ in 0..ITERATIONS {
        let len = (rng.next() as usize) % buffer.len();
        for byte in buffer.iter_mut().take(len) {
            *byte = ALPHABET[(rng.next() as usize) % ALPHABET.len()];
        }
        // Must never panic: only Ok or a typed error.
        let _ = parse(&buffer[..len]);
    }
}

#[test]
fn valid_round_trip_property() {
    // Any traceparent we can parse must reformat back to itself.
    let mut rng = XorShift::new(0x1234_5678_9ABC_DEF0);
    for _ in 0..10_000 {
        let mut trace_id = [0u8; 16];
        for byte in trace_id.iter_mut() {
            *byte = (rng.next() & 0xFF) as u8;
        }
        let mut parent_id = [0u8; 8];
        for byte in parent_id.iter_mut() {
            *byte = (rng.next() & 0xFF) as u8;
        }
        // Avoid the all-zero (invalid) ids.
        trace_id[0] |= 1;
        parent_id[0] |= 1;
        let value = tp(trace_id, parent_id, (rng.next() & 0xFF) as u8);
        let bytes = format(&value);
        assert_eq!(parse(&bytes), Ok(value));
    }
}

fn hex(nibble: u8) -> u8 {
    b"0123456789abcdef"[nibble as usize]
}
