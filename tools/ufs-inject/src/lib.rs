// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-side in-place UFS1/UFS2 image and partition writing.
//!
//! The tool opens an existing image (or a slice of a GPT image/block device), mounts it
//! read-write through [`ufs_core::Ufs`], replaces or creates a regular file or directory, syncs,
//! unmounts, runs the read-only fsck and reads the object back. It never mounts UFS through the
//! Linux kernel and never creates a fresh filesystem unless the caller already did.
//!
//! Unit tests create a fresh image with `ufs-mkfs`, then exercise create/replace/readback,
//! directories and a non-zero partition offset.

#![forbid(unsafe_code)]

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

pub use ufs_core::Timespec;
use ufs_core::check::{Mode, Report, check};
use ufs_core::{Attr, BlockIo, FileType, Ino, IoError, MountOptions, SetAttr, Ufs, UfsError};

/// One image or partition to update.
#[derive(Clone, Debug)]
pub struct Image {
    /// Host image or block-device path.
    pub path: PathBuf,
    /// Byte offset of the UFS filesystem inside `path`.
    pub offset: u64,
    /// Filesystem length in bytes; zero means "to the end of the file".
    pub size: u64,
}

impl Image {
    /// Build a source description.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>, offset: u64, size: u64) -> Self {
        Self {
            path: path.into(),
            offset,
            size,
        }
    }
}

/// Attributes for `put`.
#[derive(Clone, Copy, Debug, Default)]
pub struct PutOptions {
    /// Override the host permission bits.
    pub mode: Option<u16>,
    /// Override the host uid.
    pub uid: Option<u32>,
    /// Override the host gid.
    pub gid: Option<u32>,
    /// Override the host modification time.
    pub mtime: Option<Timespec>,
    /// Create missing parent directories.
    pub parents: bool,
}

/// Attributes for `symlink`.
#[derive(Clone, Copy, Debug, Default)]
pub struct SymlinkOptions {
    /// Owner uid, default 0.
    pub uid: Option<u32>,
    /// Owner gid, default 0.
    pub gid: Option<u32>,
    /// Modification time, default unchanged.
    pub mtime: Option<Timespec>,
    /// Create missing parent directories.
    pub parents: bool,
}

/// Attributes for `mkdir`.
#[derive(Clone, Copy, Debug, Default)]
pub struct MkdirOptions {
    /// Directory mode, default `0o755`.
    pub mode: Option<u16>,
    /// Owner uid, default 0.
    pub uid: Option<u32>,
    /// Owner gid, default 0.
    pub gid: Option<u32>,
    /// Modification time, default unchanged.
    pub mtime: Option<Timespec>,
    /// Create missing parent directories.
    pub parents: bool,
}

/// Result of a `put`.
#[derive(Clone, Debug)]
pub struct PutReport {
    /// Inode that received the data.
    pub ino: Ino,
    /// True when an existing file was truncated and replaced.
    pub replaced: bool,
    /// Number of bytes written.
    pub bytes: u64,
    /// Read-only fsck after the write.
    pub report: Report,
}

/// A file-backed block device limited to `offset..offset + size`.
struct SliceDisk {
    file: File,
    base: u64,
    size: u64,
    writable: bool,
}

impl SliceDisk {
    fn open(path: &Path, offset: u64, size: u64, writable: bool) -> Result<Self, String> {
        let mut options = OpenOptions::new();
        options.read(true);
        if writable {
            options.write(true);
        }
        let file = options
            .open(path)
            .map_err(|error| format!("open {}: {error}", path.display()))?;
        let file_len = file
            .metadata()
            .map_err(|error| format!("stat {}: {error}", path.display()))?
            .len();
        let size = if size == 0 {
            file_len
                .checked_sub(offset)
                .ok_or_else(|| format!("offset {offset} is past the end of {}", path.display()))?
        } else {
            size
        };
        if size == 0 {
            return Err(format!(
                "{} has no UFS bytes at offset {offset}",
                path.display()
            ));
        }
        if file_len != 0 {
            let end = offset
                .checked_add(size)
                .ok_or_else(|| "UFS slice overflows".to_string())?;
            if end > file_len {
                return Err(format!(
                    "UFS slice {offset}+{size} exceeds {} ({file_len} bytes)",
                    path.display()
                ));
            }
        }
        Ok(Self {
            file,
            base: offset,
            size,
            writable,
        })
    }
}

