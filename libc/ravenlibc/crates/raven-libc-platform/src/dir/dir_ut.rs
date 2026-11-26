// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Directory module tests.

#[test]
fn getcwd_reexport_matches_the_implementation() {
    let mut buffer = [0u8; 4];
    let result = crate::dir::getcwd(b"/x", Some(&mut buffer), 4);
    assert_eq!(result, Ok(None));
    assert_eq!(&buffer[..3], b"/x\0");
}

/// The public `struct dirent` is the FreeBSD profile at the S700 pin: d_fileno at 0, d_namlen
/// at 20, d_name at 24, 280 bytes. The numbers are written from `freebsd:sys/sys/dirent.h`.
#[test]
fn public_dirent_record_is_the_freebsd_profile() {
    use raven_libc_abi::structs::Dirent;

    assert_eq!(core::mem::size_of::<Dirent>(), 280);
    assert_eq!(core::mem::align_of::<Dirent>(), 8);
    assert_eq!(core::mem::offset_of!(Dirent, d_fileno), 0);
    assert_eq!(core::mem::offset_of!(Dirent, d_off), 8);
    assert_eq!(core::mem::offset_of!(Dirent, d_reclen), 16);
    assert_eq!(core::mem::offset_of!(Dirent, d_type), 18);
    assert_eq!(core::mem::offset_of!(Dirent, d_pad0), 19);
    assert_eq!(core::mem::offset_of!(Dirent, d_namlen), 20);
    assert_eq!(core::mem::offset_of!(Dirent, d_pad1), 22);
    assert_eq!(core::mem::offset_of!(Dirent, d_name), 24);
}

/// FreeBSD `_GENERIC_DIRLEN`: header + name + NUL, rounded up to eight bytes.
#[test]
fn dirent_reclen_is_the_donor_generic_dirlen() {
    use crate::dir::{DIRENT_MAX_LEN, DIRENT_MIN_LEN, dirent_reclen};

    assert_eq!(dirent_reclen(1), 32);
    assert_eq!(dirent_reclen(2), 32);
    assert_eq!(dirent_reclen(8), 40);
    assert_eq!(dirent_reclen(9), 40);
    assert_eq!(dirent_reclen(255), 280);
    assert_eq!(dirent_reclen(1), DIRENT_MIN_LEN);
    assert_eq!(dirent_reclen(255), DIRENT_MAX_LEN);
}
