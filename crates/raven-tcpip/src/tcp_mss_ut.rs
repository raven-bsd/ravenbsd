// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the internal option padding and ceiling.

use super::*;

#[test]
fn padding_matches_padtcpolen() {
    assert_eq!(pad_option_length(2), 4);
    assert_eq!(pad_option_length(3), 4);
    assert_eq!(pad_option_length(4), 4);
    assert_eq!(pad_option_length(18), 20);
    assert_eq!(pad_option_length(30), 32);
}

#[test]
fn the_option_ceiling_is_forty_bytes() {
    let options = MssOptions {
        requested_timestamp: true,
        requested_window_scale: true,
        sack_permitted: true,
        signature: true,
        ..MssOptions::default()
    };
    assert_eq!(option_length(MssState::Handshake, &options), TCP_MAXOLEN);
}

#[test]
fn fixed_maxseg_ignores_the_variable_sack_blocks() {
    let options = MssOptions {
        received_timestamp: true,
        sack_permitted: true,
        received_sack_blocks: 4,
        ..MssOptions::default()
    };
    assert!(
        tcp_maxseg(1460, MssState::Established, &options)
            < tcp_fixed_maxseg(1460, MssState::Established, &options)
    );
}

#[test]
fn no_options_short_circuits_both_forms() {
    let options = MssOptions::none();
    assert_eq!(tcp_maxseg(1460, MssState::Handshake, &options), 1460);
    assert_eq!(
        tcp_fixed_maxseg(1460, MssState::Established, &options),
        1460
    );
}
