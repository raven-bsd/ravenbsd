// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host filesystem traversal checks for `find`.

use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::PathBuf;

use raven_find::{Kind, Options, Tree, walk};
use raven_proto_fs::{S_IFDIR, S_IFLNK, S_IFREG};
use raven_tool_core::Errno;

struct HostTree;

impl Tree for HostTree {
    fn kind(&mut self, path: &str) -> Result<Kind, Errno> {
        let metadata = fs::symlink_metadata(path).map_err(|_| Errno::ENOENT)?;
        let mode = metadata.permissions().mode();
        Ok(match mode & 0o170000 {
            S_IFREG => Kind::File,
            S_IFDIR => Kind::Directory,
            S_IFLNK => Kind::Symlink,
            _ => Kind::Other,
        })
    }

    fn mode(&mut self, path: &str) -> Result<u32, Errno> {
        fs::metadata(path)
            .map(|metadata| metadata.permissions().mode())
            .map_err(|_| Errno::ENOENT)
    }

    fn children(&mut self, path: &str) -> Result<Vec<String>, Errno> {
        let mut names = Vec::new();
        for entry in fs::read_dir(path).map_err(|_| Errno::ENOENT)? {
            let entry = entry.map_err(|_| Errno::EIO)?;
            names.push(entry.file_name().to_string_lossy().to_string());
        }
        names.sort();
        Ok(names)
    }
}

fn temp(label: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("raven-find-{label}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    path
}

fn tree(root: &PathBuf) {
    fs::write(root.join("a.txt"), b"a\n").unwrap();
    fs::write(root.join("b.log"), b"b\n").unwrap();
    fs::set_permissions(root.join("a.txt"), fs::Permissions::from_mode(0o644)).unwrap();
    fs::create_dir(root.join("dir")).unwrap();
    fs::write(root.join("dir/c.txt"), b"c\n").unwrap();
    symlink(root.join("dir"), root.join("link")).unwrap();
    symlink(root, root.join("loop")).unwrap();
}

#[test]
fn walks_a_real_tree_and_filters() {
    let root = temp("walk");
    tree(&root);
    let start = root.display().to_string();

    let mut all = Vec::new();
    let options = Options {
        paths: vec![start.as_str()],
        ..Options::default()
    };
    walk(&mut HostTree, &options, &mut all).unwrap();
    assert!(all.iter().any(|path| path.ends_with("a.txt")));
    assert!(all.iter().any(|path| path.ends_with("dir/c.txt")));
    assert!(all.iter().any(|path| path.ends_with("link")));
    assert!(!all.iter().any(|path| path.ends_with("link/c.txt")));
    assert!(!all.iter().any(|path| path.ends_with("loop/raven-find")));

    let mut named = Vec::new();
    let options = Options {
        paths: vec![start.as_str()],
        name: Some("*.txt"),
        ..Options::default()
    };
    walk(&mut HostTree, &options, &mut named).unwrap();
    assert!(named.iter().any(|path| path.ends_with("a.txt")));
    assert!(named.iter().any(|path| path.ends_with("dir/c.txt")));
    assert!(!named.iter().any(|path| path.ends_with("b.log")));

    let mut files = Vec::new();
    let options = Options {
        paths: vec![start.as_str()],
        kind: Some(Kind::File),
        perm: Some(0o644),
        ..Options::default()
    };
    walk(&mut HostTree, &options, &mut files).unwrap();
    assert!(files.iter().any(|path| path.ends_with("a.txt")));
    assert!(!files.iter().any(|path| path.ends_with("link")));

    let _ = fs::remove_dir_all(&root);
}
