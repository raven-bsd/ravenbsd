// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the `EAI_*` values and their mappings.

use super::*;

fn all() -> [Eai; 11] {
    [
        Eai::BadFlags,
        Eai::NoName,
        Eai::Again,
        Eai::Fail,
        Eai::NoData,
        Eai::Family,
        Eai::SockType,
        Eai::Service,
        Eai::Memory,
        Eai::System,
        Eai::Overflow,
    ]
}

#[test]
fn eai_numeric_values_match_musl() {
    assert_eq!(
        (
            Eai::BadFlags.code(),
            Eai::NoName.code(),
            Eai::Again.code(),
            Eai::Fail.code(),
            Eai::NoData.code(),
            Eai::Family.code(),
            Eai::SockType.code(),
            Eai::Service.code(),
            Eai::Memory.code(),
            Eai::System.code(),
            Eai::Overflow.code(),
        ),
        (-1, -2, -3, -4, -5, -6, -7, -8, -10, -11, -12)
    );
}

#[test]
fn every_eai_round_trips_through_its_code() {
    for value in all() {
        assert_eq!(Eai::from_code(value.code()), Some(value));
    }
}

#[test]
fn from_code_rejects_unknown_values() {
    assert_eq!(Eai::from_code(0), None);
    assert_eq!(Eai::from_code(-9), None);
    assert_eq!(Eai::from_code(-13), None);
    assert_eq!(Eai::from_code(1), None);
}

#[test]
fn i32_conversion_uses_the_code() {
    let value: i32 = Eai::Service.into();
    assert_eq!(value, -8);
}

#[test]
fn display_matches_gai_strerror() {
    assert_eq!(alloc::format!("{}", Eai::BadFlags), "Invalid flags");
    assert_eq!(alloc::format!("{}", Eai::NoName), "Name does not resolve");
    assert_eq!(alloc::format!("{}", Eai::Again), "Try again");
    assert_eq!(alloc::format!("{}", Eai::Fail), "Non-recoverable error");
    assert_eq!(
        alloc::format!("{}", Eai::NoData),
        "Name has no usable address"
    );
    assert_eq!(alloc::format!("{}", Eai::Memory), "Out of memory");
    assert_eq!(alloc::format!("{}", Eai::Overflow), "Overflow");
}

#[test]
fn noname_maps_to_host_not_found() {
    assert_eq!(Eai::NoName.to_herror(), HOST_NOT_FOUND);
}

#[test]
fn nodata_maps_to_host_not_found_for_the_legacy_api() {
    assert_eq!(Eai::NoData.to_herror(), HOST_NOT_FOUND);
}

#[test]
fn again_maps_to_try_again() {
    assert_eq!(Eai::Again.to_herror(), TRY_AGAIN);
}

#[test]
fn fail_maps_to_no_recovery() {
    assert_eq!(Eai::Fail.to_herror(), NO_RECOVERY);
}

#[test]
fn system_maps_to_no_recovery() {
    assert_eq!(Eai::System.to_herror(), NO_RECOVERY);
}

#[test]
fn the_errno_mapping_is_nonzero_for_hard_failures() {
    assert_eq!(Eai::Again.to_errno(), raven_libc_abi::errno::EAGAIN);
    assert_ne!(Eai::Fail.to_errno(), 0);
    assert_ne!(Eai::Overflow.to_errno(), 0);
}

#[test]
fn rcode_3_is_host_not_found() {
    assert_eq!(h_errno_from_rcode(3), HOST_NOT_FOUND);
}

#[test]
fn rcode_2_is_try_again() {
    assert_eq!(h_errno_from_rcode(2), TRY_AGAIN);
}

#[test]
fn rcode_0_is_no_data() {
    assert_eq!(h_errno_from_rcode(0), NO_DATA);
}

#[test]
fn rcode_5_is_no_recovery() {
    assert_eq!(h_errno_from_rcode(5), NO_RECOVERY);
}

#[test]
fn rcode_1_is_no_recovery() {
    assert_eq!(h_errno_from_rcode(1), NO_RECOVERY);
}

#[test]
fn io_errors_map_to_system() {
    assert_eq!(eai_from_io(raven_libc_abi::errno::EIO), Eai::System);
    assert_eq!(eai_from_io(raven_libc_abi::errno::EAGAIN), Eai::System);
}

#[test]
fn eai_is_copy_and_comparable() {
    let first = Eai::Again;
    let second = first;
    assert_eq!(first, second);
}

#[test]
fn eai_hashes_by_value() {
    use alloc::collections::BTreeSet;
    let mut set: BTreeSet<i32> = BTreeSet::new();
    for value in all() {
        set.insert(value.code());
    }
    assert_eq!(set.len(), 11);
}
