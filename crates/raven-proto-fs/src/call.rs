// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Versioned control requests and responses carried by the S11 `CALL` opcode.
//!
//! A request starts with `version: u16, op: u16, flags: u32, arg0: u32, arg1: u32`, followed by
//! up to two length-prefixed paths. The number of paths and the meaning of `arg0`/`arg1` is fixed
//! by the operation. Unknown versions and operations are reported as protocol errors; a server
//! maps them to `-ENOSYS`.

use crate::ProtoError;
use crate::path::MAX_PATH;
use crate::stat::{STAT_LEN, Stat};
use crate::wire::{Reader, Writer};

/// Encoded size of the fixed [`CallRequest`] header in bytes.
pub const CALL_HEADER_LEN: usize = 16;
/// Maximum number of paths in one control request.
pub const MAX_CALL_PATHS: usize = 2;

/// `MKDIR(mode)` followed by one path.
pub const OP_MKDIR: u16 = 1;
/// `UNLINK` followed by one path.
pub const OP_UNLINK: u16 = 2;
/// `RMDIR` followed by one path.
pub const OP_RMDIR: u16 = 3;
/// `RENAME(path_from, path_to)`.
pub const OP_RENAME: u16 = 4;
/// `TRUNCATE(size)` on the open file handle; no paths.
pub const OP_TRUNCATE: u16 = 5;
/// `SYMLINK(path, target)`.
pub const OP_SYMLINK: u16 = 6;
/// `READLINK(path)` followed by one path.
pub const OP_READLINK: u16 = 7;
/// `CHMOD(mode)` followed by one path.
pub const OP_CHMOD: u16 = 8;
/// `SYNC`; no paths.
pub const OP_SYNC: u16 = 9;
/// `MOUNT(path, scheme)`; handled by `vfsd`.
pub const OP_MOUNT: u16 = 10;
/// `UMOUNT(path)`; handled by `vfsd`.
pub const OP_UMOUNT: u16 = 11;
/// `STATFS`; no paths.
pub const OP_STATFS: u16 = 12;
/// `ACCESS(mode)` followed by one path.
pub const OP_ACCESS: u16 = 13;
/// `LINK(path, target)`: add a hard link at `path` to `target`.
pub const OP_LINK: u16 = 14;
/// `CHOWN(uid, gid)` followed by one path; follows a final symlink.
pub const OP_CHOWN: u16 = 15;
/// `LCHOWN(uid, gid)` followed by one path; does not follow a final symlink.
pub const OP_LCHOWN: u16 = 16;
/// `FCHOWN(uid, gid)` on the open file handle; no paths.
pub const OP_FCHOWN: u16 = 17;
/// `MKFIFO(mode)` followed by one path.
pub const OP_MKFIFO: u16 = 18;
/// `MKNOD(mode, dev)` followed by one path.
pub const OP_MKNOD: u16 = 19;
/// `UTIMENS` followed by one path; `arg0`/`arg1` are whole seconds and the
/// operation flags select set/now/omit and whether to follow a final symlink.
pub const OP_UTIMENS: u16 = 20;
/// `STATVFS`; no paths, returns a [`StatVfs`] record.
pub const OP_STATVFS: u16 = 21;
/// `LSTAT(path)`; returns the fixed [`Stat`](crate::Stat) record without following a final symlink.
pub const OP_LSTAT: u16 = 22;
/// `MMAP` on the open file; the reply attaches a pager-backed `MemObject`.
pub const OP_MMAP: u16 = 23;

/// `MMAP` flag: the mapping is shared with the file.
pub const MMAP_SHARED: u32 = 1 << 0;

/// Encoded size of a [`StatFs`] payload in bytes.
pub const STATFS_LEN: usize = 36;
/// Encoded size of a [`StatVfs`] payload in bytes (11 64-bit fields).
pub const STATVFS_LEN: usize = 88;

