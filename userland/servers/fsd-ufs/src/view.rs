// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Adapter from `ufs-core` to the server's [`FileSystem`] interface.
//!
//! Paths are lexically normalised, symbolic links are followed with a fixed
//! depth bound, and every on-disk failure is mapped to a stable errno.

use alloc::vec;
use alloc::vec::Vec;

use raven_abi::errno::Errno;
use raven_proto_fs::{
    DIRENT_ALIGN, DIRENT_HEADER_LEN, DT_BLK, DT_CHR, DT_DIR, DT_FIFO, DT_LNK, DT_REG, DT_SOCK,
    DirEntry as ProtoDirEntry, DirEntryHeader as ProtoDirEntryHeader, MAX_PATH, Stat, StatFs,
};
use ufs_core::{Attr, BlockIo, FileType, Ino, MountOptions, SetAttr, Timespec, Ufs, UfsError};

use crate::handler::FileSystem;

/// Maximum number of symbolic links followed while resolving one path.
pub const MAX_SYMLINKS: usize = 8;
/// Buffer size for one on-disk symbolic-link target.
pub const MAX_SYMLINK_BYTES: usize = 1024;

/// A read-only UFS mount exposed through [`FileSystem`].
pub struct UfsView<D: BlockIo> {
    ufs: Ufs<D>,
    read_only: bool,
}

impl<D: BlockIo> UfsView<D> {
    /// Mount `disk` read-only.
    pub fn mount(disk: D, read_only: bool) -> Result<Self, UfsError> {
        Ok(Self {
            ufs: Ufs::mount(disk, MountOptions { read_only })?,
            read_only,
        })
    }

    /// Whether the mounted filesystem is using a WAPBL journal.
    pub fn is_journaled(&self) -> bool {
        self.ufs.is_journaled()
    }

    /// Read the target bytes of `ino`.
    fn readlink_bytes(&mut self, ino: Ino) -> Result<Vec<u8>, UfsError> {
        let mut buffer = vec![0u8; MAX_SYMLINK_BYTES + 1];
        let length = self.ufs.readlink(ino, &mut buffer)?;
        if length == 0 || length > MAX_SYMLINK_BYTES {
            return Err(UfsError::NameTooLong);
        }
        Ok(buffer[..length].to_vec())
    }

    fn ensure_writable(&self) -> Result<(), Errno> {
        if self.read_only {
            Err(Errno::EROFS)
        } else {
            Ok(())
        }
    }

    fn note_error(&mut self, error: UfsError) -> Errno {
        if matches!(error, UfsError::Io) {
            self.read_only = true;
        }
        errno(error)
    }

    fn set_owner(&mut self, ino: Ino, uid: u32, gid: u32) -> Result<(), Errno> {
        let attr = self.ufs.getattr(ino).map_err(errno)?;
        let mut set = SetAttr::default();
        let mut changed = false;
        if uid != u32::MAX && uid != attr.uid {
            set.uid = Some(uid);
            changed = true;
        }
        if gid != u32::MAX && gid != attr.gid {
            set.gid = Some(gid);
            changed = true;
        }
        if !changed {
            return Ok(());
        }
        set.mode = Some(attr.mode & !0o6000);
        self.ufs
            .setattr(ino, set)
            .map_err(|error| self.note_error(error))
    }

    /// Split a path into its resolved parent directory and final name.
    fn parent_and_name(&mut self, path: &[u8]) -> Result<(Ino, Vec<u8>), Errno> {
        let trimmed = path.strip_suffix(b"/").unwrap_or(path);
        if trimmed.is_empty() {
            return Err(Errno::EINVAL);
        }
        let (parent, name) = match trimmed.iter().rposition(|byte| *byte == b'/') {
            Some(0) => (&trimmed[..1], &trimmed[1..]),
            Some(index) => (&trimmed[..index], &trimmed[index + 1..]),
            None => (b"/".as_slice(), trimmed),
        };
        if name.is_empty() || name == b"." || name == b".." {
            return Err(Errno::EINVAL);
        }
        if name.len() > 255 {
            return Err(Errno::ENAMETOOLONG);
        }
        let dir = self.resolve_inner(parent, true)?;
        let attr = self.ufs.getattr(dir).map_err(errno)?;
        if attr.kind != FileType::Directory {
            return Err(Errno::ENOTDIR);
        }
        Ok((dir, name.to_vec()))
    }

