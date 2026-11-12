// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Stable replay harness shared by the UFS fuzz targets and the host tests.
//!
//! A fuzz input is the bytes of one on-disk region (superblock, cylinder
//! group, inode, directory block or indirect block).  Replay overlays it on the
//! committed B63 `ufs2-b4096` golden image, mounts the result read-only, walks
//! the parser entry points and requires no panic, no out-of-range read and a
//! returned value.

#![forbid(unsafe_code)]

use std::cell::{Cell, RefCell};
use std::fs;
use std::io::Read;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::OnceLock;

use flate2::read::GzDecoder;
use ufs_core::{BlockIo, IoError, MountOptions, Ufs};

/// Fuzz targets implemented by this harness.
pub const TARGETS: &[&str] = &[
    "ufs_superblock",
    "ufs_cg",
    "ufs_inode",
    "ufs_directory",
    "ufs_indirect",
];

/// One target's base image and mutable region.
pub struct Region {
    /// Full decompressed golden image.
    pub base: Vec<u8>,
    /// Byte offset of the target region.
    pub offset: usize,
    /// Maximum region length.
    pub length: usize,
}

#[derive(Clone)]
struct ReplayDisk {
    data: Rc<RefCell<Vec<u8>>>,
    bad_reads: Rc<Cell<usize>>,
}

impl ReplayDisk {
    fn new(data: Vec<u8>) -> Self {
        Self {
            data: Rc::new(RefCell::new(data)),
            bad_reads: Rc::new(Cell::new(0)),
        }
    }
}

impl BlockIo for ReplayDisk {
    fn size_bytes(&self) -> u64 {
        self.data.borrow().len() as u64
    }

    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), IoError> {
        let start = usize::try_from(offset).map_err(|_| IoError)?;
        let end = start.checked_add(buf.len()).ok_or(IoError)?;
        let Ok(data) = self.data.try_borrow() else {
            self.bad_reads.set(self.bad_reads.get() + 1);
            return Err(IoError);
        };
        let Some(source) = data.get(start..end) else {
            self.bad_reads.set(self.bad_reads.get() + 1);
            return Err(IoError);
        };
        buf.copy_from_slice(source);
        Ok(())
    }

    fn write_at(&mut self, _offset: u64, _buf: &[u8]) -> Result<(), IoError> {
        Err(IoError)
    }

    fn flush(&mut self) -> Result<(), IoError> {
        Ok(())
    }
}

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../ufs-vectors/golden/ufs2-b4096.img.gz")
}

fn base_image() -> Result<&'static Vec<u8>, String> {
    static IMAGE: OnceLock<Result<Vec<u8>, String>> = OnceLock::new();
    IMAGE
        .get_or_init(|| {
            let file = fs::File::open(golden_path())
                .map_err(|error| format!("open golden image: {error}"))?;
            let mut decoder = GzDecoder::new(file);
            let mut data = Vec::new();
            decoder
                .read_to_end(&mut data)
                .map_err(|error| format!("decompress golden image: {error}"))?;
            Ok(data)
        })
        .as_ref()
        .map_err(Clone::clone)
}

