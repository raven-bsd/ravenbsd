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

//! `realpath` over a fake in-memory tree.

use std::collections::BTreeMap;

use raven_libc_abi::errno::Errno;

use super::*;

struct Fake {
    kinds: BTreeMap<Vec<u8>, FileKind>,
    links: BTreeMap<Vec<u8>, Vec<u8>>,
    cwd: Vec<u8>,
}

impl Fake {
    fn new() -> Self {
        let mut fake = Self {
            kinds: BTreeMap::new(),
            links: BTreeMap::new(),
            cwd: b"/base".to_vec(),
        };
        for path in [
            b"/".as_slice(),
            b"/base",
            b"/base/a",
            b"/base/a/b",
            b"/a",
            b"/a/b",
            b"/a/b/c",
            b"/x",
        ] {
            fake.kinds.insert(path.to_vec(), FileKind::Dir);
        }
        fake.kinds.insert(b"/a/file".to_vec(), FileKind::Regular);
        fake.links.insert(b"/link".to_vec(), b"/a/b".to_vec());
        fake.links.insert(b"/a/rel".to_vec(), b"b/c".to_vec());
        fake.links.insert(b"/loop".to_vec(), b"/loop".to_vec());
        fake
    }

    fn add_link(&mut self, path: &[u8], target: &[u8]) {
        self.links.insert(path.to_vec(), target.to_vec());
        self.kinds.insert(path.to_vec(), FileKind::Symlink);
    }
}

impl Walker for Fake {
    fn lstat_kind(&mut self, path: &[u8]) -> Result<FileKind, Errno> {
        if let Some(kind) = self.links.get(path) {
            let _ = kind;
            return Ok(FileKind::Symlink);
        }
        self.kinds
            .get(path)
            .copied()
            .ok_or(Errno(raven_libc_abi::errno::ENOENT as u16))
    }

    fn readlink(&mut self, path: &[u8], buf: &mut [u8]) -> Result<usize, Errno> {
        let target = self
            .links
            .get(path)
            .cloned()
            .ok_or(Errno(raven_libc_abi::errno::EINVAL as u16))?;
        if target.len() > buf.len() {
            return Err(Errno(raven_libc_abi::errno::ENAMETOOLONG as u16));
        }
        buf[..target.len()].copy_from_slice(&target);
        Ok(target.len())
    }

    fn getcwd(&mut self, buf: &mut [u8]) -> Result<usize, Errno> {
        buf[..self.cwd.len()].copy_from_slice(&self.cwd);
        Ok(self.cwd.len())
    }
}

fn resolve(fake: &mut Fake, path: &[u8]) -> Result<Vec<u8>, u16> {
    let mut out = vec![0u8; raven_libc_abi::limits::PATH_MAX as usize];
    match realpath(fake, path, &mut out) {
        Ok(length) => Ok(out[..length].to_vec()),
        Err(error) => Err(error.0),
    }
}

#[test]
fn absolute_and_relative_paths() {
    let mut fake = Fake::new();
    assert_eq!(resolve(&mut fake, b"/a/b/c").unwrap(), b"/a/b/c");
    assert_eq!(resolve(&mut fake, b"a/b").unwrap(), b"/base/a/b");
    assert_eq!(resolve(&mut fake, b".").unwrap(), b"/base");
    assert_eq!(resolve(&mut fake, b"a/.").unwrap(), b"/base/a");
}

#[test]
fn dot_dot_at_root_and_trailing_slashes() {
    let mut fake = Fake::new();
    assert_eq!(resolve(&mut fake, b"/..").unwrap(), b"/");
    assert_eq!(resolve(&mut fake, b"/../../a").unwrap(), b"/a");
    assert_eq!(resolve(&mut fake, b"/a/b/").unwrap(), b"/a/b");
    assert_eq!(resolve(&mut fake, b"/a//b///").unwrap(), b"/a/b");
    assert_eq!(resolve(&mut fake, b"/").unwrap(), b"/");
}

#[test]
fn symlinks_are_resolved() {
    let mut fake = Fake::new();
    assert_eq!(resolve(&mut fake, b"/link").unwrap(), b"/a/b");
    assert_eq!(resolve(&mut fake, b"/link/c").unwrap(), b"/a/b/c");
    assert_eq!(resolve(&mut fake, b"/a/rel").unwrap(), b"/a/b/c");
}

#[test]
fn a_loop_is_eloop_after_forty_links() {
    let mut fake = Fake::new();
    assert_eq!(
        resolve(&mut fake, b"/loop"),
        Err(raven_libc_abi::errno::ELOOP as u16)
    );
    let mut fake = Fake::new();
    for index in 0..40 {
        let path = format!("/l{index}");
        let next = format!("/l{}", index + 1);
        fake.add_link(path.as_bytes(), next.as_bytes());
    }
    fake.kinds.insert(b"/l40".to_vec(), FileKind::Dir);
    assert_eq!(resolve(&mut fake, b"/l0").unwrap(), b"/l40");
    fake.add_link(b"/l39", b"/l39");
    assert_eq!(
        resolve(&mut fake, b"/l0"),
        Err(raven_libc_abi::errno::ELOOP as u16)
    );
}

#[test]
fn errors() {
    let mut fake = Fake::new();
    assert_eq!(
        resolve(&mut fake, b""),
        Err(raven_libc_abi::errno::ENOENT as u16)
    );
    assert_eq!(
        resolve(&mut fake, b"/missing"),
        Err(raven_libc_abi::errno::ENOENT as u16)
    );
    assert_eq!(
        resolve(&mut fake, b"/a/file/x"),
        Err(raven_libc_abi::errno::ENOTDIR as u16)
    );
    let mut short = [0u8; 16];
    assert_eq!(
        realpath(&mut fake, b"/a", &mut short),
        Err(Errno(raven_libc_abi::errno::EINVAL as u16))
    );
}

#[test]
fn getcwd_failure_is_reported() {
    struct NoCwd;
    impl Walker for NoCwd {
        fn lstat_kind(&mut self, _: &[u8]) -> Result<FileKind, Errno> {
            Err(Errno(raven_libc_abi::errno::ENOENT as u16))
        }
        fn readlink(&mut self, _: &[u8], _: &mut [u8]) -> Result<usize, Errno> {
            Err(Errno(raven_libc_abi::errno::ENOENT as u16))
        }
        fn getcwd(&mut self, buf: &mut [u8]) -> Result<usize, Errno> {
            buf[0] = b'.';
            Err(Errno(raven_libc_abi::errno::ENOENT as u16))
        }
    }
    let mut out = vec![0u8; raven_libc_abi::limits::PATH_MAX as usize];
    assert_eq!(
        realpath(&mut NoCwd, b"x", &mut out),
        Err(Errno(raven_libc_abi::errno::ENOENT as u16))
    );
}
