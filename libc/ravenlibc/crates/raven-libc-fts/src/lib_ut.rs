// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Unit tests for the crate constants and the public surface.
// Test-only code is exempt from the production lints (17-crate-layout 9.3).
#![allow(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

use super::*;

#[test]
fn fts_option_values_are_the_freebsd_header() {
    assert_eq!(FTS_COMFOLLOW, 0x0001);
    assert_eq!(FTS_LOGICAL, 0x0002);
    assert_eq!(FTS_NOCHDIR, 0x0004);
    assert_eq!(FTS_NOSTAT, 0x0008);
    assert_eq!(FTS_PHYSICAL, 0x0010);
    assert_eq!(FTS_SEEDOT, 0x0020);
    assert_eq!(FTS_XDEV, 0x0040);
    assert_eq!(FTS_WHITEOUT, 0x0080);
    assert_eq!(FTS_COMFOLLOWDIR, 0x0400);
    assert_eq!(FTS_NOSTAT_TYPE, 0x0800);
    assert_eq!(FTS_OPTIONMASK, 0x0cff);
    assert_eq!(FTS_NAMEONLY, 0x0100);
    assert_eq!(FTS_NAMEONLY & !FTS_OPTIONMASK, FTS_NAMEONLY);
}

#[test]
fn fts_info_values_are_the_freebsd_header() {
    assert_eq!(
        [
            FTS_D,
            FTS_DC,
            FTS_DEFAULT,
            FTS_DNR,
            FTS_DOT,
            FTS_DP,
            FTS_ERR,
            FTS_F,
            FTS_INIT,
            FTS_NS,
            FTS_NSOK,
            FTS_SL,
            FTS_SLNONE,
            FTS_W,
        ],
        [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14]
    );
}

#[test]
fn fts_instructions_and_levels() {
    assert_eq!(FTS_AGAIN, 1);
    assert_eq!(FTS_FOLLOW, 2);
    assert_eq!(FTS_NOINSTR, 3);
    assert_eq!(FTS_SKIP, 4);
    assert_eq!(FTS_ROOTPARENTLEVEL, -1);
    assert_eq!(FTS_ROOTLEVEL, 0);
    assert_eq!(AT_FDCWD, -100);
}

#[test]
fn ftw_values_are_the_freebsd_header() {
    assert_eq!(
        [FTW_F, FTW_D, FTW_DNR, FTW_DP, FTW_NS, FTW_SL, FTW_SLN],
        [0, 1, 2, 3, 4, 5, 6]
    );
    assert_eq!(FTW_PHYS, 0x01);
    assert_eq!(FTW_MOUNT, 0x02);
    assert_eq!(FTW_DEPTH, 0x04);
    assert_eq!(FTW_CHDIR, 0x08);
}

#[test]
fn limits_come_from_the_abi_crate() {
    assert_eq!(PATH_MAX, 4096);
    assert_eq!(NAME_MAX, 255);
    assert_eq!(SYMLOOP_MAX, 40);
    assert_eq!(DIRSTREAM_BUFSIZE, 2048);
}

#[test]
fn ftw_struct_defaults() {
    let ftw = Ftw::default();
    assert_eq!(ftw.base, 0);
    assert_eq!(ftw.level, 0);
}
