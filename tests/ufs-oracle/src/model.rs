// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Independent in-memory tree model.
//!
//! This is deliberately not shared with `ufs-core`: it stores a path table and
//! an inode table, derives link counts from the path references and updates
//! file contents only through explicit model operations.

use std::collections::BTreeMap;

use ufs_core::Ino;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Dir,
    File,
    Symlink,
}

#[derive(Clone, Debug)]
pub struct InodeModel {
    pub kind: Kind,
    pub mode: u16,
    pub uid: u32,
    pub gid: u32,
    pub data: Vec<u8>,
    pub target: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct Model {
    root: Ino,
    pub inodes: BTreeMap<Ino, InodeModel>,
    pub paths: BTreeMap<Vec<u8>, Ino>,
}

/// Join a relative path with a name; the model root is the empty path.
pub fn join(parent: &[u8], name: &[u8]) -> Vec<u8> {
    let mut path = Vec::from(parent);
    if !path.is_empty() {
        path.push(b'/');
    }
    path.extend_from_slice(name);
    path
}

/// Split a relative path into parent and final name.
pub fn split(path: &[u8]) -> (&[u8], &[u8]) {
    match path.iter().rposition(|byte| *byte == b'/') {
        Some(index) => (&path[..index], &path[index + 1..]),
        None => (&[], path),
    }
}

/// FNV-1a 64 used as the model/actual content digest.
pub fn digest(data: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in data {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x1000_0000_01b3);
    }
    hash
}

impl Model {
    pub fn new(root: Ino) -> Self {
        let mut model = Self {
            root,
            inodes: BTreeMap::new(),
            paths: BTreeMap::new(),
        };
        model.inodes.insert(
            root,
            InodeModel {
                kind: Kind::Dir,
                mode: 0o755,
                uid: 0,
                gid: 0,
                data: Vec::new(),
                target: Vec::new(),
            },
        );
        model.paths.insert(Vec::new(), root);
        model
    }

    pub fn root(&self) -> Ino {
        self.root
    }

    pub fn insert(&mut self, path: Vec<u8>, ino: Ino, inode: InodeModel) {
        self.paths.insert(path, ino);
        self.inodes.entry(ino).or_insert(inode);
    }

    pub fn inode(&self, ino: Ino) -> Option<&InodeModel> {
        self.inodes.get(&ino)
    }

    pub fn inode_mut(&mut self, ino: Ino) -> Option<&mut InodeModel> {
        self.inodes.get_mut(&ino)
    }

    pub fn ino(&self, path: &[u8]) -> Option<Ino> {
        self.paths.get(path).copied()
    }

    pub fn node(&self, path: &[u8]) -> Option<&InodeModel> {
        self.ino(path).and_then(|ino| self.inode(ino))
    }

    pub fn node_mut(&mut self, path: &[u8]) -> Option<&mut InodeModel> {
        let ino = self.ino(path)?;
        self.inode_mut(ino)
    }

    pub fn refs(&self, ino: Ino) -> usize {
        self.paths.values().filter(|value| **value == ino).count()
    }

    pub fn children(&self, dir: &[u8]) -> Vec<Vec<u8>> {
        self.paths
            .keys()
            .filter(|path| !path.is_empty() && split(path).0 == dir)
            .cloned()
            .collect()
    }

    pub fn child_dirs(&self, dir: &[u8]) -> usize {
        self.paths
            .iter()
            .filter(|(path, ino)| {
                !path.is_empty()
                    && split(path).0 == dir
                    && self
                        .inodes
                        .get(ino)
                        .is_some_and(|node| node.kind == Kind::Dir)
            })
            .count()
    }

    pub fn dirs(&self) -> Vec<Vec<u8>> {
        self.paths
            .iter()
            .filter(|(_, ino)| {
                self.inodes
                    .get(ino)
                    .is_some_and(|node| node.kind == Kind::Dir)
            })
            .map(|(path, _)| path.clone())
            .collect()
    }

    pub fn files(&self) -> Vec<Vec<u8>> {
        self.paths
            .iter()
            .filter(|(_, ino)| {
                self.inodes
                    .get(ino)
                    .is_some_and(|node| node.kind == Kind::File)
            })
            .map(|(path, _)| path.clone())
            .collect()
    }

    pub fn non_dirs(&self) -> Vec<Vec<u8>> {
        self.paths
            .iter()
            .filter(|(_, ino)| {
                self.inodes
                    .get(ino)
                    .is_some_and(|node| node.kind != Kind::Dir)
            })
            .map(|(path, _)| path.clone())
            .collect()
    }

    pub fn nlink(&self, ino: Ino) -> Option<u32> {
        let node = self.inodes.get(&ino)?;
        if node.kind == Kind::Dir {
            let path = self.paths.iter().find(|(_, value)| **value == ino)?;
            return Some(2 + self.child_dirs(path.0) as u32);
        }
        Some(self.refs(ino) as u32)
    }

    /// Remove one path and the inode when no other reference remains.
    pub fn remove_path(&mut self, path: &[u8]) -> Option<Ino> {
        let ino = self.paths.remove(path)?;
        if self.refs(ino) == 0 {
            self.inodes.remove(&ino);
        }
        Some(ino)
    }

    /// Move `source` and, for a directory, every descendant to `destination`.
    pub fn move_subtree(&mut self, source: &[u8], destination: &[u8]) {
        let mut prefix = Vec::from(source);
        prefix.push(b'/');
        let moves: Vec<(Vec<u8>, Ino)> = self
            .paths
            .iter()
            .filter(|(path, _)| path.as_slice() == source || path.starts_with(&prefix))
            .map(|(path, ino)| (path.clone(), *ino))
            .collect();
        for (path, _) in &moves {
            self.paths.remove(path);
        }
        let mut new_prefix = Vec::from(destination);
        new_prefix.push(b'/');
        for (path, ino) in moves {
            let moved = if path.as_slice() == source {
                Vec::from(destination)
            } else {
                let suffix = &path[source.len() + 1..];
                let mut moved = new_prefix.clone();
                moved.extend_from_slice(suffix);
                moved
            };
            self.paths.insert(moved, ino);
        }
    }
}