/// UID/GID sentinel meaning "leave this id unchanged" for `chown` operations.
pub const ID_NO_CHANGE: u32 = u32::MAX;

/// `utimensat` flag: set the access time to the current time.
pub const UTIME_NOW: u32 = 1 << 0;
/// `utimensat` flag: leave the access time unchanged.
pub const UTIME_OMIT: u32 = 1 << 1;
/// `utimensat` flag: set the modification time to the current time.
pub const UTIME_MTIME_NOW: u32 = 1 << 2;
/// `utimensat` flag: leave the modification time unchanged.
pub const UTIME_MTIME_OMIT: u32 = 1 << 3;
/// `utimensat` flag: do not follow a final symlink.
pub const UTIME_NOFOLLOW: u32 = 1 << 4;

/// One timestamp update in a [`Call::Utimens`] request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetTime {
    /// Set the timestamp to this many seconds since the Unix epoch.
    Set(i64),
    /// Set the timestamp to the server's current clock value.
    Now,
    /// Leave the timestamp unchanged.
    Omit,
}

impl SetTime {
    const fn flags(self) -> u32 {
        match self {
            Self::Set(_) => 0,
            Self::Now => UTIME_NOW,
            Self::Omit => UTIME_OMIT,
        }
    }

    const fn seconds(self) -> u32 {
        match self {
            Self::Set(seconds) => seconds as u32,
            Self::Now | Self::Omit => 0,
        }
    }
}

/// One typed control operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Call<'a> {
    /// Create a directory with `mode` at `path`.
    Mkdir {
        /// Permission bits for the new directory.
        mode: u32,
        /// Path relative to the directory handle.
        path: &'a [u8],
    },
    /// Remove one file at `path`.
    Unlink {
        /// Path relative to the directory handle.
        path: &'a [u8],
    },
    /// Remove one empty directory at `path`.
    Rmdir {
        /// Path relative to the directory handle.
        path: &'a [u8],
    },
    /// Rename `from` to `to`.
    Rename {
        /// Source path.
        from: &'a [u8],
        /// Destination path.
        to: &'a [u8],
    },
    /// Truncate the open file to `size` bytes.
    Truncate {
        /// New file length.
        size: u64,
    },
    /// Create a symbolic link at `path` pointing to `target`.
    Symlink {
        /// Link path.
        path: &'a [u8],
        /// Link target.
        target: &'a [u8],
    },
    /// Read the target of the symbolic link at `path`.
    Readlink {
        /// Link path.
        path: &'a [u8],
    },
    /// Change permission bits to `mode` at `path`.
    Chmod {
        /// New permission bits.
        mode: u32,
        /// Path relative to the directory handle.
        path: &'a [u8],
    },
    /// Flush pending data to stable storage.
    Sync,
    /// Attach `scheme` at the absolute mount point `path`.
    Mount {
        /// Absolute mount point.
        path: &'a [u8],
        /// Scheme name such as `ramfs:`.
        scheme: &'a [u8],
    },
    /// Detach the mount at the absolute mount point `path`.
    Umount {
        /// Absolute mount point.
        path: &'a [u8],
    },
    /// Read filesystem statistics.
    Statfs,
    /// Check access bits (`R`/`W`/`X`) at `path` for the caller.
    Access {
        /// Requested access bits: 4 read, 2 write, 1 execute.
        mode: u32,
        /// Path relative to the directory handle.
        path: &'a [u8],
    },
    /// Add a hard link at `path` pointing to the existing entry `target`.
    Link {
        /// New link path.
        path: &'a [u8],
        /// Existing path; a final symlink is not followed.
        target: &'a [u8],
    },
    /// Change the owner and group of `path`, following a final symlink.
    Chown {
        /// New user id, or [`ID_NO_CHANGE`].
        uid: u32,
        /// New group id, or [`ID_NO_CHANGE`].
        gid: u32,
        /// Path relative to the directory handle.
        path: &'a [u8],
    },
    /// Change the owner and group of `path`, without following a final symlink.
    Lchown {
        /// New user id, or [`ID_NO_CHANGE`].
        uid: u32,
        /// New group id, or [`ID_NO_CHANGE`].
        gid: u32,
        /// Path relative to the directory handle.
        path: &'a [u8],
    },
    /// Change the owner and group of the open file.
    Fchown {
        /// New user id, or [`ID_NO_CHANGE`].
        uid: u32,
        /// New group id, or [`ID_NO_CHANGE`].
        gid: u32,
    },
    /// Create a FIFO at `path` with permission bits `mode`.
    Mkfifo {
        /// Permission bits for the new FIFO.
        mode: u32,
        /// Path relative to the directory handle.
        path: &'a [u8],
    },
    /// Create a special node at `path`; `mode` carries the file type and permissions.
    Mknod {
        /// File type and permission bits.
        mode: u32,
        /// Device number for character/block nodes.
        dev: u32,
        /// Path relative to the directory handle.
        path: &'a [u8],
    },
    /// Set the access and modification times of `path`.
    Utimens {
        /// Path relative to the directory handle.
        path: &'a [u8],
        /// Access time update.
        atime: SetTime,
        /// Modification time update.
        mtime: SetTime,
        /// Whether to follow a final symlink.
        follow: bool,
    },
    /// Read POSIX filesystem statistics for the open file's mount.
    Statvfs,
    /// Read one path's metadata without following a final symlink.
    Lstat {
        /// Path relative to the directory handle.
        path: &'a [u8],
    },
    /// Map a range of the open file through a pager-backed memory object.
    Mmap {
        /// File offset; page-aligned.
        offset: u64,
        /// Mapping length in bytes; page-aligned.
        len: u64,
        /// Whether the mapping is shared with the file.
        shared: bool,
    },
}