impl BlockIo for SliceDisk {
    fn size_bytes(&self) -> u64 {
        self.size
    }

    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), IoError> {
        let end = offset.checked_add(buf.len() as u64).ok_or(IoError)?;
        if end > self.size {
            return Err(IoError);
        }
        self.file
            .seek(SeekFrom::Start(self.base + offset))
            .map_err(|_| IoError)?;
        self.file.read_exact(buf).map_err(|_| IoError)
    }

    fn write_at(&mut self, offset: u64, buf: &[u8]) -> Result<(), IoError> {
        if !self.writable {
            return Err(IoError);
        }
        let end = offset.checked_add(buf.len() as u64).ok_or(IoError)?;
        if end > self.size {
            return Err(IoError);
        }
        self.file
            .seek(SeekFrom::Start(self.base + offset))
            .map_err(|_| IoError)?;
        self.file.write_all(buf).map_err(|_| IoError)
    }

    fn flush(&mut self) -> Result<(), IoError> {
        if self.writable {
            self.file.sync_all().map_err(|_| IoError)?;
        }
        Ok(())
    }
}

enum Disk {
    Raw(ufs_mkfs::FileDisk),
    Slice(SliceDisk),
}

impl BlockIo for Disk {
    fn size_bytes(&self) -> u64 {
        match self {
            Self::Raw(disk) => disk.size_bytes(),
            Self::Slice(disk) => disk.size_bytes(),
        }
    }

    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), IoError> {
        match self {
            Self::Raw(disk) => disk.read_at(offset, buf),
            Self::Slice(disk) => disk.read_at(offset, buf),
        }
    }

    fn write_at(&mut self, offset: u64, buf: &[u8]) -> Result<(), IoError> {
        match self {
            Self::Raw(disk) => disk.write_at(offset, buf),
            Self::Slice(disk) => disk.write_at(offset, buf),
        }
    }

    fn flush(&mut self) -> Result<(), IoError> {
        match self {
            Self::Raw(disk) => disk.flush(),
            Self::Slice(disk) => disk.flush(),
        }
    }
}

fn open_disk(image: &Image, writable: bool) -> Result<Disk, String> {
    if image.offset == 0 && image.size == 0 && writable {
        Ok(Disk::Raw(ufs_mkfs::FileDisk::open(&image.path).map_err(
            |error| format!("open {}: {error}", image.path.display()),
        )?))
    } else {
        Ok(Disk::Slice(SliceDisk::open(
            &image.path,
            image.offset,
            image.size,
            writable,
        )?))
    }
}

fn mount_writable(image: &Image) -> Result<Ufs<Disk>, String> {
    Ufs::mount(open_disk(image, true)?, MountOptions { read_only: false })
        .map_err(|error| format!("mount {}: {error}", image.path.display()))
}

fn mount_read_only(image: &Image) -> Result<Ufs<Disk>, String> {
    Ufs::mount(open_disk(image, false)?, MountOptions { read_only: true })
        .map_err(|error| format!("mount {}: {error}", image.path.display()))
}

fn normalize(path: &str) -> Result<String, String> {
    if path.is_empty() {
        return Ok(String::from("/"));
    }
    if !path.starts_with('/') {
        return Err(format!("UFS path must be absolute: {path}"));
    }
    let mut parts = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => return Err(String::from("`..` paths are not supported")),
            _ => parts.push(part),
        }
    }
    Ok(format!("/{}", parts.join("/")))
}

const MAX_SYMLINKS: usize = 16;

