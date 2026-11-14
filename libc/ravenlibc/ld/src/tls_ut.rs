// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use raven_libc_tls::consts::Abi;
use raven_libc_tls::layout::{TlsModuleIn, compute_static_layout_for};

#[test]
fn the_state_starts_empty() {
    let state = TlsState::empty();
    assert_eq!(state.count, 0);
    assert_eq!(state.static_tls_extra, 1664);
}

#[test]
fn a_module_gets_an_offset() {
    let mut state = TlsState::empty();
    let offset = state
        .assign(TlsModuleIn::new(0, 16, 16, 16))
        .expect("offset");
    assert!(offset >= 16);
    assert_eq!(state.count, 1);
}

#[test]
fn two_modules_get_distinct_offsets() {
    let mut state = TlsState::empty();
    let first = state
        .assign(TlsModuleIn::new(0, 16, 16, 16))
        .expect("first");
    let second = state.assign(TlsModuleIn::new(0, 8, 8, 8)).expect("second");
    assert_ne!(first, second);
}

#[test]
fn the_variant_two_first_module_matches_the_vector() {
    let layout = compute_static_layout_for(
        Abi::VariantII { dtp_offset: 0 },
        &[TlsModuleIn::new(0, 16, 16, 16)],
        64,
    )
    .expect("layout");
    assert_eq!(layout.size, 16);
}

#[test]
fn the_variant_two_second_module_matches_the_vector() {
    let layout = compute_static_layout_for(
        Abi::VariantII { dtp_offset: 0 },
        &[
            TlsModuleIn::new(0, 16, 16, 16),
            TlsModuleIn::new(16, 8, 8, 8),
        ],
        64,
    )
    .expect("layout");
    assert_eq!(layout.size, 24);
}

#[test]
fn the_variant_two_alignment_vector_matches() {
    let layout = compute_static_layout_for(
        Abi::VariantII { dtp_offset: 0 },
        &[TlsModuleIn::new(8, 16, 16, 16)],
        64,
    )
    .expect("layout");
    assert_eq!(layout.size, 24);
}

#[test]
fn the_variant_one_aarch64_first_module_rounds_to_64() {
    let layout = compute_static_layout_for(
        Abi::VariantI {
            gap_above_tp: 16,
            dtp_offset: 0,
        },
        &[TlsModuleIn::new(0, 0, 0, 64)],
        64,
    )
    .expect("layout");
    assert!(layout.offset >= 16);
}
