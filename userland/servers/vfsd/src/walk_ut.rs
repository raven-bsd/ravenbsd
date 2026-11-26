// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the rooted traversal walker.

use super::*;
use std::collections::BTreeMap;

#[derive(Default)]
struct Fake {
    entries: BTreeMap<Vec<u8>, Kind>,
    links: BTreeMap<Vec<u8>, Vec<u8>>,
    max_path: usize,
}

impl Fake {
    fn new() -> Self {
        Self {
            max_path: 4095,
            ..Self::default()
        }
    }

    fn dir(&mut self, path: &[u8]) -> &mut Self {
        self.entries.insert(path.to_vec(), Kind::Directory);
        self
    }

    fn file(&mut self, path: &[u8]) -> &mut Self {
        self.entries.insert(path.to_vec(), Kind::Other);
        self
    }

    fn link(&mut self, path: &[u8], target: &[u8]) -> &mut Self {
        self.entries.insert(path.to_vec(), Kind::Symlink);
        self.links.insert(path.to_vec(), target.to_vec());
        self
    }

    fn walk(&mut self, root: &[u8], base: &[u8], path: &[u8]) -> Result<Vec<u8>, Errno> {
        let max_path = self.max_path;
        resolve(self, root, base, path, true, max_path)
    }
}

impl Namespace for Fake {
    fn lstat(&mut self, path: &[u8]) -> Result<Kind, Errno> {
        self.entries.get(path).copied().ok_or(Errno::ENOENT)
    }

    fn readlink(&mut self, path: &[u8]) -> Result<Vec<u8>, Errno> {
        self.links.get(path).cloned().ok_or(Errno::EINVAL)
    }
}

#[test]
fn root_slash_still_walks_and_follows_a_symlink() {
    let mut fs = Fake::new();
    fs.dir(b"/")
        .dir(b"/target")
        .dir(b"/target/dir")
        .link(b"/link", b"target/dir");
    assert_eq!(fs.walk(b"/", b"/", b"/link").unwrap(), b"/target/dir");
}

#[test]
fn suffix_after_a_symlink_stays_pending() {
    let mut fs = Fake::new();
    fs.dir(b"/").dir(b"/a").dir(b"/a/b").link(b"/a/l", b"b");
    assert_eq!(fs.walk(b"/", b"/", b"/a/l/c").unwrap(), b"/a/b/c");
}

#[test]
fn relative_link_resolves_against_its_containing_directory() {
    let mut fs = Fake::new();
    fs.dir(b"/").dir(b"/a").file(b"/b").link(b"/a/l", b"../b");
    assert_eq!(fs.walk(b"/", b"/", b"/a/l").unwrap(), b"/b");
}

#[test]
fn absolute_link_resolves_against_the_process_root() {
    let mut fs = Fake::new();
    fs.dir(b"/")
        .dir(b"/chroot")
        .dir(b"/chroot/etc")
        .file(b"/chroot/x");
    fs.link(b"/chroot/etc/l", b"/x");
    assert_eq!(
        fs.walk(b"/chroot", b"/chroot", b"/etc/l").unwrap(),
        b"/chroot/x"
    );
}

#[test]
fn dotdot_never_climbs_above_the_process_root() {
    let mut fs = Fake::new();
    fs.dir(b"/").dir(b"/r").file(b"/r/x");
    assert_eq!(fs.walk(b"/r", b"/r", b"/../x").unwrap(), b"/r/x");
    assert_eq!(fs.walk(b"/r", b"/r", b"../../x").unwrap(), b"/r/x");
}

#[test]
fn dotdot_after_a_symlink_pops_the_link_target() {
    let mut fs = Fake::new();
    fs.dir(b"/")
        .dir(b"/a")
        .dir(b"/a/b")
        .dir(b"/a/b/c")
        .link(b"/a/l", b"b/c");
    assert_eq!(fs.walk(b"/", b"/", b"/a/l/..").unwrap(), b"/a/b");
}

#[test]
fn a_chrooted_open_path_keeps_the_process_root() {
    let mut fs = Fake::new();
    fs.dir(b"/")
        .dir(b"/chroot")
        .dir(b"/chroot/etc")
        .file(b"/chroot/x");
    fs.link(b"/chroot/etc/l", b"/x");
    let rest = crate::route::relative_to_root("/chroot", "/chroot/etc/l").expect("prefix");
    assert_eq!(
        fs.walk(b"/chroot", b"/chroot", rest.as_bytes()).unwrap(),
        b"/chroot/x"
    );
}

#[test]
fn relative_path_starts_at_the_base() {
    let mut fs = Fake::new();
    fs.dir(b"/").dir(b"/a").dir(b"/a/b").file(b"/a/b/f");
    assert_eq!(fs.walk(b"/", b"/a/b", b"f").unwrap(), b"/a/b/f");
    assert_eq!(fs.walk(b"/", b"/a/b", b"../f").unwrap(), b"/a/f".to_vec());
}

#[test]
fn missing_final_component_is_returned_for_creation() {
    let mut fs = Fake::new();
    fs.dir(b"/").dir(b"/a");
    assert_eq!(fs.walk(b"/", b"/", b"/a/new").unwrap(), b"/a/new");
}

#[test]
fn missing_intermediate_component_is_enoent() {
    let mut fs = Fake::new();
    fs.dir(b"/").dir(b"/a");
    assert_eq!(fs.walk(b"/", b"/", b"/a/b/c"), Err(Errno::ENOENT));
}

#[test]
fn a_file_cannot_have_children() {
    let mut fs = Fake::new();
    fs.dir(b"/").file(b"/a");
    assert_eq!(fs.walk(b"/", b"/", b"/a/b"), Err(Errno::ENOTDIR));
}

#[test]
fn the_final_symlink_is_not_followed_when_asked() {
    let mut fs = Fake::new();
    fs.dir(b"/").dir(b"/a").link(b"/a/l", b"../b");
    let result = resolve(&mut fs, b"/", b"/", b"/a/l", false, 4095);
    assert_eq!(result, Ok(b"/a/l".to_vec()));
}

#[test]
fn symlink_depth_is_bounded() {
    let mut fs = Fake::new();
    fs.dir(b"/").link(b"/l", b"/l");
    assert_eq!(fs.walk(b"/", b"/", b"/l"), Err(Errno::ELOOP));
}

#[test]
fn non_utf8_components_are_preserved() {
    let mut fs = Fake::new();
    fs.dir(b"/").dir(b"/a").file(b"/a/\xff\xfe");
    assert_eq!(
        fs.walk(b"/", b"/", b"/a/\xff\xfe"),
        Ok(b"/a/\xff\xfe".to_vec())
    );
}

#[test]
fn resolved_path_over_the_limit_is_rejected() {
    let mut fs = Fake::new();
    fs.max_path = 8;
    fs.dir(b"/").link(b"/l", b"/abcdefghij");
    assert_eq!(fs.walk(b"/", b"/", b"/l"), Err(Errno::ENAMETOOLONG));
}

#[test]
fn base_outside_the_root_is_rejected() {
    let mut fs = Fake::new();
    fs.dir(b"/").dir(b"/r").dir(b"/other");
    assert_eq!(fs.walk(b"/r", b"/other", b"x"), Err(Errno::ENOENT));
}
