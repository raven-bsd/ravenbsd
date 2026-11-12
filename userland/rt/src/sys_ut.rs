// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for [`crate::sys`].

use super::*;
use raven_abi::types::clock;

#[test]
fn clock_constants_are_the_linux_ones() {
    assert_eq!(clock::REALTIME, 0);
    assert_eq!(clock::MONOTONIC, 1);
}

#[test]
fn handle_conversion_keeps_abi_bits() {
    let handle = Handle(0x1234_5678);
    assert_eq!(handle.0 as usize, 0x1234_5678);
}

fn initfs_header(total: u64) -> [u8; 48] {
    let mut header = [0u8; 48];
    header[..8].copy_from_slice(INITFS_MAGIC);
    header[INITFS_TOTAL_SIZE_OFFSET..INITFS_TOTAL_SIZE_OFFSET + 8]
        .copy_from_slice(&total.to_le_bytes());
    header
}

#[test]
fn initfs_header_reads_the_total_size() {
    assert_eq!(initfs_total_size(&initfs_header(8192)), Ok(8192));
}

#[test]
fn initfs_header_rejects_bad_magic_and_short_input() {
    let mut header = initfs_header(4096);
    header[0] = b'X';
    assert_eq!(initfs_total_size(&header), Err(Errno::EINVAL));
    assert_eq!(initfs_total_size(&[]), Err(Errno::EINVAL));
    assert_eq!(initfs_total_size(&header[..40]), Err(Errno::EINVAL));
}

#[test]
fn initfs_header_rejects_bad_total_size() {
    assert_eq!(initfs_total_size(&initfs_header(0)), Err(Errno::EINVAL));
    assert_eq!(initfs_total_size(&initfs_header(4097)), Err(Errno::EINVAL));
}

#[test]
fn initfs_size_must_fit_the_fixed_window() {
    let window = USER_END - INITFS_WINDOW;
    assert_eq!(validate_initfs_size(window as u64), Ok(window));
    assert_eq!(
        validate_initfs_size(window as u64 + PAGE_SIZE as u64),
        Err(Errno::ENOMEM)
    );
    let huge = 0xFFFF_FFFF_FFFF_F000u64;
    assert_eq!(validate_initfs_size(huge), Err(Errno::ENOMEM));
}

#[test]
fn transfer_rejects_none_handles_and_empty_rights() {
    assert_eq!(
        handle_transfer(Handle::NONE, Handle(1), Rights::READ),
        Err(Errno::EINVAL)
    );
    assert_eq!(
        handle_transfer(Handle(1), Handle::NONE, Rights::READ),
        Err(Errno::EINVAL)
    );
    assert_eq!(
        handle_transfer(Handle(1), Handle(2), Rights::NONE),
        Err(Errno::EINVAL)
    );
}

#[test]
fn transfer_validation_accepts_real_arguments() {
    assert_eq!(
        validate_transfer(
            Handle::SELF_PROC,
            Handle::INITFS,
            Rights::READ.union(Rights::MAP)
        ),
        Ok(())
    );
}

#[test]
fn handle_return_maps_errno_and_values() {
    assert_eq!(handle_from_ret(Errno::EPERM.to_ret()), Err(Errno::EPERM));
    assert_eq!(handle_from_ret(7), Ok(Handle(7)));
}
