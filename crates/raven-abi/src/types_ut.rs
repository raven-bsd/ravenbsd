// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for handles, rights, map flags and futex flags.

use super::*;

#[test]
fn umask_normalizes_to_freebsd_allperms() {
    assert_eq!(umask::CMASK, 0o022, "FreeBSD sys/sys/param.h CMASK");
    assert_eq!(umask::normalize(0o022), 0o022);
    assert_eq!(umask::normalize(0o1_7777), 0o7777, "only ALLPERMS survive");
}

#[test]
fn open_create_mode_matches_freebsd_vfs_syscalls() {
    // FreeBSD vfs_syscalls.c sys_open: cmode = ((mode & ~pd_cmask) & ALLPERMS) & ~S_ISTXT.
    assert_eq!(umask::open_create_mode(0o666, 0o022), 0o644);
    assert_eq!(umask::open_create_mode(0o777, 0o077), 0o700);
    assert_eq!(
        umask::open_create_mode(0o7777, 0),
        0o6777,
        "only S_ISTXT is stripped; S_ISUID/S_ISGID survive ALLPERMS"
    );
    assert_eq!(umask::open_create_mode(0o666, 0o7777), 0);
}

#[test]
fn creation_context_is_a_fixed_wire_record() {
    assert_eq!(creation::VERSION, 1);
    assert_eq!(core::mem::size_of::<creation::Context>(), creation::SIZE);
    assert_eq!(core::mem::align_of::<creation::Context>(), 8);
    assert_eq!(core::mem::offset_of!(creation::Context, version), 0);
    assert_eq!(core::mem::offset_of!(creation::Context, size), 4);
    assert_eq!(core::mem::offset_of!(creation::Context, request_id), 8);
    assert_eq!(core::mem::offset_of!(creation::Context, pid), 16);
    assert_eq!(core::mem::offset_of!(creation::Context, uid), 24);
    assert_eq!(core::mem::offset_of!(creation::Context, umask), 40);
    assert_eq!(core::mem::offset_of!(creation::Context, mode), 44);
    assert_eq!(core::mem::offset_of!(creation::Context, groups), 56);
}

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
    assert_eq!(map::REPLACE, 4);
    assert_eq!(map::pack(0, map::FIXED | map::REPLACE), 0x500);
    assert_eq!(map::REPLACE & map::SHARED, 0);
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
