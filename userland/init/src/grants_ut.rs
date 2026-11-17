// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the service grant policy.

use super::{GrantSpec, parse_grants};
use alloc::string::ToString;
use alloc::vec;
use raven_abi::types::{Handle, Rights};
use raven_svc_manifest::Manifest;

/// The shared devd entry used by the release, server and hardware profiles: no device grant.
const RELEASE_DEVD: &str = include_str!("../../../etc/services.d/devd.toml");
/// The QEMU server profile's devd entry: PCI mode, no device grant.
const SERVER_DEVD: &str = include_str!("../../../profiles/server.d/devd.manifest");
/// The P10 qtest entry: devd under its real name with the platform root capability.
const PI4_DEVD_FIXTURE: &str = include_str!("../../../tests/fixtures/pi4-devd/etc/devd.toml");

fn names(values: &[&str]) -> alloc::vec::Vec<alloc::string::String> {
    values.iter().map(|value| value.to_string()).collect()
}

fn declared(text: &str) -> Manifest {
    Manifest::parse(text).expect("manifest parses")
}

fn declares_platform_root(manifest: &Manifest) -> bool {
    manifest.grants.iter().any(|grant| grant == "platform-root")
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
fn platform_root_is_accepted_for_devd() {
    assert_eq!(
        parse_grants("devd", &names(&["platform-root"])),
        Ok(vec![GrantSpec::PlatformRoot])
    );
}

#[test]
fn platform_root_is_rejected_for_an_unprivileged_service() {
    let error = parse_grants("logd", &names(&["platform-root"])).unwrap_err();
    assert!(error.contains("reserved for devd"));
}

#[test]
fn platform_root_is_rejected_for_pcid() {
    assert!(parse_grants("pcid", &names(&["platform-root"])).is_err());
}

#[test]
fn platform_root_is_rejected_for_a_service_with_a_devd_prefix() {
    assert!(parse_grants("devd-helper", &names(&["platform-root"])).is_err());
}

#[test]
fn devd_without_the_capability_gets_no_device_grant() {
    assert_eq!(
        parse_grants("devd", &names(&["initfs"])),
        Ok(vec![GrantSpec::Initfs])
    );
}

#[test]
fn platform_root_is_the_root_device_with_admin_only() {
    assert_eq!(GrantSpec::PlatformRoot.handle(), Handle::ROOT_DEVICE);
    assert_eq!(GrantSpec::PlatformRoot.rights(), Rights::ADMIN);
}

#[test]
fn platform_root_must_be_the_only_grant() {
    assert!(parse_grants("devd", &names(&["initfs", "platform-root"])).is_err());
    assert!(parse_grants("devd", &names(&["platform-root", "initfs"])).is_err());
    assert!(parse_grants("devd", &names(&["platform-root", "platform-root"])).is_err());
}

#[test]
fn release_and_server_devd_entries_do_not_declare_platform_root() {
    for text in [RELEASE_DEVD, SERVER_DEVD] {
        let manifest = declared(text);
        assert_eq!(manifest.name, "devd");
        assert!(!declares_platform_root(&manifest));
        assert!(
            !parse_grants(&manifest.name, &manifest.grants)
                .expect("grant policy accepts the manifest")
                .contains(&GrantSpec::PlatformRoot)
        );
    }
}

#[test]
fn the_pi4_fixture_runs_devd_under_its_real_name_with_platform_root() {
    let manifest = declared(PI4_DEVD_FIXTURE);
    assert_eq!(manifest.name, "devd");
    assert!(declares_platform_root(&manifest));
    assert_eq!(
        parse_grants(&manifest.name, &manifest.grants),
        Ok(vec![GrantSpec::PlatformRoot])
    );
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
