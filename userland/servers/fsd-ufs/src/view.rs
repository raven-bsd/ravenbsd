// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Adapter from `ufs-core` to the server's [`FileSystem`] interface.
//!
//! Paths are lexically normalised, symbolic links are followed with a fixed
//! depth bound, and every on-disk failure is mapped to a stable errno.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;

use raven_abi::errno::Errno;
use raven_proto_fs::{
    DIRENT_ALIGN, DIRENT_HEADER_LEN, DT_BLK, DT_CHR, DT_DIR, DT_FIFO, DT_LNK, DT_REG, DT_SOCK,
    DirEntry as ProtoDirEntry, DirEntryHeader as ProtoDirEntryHeader, MAX_PATH, PathError, Stat,
    StatFs, normalize_absolute,
};
use ufs_core::{Attr, BlockIo, FileType, Ino, MountOptions, Ufs, UfsError};

use crate::handler::FileSystem;

/// Maximum number of symbolic links followed while resolving one path.
pub const MAX_SYMLINKS: usize = 8;
/// Buffer size for one on-disk symbolic-link target.
pub const MAX_SYMLINK_BYTES: usize = 1024;

/// A read-only UFS mount exposed through [`FileSystem`].
pub struct UfsView<D: BlockIo> {
    ufs: Ufs<D>,
}

impl<D: BlockIo> UfsView<D> {
    /// Mount `disk` read-only.
    pub fn mount(disk: D, read_only: bool) -> Result<Self, UfsError> {
        Ok(Self {
            ufs: Ufs::mount(disk, MountOptions { read_only })?,
        })
    }

    /// Read the target of `ino` as UTF-8.
    fn readlink_string(&mut self, ino: Ino) -> Result<String, UfsError> {
        let mut buffer = vec![0u8; MAX_SYMLINK_BYTES];
        let length = self.ufs.readlink(ino, &mut buffer)?;
        if length == 0 || length == buffer.len() {
            return Err(UfsError::NameTooLong);
        }
        let target = core::str::from_utf8(&buffer[..length]).map_err(|_| UfsError::Invalid)?;
        Ok(target.to_string())
    }

    fn resolve_inner(&mut self, path: &str, follow_last: bool) -> Result<Ino, Errno> {
        let mut current = normalize_path(path)?;
        for _ in 0..=MAX_SYMLINKS {
            match self.walk(&current, follow_last) {
                Ok(ino) => return Ok(ino),
                Err(WalkError::Expand {
                    parent,
                    target,
                    rest,
                }) => current = rebuild_path(&parent, &target, &rest)?,
                Err(WalkError::Ufs(error)) => return Err(errno(error)),
            }
        }
        Err(Errno::ELOOP)
    }

    fn walk(&mut self, path: &str, follow_last: bool) -> Result<Ino, WalkError> {
        let mut ino = self.ufs.root();
        let mut parent = String::from("/");
        let mut rest = path.trim_start_matches('/');
        loop {
            if rest.is_empty() {
                return Ok(ino);
            }
            let (name, tail) = match rest.split_once('/') {
                Some((name, tail)) => (name, tail.trim_start_matches('/')),
                None => (rest, ""),
            };
            let child = self
                .ufs
                .lookup(ino, name.as_bytes())
                .map_err(WalkError::Ufs)?;
            let attr = self.ufs.getattr(child).map_err(WalkError::Ufs)?;
            let last = tail.is_empty();
            if attr.kind == FileType::Symlink && (follow_last || !last) {
                let target = self.readlink_string(child).map_err(WalkError::Ufs)?;
                return Err(WalkError::Expand {
                    parent,
                    target,
                    rest: tail.to_string(),
                });
            }
            if !last && attr.kind != FileType::Directory {
                return Err(WalkError::Ufs(UfsError::NotDir));
            }
            parent = if parent == "/" {
                format!("/{name}")
            } else {
                format!("{parent}/{name}")
            };
            ino = child;
            rest = tail;
        }
    }
}

enum WalkError {
    Ufs(UfsError),
    Expand {
        parent: String,
        target: String,
        rest: String,
    },
}

fn normalize_path(path: &str) -> Result<String, Errno> {
    // `vfsd` passes the suffix inside a mount without its leading slash; the
    // server root is also opened as an empty path.
    let path = if path.is_empty() {
        "/".to_string()
    } else if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    };
    let mut out = [0u8; MAX_PATH];
    let normalized = normalize_absolute(&path, &mut out).map_err(path_errno)?;
    Ok(normalized.to_string())
}

fn rebuild_path(parent: &str, target: &str, rest: &str) -> Result<String, Errno> {
    let combined = if target.starts_with('/') {
        if rest.is_empty() {
            target.to_string()
        } else {
            format!("{target}/{rest}")
        }
    } else if parent == "/" {
        if rest.is_empty() {
            format!("/{target}")
        } else {
            format!("/{target}/{rest}")
        }
    } else if rest.is_empty() {
        format!("{parent}/{target}")
    } else {
        format!("{parent}/{target}/{rest}")
    };
    normalize_path(&combined)
}

fn path_errno(error: PathError) -> Errno {
    error.errno()
}