    fn resolve_inner(&mut self, path: &[u8], follow_last: bool) -> Result<Ino, Errno> {
        if path.len() > MAX_PATH {
            return Err(Errno::ENAMETOOLONG);
        }
        let mut current = path.to_vec();
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

    fn walk(&mut self, path: &[u8], follow_last: bool) -> Result<Ino, WalkError> {
        let mut ino = self.ufs.root();
        let mut parent = b"/".to_vec();
        let mut rest = path.strip_prefix(b"/").unwrap_or(path);
        loop {
            rest = rest.strip_prefix(b"/").unwrap_or(rest);
            if rest.is_empty() {
                return Ok(ino);
            }
            let (name, tail) = match rest.iter().position(|byte| *byte == b'/') {
                Some(index) => (&rest[..index], &rest[index + 1..]),
                None => (rest, &b""[..]),
            };
            let child = self.ufs.lookup(ino, name).map_err(WalkError::Ufs)?;
            let attr = self.ufs.getattr(child).map_err(WalkError::Ufs)?;
            let last = tail.is_empty();
            if attr.kind == FileType::Symlink && (follow_last || !last) {
                let target = self.readlink_bytes(child).map_err(WalkError::Ufs)?;
                return Err(WalkError::Expand {
                    parent,
                    target,
                    rest: tail.to_vec(),
                });
            }
            if !last && attr.kind != FileType::Directory {
                return Err(WalkError::Ufs(UfsError::NotDir));
            }
            if parent != b"/" {
                parent.push(b'/');
            }
            parent.extend_from_slice(name);
            ino = child;
            rest = tail;
        }
    }
}

enum WalkError {
    Ufs(UfsError),
    Expand {
        parent: Vec<u8>,
        target: Vec<u8>,
        rest: Vec<u8>,
    },
}

fn rebuild_path(parent: &[u8], target: &[u8], rest: &[u8]) -> Result<Vec<u8>, Errno> {
    let mut combined = Vec::new();
    if target.first() == Some(&b'/') {
        combined.extend_from_slice(target);
    } else {
        combined.extend_from_slice(parent);
        if combined.last() != Some(&b'/') {
            combined.push(b'/');
        }
        combined.extend_from_slice(target);
    }
    if !rest.is_empty() {
        if combined.last() != Some(&b'/') {
            combined.push(b'/');
        }
        combined.extend_from_slice(rest);
    }
    if combined.len() > MAX_PATH {
        return Err(Errno::ENAMETOOLONG);
    }
    Ok(combined)
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
    fn resolve(&mut self, path: &[u8]) -> Result<u32, Errno> {
        self.resolve_inner(path, true)
    }

    fn resolve_nofollow(&mut self, path: &[u8]) -> Result<u32, Errno> {
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
            // `ufs_core` counts `blocks` in `fragment_size` units; the wire record has one
            // size field, so report the fragment size or df multiplies every count by the
            // 8x larger filesystem block size.
            block_size: stat.fragment_size,
            blocks: stat.blocks,
            free_blocks: stat.blocks_free,
            files: stat.files,
            free_files: stat.files_free,
        }
    }

    fn is_read_only(&self) -> bool {
        self.read_only
    }

    fn create_file(
        &mut self,
        path: &[u8],
        mode: u16,
        uid: u32,
        gid: u32,
        exclusive: bool,
    ) -> Result<u32, Errno> {
        self.ensure_writable()?;
        let (parent, name) = self.parent_and_name(path)?;
        match self.ufs.create(parent, &name, mode, uid, gid) {
            Ok(ino) => Ok(ino),
            Err(UfsError::Exists) if !exclusive => self.resolve(path),
            Err(error) => Err(self.note_error(error)),
        }
    }

    fn mkdir(&mut self, path: &[u8], mode: u16, uid: u32, gid: u32) -> Result<(), Errno> {
        self.ensure_writable()?;
        let (parent, name) = self.parent_and_name(path)?;
        self.ufs
            .mkdir(parent, &name, mode, uid, gid)
            .map(|_| ())
            .map_err(|error| self.note_error(error))
    }

    fn unlink(&mut self, path: &[u8]) -> Result<(), Errno> {
        self.ensure_writable()?;
        let (parent, name) = self.parent_and_name(path)?;
        self.ufs
            .unlink(parent, &name)
            .map_err(|error| self.note_error(error))
    }

    fn rmdir(&mut self, path: &[u8]) -> Result<(), Errno> {
        self.ensure_writable()?;
        let (parent, name) = self.parent_and_name(path)?;
        self.ufs
            .rmdir(parent, &name)
            .map_err(|error| self.note_error(error))
    }

