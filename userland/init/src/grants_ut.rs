// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the service grant policy.

use super::{GrantSpec, parse_grants};
use alloc::string::ToString;
use alloc::vec;
use raven_abi::types::{Handle, Rights};

fn names(values: &[&str]) -> alloc::vec::Vec<alloc::string::String> {
    values.iter().map(|value| value.to_string()).collect()
}

#[test]
fn initfs_is_accepted_for_any_service() {
    assert_eq!(
        parse_grants("logd", &names(&["initfs"])),
        Ok(vec![GrantSpec::Initfs])
    );
}

#[test]
fn root_is_accepted_for_pcid() {
    assert_eq!(
        parse_grants("pcid", &names(&["root"])),
        Ok(vec![GrantSpec::Root])
    );
}

#[test]
fn root_is_rejected_for_another_service() {
    let error = parse_grants("devd", &names(&["root"])).unwrap_err();
    assert!(error.contains("reserved for pcid"));
}

#[test]
fn root_is_rejected_for_a_service_with_a_pcid_prefix() {
    assert!(parse_grants("pcid-helper", &names(&["root"])).is_err());
}

#[test]
fn unknown_grants_are_rejected() {
    assert!(parse_grants("pcid", &names(&["block"])).is_err());
}

#[test]
fn empty_grants_are_empty() {
    assert_eq!(parse_grants("pcid", &[]), Ok(vec![]));
}

#[test]
fn root_rights_are_admin_dup_transfer() {
    assert_eq!(
        GrantSpec::Root.rights(),
        Rights::ADMIN.union(Rights::DUP).union(Rights::TRANSFER)
    );
    assert_eq!(GrantSpec::Root.handle(), Handle::ROOT_DEVICE);
}

#[test]
fn initfs_rights_are_read_map_exec_transfer() {
    assert_eq!(
        GrantSpec::Initfs.rights(),
        Rights::READ
            .union(Rights::MAP)
            .union(Rights::EXEC)
            .union(Rights::TRANSFER)
    );
    assert_eq!(GrantSpec::Initfs.handle(), Handle::INITFS);
}