impl Call<'_> {
    /// Wire operation code.
    pub const fn op(&self) -> u16 {
        match self {
            Self::Mkdir { .. } => OP_MKDIR,
            Self::Unlink { .. } => OP_UNLINK,
            Self::Rmdir { .. } => OP_RMDIR,
            Self::Rename { .. } => OP_RENAME,
            Self::Truncate { .. } => OP_TRUNCATE,
            Self::Symlink { .. } => OP_SYMLINK,
            Self::Readlink { .. } => OP_READLINK,
            Self::Chmod { .. } => OP_CHMOD,
            Self::Sync => OP_SYNC,
            Self::Mount { .. } => OP_MOUNT,
            Self::Umount { .. } => OP_UMOUNT,
            Self::Statfs => OP_STATFS,
            Self::Access { .. } => OP_ACCESS,
            Self::Link { .. } => OP_LINK,
            Self::Chown { .. } => OP_CHOWN,
            Self::Lchown { .. } => OP_LCHOWN,
            Self::Fchown { .. } => OP_FCHOWN,
            Self::Mkfifo { .. } => OP_MKFIFO,
            Self::Mknod { .. } => OP_MKNOD,
            Self::Utimens { .. } => OP_UTIMENS,
            Self::Statvfs => OP_STATVFS,
            Self::Lstat { .. } => OP_LSTAT,
            Self::Mmap { .. } => OP_MMAP,
        }
    }

    /// `arg0` and `arg1` for this operation.
    const fn args(&self) -> (u32, u32) {
        match self {
            Self::Mkdir { mode, .. } | Self::Chmod { mode, .. } => (*mode, 0),
            Self::Access { mode, .. } => (*mode, 0),
            Self::Truncate { size } => (*size as u32, (*size >> 32) as u32),
            Self::Mmap { offset, .. } => (*offset as u32, (*offset >> 32) as u32),
            Self::Chown { uid, gid, .. }
            | Self::Lchown { uid, gid, .. }
            | Self::Fchown { uid, gid } => (*uid, *gid),
            Self::Mkfifo { mode, .. } => (*mode, 0),
            Self::Mknod { mode, dev, .. } => (*mode, *dev),
            Self::Utimens { atime, mtime, .. } => (atime.seconds(), mtime.seconds()),
            _ => (0, 0),
        }
    }

    /// Operation flags; zero for every operation except `UTIMENS`.
    const fn flags(&self) -> u32 {
        match self {
            Self::Utimens {
                atime,
                mtime,
                follow,
                ..
            } => {
                let mut flags = atime.flags() | (mtime.flags() << 2);
                if !*follow {
                    flags |= UTIME_NOFOLLOW;
                }
                flags
            }
            _ => 0,
        }
    }

    /// The paths carried by this operation, in wire order.
    const fn paths(&self) -> (Option<&[u8]>, Option<&[u8]>) {
        match self {
            Self::Mkdir { path, .. }
            | Self::Unlink { path }
            | Self::Rmdir { path }
            | Self::Readlink { path }
            | Self::Chmod { path, .. }
            | Self::Umount { path } => (Some(path), None),
            Self::Access { path, .. }
            | Self::Chown { path, .. }
            | Self::Lchown { path, .. }
            | Self::Mkfifo { path, .. }
            | Self::Mknod { path, .. }
            | Self::Utimens { path, .. }
            | Self::Lstat { path } => (Some(path), None),
            Self::Rename { from, to } => (Some(from), Some(to)),
            Self::Link { path, target } => (Some(path), Some(target)),
            Self::Symlink { path, target } => (Some(path), Some(target)),
            Self::Mount { path, scheme } => (Some(path), Some(scheme)),
            Self::Truncate { .. }
            | Self::Sync
            | Self::Statfs
            | Self::Fchown { .. }
            | Self::Statvfs
            | Self::Mmap { .. } => (None, None),
        }
    }
}

