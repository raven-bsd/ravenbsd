// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Shared test helpers: block devices, manifest parsing, tree walking and SHA-256.
#![allow(dead_code)] // the module is shared by several test binaries; not every helper is used in each

pub mod sha256;

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::string::String;
use std::vec::Vec;

use flate2::read::GzDecoder;
use ufs_core::{BlockIo, Ino, IoError, MountOptions, Ufs, UfsError};

/// A `Vec<u8>`-backed block device.
pub struct VecDisk {
    pub data: Vec<u8>,
}

impl VecDisk {
    pub fn new(data: Vec<u8>) -> Self {
        Self { data }
    }
}

impl BlockIo for VecDisk {
    fn size_bytes(&self) -> u64 {
        self.data.len() as u64
    }

    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), IoError> {
        let start = usize::try_from(offset).map_err(|_| IoError)?;
        let end = start.checked_add(buf.len()).ok_or(IoError)?;
        let src = self.data.get(start..end).ok_or(IoError)?;
        buf.copy_from_slice(src);
        Ok(())
    }

    fn write_at(&mut self, offset: u64, buf: &[u8]) -> Result<(), IoError> {
        let start = usize::try_from(offset).map_err(|_| IoError)?;
        let end = start.checked_add(buf.len()).ok_or(IoError)?;
        let dst = self.data.get_mut(start..end).ok_or(IoError)?;
        dst.copy_from_slice(buf);
        Ok(())
    }

    fn flush(&mut self) -> Result<(), IoError> {
        Ok(())
    }
}

/// A block device whose bytes can be mutated while a mount attempt owns a clone.
#[derive(Clone)]
pub struct SharedDisk {
    pub data: Rc<RefCell<Vec<u8>>>,
    pub bad_reads: Rc<Cell<u64>>,
}

impl SharedDisk {
    pub fn new(data: Vec<u8>) -> Self {
        Self {
            data: Rc::new(RefCell::new(data)),
            bad_reads: Rc::new(Cell::new(0)),
        }
    }

    pub fn bad_reads(&self) -> u64 {
        self.bad_reads.get()
    }
}

impl BlockIo for SharedDisk {
    fn size_bytes(&self) -> u64 {
        self.data.borrow().len() as u64
    }

    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), IoError> {
        let data = self.data.borrow();
        let start = usize::try_from(offset).map_err(|_| IoError)?;
        let end = start.checked_add(buf.len()).ok_or(IoError)?;
        let Some(src) = data.get(start..end) else {
            self.bad_reads.set(self.bad_reads.get() + 1);
            return Err(IoError);
        };
        buf.copy_from_slice(src);
        Ok(())
    }

    fn write_at(&mut self, offset: u64, buf: &[u8]) -> Result<(), IoError> {
        let mut data = self.data.borrow_mut();
        let start = usize::try_from(offset).map_err(|_| IoError)?;
        let end = start.checked_add(buf.len()).ok_or(IoError)?;
        let dst = data.get_mut(start..end).ok_or(IoError)?;
        dst.copy_from_slice(buf);
        Ok(())
    }

    fn flush(&mut self) -> Result<(), IoError> {
        Ok(())
    }
}

/// One manifest line.
#[derive(Clone, Debug)]
pub struct ManifestEntry {
    pub path: String,
    pub kind: String,
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub size: Option<u64>,
    pub sha256: Option<String>,
    pub link: Option<String>,
    pub nlink: u64,
}

/// One path's metadata collected from the mounted image.
#[derive(Clone, Debug)]
pub struct Meta {
    pub kind: String,
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub size: Option<u64>,
    pub sha256: Option<String>,
    pub link: Option<String>,
    pub nlink: u64,
}

pub fn vectors_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/ufs-vectors")
}

pub fn load_gz(path: &Path) -> Vec<u8> {
    let file = fs::File::open(path).expect("open gzip vector");
    let mut decoder = GzDecoder::new(file);
    let mut data = Vec::new();
    decoder.read_to_end(&mut data).expect("decompress vector");
    data
}

pub fn load_manifest(path: &Path) -> Vec<ManifestEntry> {
    let text = fs::read_to_string(path).expect("read manifest");
    let mut out = Vec::new();
    for line in text.lines() {
        if line.is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split('|').collect();
        assert_eq!(fields.len(), 9, "manifest line: {line}");
        out.push(ManifestEntry {
            path: fields[0].to_string(),
            kind: fields[1].to_string(),
            mode: u32::from_str_radix(fields[2], 8).expect("manifest mode"),
            uid: fields[3].parse().expect("manifest uid"),
            gid: fields[4].parse().expect("manifest gid"),
            size: parse_optional(fields[5]),
            sha256: parse_optional_string(fields[6]),
            link: parse_optional_string(fields[7]),
            nlink: fields[8].parse().expect("manifest nlink"),
        });
    }
    out
}

fn parse_optional(value: &str) -> Option<u64> {
    if value == "-" {
        None
    } else {
        Some(value.parse().expect("manifest size"))
    }
}