fn resolve_node<D: BlockIo>(
    ufs: &mut Ufs<D>,
    path: &str,
    follow_last: bool,
    depth: usize,
) -> Result<(Ino, Attr), String> {
    if depth > MAX_SYMLINKS {
        return Err(format!("too many symlinks resolving {path}"));
    }
    let normalized = normalize(path)?;
    if normalized == "/" {
        let ino = ufs.root();
        let attr = ufs.getattr(ino).map_err(|error| error.to_string())?;
        return Ok((ino, attr));
    }
    let parts: Vec<&str> = normalized
        .trim_start_matches('/')
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();
    let mut ino = ufs.root();
    for (index, part) in parts.iter().enumerate() {
        let last = index + 1 == parts.len();
        let child = ufs
            .lookup(ino, part.as_bytes())
            .map_err(|error| format!("lookup {part}: {error}"))?;
        let attr = ufs
            .getattr(child)
            .map_err(|error| format!("stat {part}: {error}"))?;
        if attr.kind == FileType::Symlink && (follow_last || !last) {
            let mut target = [0u8; 4096];
            let length = ufs
                .readlink(child, &mut target)
                .map_err(|error| format!("readlink {part}: {error}"))?;
            let target = core::str::from_utf8(&target[..length])
                .map_err(|_| format!("symlink {part} is not UTF-8"))?;
            let base = if target.starts_with('/') {
                String::new()
            } else {
                let prefix = parts[..index].join("/");
                if prefix.is_empty() {
                    String::new()
                } else {
                    format!("/{prefix}")
                }
            };
            let rest = parts[index + 1..].join("/");
            let combined = format!("{base}/{target}");
            let next = if rest.is_empty() {
                combined
            } else {
                format!("{combined}/{rest}")
            };
            return resolve_node(ufs, &next, follow_last, depth + 1);
        }
        if !last && attr.kind != FileType::Directory {
            return Err(format!("{part} is not a directory"));
        }
        ino = child;
    }
    let attr = ufs.getattr(ino).map_err(|error| error.to_string())?;
    Ok((ino, attr))
}

fn resolve_dir<D: BlockIo>(ufs: &mut Ufs<D>, path: &str) -> Result<Ino, String> {
    let (ino, attr) = resolve_node(ufs, path, true, 0)?;
    if attr.kind != FileType::Directory {
        return Err(format!("{path} is not a directory"));
    }
    Ok(ino)
}

fn ensure_dir<D: BlockIo>(
    ufs: &mut Ufs<D>,
    path: &str,
    mode: u16,
    uid: u32,
    gid: u32,
) -> Result<Ino, String> {
    let normalized = normalize(path)?;
    let mut ino = ufs.root();
    if normalized == "/" {
        return Ok(ino);
    }
    for part in normalized.trim_start_matches('/').split('/') {
        if part.is_empty() {
            continue;
        }
        match ufs.lookup(ino, part.as_bytes()) {
            Ok(next) => {
                let attr = ufs
                    .getattr(next)
                    .map_err(|error| format!("stat {part}: {error}"))?;
                if attr.kind != FileType::Directory {
                    return Err(format!("{part} exists and is not a directory"));
                }
                ino = next;
            }
            Err(UfsError::NotFound) => {
                ino = ufs
                    .mkdir(ino, part.as_bytes(), mode, uid, gid)
                    .map_err(|error| format!("mkdir {part}: {error}"))?;
            }
            Err(error) => return Err(format!("lookup {part}: {error}")),
        }
    }
    Ok(ino)
}

fn parent_and_name<D: BlockIo>(
    ufs: &mut Ufs<D>,
    dest: &str,
    parents: bool,
) -> Result<(Ino, String), String> {
    let normalized = normalize(dest)?;
    if normalized == "/" {
        return Err(String::from("destination is the filesystem root"));
    }
    let (parent, name) = normalized
        .rsplit_once('/')
        .ok_or_else(|| format!("bad destination {dest}"))?;
    let parent = if parent.is_empty() { "/" } else { parent };
    let dir = if parents {
        ensure_dir(ufs, parent, 0o755, 0, 0)?
    } else {
        resolve_dir(ufs, parent)?
    };
    Ok((dir, name.to_string()))
}

fn host_attributes(host: &Path) -> Result<(u16, u32, u32, Timespec, Vec<u8>), String> {
    let metadata =
        fs::metadata(host).map_err(|error| format!("stat {}: {error}", host.display()))?;
    if !metadata.is_file() {
        return Err(format!("{} is not a regular file", host.display()));
    }
    let mode = (metadata.permissions().mode() & 0o7777) as u16;
    let mtime = Timespec {
        sec: metadata.mtime(),
        nsec: u32::try_from(metadata.mtime_nsec()).unwrap_or(0) % 1_000_000_000,
    };
    let data = fs::read(host).map_err(|error| format!("read {}: {error}", host.display()))?;
    Ok((mode, metadata.uid(), metadata.gid(), mtime, data))
}

