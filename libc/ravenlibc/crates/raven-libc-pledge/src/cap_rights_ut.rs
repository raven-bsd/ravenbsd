// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the Capsicum rights operations.

use super::*;
use crate::consts::{CAP_CREATE, CAP_FSTAT, CAP_FSYNC, CAP_FTRUNCATE, CAP_SEEK, CAP_SEEK_TELL};

#[test]
fn the_empty_set_has_only_index_bits() {
    let rights = CapRights::empty();
    assert_eq!(rights.r, [CAP_INDEX0, CAP_INDEX1]);
    assert!(rights.is_empty());
    assert!(rights.is_valid());
}

#[test]
fn init_adds_rights() {
    let rights = CapRights::init(&[CAP_READ, CAP_WRITE]);
    assert!(rights.is_set(&[CAP_READ]));
    assert!(rights.is_set(&[CAP_WRITE]));
    assert!(rights.is_set(&[CAP_READ, CAP_WRITE]));
    assert!(!rights.is_set(&[CAP_FEXECVE]));
}

#[test]
fn set_and_clear_round_trip() {
    let mut rights = CapRights::empty();
    rights.set(CAP_READ).expect("set");
    assert!(rights.is_set(&[CAP_READ]));
    rights.clear(CAP_READ).expect("clear");
    assert!(!rights.is_set(&[CAP_READ]));
    assert!(rights.is_valid());
}

#[test]
fn a_bad_index_is_rejected() {
    assert_eq!(CapRights::empty().set(0x2), Err(CapError::BadIndex));
    assert_eq!(
        CapRights::empty().set(CAP_INDEX0 | CAP_INDEX1),
        Err(CapError::BadIndex)
    );
}

#[test]
fn is_set_requires_every_right() {
    let rights = CapRights::init(&[CAP_READ]);
    assert!(!rights.is_set(&[CAP_READ, CAP_WRITE]));
}

#[test]
fn merge_and_remove_work() {
    let mut rights = CapRights::init(&[CAP_READ]);
    rights.merge(&CapRights::init(&[CAP_WRITE]));
    assert!(rights.is_set(&[CAP_READ, CAP_WRITE]));
    rights.remove(&CapRights::init(&[CAP_READ]));
    assert!(!rights.is_set(&[CAP_READ]));
    assert!(rights.is_set(&[CAP_WRITE]));
    assert!(rights.is_valid());
}

#[test]
fn contains_and_intersect_work() {
    let rights = CapRights::init(&[CAP_READ, CAP_WRITE]);
    assert!(rights.contains(&CapRights::init(&[CAP_READ])));
    assert!(!rights.contains(&CapRights::init(&[CAP_FEXECVE])));
    let mut value = rights;
    value.intersect(&CapRights::init(&[CAP_WRITE]));
    assert_eq!(value, CapRights::init(&[CAP_WRITE]));
}

#[test]
fn cap_all_round_trips() {
    let rights = CapRights::init(&[CAP_ALL0, CAP_ALL1]);
    assert!(rights.is_valid());
    assert!(rights.is_set(&[CAP_READ, CAP_WRITE, CAP_FEXECVE]));
}

#[test]
fn version_bits_invalidate() {
    let mut rights = CapRights::empty();
    rights.r[0] |= 1u64 << 62;
    assert!(!rights.is_valid());
}

#[test]
fn a_wrong_index_bit_invalidates() {
    let mut rights = CapRights::empty();
    rights.r[0] |= CAP_INDEX1;
    assert!(!rights.is_valid());
}

#[test]
fn an_extra_index_bit_invalidates() {
    let mut rights = CapRights::empty();
    rights.r[0] |= 1u64 << 59;
    assert!(!rights.is_valid());
}

#[test]
fn to_raven_reads_read_and_pread() {
    assert_eq!(to_raven_rights(&CapRights::init(&[CAP_READ])), 1);
    assert_eq!(to_raven_rights(&CapRights::init(&[CAP_PREAD])), 1);
}

#[test]
fn to_raven_reads_write_and_pwrite() {
    assert_eq!(to_raven_rights(&CapRights::init(&[CAP_WRITE])), 2);
    assert_eq!(to_raven_rights(&CapRights::init(&[CAP_PWRITE])), 2);
}

#[test]
fn to_raven_maps_every_mmap_right() {
    assert_eq!(to_raven_rights(&CapRights::init(&[CAP_MMAP])), 4);
    assert_eq!(to_raven_rights(&CapRights::init(&[CAP_MMAP_R])), 5);
    assert_eq!(to_raven_rights(&CapRights::init(&[CAP_MMAP_W])), 6);
    assert_eq!(to_raven_rights(&CapRights::init(&[CAP_MMAP_X])), 4);
}

#[test]
fn to_raven_maps_fexecve() {
    assert_eq!(to_raven_rights(&CapRights::init(&[CAP_FEXECVE])), 8);
}

#[test]
fn to_raven_ignores_other_rights() {
    assert_eq!(to_raven_rights(&CapRights::init(&[CAP_FSTAT])), 0);
    assert_eq!(to_raven_rights(&CapRights::init(&[CAP_CREATE])), 0);
    assert_eq!(to_raven_rights(&CapRights::init(&[CAP_FSYNC])), 0);
    assert_eq!(to_raven_rights(&CapRights::init(&[CAP_FTRUNCATE])), 0);
    assert_eq!(
        to_raven_rights(&CapRights::init(&[CAP_SEEK, CAP_SEEK_TELL])),
        0
    );
}

#[test]
fn the_raven_bit_values_match_the_contract() {
    use raven_abi::types::Rights;
    assert_eq!(
        to_raven_rights(&CapRights::init(&[CAP_READ])),
        Rights::READ.bits()
    );
    assert_eq!(
        to_raven_rights(&CapRights::init(&[CAP_WRITE])),
        Rights::WRITE.bits()
    );
    assert_eq!(
        to_raven_rights(&CapRights::init(&[CAP_MMAP])),
        Rights::MAP.bits()
    );
    assert_eq!(
        to_raven_rights(&CapRights::init(&[CAP_FEXECVE])),
        Rights::EXEC.bits()
    );
}

#[test]
fn the_constants_match_the_installed_header_names() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../include/sys/capsicum.h");
    let Ok(text) = std::fs::read_to_string(path) else {
        eprintln!("SKIP: capsicum.h not installed yet");
        return;
    };
    for name in [
        "CAP_READ",
        "CAP_WRITE",
        "CAP_SEEK_TELL",
        "CAP_SEEK",
        "CAP_PREAD",
        "CAP_PWRITE",
        "CAP_MMAP",
        "CAP_MMAP_R",
        "CAP_MMAP_W",
        "CAP_MMAP_X",
        "CAP_CREATE",
        "CAP_FEXECVE",
        "CAP_FSYNC",
        "CAP_FTRUNCATE",
        "CAP_FSTAT",
        "CAP_ALL0",
        "CAP_ALL1",
    ] {
        assert!(text.contains(&format!("#define\t{name}")), "{name}");
    }
}

#[test]
fn an_empty_rights_set_is_not_set() {
    let rights = CapRights::empty();
    assert!(rights.is_set(&[]));
    assert!(!rights.is_set(&[CAP_READ]));
}