fn parse_optional_string(value: &str) -> Option<String> {
    if value == "-" {
        None
    } else {
        Some(value.to_string())
    }
}

pub fn read_all<D: BlockIo>(ufs: &mut Ufs<D>, ino: Ino, size: u64) -> Result<Vec<u8>, UfsError> {
    let len = usize::try_from(size).map_err(|_| UfsError::Invalid)?;
    let mut data = vec![0u8; len];
    let mut done = 0usize;
    while done < len {
        let n = ufs.read(ino, done as u64, &mut data[done..])?;
        if n == 0 {
            return Err(UfsError::Corrupt("short read"));
        }
        done += n;
    }
    Ok(data)
}

/// Walk the tree from `ino`, recording every path (`.`/`..` excluded).
pub fn walk<D: BlockIo>(
    ufs: &mut Ufs<D>,
    ino: Ino,
    path: &str,
    out: &mut BTreeMap<String, Meta>,
) -> Result<(), UfsError> {
    let attr = ufs.getattr(ino)?;
    let kind = kind_name(attr.kind).to_string();
    let (size, sha256, link) = match attr.kind {
        ufs_core::FileType::Regular => {
            let data = read_all(ufs, ino, attr.size)?;
            (Some(attr.size), Some(hex(&sha256::digest(&data))), None)
        }
        ufs_core::FileType::Directory => (None, None, None),
        ufs_core::FileType::Symlink => {
            let mut buf = vec![0u8; usize::try_from(attr.size).unwrap_or(0)];
            let n = ufs.readlink(ino, &mut buf)?;
            buf.truncate(n);
            (
                Some(attr.size),
                None,
                Some(String::from_utf8_lossy(&buf).into_owned()),
            )
        }
        _ => (Some(0), None, None),
    };
    out.insert(
        path.to_string(),
        Meta {
            kind,
            mode: u32::from(attr.mode),
            uid: attr.uid,
            gid: attr.gid,
            size,
            sha256,
            link,
            nlink: u64::from(attr.nlink),
        },
    );

    if attr.kind == ufs_core::FileType::Directory {
        let mut children: Vec<(Ino, String)> = Vec::new();
        ufs.readdir(ino, 0, &mut |entry, _cookie| {
            if entry.name != b"." && entry.name != b".." {
                let name = String::from_utf8(entry.name.clone()).expect("UTF-8 test name");
                children.push((entry.ino, name));
            }
            true
        })?;
        for (child, name) in children {
            let child_path = if path == "/" {
                format!("/{name}")
            } else {
                format!("{path}/{name}")
            };
            walk(ufs, child, &child_path, out)?;
        }
    }
    Ok(())
}

pub fn check_manifest(got: &BTreeMap<String, Meta>, want: &[ManifestEntry]) -> Result<(), String> {
    if got.len() != want.len() {
        return Err(format!(
            "path count: image {} manifest {}",
            got.len(),
            want.len()
        ));
    }
    for entry in want {
        let meta = got
            .get(&entry.path)
            .ok_or_else(|| format!("missing path {}", entry.path))?;
        if meta.kind != entry.kind {
            return Err(format!(
                "{}: type {} != {}",
                entry.path, meta.kind, entry.kind
            ));
        }
        if meta.mode != entry.mode {
            return Err(format!(
                "{}: mode {:04o} != {:04o}",
                entry.path, meta.mode, entry.mode
            ));
        }
        if meta.uid != entry.uid || meta.gid != entry.gid {
            return Err(format!("{}: uid/gid", entry.path));
        }
        if meta.size != entry.size {
            return Err(format!(
                "{}: size {:?} != {:?}",
                entry.path, meta.size, entry.size
            ));
        }
        if meta.sha256 != entry.sha256 {
            return Err(format!("{}: sha256", entry.path));
        }
        if meta.link != entry.link {
            return Err(format!("{}: link target", entry.path));
        }
        if meta.nlink != entry.nlink {
            return Err(format!(
                "{}: nlink {} != {}",
                entry.path, meta.nlink, entry.nlink
            ));
        }
    }
    Ok(())
}

pub fn kind_name(kind: ufs_core::FileType) -> &'static str {
    match kind {
        ufs_core::FileType::Regular => "file",
        ufs_core::FileType::Directory => "dir",
        ufs_core::FileType::Symlink => "symlink",
        ufs_core::FileType::CharDevice => "char",
        ufs_core::FileType::BlockDevice => "block",
        ufs_core::FileType::Fifo => "fifo",
        ufs_core::FileType::Socket => "socket",
    }
}

pub fn hex(bytes: &[u8; 32]) -> String {
    let mut out = String::with_capacity(64);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

pub fn mount_image(name: &str) -> Ufs<VecDisk> {
    let path = vectors_dir().join("golden").join(format!("{name}.img.gz"));
    let data = load_gz(&path);
    Ufs::mount(VecDisk::new(data), MountOptions { read_only: true }).expect("mount golden image")
}
