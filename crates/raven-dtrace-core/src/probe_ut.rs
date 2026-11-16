// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the probe registry.

use super::*;

#[test]
fn the_syscall_probe_is_registered() {
    let probe = probe(SYSCALL_ENTRY_ID).expect("syscall probe");
    assert_eq!(probe.provider, "syscall");
    assert_eq!(probe.name, "entry");
    assert_eq!(probe.id, SYSCALL_ENTRY_ID);
}

#[test]
fn lookup_by_name_matches_provider_and_name() {
    assert_eq!(
        probe_by_name("syscall", "entry").map(|probe| probe.id),
        Some(SYSCALL_ENTRY_ID)
    );
    assert_eq!(probe_by_name("syscall", "return"), None);
    assert_eq!(probe_by_name("fbt", "entry"), None);
}

#[test]
fn an_unknown_id_is_none() {
    assert_eq!(probe(0), None);
    assert_eq!(probe(999), None);
}

#[test]
fn info_is_filled_and_nul_padded() {
    let probe = probe(SYSCALL_ENTRY_ID).expect("syscall probe");
    let mut info = ProbeInfo::default();
    fill_info(probe, &mut info);
    assert_eq!(info.id, SYSCALL_ENTRY_ID);
    assert_eq!(&info.provider[..7], b"syscall");
    assert_eq!(info.provider[7], 0);
    assert_eq!(&info.name[..5], b"entry");
    assert_eq!(info.name[5], 0);
    assert_eq!(size_of::<ProbeInfo>(), 52);
}

#[test]
fn a_long_name_is_truncated_without_a_panic() {
    let probe = Probe {
        id: 7,
        provider: "a-provider-name-longer-than-sixteen",
        module: "",
        function: "",
        name: "a-probe-name-longer-than-thirty-two-bytes",
    };
    let mut info = ProbeInfo::default();
    fill_info(&probe, &mut info);
    assert_eq!(info.provider[15], 0);
    assert_eq!(info.name[31], 0);
}
