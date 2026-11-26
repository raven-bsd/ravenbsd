// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use alloc::collections::{BTreeMap, BTreeSet};

use super::*;
use raven_libc_abi::stat::S_IFREG;

struct Fake {
    dirs: BTreeSet<Vec<u8>>,
    files: BTreeSet<Vec<u8>>,
    links: BTreeMap<Vec<u8>, Vec<u8>>,
    cwd: Vec<u8>,
}

impl Fake {
    fn new() -> Self {
        let mut dirs = BTreeSet::new();
        for path in [
            b"/".as_slice(),
            b"/a",
            b"/a/b",
            b"/home",
            b"/home/user",
            b"/b",
        ] {
            dirs.insert(path.to_vec());
        }
        let mut files = BTreeSet::new();
        for path in [b"/a/file".as_slice(), b"/home/user/file", b"/b/file"] {
            files.insert(path.to_vec());
        }
        let mut links = BTreeMap::new();
        links.insert(b"/a/link".to_vec(), b"../b".to_vec());
        links.insert(b"/a/abs".to_vec(), b"/home/user".to_vec());
        links.insert(b"/home/user/self".to_vec(), b"self".to_vec());
        links.insert(b"/a/broken".to_vec(), b"missing".to_vec());
        Self {
            dirs,
            files,
            links,
            cwd: b"/home/user".to_vec(),
        }
    }
}

impl PathOps for Fake {
    fn read_link(&mut self, path: &[u8], out: &mut [u8]) -> Result<usize, Errno> {
        if let Some(target) = self.links.get(path) {
            out.get_mut(..target.len())
                .ok_or(Errno::ENAMETOOLONG)?
                .copy_from_slice(target);
            return Ok(target.len());
        }
        if self.dirs.contains(path) || self.files.contains(path) {
            return Err(Errno::EINVAL);
        }
        Err(Errno::ENOENT)
    }

    fn stat(&mut self, path: &[u8]) -> Result<Stat, Errno> {
        if self.dirs.contains(path) || self.files.contains(path) {
            let mut stat = Stat::ZERO;
            stat.st_mode = if self.dirs.contains(path) {
                S_IFDIR as u16
            } else {
                S_IFREG as u16
            };
            return Ok(stat);
        }
        Err(Errno::ENOENT)
    }

    fn getcwd(&mut self) -> Result<Vec<u8>, Errno> {
        Ok(self.cwd.clone())
    }
}

fn resolve(input: &[u8]) -> Result<Vec<u8>, Errno> {
    canonicalize(&mut Fake::new(), input)
}

#[test]
fn an_absolute_path_drops_dot_and_dotdot() {
    assert_eq!(resolve(b"/a/./b/../file").unwrap(), b"/a/file");
}

#[test]
fn a_relative_path_starts_at_the_cwd() {
    assert_eq!(resolve(b"file").unwrap(), b"/home/user/file");
}

#[test]
fn a_relative_symlink_is_followed() {
    assert_eq!(resolve(b"/a/link/file").unwrap(), b"/b/file");
}

#[test]
fn an_absolute_symlink_is_followed() {
    assert_eq!(resolve(b"/a/abs/file").unwrap(), b"/home/user/file");
}

#[test]
fn root_stays_root() {
    assert_eq!(resolve(b"/").unwrap(), b"/");
    assert_eq!(resolve(b"/..").unwrap(), b"/");
}

#[test]
fn dotdot_at_root_is_ignored() {
    assert_eq!(resolve(b"/a/../..").unwrap(), b"/");
}

#[test]
fn a_missing_component_is_enoent() {
    assert_eq!(resolve(b"/missing"), Err(Errno::ENOENT));
}

#[test]
fn a_broken_symlink_is_enoent() {
    assert_eq!(resolve(b"/a/broken"), Err(Errno::ENOENT));
}

#[test]
fn a_symlink_loop_is_eloop() {
    assert_eq!(resolve(b"/home/user/self"), Err(Errno::ELOOP));
}

#[test]
fn a_trailing_slash_on_a_file_is_enotdir() {
    assert_eq!(resolve(b"/a/file/"), Err(Errno::ENOTDIR));
}

#[test]
fn a_trailing_slash_on_a_directory_is_kept() {
    assert_eq!(resolve(b"/a/b/").unwrap(), b"/a/b");
}

#[test]
fn an_empty_input_is_enoent() {
    assert_eq!(resolve(b""), Err(Errno::ENOENT));
}