/// One control request: the typed operation plus its reserved `flags` word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CallRequest<'a> {
    /// Reserved flags word; send zero in this protocol version.
    pub flags: u32,
    /// Typed operation.
    pub call: Call<'a>,
}

impl<'a> CallRequest<'a> {
    /// Build a request with zero flags.
    pub const fn new(call: Call<'a>) -> Self {
        Self { flags: 0, call }
    }

    /// The operation code.
    pub const fn op(&self) -> u16 {
        self.call.op()
    }

    /// Encode the request into `out`; returns the number of bytes written.
    pub fn encode(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        let (arg0, arg1) = self.call.args();
        let flags = self.flags | self.call.flags();
        let mut writer = Writer::new(out);
        writer.put_u16(crate::VERSION)?;
        writer.put_u16(self.call.op())?;
        writer.put_u32(flags)?;
        writer.put_u32(arg0)?;
        writer.put_u32(arg1)?;
        if let Call::Mmap { len, shared, .. } = &self.call {
            writer.put_u64(*len)?;
            writer.put_u32(if *shared { MMAP_SHARED } else { 0 })?;
        }
        let (first, second) = self.call.paths();
        if let Some(path) = first {
            write_path(&mut writer, path)?;
        }
        if let Some(path) = second {
            write_path(&mut writer, path)?;
        }
        Ok(writer.position())
    }

