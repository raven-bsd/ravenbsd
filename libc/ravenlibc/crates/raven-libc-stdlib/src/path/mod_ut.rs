// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::assertions_on_constants
)]

//! The path group surface.

use super::*;

#[test]
fn constants_and_reexports() {
    assert_eq!(MAXSYMLINKS, 40);
    assert_eq!(FileKind::Dir, FileKind::Dir);
    struct NoWalker;
    impl Walker for NoWalker {
        fn lstat_kind(&mut self, _: &[u8]) -> Result<FileKind, raven_libc_abi::errno::Errno> {
            Err(raven_libc_abi::errno::Errno(
                raven_libc_abi::errno::ENOENT as u16,
            ))
        }
        fn readlink(
            &mut self,
            _: &[u8],
            _: &mut [u8],
        ) -> Result<usize, raven_libc_abi::errno::Errno> {
            Err(raven_libc_abi::errno::Errno(
                raven_libc_abi::errno::ENOENT as u16,
            ))
        }
        fn getcwd(&mut self, _: &mut [u8]) -> Result<usize, raven_libc_abi::errno::Errno> {
            Err(raven_libc_abi::errno::Errno(
                raven_libc_abi::errno::ENOENT as u16,
            ))
        }
    }
    let mut out = vec![0u8; raven_libc_abi::limits::PATH_MAX as usize];
    assert!(realpath(&mut NoWalker, b"", &mut out).is_err());
}

#[test]
fn file_kinds_are_distinct() {
    assert_ne!(FileKind::Regular, FileKind::Dir);
    assert_ne!(FileKind::Dir, FileKind::Symlink);
}

#[test]
fn max_symlinks_is_forty() {
    assert_eq!(MAXSYMLINKS, 40);
}