fn set_attributes<D: BlockIo>(
    ufs: &mut Ufs<D>,
    ino: Ino,
    mode: Option<u16>,
    uid: Option<u32>,
    gid: Option<u32>,
    mtime: Option<Timespec>,
) -> Result<(), String> {
    ufs.setattr(
        ino,
        SetAttr {
            mode,
            uid,
            gid,
            mtime,
            ..SetAttr::default()
        },
    )
    .map_err(|error| format!("setattr: {error}"))
}

fn check_image(image: &Image) -> Result<Report, String> {
    let mut disk = open_disk(image, false)?;
    check(&mut disk, Mode::ReadOnly).map_err(|error| format!("fsck: {error}"))
}

fn read_back(image: &Image, path: &str, expected: &[u8]) -> Result<(), String> {
    let data = cat(image, path)?;
    if data != expected {
        return Err(format!(
            "read back mismatch at {path}: wrote {} bytes, read {}",
            expected.len(),
            data.len()
        ));
    }
    Ok(())
}

/// Replace or create a regular file.
///
/// # Errors
/// Returns a message for a bad path, a non-regular destination, a failed write, a dirty fsck
/// result or a read-back mismatch.
pub fn put(
    image: &Image,
    host: &Path,
    dest: &str,
    options: &PutOptions,
) -> Result<PutReport, String> {
    let (host_mode, host_uid, host_gid, host_mtime, data) = host_attributes(host)?;
    let mode = options.mode.unwrap_or(host_mode);
    let uid = options.uid.unwrap_or(host_uid);
    let gid = options.gid.unwrap_or(host_gid);
    let mtime = options.mtime.unwrap_or(host_mtime);
    let mut ufs = mount_writable(image)?;
    let (parent, name) = parent_and_name(&mut ufs, dest, options.parents)?;
    let existing = match ufs.lookup(parent, name.as_bytes()) {
        Ok(ino) => Some(ino),
        Err(UfsError::NotFound) => None,
        Err(error) => return Err(format!("lookup {dest}: {error}")),
    };
    let (ino, replaced) = if let Some(ino) = existing {
        let attr = ufs
            .getattr(ino)
            .map_err(|error| format!("stat {dest}: {error}"))?;
        if attr.kind != FileType::Regular {
            return Err(format!("{dest} exists and is not a regular file"));
        }
        ufs.truncate(ino, 0)
            .map_err(|error| format!("truncate {dest}: {error}"))?;
        (ino, true)
    } else {
        let ino = ufs
            .create(parent, name.as_bytes(), mode, uid, gid)
            .map_err(|error| format!("create {dest}: {error}"))?;
        (ino, false)
    };
    if !data.is_empty() {
        let written = ufs
            .write(ino, 0, &data)
            .map_err(|error| format!("write {dest}: {error}"))?;
        if written != data.len() {
            return Err(format!("short write at {dest}: {written}/{}", data.len()));
        }
    }
    set_attributes(&mut ufs, ino, Some(mode), Some(uid), Some(gid), Some(mtime))?;
    let _disk = ufs
        .unmount()
        .map_err(|error| format!("unmount {}: {error}", image.path.display()))?;
    let report = check_image(image)?;
    if !report.clean_after {
        return Err(format!(
            "fsck after write is not clean ({} finding(s))",
            report.findings.len()
        ));
    }
    read_back(image, dest, &data)?;
    Ok(PutReport {
        ino,
        replaced,
        bytes: data.len() as u64,
        report,
    })
}

/// Create a directory (and optionally missing parents).
///
/// # Errors
/// Returns a message for a bad path, a non-directory collision, a failed write or a dirty fsck.
pub fn mkdir(image: &Image, path: &str, options: &MkdirOptions) -> Result<Ino, String> {
    let mode = options.mode.unwrap_or(0o755);
    let uid = options.uid.unwrap_or(0);
    let gid = options.gid.unwrap_or(0);
    let mut ufs = mount_writable(image)?;
    let normalized = normalize(path)?;
    if normalized == "/" {
        return Err(String::from("the filesystem root already exists"));
    }
    let ino = if options.parents {
        ensure_dir(&mut ufs, &normalized, mode, uid, gid)?
    } else {
        let (parent, name) = parent_and_name(&mut ufs, &normalized, false)?;
        match ufs.lookup(parent, name.as_bytes()) {
            Ok(ino) => {
                let attr = ufs
                    .getattr(ino)
                    .map_err(|error| format!("stat {path}: {error}"))?;
                if attr.kind != FileType::Directory {
                    return Err(format!("{path} exists and is not a directory"));
                }
                ino
            }
            Err(UfsError::NotFound) => ufs
                .mkdir(parent, name.as_bytes(), mode, uid, gid)
                .map_err(|error| format!("mkdir {path}: {error}"))?,
            Err(error) => return Err(format!("lookup {path}: {error}")),
        }
    };
    set_attributes(
        &mut ufs,
        ino,
        Some(mode),
        Some(uid),
        Some(gid),
        options.mtime,
    )?;
    let _disk = ufs
        .unmount()
        .map_err(|error| format!("unmount {}: {error}", image.path.display()))?;
    let report = check_image(image)?;
    if !report.clean_after {
        return Err(format!(
            "fsck after mkdir is not clean ({} finding(s))",
            report.findings.len()
        ));
    }
    Ok(ino)
}