    /// Decode one complete control request.
    pub fn decode(input: &'a [u8]) -> Result<Self, ProtoError> {
        let mut reader = Reader::new(input);
        let version = reader.get_u16()?;
        if version != crate::VERSION {
            return Err(ProtoError::UnsupportedVersion);
        }
        let op = reader.get_u16()?;
        let flags = reader.get_u32()?;
        let arg0 = reader.get_u32()?;
        let arg1 = reader.get_u32()?;
        let (mmap_len, mmap_flags) = if op == OP_MMAP {
            (reader.get_u64()?, reader.get_u32()?)
        } else {
            (0, 0)
        };
        let path_count = path_count(op).ok_or(ProtoError::UnknownOp(op))?;
        let first = if path_count >= 1 {
            Some(read_path(&mut reader)?)
        } else {
            None
        };
        let second = if path_count >= 2 {
            Some(read_path(&mut reader)?)
        } else {
            None
        };
        if !reader.is_empty() {
            return Err(ProtoError::TrailingBytes);
        }
        let call = match op {
            OP_MKDIR => Call::Mkdir {
                mode: arg0,
                path: first.ok_or(ProtoError::Truncated)?,
            },
            OP_UNLINK => Call::Unlink {
                path: first.ok_or(ProtoError::Truncated)?,
            },
            OP_RMDIR => Call::Rmdir {
                path: first.ok_or(ProtoError::Truncated)?,
            },
            OP_RENAME => Call::Rename {
                from: first.ok_or(ProtoError::Truncated)?,
                to: second.ok_or(ProtoError::Truncated)?,
            },
            OP_TRUNCATE => Call::Truncate {
                size: u64::from(arg0) | (u64::from(arg1) << 32),
            },
            OP_SYMLINK => Call::Symlink {
                path: first.ok_or(ProtoError::Truncated)?,
                target: second.ok_or(ProtoError::Truncated)?,
            },
            OP_READLINK => Call::Readlink {
                path: first.ok_or(ProtoError::Truncated)?,
            },
            OP_CHMOD => Call::Chmod {
                mode: arg0,
                path: first.ok_or(ProtoError::Truncated)?,
            },
            OP_SYNC => Call::Sync,
            OP_MOUNT => Call::Mount {
                path: first.ok_or(ProtoError::Truncated)?,
                scheme: second.ok_or(ProtoError::Truncated)?,
            },
            OP_UMOUNT => Call::Umount {
                path: first.ok_or(ProtoError::Truncated)?,
            },
            OP_STATFS => Call::Statfs,
            OP_ACCESS => Call::Access {
                mode: arg0,
                path: first.ok_or(ProtoError::Truncated)?,
            },
            OP_LINK => Call::Link {
                path: first.ok_or(ProtoError::Truncated)?,
                target: second.ok_or(ProtoError::Truncated)?,
            },
            OP_CHOWN => Call::Chown {
                uid: arg0,
                gid: arg1,
                path: first.ok_or(ProtoError::Truncated)?,
            },
            OP_LCHOWN => Call::Lchown {
                uid: arg0,
                gid: arg1,
                path: first.ok_or(ProtoError::Truncated)?,
            },
            OP_FCHOWN => Call::Fchown {
                uid: arg0,
                gid: arg1,
            },
            OP_MKFIFO => Call::Mkfifo {
                mode: arg0,
                path: first.ok_or(ProtoError::Truncated)?,
            },
            OP_MKNOD => Call::Mknod {
                mode: arg0,
                dev: arg1,
                path: first.ok_or(ProtoError::Truncated)?,
            },
            OP_UTIMENS => {
                let allowed =
                    UTIME_NOW | UTIME_OMIT | UTIME_MTIME_NOW | UTIME_MTIME_OMIT | UTIME_NOFOLLOW;
                if flags & !allowed != 0
                    || flags & UTIME_NOW != 0 && flags & UTIME_OMIT != 0
                    || flags & UTIME_MTIME_NOW != 0 && flags & UTIME_MTIME_OMIT != 0
                {
                    return Err(ProtoError::Malformed);
                }
                let atime = if flags & UTIME_NOW != 0 {
                    SetTime::Now
                } else if flags & UTIME_OMIT != 0 {
                    SetTime::Omit
                } else {
                    SetTime::Set(i64::from(arg0 as i32))
                };
                let mtime = if flags & UTIME_MTIME_NOW != 0 {
                    SetTime::Now
                } else if flags & UTIME_MTIME_OMIT != 0 {
                    SetTime::Omit
                } else {
                    SetTime::Set(i64::from(arg1 as i32))
                };
                Call::Utimens {
                    path: first.ok_or(ProtoError::Truncated)?,
                    atime,
                    mtime,
                    follow: flags & UTIME_NOFOLLOW == 0,
                }
            }
            OP_STATVFS => Call::Statvfs,
            OP_LSTAT => Call::Lstat {
                path: first.ok_or(ProtoError::Truncated)?,
            },
            OP_MMAP => {
                if mmap_flags & !MMAP_SHARED != 0 {
                    return Err(ProtoError::Malformed);
                }
                Call::Mmap {
                    offset: u64::from(arg0) | (u64::from(arg1) << 32),
                    len: mmap_len,
                    shared: mmap_flags & MMAP_SHARED != 0,
                }
            }
            _ => return Err(ProtoError::UnknownOp(op)),
        };
        Ok(Self { flags, call })
    }
}

