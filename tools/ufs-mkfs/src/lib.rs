// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-side UFS image creation and directory population.

#![forbid(unsafe_code)]

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

pub use ufs_core::mkfs::MkfsOptions;
use ufs_core::mkfs::format;
use ufs_core::{BlockIo, FileType, Ino, IoError, MountOptions, SetAttr, Ufs};

/// A file-backed block device.
pub struct FileDisk {
    file: File,
    size: u64,
}

impl FileDisk {
    /// Open an existing image for reading and writing.
    pub fn open(path: &Path) -> Result<Self, String> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .map_err(|error| format!("open {}: {error}", path.display()))?;
        let size = file
            .metadata()
            .map_err(|error| format!("stat {}: {error}", path.display()))?
            .len();
        Ok(Self { file, size })
    }

    fn create(path: &Path, size: u64) -> Result<Self, String> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .map_err(|error| format!("open {}: {error}", path.display()))?;
        if size != 0 {
            file.set_len(size)
                .map_err(|error| format!("set length of {}: {error}", path.display()))?;
        }
        let size = file
            .metadata()
            .map_err(|error| format!("stat {}: {error}", path.display()))?
            .len();
        Ok(Self { file, size })
    }
}

impl BlockIo for FileDisk {
    fn size_bytes(&self) -> u64 {
        self.size
    }

    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), IoError> {
        self.file
            .seek(SeekFrom::Start(offset))
            .map_err(|_| IoError)?;
        self.file.read_exact(buf).map_err(|_| IoError)
    }

    fn write_at(&mut self, offset: u64, buf: &[u8]) -> Result<(), IoError> {
        self.file
            .seek(SeekFrom::Start(offset))
            .map_err(|_| IoError)?;
        self.file.write_all(buf).map_err(|_| IoError)
    }

    fn flush(&mut self) -> Result<(), IoError> {
        self.file.sync_all().map_err(|_| IoError)
    }
}

/// Create an image and optionally populate it from `host_dir`.
pub fn format_image(
    path: &Path,
    opts: &MkfsOptions<'_>,
    host_dir: Option<&Path>,
) -> Result<(), String> {
    let disk = FileDisk::create(path, opts.size)?;
    if disk.size == 0 {
        return Err(format!("{} is empty; pass -s", path.display()));
    }
    let mut ufs =
        format(disk, opts).map_err(|error| format!("format {}: {error}", path.display()))?;
    if let Some(host) = host_dir {
        populate(&mut ufs, host)?;
    }
    ufs.unmount()
        .map_err(|error| format!("unmount {}: {error}", path.display()))?;
    Ok(())
}

/// Populate a mounted filesystem root from a host directory.
///
/// Files, directories and symlinks are copied in sorted name order with owner
/// 0:0 and the host permission bits. A pre-existing directory with the same
/// name (notably `lost+found`) is reused; any other collision is an error.
pub fn populate<D: BlockIo>(ufs: &mut Ufs<D>, host_dir: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(host_dir)
        .map_err(|error| format!("stat {}: {error}", host_dir.display()))?;
    if !metadata.is_dir() {
        return Err(format!("{} is not a directory", host_dir.display()));
    }
    let root = ufs.root();
    let root_mode = (metadata.permissions().mode() & 0o7777) as u16;
    ufs.setattr(
        root,
        SetAttr {
            mode: Some(root_mode),
            uid: Some(0),
            gid: Some(0),
            ..SetAttr::default()
        },
    )
    .map_err(|error| format!("set root attributes: {error}"))?;
    populate_dir(ufs, root, host_dir)
}

fn populate_dir<D: BlockIo>(ufs: &mut Ufs<D>, dir: Ino, host: &Path) -> Result<(), String> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(host).map_err(|error| format!("read {}: {error}", host.display()))? {
        let entry = entry.map_err(|error| format!("read {}: {error}", host.display()))?;
        entries.push(entry.path());
    }
    entries.sort_by(|left, right| {
        left.file_name()
            .map(|name| name.as_bytes().to_vec())
            .cmp(&right.file_name().map(|name| name.as_bytes().to_vec()))
    });

    for path in entries {
        let name = path
            .file_name()
            .ok_or_else(|| format!("bad host path {}", path.display()))?;
        let name = name.as_bytes();
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| format!("stat {}: {error}", path.display()))?;
        let mode = metadata.permissions().mode() & 0o7777;
        if metadata.is_dir() {
            let child = match ufs.lookup(dir, name) {
                Ok(existing) => {
                    let attr = ufs
                        .getattr(existing)
                        .map_err(|error| format!("getattr {}: {error}", path.display()))?;
                    if attr.kind != FileType::Directory {
                        return Err(format!("{} exists and is not a directory", path.display()));
                    }
                    existing
                }
                Err(ufs_core::UfsError::NotFound) => ufs
                    .mkdir(dir, name, mode as u16, 0, 0)
                    .map_err(|error| format!("mkdir {}: {error}", path.display()))?,
                Err(error) => return Err(format!("lookup {}: {error}", path.display())),
            };
            populate_dir(ufs, child, &path)?;
        } else if metadata.is_file() {
            let data =
                fs::read(&path).map_err(|error| format!("read {}: {error}", path.display()))?;
            let ino = ufs
                .create(dir, name, mode as u16, 0, 0)
                .map_err(|error| format!("create {}: {error}", path.display()))?;
            if !data.is_empty() {
                ufs.write(ino, 0, &data)
                    .map_err(|error| format!("write {}: {error}", path.display()))?;
            }
        } else if metadata.is_symlink() {
            let target = fs::read_link(&path)
                .map_err(|error| format!("readlink {}: {error}", path.display()))?;
            ufs.symlink(dir, name, target.as_os_str().as_bytes(), 0, 0)
                .map_err(|error| format!("symlink {}: {error}", path.display()))?;
        } else {
            return Err(format!("unsupported file type: {}", path.display()));
        }
    }
    Ok(())
}

/// Mount an image read-only for verification.
pub fn open_read_only(path: &Path) -> Result<Ufs<FileDisk>, String> {
    let disk = FileDisk::open(path)?;
    Ufs::mount(disk, MountOptions { read_only: true })
        .map_err(|error| format!("mount {}: {error}", path.display()))
}