/// Create or replace a symbolic link.
///
/// # Errors
/// Returns a message for a missing parent, a non-symlink collision, a bad target or a dirty fsck.
pub fn symlink(
    image: &Image,
    target: &str,
    path: &str,
    options: &SymlinkOptions,
) -> Result<Ino, String> {
    if target.is_empty() {
        return Err(String::from("empty symlink target"));
    }
    let uid = options.uid.unwrap_or(0);
    let gid = options.gid.unwrap_or(0);
    let mut ufs = mount_writable(image)?;
    let (parent, name) = parent_and_name(&mut ufs, path, options.parents)?;
    match ufs.lookup(parent, name.as_bytes()) {
        Ok(existing) => {
            let attr = ufs
                .getattr(existing)
                .map_err(|error| format!("stat {path}: {error}"))?;
            if attr.kind == FileType::Directory {
                return Err(format!("{path} is a directory"));
            }
            ufs.unlink(parent, name.as_bytes())
                .map_err(|error| format!("unlink {path}: {error}"))?;
        }
        Err(UfsError::NotFound) => {}
        Err(error) => return Err(format!("lookup {path}: {error}")),
    }
    let ino = ufs
        .symlink(parent, name.as_bytes(), target.as_bytes(), uid, gid)
        .map_err(|error| format!("symlink {path}: {error}"))?;
    if options.mtime.is_some() {
        set_attributes(&mut ufs, ino, None, None, None, options.mtime)?;
    }
    let _disk = ufs
        .unmount()
        .map_err(|error| format!("unmount {}: {error}", image.path.display()))?;
    let report = check_image(image)?;
    if !report.clean_after {
        return Err(format!(
            "fsck after symlink is not clean ({} finding(s))",
            report.findings.len()
        ));
    }
    Ok(ino)
}

fn remove_entry<D: BlockIo>(
    ufs: &mut Ufs<D>,
    parent: Ino,
    name: &[u8],
    path: &str,
    recursive: bool,
) -> Result<(), String> {
    let ino = ufs
        .lookup(parent, name)
        .map_err(|error| format!("lookup {path}: {error}"))?;
    let attr = ufs
        .getattr(ino)
        .map_err(|error| format!("stat {path}: {error}"))?;
    if attr.kind != FileType::Directory {
        return ufs
            .unlink(parent, name)
            .map_err(|error| format!("unlink {path}: {error}"));
    }
    if !recursive {
        return ufs
            .rmdir(parent, name)
            .map_err(|error| format!("rmdir {path}: {error}"));
    }
    let mut children = Vec::new();
    ufs.readdir(ino, 0, &mut |entry, _| {
        if entry.name != b"." && entry.name != b".." {
            children.push(entry.name.clone());
        }
        true
    })
    .map_err(|error| format!("readdir {path}: {error}"))?;
    let base = path.trim_end_matches('/');
    for child in children {
        let child_name = String::from_utf8_lossy(&child);
        let child_path = format!("{base}/{child_name}");
        remove_entry(ufs, ino, &child, &child_path, true)?;
    }
    ufs.rmdir(parent, name)
        .map_err(|error| format!("rmdir {path}: {error}"))
}