/// Filesystem statistics payload (`STATFS` success).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StatFs {
    /// Preferred block size in bytes.
    pub block_size: u32,
    /// Total blocks.
    pub blocks: u64,
    /// Free blocks.
    pub free_blocks: u64,
    /// Total inodes.
    pub files: u64,
    /// Free inodes.
    pub free_files: u64,
}

impl StatFs {
    /// Encode the 36-byte payload.
    pub fn encode(&self, out: &mut [u8; STATFS_LEN]) -> Result<(), ProtoError> {
        let mut writer = Writer::new(out);
        writer.put_u32(self.block_size)?;
        writer.put_u64(self.blocks)?;
        writer.put_u64(self.free_blocks)?;
        writer.put_u64(self.files)?;
        writer.put_u64(self.free_files)?;
        Ok(())
    }

    /// Decode exactly one 36-byte payload.
    pub fn decode(input: &[u8]) -> Result<Self, ProtoError> {
        if input.len() != STATFS_LEN {
            return Err(if input.len() < STATFS_LEN {
                ProtoError::Truncated
            } else {
                ProtoError::TrailingBytes
            });
        }
        let mut reader = Reader::new(input);
        Ok(Self {
            block_size: reader.get_u32()?,
            blocks: reader.get_u64()?,
            free_blocks: reader.get_u64()?,
            files: reader.get_u64()?,
            free_files: reader.get_u64()?,
        })
    }
}

/// POSIX `statvfs` payload (`STATVFS` success).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StatVfs {
    /// Filesystem block size.
    pub block_size: u64,
    /// Fundamental fragment size.
    pub fragment_size: u64,
    /// Total blocks.
    pub blocks: u64,
    /// Free blocks.
    pub free_blocks: u64,
    /// Free blocks available to an unprivileged caller.
    pub available_blocks: u64,
    /// Total file nodes.
    pub files: u64,
    /// Free file nodes.
    pub free_files: u64,
    /// Free file nodes available to an unprivileged caller.
    pub available_files: u64,
    /// Filesystem id.
    pub fsid: u64,
    /// Mount flags.
    pub flags: u64,
    /// Maximum filename length.
    pub name_max: u64,
}

impl StatVfs {
    /// Encode the 88-byte payload.
    pub fn encode(&self, out: &mut [u8; STATVFS_LEN]) -> Result<(), ProtoError> {
        let mut writer = Writer::new(out);
        writer.put_u64(self.block_size)?;
        writer.put_u64(self.fragment_size)?;
        writer.put_u64(self.blocks)?;
        writer.put_u64(self.free_blocks)?;
        writer.put_u64(self.available_blocks)?;
        writer.put_u64(self.files)?;
        writer.put_u64(self.free_files)?;
        writer.put_u64(self.available_files)?;
        writer.put_u64(self.fsid)?;
        writer.put_u64(self.flags)?;
        writer.put_u64(self.name_max)?;
        Ok(())
    }

