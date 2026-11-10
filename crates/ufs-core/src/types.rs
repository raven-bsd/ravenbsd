// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Public value types of the read-only API.

use alloc_crate::vec::Vec;

/// An inode number.
pub type Ino = u32;

/// The kind of a filesystem object, independent of its permission bits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileType {
    /// A regular file.
    Regular,
    /// A directory.
    Directory,
    /// A symbolic link.
    Symlink,
    /// A character device node.
    CharDevice,
    /// A block device node.
    BlockDevice,
    /// A FIFO (named pipe).
    Fifo,
    /// A Unix-domain socket node.
    Socket,
}

/// A timestamp with nanosecond precision.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Timespec {
    /// Seconds since the Unix epoch.
    pub sec: i64,
    /// Nanoseconds within the second, in `0..1_000_000_000`.
    pub nsec: u32,
}

/// Inode attributes returned by [`crate::Ufs::getattr`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Attr {
    /// Inode number.
    pub ino: Ino,
    /// Object kind.
    pub kind: FileType,
    /// Permission and set-id bits (`0o7777`), without the file-type bits.
    pub mode: u16,
    /// Hard-link count.
    pub nlink: u32,
    /// Owner user id.
    pub uid: u32,
    /// Owner group id.
    pub gid: u32,
    /// File size in bytes.
    pub size: u64,
    /// Number of 512-byte blocks actually held, as stored in the inode.
    pub blocks: u64,
    /// Last access time.
    pub atime: Timespec,
    /// Last modification time.
    pub mtime: Timespec,
    /// Last inode-change time.
    pub ctime: Timespec,
    /// Device number for character and block devices, otherwise zero.
    pub rdev: u64,
    /// Inode generation number.
    pub r#gen: u32,
    /// Inode flags (UFS `di_flags`).
    pub flags: u32,
}

/// One directory entry returned by [`crate::Ufs::readdir`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirEntry {
    /// Inode number of the entry.
    pub ino: Ino,
    /// Object kind, from the on-disk `d_type` or the inode when the type is unknown.
    pub kind: FileType,
    /// Entry name without a terminating NUL.
    pub name: Vec<u8>,
}

/// Filesystem statistics returned by [`crate::Ufs::statfs`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StatFs {
    /// Fundamental block size in bytes.
    pub block_size: u32,
    /// Fragment size in bytes.
    pub fragment_size: u32,
    /// Total data fragments in the filesystem.
    pub blocks: u64,
    /// Free fragments.
    pub blocks_free: u64,
    /// Total inode slots minus the reserved root inode.
    pub files: u64,
    /// Free inodes.
    pub files_free: u64,
}

/// Options for [`crate::Ufs::mount`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MountOptions {
    /// K51 accepts only read-only mounts.
    pub read_only: bool,
}