/// Remove a file, symlink or empty directory; `recursive` removes a directory tree.
///
/// # Errors
/// Returns a message for a missing path, a non-empty directory without `recursive`, a bad path
/// or a dirty fsck after the removal.
pub fn remove(image: &Image, path: &str, recursive: bool) -> Result<(), String> {
    let normalized = normalize(path)?;
    if normalized == "/" {
        return Err(String::from("the filesystem root cannot be removed"));
    }
    let mut ufs = mount_writable(image)?;
    let (parent, name) = parent_and_name(&mut ufs, &normalized, false)?;
    remove_entry(&mut ufs, parent, name.as_bytes(), &normalized, recursive)?;
    let _disk = ufs
        .unmount()
        .map_err(|error| format!("unmount {}: {error}", image.path.display()))?;
    let report = check_image(image)?;
    if !report.clean_after {
        return Err(format!(
            "fsck after rm is not clean ({} finding(s))",
            report.findings.len()
        ));
    }
    Ok(())
}

/// Read a regular file.
///
/// # Errors
/// Returns a message for a missing path, a non-regular object or an I/O error.
pub fn cat(image: &Image, path: &str) -> Result<Vec<u8>, String> {
    let mut ufs = mount_read_only(image)?;
    let (ino, attr) = resolve_node(&mut ufs, path, true, 0)?;
    if attr.kind != FileType::Regular {
        return Err(format!("{path} is not a regular file"));
    }
    let size = usize::try_from(attr.size).map_err(|_| format!("{path} is too large"))?;
    let mut data = Vec::with_capacity(size);
    let mut buffer = vec![0u8; 64 * 1024];
    while data.len() < size {
        let want = (size - data.len()).min(buffer.len());
        let read = ufs
            .read(ino, data.len() as u64, &mut buffer[..want])
            .map_err(|error| format!("read {path}: {error}"))?;
        if read == 0 {
            break;
        }
        data.extend_from_slice(&buffer[..read]);
    }
    if data.len() != size {
        return Err(format!(
            "{path}: expected {size} bytes, read {}",
            data.len()
        ));
    }
    Ok(data)
}

/// List a directory in name order.
///
/// # Errors
/// Returns a message for a missing path or a non-directory object.
pub fn list(image: &Image, path: &str) -> Result<Vec<String>, String> {
    let mut ufs = mount_read_only(image)?;
    let ino = resolve_dir(&mut ufs, path)?;
    let mut names = Vec::new();
    ufs.readdir(ino, 0, &mut |entry, _| {
        if entry.name != b"." && entry.name != b".." {
            names.push(String::from_utf8_lossy(&entry.name).into_owned());
        }
        true
    })
    .map_err(|error| format!("readdir {path}: {error}"))?;
    names.sort();
    Ok(names)
}

/// Stat a path (following a final symlink).
///
/// # Errors
/// Returns a message for a missing path.
pub fn stat(image: &Image, path: &str) -> Result<Attr, String> {
    let mut ufs = mount_read_only(image)?;
    let (_, attr) = resolve_node(&mut ufs, path, true, 0)?;
    Ok(attr)
}

/// Stat a path without following a final symlink.
///
/// # Errors
/// Returns a message for a missing path.
pub fn lstat(image: &Image, path: &str) -> Result<Attr, String> {
    let mut ufs = mount_read_only(image)?;
    let (_, attr) = resolve_node(&mut ufs, path, false, 0)?;
    Ok(attr)
}

/// Run the read-only fsck.
///
/// # Errors
/// Returns a message only when the image cannot be opened or checked; a non-clean report is a
/// successful return with `clean_after == false`.
pub fn fsck(image: &Image) -> Result<Report, String> {
    check_image(image)
}

/// Preen the UFS slice in place: repair the checker's normal findings and verify the result.
///
/// # Errors
/// Returns a message when the image cannot be opened or repaired; a report with
/// `clean_after == false` is returned for findings the checker cannot resolve.
pub fn repair(image: &Image) -> Result<Report, String> {
    let mut disk = open_disk(image, true)?;
    let report = check(&mut disk, Mode::Repair).map_err(|error| format!("fsck repair: {error}"))?;
    disk.flush()
        .map_err(|_| format!("flush {}", image.path.display()))?;
    Ok(report)
}

/// The four permission bits and type for a `stat` line.
#[must_use]
pub fn type_name(kind: FileType) -> &'static str {
    match kind {
        FileType::Regular => "file",
        FileType::Directory => "directory",
        FileType::Symlink => "symlink",
        FileType::CharDevice => "char-device",
        FileType::BlockDevice => "block-device",
        FileType::Fifo => "fifo",
        FileType::Socket => "socket",
    }
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
