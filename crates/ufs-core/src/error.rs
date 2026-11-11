// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Error types shared by the core.

use core::fmt;

/// An opaque block-device error.
///
/// The core does not need to know why a read failed; the host frontend maps its own error type
/// to this marker and maps it back if it needs the detail.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IoError;

/// Why a filesystem operation failed.
///
/// The variants carry a short static description where the caller may need to distinguish
/// specific validation failures in tests or logs. No variant carries host-specific data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UfsError {
    /// The block device reported a read or write error.
    Io,
    /// The superblock is missing or internally inconsistent.
    BadSuperblock(&'static str),
    /// A valid but unsupported on-disk format was found.
    Unsupported(&'static str),
    /// On-disk data is structurally invalid.
    Corrupt(&'static str),
    /// The requested name or inode does not exist.
    NotFound,
    /// A directory entry with this name already exists.
    Exists,
    /// A directory still contains entries other than `.` and `..`.
    NotEmpty,
    /// A path component was expected to name a directory.
    NotDir,
    /// A file operation was attempted on a directory.
    IsDir,
    /// A name or symlink target does not fit the supplied buffer or the on-disk limit.
    NameTooLong,
    /// The operation or argument is not valid for this object.
    Invalid,
    /// The operation needs a writable mount; this core is read-only.
    ReadOnly,
    /// The filesystem has no free block, fragment or inode.
    NoSpace,
}

impl fmt::Display for UfsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io => f.write_str("block device I/O error"),
            Self::BadSuperblock(reason) => write!(f, "bad superblock: {reason}"),
            Self::Unsupported(reason) => write!(f, "unsupported: {reason}"),
            Self::Corrupt(reason) => write!(f, "corrupt filesystem: {reason}"),
            Self::NotFound => f.write_str("not found"),
            Self::Exists => f.write_str("file exists"),
            Self::NotEmpty => f.write_str("directory not empty"),
            Self::NotDir => f.write_str("not a directory"),
            Self::IsDir => f.write_str("is a directory"),
            Self::NameTooLong => f.write_str("name too long"),
            Self::Invalid => f.write_str("invalid argument"),
            Self::ReadOnly => f.write_str("read-only filesystem"),
            Self::NoSpace => f.write_str("no space left"),
        }
    }
}

impl From<IoError> for UfsError {
    fn from(_: IoError) -> Self {
        Self::Io
    }
}
