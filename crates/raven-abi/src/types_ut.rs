// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for handles, rights, map flags and futex flags.

use super::*;

#[test]
fn handle_packing() {
    let h = Handle::new(0x00AB_CDEF, 0x7F);
    assert_eq!(h.index(), 0x00AB_CDEF);
    assert_eq!(h.generation(), 0x7F);
    assert_eq!(
        Handle::new(0x0100_0000, 1).index(),
        0,
        "index is masked to 24 bits"
    );
}

#[test]
fn rights_only_shrink_on_dup() {
    let r = Rights::READ.union(Rights::WRITE).union(Rights::DUP);
    let d = r.intersect(Rights::READ);
    assert!(d.contains(Rights::READ));
    assert!(!d.contains(Rights::WRITE));
    assert!(!r.intersect(Rights::MAP).contains(Rights::MAP));
}

#[test]
fn map_flag_packing() {
    let f = map::pack(prot::READ | prot::WRITE, map::FIXED | map::SHARED);
    assert_eq!(f & prot::MASK, 3);
    assert_eq!(f >> map::SHIFT, 3);
}

#[test]
fn inheritance_flag_values_are_distinct() {
    assert_eq!(map::NOFORK, 8);
    assert_eq!(map::ZERO_ON_FORK, 16);
    assert_eq!(map::FIXED & map::NOFORK, 0);
    assert_eq!(map::SHARED & map::ZERO_ON_FORK, 0);
    assert_eq!(map::NOFORK & map::ZERO_ON_FORK, 0);
    assert_eq!(map::pack(0, map::NOFORK), map::NOFORK << map::SHIFT);
    assert_eq!(
        map::pack(0, map::ZERO_ON_FORK),
        map::ZERO_ON_FORK << map::SHIFT
    );
}

#[test]
fn futex_private_flag_is_bit_7() {
    assert_eq!(futex::PRIVATE, 0x80);
    assert_eq!(futex::PRIVATE & (futex::WAIT | futex::WAKE), 0);
}

#[test]
fn robust_owner_bits_are_disjoint() {
    assert_eq!(futex::OWNER_DIED, 0x4000_0000);
    assert_eq!(futex::OWNER_TID, 0x3fff_ffff);
    assert_eq!(futex::OWNER_DIED & futex::OWNER_TID, 0);
}
