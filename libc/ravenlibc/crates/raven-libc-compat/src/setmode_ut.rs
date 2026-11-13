// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `setmode`/`getmode` tests.
//!
//! The rows are the canonical `chmod(1)` cases; the full 200-spec x 16-mode x 4-umask
//! differential against the C original is the `compat-setmode` oracle family
//! (`tests/libc/oracle/recipes/compat-setmode`).

#![allow(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::byte_char_slices,
    clippy::field_reassign_with_default,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented
)]

use super::*;

fn mode(spec: &[u8], omode: u32, umask: u32) -> u32 {
    setmode(spec).unwrap().getmode(omode, umask)
}

#[test]
fn absolute_modes() {
    assert_eq!(mode(b"755", 0, 0o022), 0o755);

    assert_eq!(mode(b"0755", 0, 0o022), 0o755);
    assert_eq!(mode(b"0", 0o777, 0), 0);
    assert_eq!(mode(b"7777", 0, 0), 0o7777);
}

#[test]
fn absolute_modes_are_mask_relative_by_the_donor_rule() {
    // ADDCMD('=', ..., perm, mask): the '+' half is `mask & perm`.
    assert_eq!(mode(b"777", 0, 0o022), 0o777);
    assert_eq!(mode(b"777", 0, 0), 0o777);
}

#[test]
fn add_and_remove_permissions() {
    assert_eq!(mode(b"u+x", 0o644, 0), 0o744);
    assert_eq!(mode(b"u-x", 0o755, 0), 0o655);
    assert_eq!(mode(b"go-w", 0o777, 0), 0o755);
    assert_eq!(mode(b"a+x", 0o644, 0), 0o755);
    assert_eq!(mode(b"+x", 0o644, 0), 0o755);
    assert_eq!(mode(b"-w", 0o666, 0), 0o444);
}

#[test]
fn assignment_with_and_without_who() {
    assert_eq!(mode(b"a=r", 0o777, 0), 0o444);
    assert_eq!(mode(b"u=rwx,g=rx,o=", 0o777, 0), 0o750);
    assert_eq!(mode(b"a=", 0o777, 0), 0);
    assert_eq!(mode(b"=rw", 0o777, 0), 0o666);
    assert_eq!(mode(b"=rw", 0o777, 0o022), 0o644);
}

#[test]
fn set_id_and_sticky_bits() {
    assert_eq!(mode(b"u+s", 0o644, 0), 0o4644);
    assert_eq!(mode(b"g+s", 0o644, 0), 0o2644);
    assert_eq!(mode(b"+s", 0o644, 0), 0o6644);
    assert_eq!(mode(b"+t", 0o777, 0), 0o1777);
    assert_eq!(mode(b"o+s", 0o644, 0), 0o644);
    assert_eq!(mode(b"a-s", 0o7777, 0), 0o1777);
    assert_eq!(mode(b"a-t", 0o1777, 0), 0o777);
}

#[test]
fn copy_permissions() {
    assert_eq!(mode(b"g=u", 0o741, 0), 0o771);
    assert_eq!(mode(b"o=u", 0o741, 0), 0o747);
    assert_eq!(mode(b"u=g", 0o741, 0), 0o441);
    assert_eq!(mode(b"g+u", 0o700, 0), 0o770);
    assert_eq!(mode(b"g-u", 0o770, 0), 0o700);
    assert_eq!(mode(b"o=g", 0o740, 0), 0o744);
    assert_eq!(mode(b"u=g,o=g", 0o741, 0), 0o444);
    // The `X` after a copy operation.
    assert_eq!(mode(b"u+X", 0o644, 0), 0o644);
    assert_eq!(mode(b"u+X", 0o744, 0), 0o744);
}

#[test]
fn capital_x_depends_on_the_original_mode() {
    assert_eq!(mode(b"a+X", 0o644, 0), 0o644);
    assert_eq!(mode(b"a+X", 0o700, 0), 0o711);
    assert_eq!(mode(b"go+X", 0o750, 0), 0o751);
    // A directory always gets the x bits.
    assert_eq!(mode(b"a+X", S_IFDIR | 0o644, 0), S_IFDIR | 0o755);
}

#[test]
fn no_who_uses_the_umask() {
    assert_eq!(mode(b"+w", 0o644, 0), 0o666);
    assert_eq!(mode(b"+w", 0o644, 0o022), 0o644);
    assert_eq!(mode(b"+w", 0o644, 0o222), 0o644);
    assert_eq!(mode(b"=w", 0o777, 0o022), 0o200);
}

#[test]
fn multiple_clauses_with_commas() {
    assert_eq!(mode(b"u+x,g+x,o+x", 0, 0), 0o111);
    assert_eq!(mode(b"u+r-w+x", 0, 0), 0o500);
    // A comma is optional in the donor: a second who starts a new clause.
    assert_eq!(mode(b"u+xg-w", 0o777, 0), 0o577);
}

#[test]
fn umask_is_applied_at_getmode_time() {
    let set = setmode(b"=rw").unwrap();
    assert_eq!(set.getmode(0o777, 0), 0o666);
    assert_eq!(set.getmode(0o777, 0o022), 0o644);
    assert_eq!(set.getmode(0o777, 0o077), 0o600);
}

#[test]
fn malformed_specs_are_none() {
    for spec in [
        b"".as_slice(),
        b"u+z",
        b"u",
        b"q+r",
        b"08",
        b"999",
        b"u+q",
        b"u=r/",
    ] {
        assert!(setmode(spec).is_none(), "spec {spec:?}");
    }
}

#[test]
fn donor_accepts_an_empty_assignment_after_a_clause() {
    // The OpenBSD donor's `=x=` is two clauses: `=x` then `=` (which clears everything).
    assert_eq!(mode(b"=x=", 0o777, 0), 0);
}

#[test]
fn a_single_spec_does_not_mutate_the_original_list() {
    let set = setmode(b"u+x").unwrap();
    let first = set.getmode(0o644, 0);
    let second = set.getmode(0o644, 0);
    assert_eq!(first, second);
}

#[test]
fn two_hundred_specs_stay_within_the_mode_bits() {
    const WHO: [&[u8]; 6] = [b"", b"u", b"g", b"o", b"a", b"ug"];
    const OPS: [&[u8]; 3] = [b"+", b"-", b"="];
    const PERMS: [&[u8]; 8] = [b"", b"r", b"w", b"x", b"rw", b"rx", b"rwx", b"X"];
    let mut count = 0usize;
    for who in WHO {
        for op in OPS {
            for perm in PERMS {
                let mut spec = alloc::vec::Vec::new();
                spec.extend_from_slice(who);
                spec.extend_from_slice(op);
                spec.extend_from_slice(perm);
                if let Some(set) = setmode(&spec) {
                    count = count.wrapping_add(1);
                    for omode in [0u32, 0o644, 0o755, 0o7777, S_IFDIR | 0o755] {
                        for umask in [0u32, 0o022, 0o077, 0o777] {
                            let result = set.getmode(omode, umask);
                            assert_eq!(
                                result & !0o7777 & !S_IFDIR,
                                0,
                                "spec {spec:?} omode {omode:o} umask {umask:o} -> {result:o}"
                            );
                        }
                    }
                }
            }
        }
    }
    assert_eq!(count, 6 * 3 * 8);
}
