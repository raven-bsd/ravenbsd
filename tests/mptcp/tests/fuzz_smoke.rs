// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Deterministic replay of the `mptcp_options` fuzz target.
//!
//! The nightly `cargo fuzz` target is in `tests/mptcp/fuzz`; this test runs the same invariant
//! without nightly: every byte string is either rejected or re-encodes to the bytes it parsed.

use raven_mptcp::MptcpOption;

/// A small deterministic generator so the replay is identical on every host.
fn next(state: &mut u64) -> u64 {
    *state = state
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407);
    *state
}

#[test]
fn ten_thousand_arbitrary_option_blocks_are_total() {
    let mut state = 0x1357_9bdf_2468_ace0u64;
    for iteration in 0..10_000 {
        let length = (next(&mut state) % 65) as usize;
        let mut bytes = vec![0u8; length];
        for byte in &mut bytes {
            *byte = (next(&mut state) >> 24) as u8;
        }

        // The whole block must not panic; errors are expected and valid.
        let _ = raven_mptcp::option::parse_options(&bytes);

        if let Ok((option, used)) = MptcpOption::parse(&bytes) {
            assert!(
                used <= bytes.len(),
                "iteration {iteration}: used past input"
            );
            assert_eq!(
                option.to_vec(),
                &bytes[..used],
                "iteration {iteration}: accepted bytes are not canonical"
            );
        }
    }
}

#[test]
fn a_valid_vector_survives_trailing_garbage() {
    let mut bytes = vec![30, 4, 0x01, 0x01];
    bytes.extend_from_slice(&[0xff; 16]);
    let (option, used) = MptcpOption::parse(&bytes).expect("parsed prefix");
    assert_eq!(used, 4);
    assert_eq!(option.to_vec(), bytes[..4]);
}