    fn rename(&mut self, from: &[u8], to: &[u8]) -> Result<(), Errno> {
        self.ensure_writable()?;
        let (old_dir, old_name) = self.parent_and_name(from)?;
        let (new_dir, new_name) = self.parent_and_name(to)?;
        self.ufs
            .rename(old_dir, &old_name, new_dir, &new_name)
            .map_err(|error| self.note_error(error))
    }

    fn symlink(&mut self, path: &[u8], target: &[u8], uid: u32, gid: u32) -> Result<(), Errno> {
        self.ensure_writable()?;
        let (parent, name) = self.parent_and_name(path)?;
        self.ufs
            .symlink(parent, &name, target, uid, gid)
            .map(|_| ())
            .map_err(|error| self.note_error(error))
    }

    fn link(&mut self, path: &[u8], target: &[u8]) -> Result<(), Errno> {
        self.ensure_writable()?;
        let (parent, name) = self.parent_and_name(path)?;
        let target = self.resolve_inner(target, false)?;
        if self.ufs.getattr(target).map_err(errno)?.kind == FileType::Directory {
            return Err(Errno::EPERM);
        }
        self.ufs
            .link(parent, &name, target)
            .map_err(|error| self.note_error(error))
    }

    fn chmod(&mut self, path: &[u8], mode: u16) -> Result<(), Errno> {
        self.ensure_writable()?;
        let ino = self.resolve(path)?;
        self.ufs
            .setattr(
                ino,
                SetAttr {
                    mode: Some(mode),
                    ..SetAttr::default()
                },
            )
            .map_err(|error| self.note_error(error))
    }

    fn chown(&mut self, path: &[u8], uid: u32, gid: u32, follow: bool) -> Result<(), Errno> {
        self.ensure_writable()?;
        let ino = self.resolve_inner(path, follow)?;
        self.set_owner(ino, uid, gid)
    }

    fn fchown(&mut self, ino: u32, uid: u32, gid: u32) -> Result<(), Errno> {
        self.ensure_writable()?;
        self.set_owner(ino, uid, gid)
    }

    fn utimens(
        &mut self,
        path: &[u8],
        atime: Option<i64>,
        mtime: Option<i64>,
        follow: bool,
    ) -> Result<(), Errno> {
        self.ensure_writable()?;
        let ino = self.resolve_inner(path, follow)?;
        self.ufs
            .setattr(
                ino,
                SetAttr {
                    atime: atime.map(|sec| Timespec { sec, nsec: 0 }),
                    mtime: mtime.map(|sec| Timespec { sec, nsec: 0 }),
                    ..SetAttr::default()
                },
            )
            .map_err(|error| self.note_error(error))
    }

    fn mkfifo(&mut self, path: &[u8], mode: u16, uid: u32, gid: u32) -> Result<(), Errno> {
        self.ensure_writable()?;
        let (parent, name) = self.parent_and_name(path)?;
        self.ufs
            .mknod(parent, &name, 0o010000 | (mode & 0o7777), 0, uid, gid)
            .map(|_| ())
            .map_err(|error| self.note_error(error))
    }

    fn mknod(&mut self, path: &[u8], mode: u16, dev: u32, uid: u32, gid: u32) -> Result<(), Errno> {
        self.ensure_writable()?;
        let kind = mode & 0o170000;
        if !matches!(kind, 0o010000 | 0o020000 | 0o060000 | 0o140000) {
            return Err(Errno::EINVAL);
        }
        let (parent, name) = self.parent_and_name(path)?;
        self.ufs
            .mknod(parent, &name, mode, u64::from(dev), uid, gid)
            .map(|_| ())
            .map_err(|error| self.note_error(error))
    }

    fn truncate(&mut self, ino: u32, size: u64) -> Result<(), Errno> {
        self.ensure_writable()?;
        self.ufs
            .truncate(ino, size)
            .map_err(|error| self.note_error(error))
    }

    fn write(&mut self, ino: u32, offset: u64, data: &[u8]) -> Result<usize, Errno> {
        self.ensure_writable()?;
        self.ufs
            .write(ino, offset, data)
            .map_err(|error| self.note_error(error))
    }

    fn sync(&mut self) -> Result<(), Errno> {
        if self.read_only {
            return Ok(());
        }
        self.ufs.sync().map_err(|error| self.note_error(error))
    }
}

#[cfg(test)]
#[path = "view_ut.rs"]
mod tests;

fn record_len(name_len: usize) -> usize {
    (DIRENT_HEADER_LEN + name_len + (DIRENT_ALIGN - 1)) & !(DIRENT_ALIGN - 1)
}