/// Map a core error to the protocol errno.
pub fn errno(error: UfsError) -> Errno {
    match error {
        UfsError::NotFound => Errno::ENOENT,
        UfsError::Exists => Errno::EEXIST,
        UfsError::NotEmpty => Errno::ENOTEMPTY,
        UfsError::NotDir => Errno::ENOTDIR,
        UfsError::IsDir => Errno::EISDIR,
        UfsError::NameTooLong => Errno::ENAMETOOLONG,
        UfsError::ReadOnly => Errno::EROFS,
        UfsError::NoSpace => Errno::ENOSPC,
        UfsError::Invalid => Errno::EINVAL,
        UfsError::Io
        | UfsError::BadSuperblock(_)
        | UfsError::Unsupported(_)
        | UfsError::Corrupt(_) => Errno::EIO,
    }
}

fn type_mode(kind: FileType) -> u32 {
    use raven_proto_fs::{S_IFBLK, S_IFCHR, S_IFDIR, S_IFIFO, S_IFLNK, S_IFREG, S_IFSOCK};
    match kind {
        FileType::Regular => S_IFREG,
        FileType::Directory => S_IFDIR,
        FileType::Symlink => S_IFLNK,
        FileType::CharDevice => S_IFCHR,
        FileType::BlockDevice => S_IFBLK,
        FileType::Fifo => S_IFIFO,
        FileType::Socket => S_IFSOCK,
    }
}

fn dirent_kind(kind: FileType) -> u8 {
    match kind {
        FileType::Regular => DT_REG,
        FileType::Directory => DT_DIR,
        FileType::Symlink => DT_LNK,
        FileType::CharDevice => DT_CHR,
        FileType::BlockDevice => DT_BLK,
        FileType::Fifo => DT_FIFO,
        FileType::Socket => DT_SOCK,
    }
}

fn wire_stat(attr: Attr) -> Stat {
    Stat {
        ino: u64::from(attr.ino),
        size: attr.size,
        blocks: attr.blocks,
        mtime: attr.mtime.sec,
        ctime: attr.ctime.sec,
        atime: attr.atime.sec,
        mode: type_mode(attr.kind) | u32::from(attr.mode),
        nlink: attr.nlink,
        uid: attr.uid,
        gid: attr.gid,
    }
}

impl<D: BlockIo> FileSystem for UfsView<D> {
    fn resolve(&mut self, path: &str) -> Result<u32, Errno> {
        self.resolve_inner(path, true)
    }

    fn resolve_nofollow(&mut self, path: &str) -> Result<u32, Errno> {
        self.resolve_inner(path, false)
    }

    fn stat(&mut self, ino: u32) -> Result<Stat, Errno> {
        let attr = self.ufs.getattr(ino).map_err(errno)?;
        Ok(wire_stat(attr))
    }

    fn read(&mut self, ino: u32, offset: u64, buffer: &mut [u8]) -> Result<usize, Errno> {
        self.ufs.read(ino, offset, buffer).map_err(errno)
    }

    fn readdir(&mut self, ino: u32, cookie: u64, buffer: &mut [u8]) -> Result<(usize, u64), Errno> {
        let mut written = 0usize;
        let mut next = cookie;
        let mut full = false;
        let mut failure: Option<Errno> = None;
        let result = self.ufs.readdir(ino, cookie, &mut |entry, next_cookie| {
            if entry.name.is_empty() || entry.name.len() > u8::MAX as usize {
                failure = Some(Errno::ENAMETOOLONG);
                return false;
            }
            let header = ProtoDirEntryHeader {
                ino: u64::from(entry.ino),
                kind: dirent_kind(entry.kind),
                name_len: entry.name.len() as u8,
                pad: [0; 6],
            };
            let length = record_len(entry.name.len());
            let Some(end) = written.checked_add(length) else {
                failure = Some(Errno::EIO);
                return false;
            };
            if end > buffer.len() {
                full = true;
                return false;
            }
            let wire = ProtoDirEntry {
                header,
                name: entry.name.as_slice(),
            };
            if wire.encode(&mut buffer[written..end]).is_err() {
                failure = Some(Errno::EIO);
                return false;
            }
            written = end;
            next = next_cookie;
            true
        });
        if let Some(error) = failure {
            return Err(error);
        }
        result.map_err(errno)?;
        if written == 0 && full {
            return Err(Errno::EINVAL);
        }
        Ok((written, next))
    }

    fn readlink(&mut self, ino: u32, buffer: &mut [u8]) -> Result<usize, Errno> {
        self.ufs.readlink(ino, buffer).map_err(errno)
    }

    fn statfs(&self) -> StatFs {
        let stat = self.ufs.statfs();
        StatFs {
            block_size: stat.block_size,
            blocks: stat.blocks,
            free_blocks: stat.blocks_free,
            files: stat.files,
            free_files: stat.files_free,
        }
    }
}

#[cfg(test)]
#[path = "view_ut.rs"]
mod tests;

fn record_len(name_len: usize) -> usize {
    (DIRENT_HEADER_LEN + name_len + (DIRENT_ALIGN - 1)) & !(DIRENT_ALIGN - 1)
}