    /// Decode exactly one 88-byte payload.
    pub fn decode(input: &[u8]) -> Result<Self, ProtoError> {
        if input.len() != STATVFS_LEN {
            return Err(if input.len() < STATVFS_LEN {
                ProtoError::Truncated
            } else {
                ProtoError::TrailingBytes
            });
        }
        let mut reader = Reader::new(input);
        Ok(Self {
            block_size: reader.get_u64()?,
            fragment_size: reader.get_u64()?,
            blocks: reader.get_u64()?,
            free_blocks: reader.get_u64()?,
            available_blocks: reader.get_u64()?,
            files: reader.get_u64()?,
            free_files: reader.get_u64()?,
            available_files: reader.get_u64()?,
            fsid: reader.get_u64()?,
            flags: reader.get_u64()?,
            name_max: reader.get_u64()?,
        })
    }
}

impl From<StatFs> for StatVfs {
    fn from(value: StatFs) -> Self {
        Self {
            block_size: u64::from(value.block_size),
            fragment_size: u64::from(value.block_size),
            blocks: value.blocks,
            free_blocks: value.free_blocks,
            available_blocks: value.free_blocks,
            files: value.files,
            free_files: value.free_files,
            available_files: value.free_files,
            fsid: 0,
            flags: 0,
            name_max: 255,
        }
    }
}

/// One control response.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallResponse<'a> {
    /// Success or failure without a payload.
    Status(i32),
    /// `READLINK` success with the link target.
    Readlink {
        /// Zero on success.
        status: i32,
        /// Link target bytes.
        target: &'a [u8],
    },
    /// `STATFS` success with the filesystem record.
    StatFs {
        /// Zero on success.
        status: i32,
        /// Filesystem statistics.
        statfs: StatFs,
    },
    /// `STATVFS` success with the POSIX filesystem record.
    StatVfs {
        /// Zero on success.
        status: i32,
        /// POSIX filesystem statistics.
        statvfs: StatVfs,
    },
    /// `LSTAT` success with the fixed metadata record.
    Stat {
        /// Zero on success.
        status: i32,
        /// Metadata for the path.
        stat: Stat,
    },
}

impl<'a> CallResponse<'a> {
    /// Response status (zero or a negative errno).
    pub const fn status(&self) -> i32 {
        match self {
            Self::Status(status)
            | Self::Readlink { status, .. }
            | Self::StatFs { status, .. }
            | Self::StatVfs { status, .. }
            | Self::Stat { status, .. } => *status,
        }
    }

    /// Encode the response into `out`; returns the number of bytes written.
    pub fn encode(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        validate_status(self.status())?;
        let mut writer = Writer::new(out);
        writer.put_i32(self.status())?;
        match self {
            Self::Status(_) => {}
            Self::Readlink { target, .. } => {
                validate_path(target)?;
                writer.put_u16(
                    u16::try_from(target.len()).map_err(|_| ProtoError::LengthOutOfRange)?,
                )?;
                writer.put_bytes(target)?;
            }
            Self::StatFs { statfs, .. } => {
                writer.put_u32(statfs.block_size)?;
                writer.put_u64(statfs.blocks)?;
                writer.put_u64(statfs.free_blocks)?;
                writer.put_u64(statfs.files)?;
                writer.put_u64(statfs.free_files)?;
            }
            Self::StatVfs { statvfs, .. } => {
                let mut payload = [0u8; STATVFS_LEN];
                statvfs.encode(&mut payload)?;
                writer.put_bytes(&payload)?;
            }
            Self::Stat { stat, .. } => {
                let mut payload = [0u8; STAT_LEN];
                stat.encode(&mut payload)?;
                writer.put_bytes(&payload)?;
            }
        }
        Ok(writer.position())
    }

