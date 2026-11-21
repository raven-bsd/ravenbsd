// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the crate-level run path.

use super::*;

fn minimal_pcap(link_type: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&0xa1b2_c3d4u32.to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&4u16.to_le_bytes());
    bytes.extend_from_slice(&0i32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&65535u32.to_le_bytes());
    bytes.extend_from_slice(&link_type.to_le_bytes());
    bytes
}

#[test]
fn execute_requires_a_capture_source() {
    let options = cli::Options::default();
    assert_eq!(execute(&[], &options), Err(RunError::MissingInput));
}

#[test]
fn execute_rejects_an_unknown_link_type() {
    let options = cli::Options {
        read: Some(String::from("x.pcap")),
        ..cli::Options::default()
    };
    let error = execute(&minimal_pcap(147), &options).unwrap_err();
    assert_eq!(
        error,
        RunError::Pcap(pcap::PcapError::UnsupportedLinkType(147))
    );
}

#[test]
fn execute_reports_a_bad_filter() {
    let options = cli::Options {
        read: Some(String::from("x.pcap")),
        filter: Some(String::from("bogus")),
        ..cli::Options::default()
    };
    let error = execute(&minimal_pcap(pcap::LINKTYPE_ETHERNET), &options).unwrap_err();
    assert!(matches!(error, RunError::Filter(_)));
}
