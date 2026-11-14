// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The four lookups and the two enumerations over C51's plain-file source. `FilesSource` is
//! created per call (no cache, C51); the error type is the positive C `errno`. The two source
//! traits share the method names, so every call is trait-qualified.

use alloc::vec::Vec;

use raven_libc_abi::errno::ENOENT;
use raven_libc_pwd::{FilesSource, Group, GroupSource, Passwd, PasswdSource};

use super::source::ShellFiles;

/// The positive `errno` of a C string that is not UTF-8.
fn text(value: &[u8]) -> Result<&str, i32> {
    core::str::from_utf8(value).map_err(|_| ENOENT)
}

/// `getpwnam`'s lookup.
pub(crate) fn passwd_by_name(name: &[u8]) -> Result<Option<Passwd>, i32> {
    let name = text(name)?;
    let source = FilesSource::new(&ShellFiles);
    PasswdSource::by_name(&source, name)
}

/// `getpwuid`'s lookup.
pub(crate) fn passwd_by_uid(uid: u32) -> Result<Option<Passwd>, i32> {
    let source = FilesSource::new(&ShellFiles);
    PasswdSource::by_uid(&source, uid)
}

/// `getpwent`'s list.
pub(crate) fn passwd_entries() -> Result<Vec<Passwd>, i32> {
    let source = FilesSource::new(&ShellFiles);
    PasswdSource::entries(&source)
}

/// `getgrnam`'s lookup.
pub(crate) fn group_by_name(name: &[u8]) -> Result<Option<Group>, i32> {
    let name = text(name)?;
    let source = FilesSource::new(&ShellFiles);
    GroupSource::by_name(&source, name)
}

/// `getgrgid`'s lookup.
pub(crate) fn group_by_gid(gid: u32) -> Result<Option<Group>, i32> {
    let source = FilesSource::new(&ShellFiles);
    GroupSource::by_gid(&source, gid)
}

/// `getgrent`'s list.
pub(crate) fn group_entries() -> Result<Vec<Group>, i32> {
    let source = FilesSource::new(&ShellFiles);
    GroupSource::entries(&source)
}
