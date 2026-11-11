// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Shared file protocol for `initfsd`, `ramfsd`, `vfsd` and later `fsd-ufs`.
//!
//! This crate is the typed layer above the S11 scheme packets
//! (`architecture/03-system-servers.md` section 2, `architecture/16-first-shell.md` section 3).
//! It defines only what travels inside a captured request or response:
//!
//! * [`Stat`] - the fixed 64-byte `FSTAT` record;
//! * [`DirEntry`] and [`DirEntryHeader`] - packed records returned by `READ` on a directory handle;
//! * [`CallRequest`] / [`Call`] and [`CallResponse`] - the versioned control protocol carried by
//!   the S11 `CALL` opcode;
//! * [`normalize_absolute`] and [`split_first_component`] - allocation-free path helpers;
//! * the Linux-valued open, mode and directory-entry constants in [`flags`].
//!
//! The crate is `no_std`, has one dependency (`raven-abi` for errno values), forbids `unsafe`,
//! never allocates while encoding and never panics while decoding. Every integer is written with
//! explicit little-endian `to_le_bytes` calls. Every length is checked against a documented bound
//! before use. See `README.md` for the byte tables that match the golden tests.
//!
//! `OPEN`, `READ`, `WRITE`, `CLOSE`, `DUP` and `FSTAT` keep the S11 opcodes. `OPEN` carries a path
//! relative to the server root (at most 255 bytes) and the validated [`OpenFlags`] wire bit set.
//! `READ` and `WRITE` are sequential per open-file description; a nonzero wire offset is invalid.
//! `READ` on a directory returns [`DirEntry`] records. `FSTAT` returns a [`Stat`] record. All path
//! control operations use [`CallRequest`]. The authoritative contract is
//! `docs/design/fs-contract.md`.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod call;
pub mod dirent;
pub mod flags;
pub mod open;
pub mod path;
pub mod stat;
pub mod wire;

use core::fmt;

pub use call::{
    CALL_HEADER_LEN, Call, CallRequest, CallResponse, MAX_CALL_PATHS, OP_CHMOD, OP_MKDIR, OP_MOUNT,
    OP_READLINK, OP_RENAME, OP_RMDIR, OP_STATFS, OP_SYMLINK, OP_SYNC, OP_TRUNCATE, OP_UMOUNT,
    OP_UNLINK, STATFS_LEN, StatFs,
};
pub use dirent::{
    DIRENT_ALIGN, DIRENT_HEADER_LEN, DirCookie, DirEntry, DirEntryHeader, DirEntryIter, MAX_NAME,
};
pub use flags::{
    DT_BLK, DT_CHR, DT_DIR, DT_FIFO, DT_LNK, DT_REG, DT_SOCK, DT_UNKNOWN, O_ACCMODE, O_APPEND,
    O_CREAT, O_DIRECTORY, O_EXCL, O_NOFOLLOW, O_RDONLY, O_RDWR, O_TRUNC, O_WRONLY, OPEN_FLAGS_MASK,
    S_IFBLK, S_IFCHR, S_IFDIR, S_IFIFO, S_IFLNK, S_IFMT, S_IFREG, S_IFSOCK, dirent_kind,
};
pub use open::{
    CREATE_FILE_MODE, DEFAULT_UMASK, OpenFlags, WIRE_OPEN_APPEND, WIRE_OPEN_CREATE,
    WIRE_OPEN_DIRECTORY, WIRE_OPEN_EXCL, WIRE_OPEN_MASK, WIRE_OPEN_NOFOLLOW, WIRE_OPEN_READ,
    WIRE_OPEN_TRUNC, WIRE_OPEN_WRITE,
};
pub use path::{
    MAX_COMPONENT, MAX_PATH, NormalizedPath, PathError, normalize_absolute, normalize_request,
    split_first_component,
};
pub use raven_abi::errno::Errno;
pub use stat::{STAT_LEN, Stat};
pub use wire::{Reader, WireError, Writer};

/// Version of the filesystem control protocol.
pub const VERSION: u16 = 1;

/// Alias used by crates that name the version after the protocol.
pub const PROTOCOL_VERSION: u16 = VERSION;

/// Why a filesystem message was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProtoError {
    /// The input ended before the message did.
    Truncated,
    /// The input continues after the message ended.
    TrailingBytes,
    /// The first field is a version this crate does not speak.
    UnsupportedVersion,
    /// The operation code is unknown.
    UnknownOp(u16),
    /// A length or count is outside its documented bound.
    LengthOutOfRange,
    /// A field is well formed but violates an invariant.
    Malformed,
    /// A name or path is not valid UTF-8.
    InvalidUtf8,
    /// A directory entry name is empty or contains `/` or NUL.
    InvalidName,
    /// A response status is outside the documented errno range.
    BadStatus,
    /// Open flags contain unknown bits or an invalid combination.
    InvalidOpenFlags,
}

impl ProtoError {
    /// The Linux errno a server should return for this failure.
    ///
    /// An unknown operation or version is `ENOSYS`, as the protocol requires; framing errors are
    /// `EPROTO`; malformed fields and names are `EINVAL`.
    pub const fn errno(self) -> Errno {
        match self {
            Self::UnsupportedVersion | Self::UnknownOp(_) => Errno::ENOSYS,
            Self::Truncated | Self::TrailingBytes | Self::Malformed | Self::BadStatus => {
                Errno::EPROTO
            }
            Self::LengthOutOfRange
            | Self::InvalidUtf8
            | Self::InvalidName
            | Self::InvalidOpenFlags => Errno::EINVAL,
        }
    }
}

impl From<WireError> for ProtoError {
    fn from(error: WireError) -> Self {
        match error {
            WireError::Truncated => Self::Truncated,
            WireError::TooLong => Self::LengthOutOfRange,
            WireError::Invalid => Self::Malformed,
        }
    }
}

impl fmt::Display for ProtoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated => formatter.write_str("truncated message"),
            Self::TrailingBytes => formatter.write_str("trailing bytes after the message"),
            Self::UnsupportedVersion => formatter.write_str("unsupported protocol version"),
            Self::UnknownOp(op) => write!(formatter, "unknown operation {op}"),
            Self::LengthOutOfRange => formatter.write_str("length or count out of range"),
            Self::Malformed => formatter.write_str("malformed field"),
            Self::InvalidUtf8 => formatter.write_str("name or path is not UTF-8"),
            Self::InvalidName => formatter.write_str("invalid directory entry name"),
            Self::BadStatus => formatter.write_str("status outside the errno range"),
            Self::InvalidOpenFlags => formatter.write_str("invalid open flags"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_errno_mapping_is_stable() {
        assert_eq!(ProtoError::UnknownOp(99).errno(), Errno::ENOSYS);
        assert_eq!(ProtoError::UnsupportedVersion.errno(), Errno::ENOSYS);
        assert_eq!(ProtoError::Truncated.errno(), Errno::EPROTO);
        assert_eq!(ProtoError::InvalidName.errno(), Errno::EINVAL);
        assert_eq!(ProtoError::LengthOutOfRange.errno(), Errno::EINVAL);
        assert_eq!(ProtoError::InvalidOpenFlags.errno(), Errno::EINVAL);
    }

    #[test]
    fn version_is_one() {
        assert_eq!(VERSION, 1);
        assert_eq!(PROTOCOL_VERSION, VERSION);
    }
}