    /// Decode one response for the request operation `op`.
    pub fn decode(op: u16, input: &'a [u8]) -> Result<Self, ProtoError> {
        let mut reader = Reader::new(input);
        let status = reader.get_i32()?;
        validate_status(status)?;
        if status != 0 {
            if !reader.is_empty() {
                return Err(ProtoError::TrailingBytes);
            }
            return Ok(Self::Status(status));
        }
        match op {
            OP_READLINK => {
                let length = reader.get_u16()? as usize;
                if length == 0 || length > MAX_PATH {
                    return Err(ProtoError::LengthOutOfRange);
                }
                let target = reader.get_bytes(length)?;
                validate_path(target)?;
                if !reader.is_empty() {
                    return Err(ProtoError::TrailingBytes);
                }
                Ok(Self::Readlink { status, target })
            }
            OP_STATFS => {
                let bytes = reader.get_bytes(STATFS_LEN)?;
                if !reader.is_empty() {
                    return Err(ProtoError::TrailingBytes);
                }
                Ok(Self::StatFs {
                    status,
                    statfs: StatFs::decode(bytes)?,
                })
            }
            OP_STATVFS => {
                let bytes = reader.get_bytes(STATVFS_LEN)?;
                if !reader.is_empty() {
                    return Err(ProtoError::TrailingBytes);
                }
                Ok(Self::StatVfs {
                    status,
                    statvfs: StatVfs::decode(bytes)?,
                })
            }
            OP_LSTAT => {
                let bytes = reader.get_bytes(STAT_LEN)?;
                if !reader.is_empty() {
                    return Err(ProtoError::TrailingBytes);
                }
                Ok(Self::Stat {
                    status,
                    stat: Stat::decode(bytes)?,
                })
            }
            OP_MKDIR | OP_UNLINK | OP_RMDIR | OP_RENAME | OP_TRUNCATE | OP_SYMLINK | OP_CHMOD
            | OP_SYNC | OP_MOUNT | OP_UMOUNT | OP_ACCESS | OP_LINK | OP_CHOWN | OP_LCHOWN
            | OP_FCHOWN | OP_MKFIFO | OP_MKNOD | OP_UTIMENS => {
                if !reader.is_empty() {
                    return Err(ProtoError::TrailingBytes);
                }
                Ok(Self::Status(status))
            }
            _ => Err(ProtoError::UnknownOp(op)),
        }
    }
}

const fn path_count(op: u16) -> Option<usize> {
    match op {
        OP_MKDIR | OP_UNLINK | OP_RMDIR | OP_READLINK | OP_CHMOD | OP_UMOUNT | OP_ACCESS
        | OP_CHOWN | OP_LCHOWN | OP_MKFIFO | OP_MKNOD | OP_UTIMENS | OP_LSTAT => Some(1),
        OP_RENAME | OP_SYMLINK | OP_MOUNT | OP_LINK => Some(2),
        OP_TRUNCATE | OP_SYNC | OP_STATFS | OP_FCHOWN | OP_STATVFS | OP_MMAP => Some(0),
        _ => None,
    }
}

fn write_path(writer: &mut Writer<'_>, path: &[u8]) -> Result<(), ProtoError> {
    validate_path(path)?;
    writer.put_u16(u16::try_from(path.len()).map_err(|_| ProtoError::LengthOutOfRange)?)?;
    writer.put_bytes(path)?;
    Ok(())
}

fn read_path<'a>(reader: &mut Reader<'a>) -> Result<&'a [u8], ProtoError> {
    let length = reader.get_u16()? as usize;
    if length == 0 || length > MAX_PATH {
        return Err(ProtoError::LengthOutOfRange);
    }
    let path = reader.get_bytes(length)?;
    validate_path(path)?;
    Ok(path)
}

fn validate_path(path: &[u8]) -> Result<(), ProtoError> {
    if path.is_empty() || path.len() > MAX_PATH {
        return Err(ProtoError::LengthOutOfRange);
    }
    if path.contains(&0) {
        return Err(ProtoError::Malformed);
    }
    Ok(())
}

const fn validate_status(status: i32) -> Result<(), ProtoError> {
    if status <= 0 && status >= -4095 {
        Ok(())
    } else {
        Err(ProtoError::BadStatus)
    }
}

#[cfg(test)]
#[path = "call_ut.rs"]
mod tests;
