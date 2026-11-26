// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `ShellUnix` flag tests.

use super::*;
use raven_libc_abi::socket::SOCK_STREAM;

#[test]
fn stream_pair_has_no_extra_flags() {
    assert_eq!(duplex_flags(SOCK_STREAM, false, false), 0);
}

#[test]
fn datagram_pair_sets_the_message_mode_bit() {
    assert_eq!(duplex_flags(SOCK_DGRAM, false, false), DUPLEX_DATAGRAM);
}

#[test]
fn descriptor_flags_are_forwarded_to_the_kernel_pair() {
    assert_eq!(duplex_flags(SOCK_STREAM, true, false), DUPLEX_O_NONBLOCK);
    assert_eq!(duplex_flags(SOCK_STREAM, false, true), DUPLEX_O_CLOEXEC);
    assert_eq!(
        duplex_flags(SOCK_DGRAM, true, true),
        DUPLEX_DATAGRAM | DUPLEX_O_NONBLOCK | DUPLEX_O_CLOEXEC
    );
}

fn raw_record(handles: &[u32]) -> Vec<u8> {
    let length = CONTROL_HEADER as u32 + handles.len() as u32 * 4;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&length.to_le_bytes());
    bytes.extend_from_slice(&(SOL_SOCKET as u32).to_le_bytes());
    bytes.extend_from_slice(&(SCM_RIGHTS as u32).to_le_bytes());
    bytes.extend_from_slice(&(handles.len() as u32).to_le_bytes());
    for handle in handles {
        bytes.extend_from_slice(&handle.to_le_bytes());
    }
    bytes
}

fn joined(records: &[Vec<u8>]) -> Vec<u8> {
    records.iter().flatten().copied().collect()
}

#[test]
fn one_raw_record_keeps_its_handle() {
    let control = raw_record(&[7]);
    let records = parse_raw_controls(&control, control.len(), 1).expect("one record");
    assert_eq!(records, vec![vec![7]]);
}

#[test]
fn two_raw_records_keep_the_sender_boundaries() {
    let control = joined(&[raw_record(&[3]), raw_record(&[4, 5])]);
    let records = parse_raw_controls(&control, control.len(), 3).expect("two records");
    assert_eq!(records, vec![vec![3], vec![4, 5]]);
}

#[test]
fn a_handle_total_that_disagrees_is_rejected() {
    let control = joined(&[raw_record(&[3]), raw_record(&[4, 5])]);
    assert_eq!(
        parse_raw_controls(&control, control.len(), 2),
        Err(Errno::EPROTO)
    );
}

#[test]
fn a_record_that_runs_past_the_used_length_is_rejected() {
    let control = raw_record(&[3]);
    assert_eq!(
        parse_raw_controls(&control, control.len() - 1, 1),
        Err(Errno::EPROTO)
    );
}

#[test]
fn a_record_with_a_foreign_level_or_type_is_rejected() {
    let mut wrong_level = raw_record(&[3]);
    wrong_level[4..8].copy_from_slice(&2u32.to_le_bytes());
    assert_eq!(
        parse_raw_controls(&wrong_level, wrong_level.len(), 1),
        Err(Errno::EPROTO)
    );
    let mut wrong_type = raw_record(&[3]);
    wrong_type[8..12].copy_from_slice(&2u32.to_le_bytes());
    assert_eq!(
        parse_raw_controls(&wrong_type, wrong_type.len(), 1),
        Err(Errno::EPROTO)
    );
}

#[test]
fn a_record_whose_length_is_not_handle_aligned_is_rejected() {
    let mut control = raw_record(&[3]);
    control[0..4].copy_from_slice(&(CONTROL_HEADER as u32 + 3).to_le_bytes());
    assert_eq!(
        parse_raw_controls(&control, control.len(), 1),
        Err(Errno::EPROTO)
    );
}
