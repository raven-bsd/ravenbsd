// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Whole-tree comparison between the independent model and a mounted image.

use std::collections::BTreeMap;

use ufs_core::{FileType, Ino, Ufs};

use crate::image::VecDisk;
use crate::model::{Kind, Model, digest, join};

struct Actual {
    ino: Ino,
    kind: FileType,
    mode: u16,
    uid: u32,
    gid: u32,
    size: u64,
    digest: u64,
    target: Vec<u8>,
    nlink: u32,
}

fn walk(
    ufs: &mut Ufs<VecDisk>,
    dir: Ino,
    path: &[u8],
    out: &mut BTreeMap<Vec<u8>, Actual>,
) -> Result<(), String> {
    let mut children = Vec::new();
    ufs.readdir(dir, 0, &mut |entry, _| {
        if entry.name != b"." && entry.name != b".." {
            children.push((entry.name.clone(), entry.ino));
        }
        true
    })
    .map_err(|error| format!("readdir {}: {error}", String::from_utf8_lossy(path)))?;
    for (name, ino) in children {
        let child = join(path, &name);
        let attr = ufs
            .getattr(ino)
            .map_err(|error| format!("getattr {}: {error}", String::from_utf8_lossy(&child)))?;
        let mut content = Vec::new();
        let mut target = Vec::new();
        match attr.kind {
            FileType::Regular => {
                let size = usize::try_from(attr.size)
                    .map_err(|_| format!("file too large: {}", String::from_utf8_lossy(&child)))?;
                content = vec![0u8; size];
                let got = ufs.read(ino, 0, &mut content).map_err(|error| {
                    format!("read {}: {error}", String::from_utf8_lossy(&child))
                })?;
                if got != size {
                    return Err(format!(
                        "short read {}: {got} != {size}",
                        String::from_utf8_lossy(&child)
                    ));
                }
            }
            FileType::Symlink => {
                let size = usize::try_from(attr.size).map_err(|_| {
                    format!("symlink too large: {}", String::from_utf8_lossy(&child))
                })?;
                target = vec![0u8; size];
                let got = ufs.readlink(ino, &mut target).map_err(|error| {
                    format!("readlink {}: {error}", String::from_utf8_lossy(&child))
                })?;
                if got != size {
                    return Err(format!(
                        "short readlink {}: {got} != {size}",
                        String::from_utf8_lossy(&child)
                    ));
                }
            }
            FileType::Directory => {}
            other => {
                return Err(format!(
                    "unexpected {other:?} at {}",
                    String::from_utf8_lossy(&child)
                ));
            }
        }
        out.insert(
            child.clone(),
            Actual {
                ino,
                kind: attr.kind,
                mode: attr.mode,
                uid: attr.uid,
                gid: attr.gid,
                size: attr.size,
                digest: digest(&content),
                target,
                nlink: attr.nlink,
            },
        );
        if attr.kind == FileType::Directory {
            walk(ufs, ino, &child, out)?;
        }
    }
    Ok(())
}

/// Compare every path, type, mode, owner, file content, symlink target and link count.
///
/// Directory inode sizes are validated as a multiple of 512 bytes but are not
/// predicted by the model (directory packing is not part of the namespace model).
pub fn compare(ufs: &mut Ufs<VecDisk>, model: &Model) -> Result<(), String> {
    let mut actual = BTreeMap::new();
    let root = model.root();
    let attr = ufs
        .getattr(root)
        .map_err(|error| format!("getattr model root: {error}"))?;
    actual.insert(
        Vec::new(),
        Actual {
            ino: root,
            kind: attr.kind,
            mode: attr.mode,
            uid: attr.uid,
            gid: attr.gid,
            size: attr.size,
            digest: digest(&[]),
            target: Vec::new(),
            nlink: attr.nlink,
        },
    );
    walk(ufs, root, b"", &mut actual)?;

    if actual.len() != model.paths.len() {
        return Err(format!(
            "tree size: actual {} model {}",
            actual.len(),
            model.paths.len()
        ));
    }
    for (path, ino) in &model.paths {
        let node = model
            .inode(*ino)
            .ok_or_else(|| format!("model inode {ino} missing"))?;
        let got = actual
            .get(path)
            .ok_or_else(|| format!("missing actual path {}", String::from_utf8_lossy(path)))?;
        let label = String::from_utf8_lossy(path);
        if got.ino != *ino {
            return Err(format!("{label}: inode {} != {ino}", got.ino));
        }
        let want_kind = match node.kind {
            Kind::Dir => FileType::Directory,
            Kind::File => FileType::Regular,
            Kind::Symlink => FileType::Symlink,
        };
        if got.kind != want_kind {
            return Err(format!("{label}: type {:?} != {want_kind:?}", got.kind));
        }
        if got.mode != node.mode {
            return Err(format!("{label}: mode {:o} != {:o}", got.mode, node.mode));
        }
        if got.uid != node.uid || got.gid != node.gid {
            return Err(format!(
                "{label}: owner {}:{} != {}:{}",
                got.uid, got.gid, node.uid, node.gid
            ));
        }
        let want_size = match node.kind {
            Kind::File => node.data.len() as u64,
            Kind::Symlink => node.target.len() as u64,
            Kind::Dir => got.size,
        };
        if got.size != want_size {
            return Err(format!("{label}: size {} != {want_size}", got.size));
        }
        if node.kind == Kind::Dir && (got.size < 512 || got.size % 512 != 0) {
            return Err(format!("{label}: invalid directory size {}", got.size));
        }
        if node.kind == Kind::File && got.digest != digest(&node.data) {
            return Err(format!("{label}: content digest differs"));
        }
        if node.kind == Kind::Symlink && got.target != node.target {
            return Err(format!("{label}: symlink target differs"));
        }
        let want_links = model
            .nlink(*ino)
            .ok_or_else(|| format!("model link count for {ino}"))?;
        if got.nlink != want_links {
            return Err(format!("{label}: nlink {} != {want_links}", got.nlink));
        }
    }
    Ok(())
}