fn le32(data: &[u8], offset: usize) -> Result<u32, String> {
    let bytes = data
        .get(offset..offset + 4)
        .ok_or_else(|| "short superblock field".to_string())?;
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn le64(data: &[u8], offset: usize) -> Result<u64, String> {
    let bytes = data
        .get(offset..offset + 8)
        .ok_or_else(|| "short inode field".to_string())?;
    let mut value = [0u8; 8];
    value.copy_from_slice(bytes);
    Ok(u64::from_le_bytes(value))
}

struct Geometry {
    bsize: usize,
    fsize: usize,
    inode_size: usize,
    iblkno: usize,
    cblkno: usize,
    cgsize: usize,
}

fn geometry(base: &[u8]) -> Result<Geometry, String> {
    let superblock = 8192usize;
    let bsize = le32(base, superblock + 48)? as usize;
    let fsize = le32(base, superblock + 52)? as usize;
    let inopb = le32(base, superblock + 120)? as usize;
    if bsize == 0 || fsize == 0 || inopb == 0 || !bsize.is_multiple_of(inopb) {
        return Err("bad geometry".to_string());
    }
    Ok(Geometry {
        bsize,
        fsize,
        inode_size: bsize / inopb,
        iblkno: le32(base, superblock + 16)? as usize,
        cblkno: le32(base, superblock + 12)? as usize,
        cgsize: le32(base, superblock + 160)? as usize,
    })
}

fn inode_offset(geometry: &Geometry, ino: u64) -> Result<usize, String> {
    let ino = usize::try_from(ino).map_err(|_| "inode too large".to_string())?;
    geometry
        .iblkno
        .checked_mul(geometry.fsize)
        .and_then(|base| base.checked_add(ino.checked_mul(geometry.inode_size)?))
        .ok_or_else(|| "inode offset overflow".to_string())
}

/// Return the base image and the region a target mutates.
pub fn region(target: &str) -> Result<Region, String> {
    let base = base_image()?.clone();
    let geometry = geometry(&base)?;
    let (offset, length) = match target {
        "ufs_superblock" => (8192, 1376),
        "ufs_cg" => (
            geometry
                .cblkno
                .checked_mul(geometry.fsize)
                .ok_or_else(|| "cg offset".to_string())?,
            geometry.cgsize,
        ),
        "ufs_inode" => (inode_offset(&geometry, 2)?, geometry.inode_size),
        "ufs_directory" => {
            let inode = inode_offset(&geometry, 2)?;
            let block = le64(&base, inode + 112)?;
            (
                usize::try_from(block)
                    .map_err(|_| "directory block too large".to_string())?
                    .checked_mul(geometry.fsize)
                    .ok_or_else(|| "directory offset".to_string())?,
                geometry.fsize,
            )
        }
        "ufs_indirect" => {
            let disk = ReplayDisk::new(base.clone());
            let mut ufs = Ufs::mount(disk, MountOptions { read_only: true })
                .map_err(|error| format!("base mount: {error}"))?;
            let ino = ufs
                .lookup(2, b"blocks600.bin")
                .map_err(|error| format!("base lookup: {error}"))?;
            let inode = inode_offset(&geometry, u64::from(ino))?;
            let block = le64(&base, inode + 208)?;
            (
                usize::try_from(block)
                    .map_err(|_| "indirect block too large".to_string())?
                    .checked_mul(geometry.fsize)
                    .ok_or_else(|| "indirect offset".to_string())?,
                geometry.bsize,
            )
        }
        other => return Err(format!("unknown target {other}")),
    };
    if offset
        .checked_add(length)
        .is_none_or(|end| end > base.len())
    {
        return Err(format!("{target} region outside image"));
    }
    Ok(Region {
        base,
        offset,
        length,
    })
}

/// Replay one fuzz input for `target`.
pub fn replay(target: &str, input: &[u8]) -> Result<(), String> {
    let region = region(target)?;
    let mut data = region.base;
    let length = input.len().min(region.length);
    let target_bytes = data
        .get_mut(region.offset..region.offset + length)
        .ok_or_else(|| "region slice".to_string())?;
    target_bytes.copy_from_slice(&input[..length]);

    let disk = ReplayDisk::new(data);
    let bad_reads = Rc::clone(&disk.bad_reads);
    let outcome = catch_unwind(AssertUnwindSafe(|| run(target, disk)));
    if outcome.is_err() {
        return Err(format!("{target}: panic"));
    }
    if bad_reads.get() != 0 {
        return Err(format!("{target}: {} out-of-range reads", bad_reads.get()));
    }
    Ok(())
}

fn run(target: &str, disk: ReplayDisk) {
    let Ok(mut ufs) = Ufs::mount(disk, MountOptions { read_only: true }) else {
        return;
    };
    let _ = ufs.getattr(2);
    let _ = ufs.lookup(2, b"empty");
    let _ = ufs.lookup(2, b"blocks600.bin");
    let mut entries = 0usize;
    let _ = ufs.readdir(2, 0, &mut |_entry, _cookie| {
        entries += 1;
        entries < 8
    });
    let _ = ufs.read(3, 0, &mut [0u8; 64]);
    if target == "ufs_indirect"
        && let Ok(ino) = ufs.lookup(2, b"blocks600.bin")
    {
        let _ = ufs.read(ino, 20 * 4096, &mut [0u8; 4096]);
    }
}
