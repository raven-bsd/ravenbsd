// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Open flags on the filesystem wire.
//!
//! The S11 `OPEN` packet has a 16-bit `flags` field.  It does **not** carry Linux
//! `O_*` values directly: [`OpenFlags`] defines the compact wire bit set, and
//! [`OpenFlags::from_linux`] / [`OpenFlags::to_linux`] translate it for client
//! APIs and tests.
//!
//! | Bit | Wire name | Meaning |
//! |---|---|---|
//! | 0 | `WIRE_OPEN_READ` | read access (`O_RDONLY` or `O_RDWR`) |
//! | 1 | `WIRE_OPEN_WRITE` | write access (`O_WRONLY` or `O_RDWR`) |
//! | 2 | `WIRE_OPEN_CREATE` | create a missing file (`O_CREAT`) |
//! | 3 | `WIRE_OPEN_EXCL` | fail if it already exists (`O_EXCL`) |
//! | 4 | `WIRE_OPEN_TRUNC` | truncate on open (`O_TRUNC`) |
//! | 5 | `WIRE_OPEN_APPEND` | write at the end (`O_APPEND`) |
//! | 6 | `WIRE_OPEN_DIRECTORY` | require a directory (`O_DIRECTORY`) |
//! | 7 | `WIRE_OPEN_NOFOLLOW` | do not follow a final symlink (`O_NOFOLLOW`) |
//!
//! At least one of READ or WRITE is required.  EXCL requires CREATE, TRUNC and
//! APPEND require WRITE, and CREATE and TRUNC cannot be combined with DIRECTORY.
//! Unknown bits and invalid combinations are rejected with
//! [`ProtoError::InvalidOpenFlags`] (`EINVAL`).
//!
//! `O_CREAT` uses [`CREATE_FILE_MODE`] masked by [`DEFAULT_UMASK`].  Protocol
//! version 1 has no per-client umask or mode on the wire; a client that needs a
//! different mode calls `CHMOD` after the create.

use crate::ProtoError;
use crate::flags::{
    O_ACCMODE, O_APPEND, O_CREAT, O_DIRECTORY, O_EXCL, O_NOFOLLOW, O_RDONLY, O_RDWR, O_TRUNC,
    O_WRONLY,
};

/// Wire flag: read access.
pub const WIRE_OPEN_READ: u16 = 1 << 0;
/// Wire flag: write access.
pub const WIRE_OPEN_WRITE: u16 = 1 << 1;
/// Wire flag: create a missing file.
pub const WIRE_OPEN_CREATE: u16 = 1 << 2;
/// Wire flag: fail when the file already exists.
pub const WIRE_OPEN_EXCL: u16 = 1 << 3;
/// Wire flag: truncate the file on open.
pub const WIRE_OPEN_TRUNC: u16 = 1 << 4;
/// Wire flag: append writes at the end of the file.
pub const WIRE_OPEN_APPEND: u16 = 1 << 5;
/// Wire flag: require the path to name a directory.
pub const WIRE_OPEN_DIRECTORY: u16 = 1 << 6;
/// Wire flag: do not follow a final symbolic link.
pub const WIRE_OPEN_NOFOLLOW: u16 = 1 << 7;

/// Mask of every wire open flag defined by protocol version 1.
pub const WIRE_OPEN_MASK: u16 = WIRE_OPEN_READ
    | WIRE_OPEN_WRITE
    | WIRE_OPEN_CREATE
    | WIRE_OPEN_EXCL
    | WIRE_OPEN_TRUNC
    | WIRE_OPEN_APPEND
    | WIRE_OPEN_DIRECTORY
    | WIRE_OPEN_NOFOLLOW;

/// Permission bits requested for a file created by `O_CREAT`.
pub const CREATE_FILE_MODE: u32 = 0o666;
/// Server-side umask applied to [`CREATE_FILE_MODE`] in protocol version 1.
pub const DEFAULT_UMASK: u32 = 0o022;

/// Validated open flags in wire form.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OpenFlags(u16);

impl OpenFlags {
    /// Read-only access (`O_RDONLY`).
    pub const READ: Self = Self(WIRE_OPEN_READ);
    /// Write-only access (`O_WRONLY`).
    pub const WRITE: Self = Self(WIRE_OPEN_WRITE);
    /// Read-write access (`O_RDWR`).
    pub const READ_WRITE: Self = Self(WIRE_OPEN_READ | WIRE_OPEN_WRITE);

    /// Validate a raw wire value.
    pub const fn from_wire(raw: u16) -> Result<Self, ProtoError> {
        if raw & !WIRE_OPEN_MASK != 0 {
            return Err(ProtoError::InvalidOpenFlags);
        }
        let readable = raw & WIRE_OPEN_READ != 0;
        let writable = raw & WIRE_OPEN_WRITE != 0;
        if !readable && !writable {
            return Err(ProtoError::InvalidOpenFlags);
        }
        if raw & WIRE_OPEN_EXCL != 0 && raw & WIRE_OPEN_CREATE == 0 {
            return Err(ProtoError::InvalidOpenFlags);
        }
        if raw & (WIRE_OPEN_TRUNC | WIRE_OPEN_APPEND) != 0 && !writable {
            return Err(ProtoError::InvalidOpenFlags);
        }
        if raw & WIRE_OPEN_DIRECTORY != 0 && raw & (WIRE_OPEN_CREATE | WIRE_OPEN_TRUNC) != 0 {
            return Err(ProtoError::InvalidOpenFlags);
        }
        Ok(Self(raw))
    }

    /// Translate Linux-valued open flags into the wire representation.
    ///
    /// `flags` must contain only `O_ACCMODE`, `O_CREAT`, `O_EXCL`, `O_TRUNC`,
    /// `O_APPEND`, `O_DIRECTORY` and `O_NOFOLLOW`.  `O_RDONLY` is zero in the
    /// Linux numbering and maps to [`WIRE_OPEN_READ`].
    pub const fn from_linux(flags: u32) -> Result<Self, ProtoError> {
        let known = O_ACCMODE | O_CREAT | O_EXCL | O_TRUNC | O_APPEND | O_DIRECTORY | O_NOFOLLOW;
        if flags & !known != 0 {
            return Err(ProtoError::InvalidOpenFlags);
        }
        let mut wire = match flags & O_ACCMODE {
            O_RDONLY => WIRE_OPEN_READ,
            O_WRONLY => WIRE_OPEN_WRITE,
            O_RDWR => WIRE_OPEN_READ | WIRE_OPEN_WRITE,
            _ => return Err(ProtoError::InvalidOpenFlags),
        };
        if flags & O_CREAT != 0 {
            wire |= WIRE_OPEN_CREATE;
        }
        if flags & O_EXCL != 0 {
            wire |= WIRE_OPEN_EXCL;
        }
        if flags & O_TRUNC != 0 {
            wire |= WIRE_OPEN_TRUNC;
        }
        if flags & O_APPEND != 0 {
            wire |= WIRE_OPEN_APPEND;
        }
        if flags & O_DIRECTORY != 0 {
            wire |= WIRE_OPEN_DIRECTORY;
        }
        if flags & O_NOFOLLOW != 0 {
            wire |= WIRE_OPEN_NOFOLLOW;
        }
        Self::from_wire(wire)
    }

    /// The raw wire value.
    pub const fn to_wire(self) -> u16 {
        self.0
    }

    /// Translate back to Linux-valued open flags.
    pub const fn to_linux(self) -> u32 {
        let mut flags = match (self.readable(), self.writable()) {
            (true, false) => O_RDONLY,
            (false, true) => O_WRONLY,
            _ => O_RDWR,
        };
        if self.create() {
            flags |= O_CREAT;
        }
        if self.exclusive() {
            flags |= O_EXCL;
        }
        if self.truncate() {
            flags |= O_TRUNC;
        }
        if self.append() {
            flags |= O_APPEND;
        }
        if self.directory() {
            flags |= O_DIRECTORY;
        }
        if self.nofollow() {
            flags |= O_NOFOLLOW;
        }
        flags
    }

    /// Whether read access was requested.
    pub const fn readable(self) -> bool {
        self.0 & WIRE_OPEN_READ != 0
    }

    /// Whether write access was requested.
    pub const fn writable(self) -> bool {
        self.0 & WIRE_OPEN_WRITE != 0
    }

    /// Whether a missing file should be created.
    pub const fn create(self) -> bool {
        self.0 & WIRE_OPEN_CREATE != 0
    }

    /// Whether an existing file must make the open fail.
    pub const fn exclusive(self) -> bool {
        self.0 & WIRE_OPEN_EXCL != 0
    }

    /// Whether the file should be truncated on open.
    pub const fn truncate(self) -> bool {
        self.0 & WIRE_OPEN_TRUNC != 0
    }

    /// Whether writes go to the current end of the file.
    pub const fn append(self) -> bool {
        self.0 & WIRE_OPEN_APPEND != 0
    }

    /// Whether the path must name a directory.
    pub const fn directory(self) -> bool {
        self.0 & WIRE_OPEN_DIRECTORY != 0
    }

    /// Whether a final symbolic link must not be followed.
    pub const fn nofollow(self) -> bool {
        self.0 & WIRE_OPEN_NOFOLLOW != 0
    }

    /// The mode a newly created file receives, or `None` without `O_CREAT`.
    pub const fn create_mode(self) -> Option<u32> {
        if self.create() {
            Some(CREATE_FILE_MODE & !DEFAULT_UMASK)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_bits_are_stable() {
        assert_eq!(WIRE_OPEN_READ, 0x0001);
        assert_eq!(WIRE_OPEN_WRITE, 0x0002);
        assert_eq!(WIRE_OPEN_CREATE, 0x0004);
        assert_eq!(WIRE_OPEN_EXCL, 0x0008);
        assert_eq!(WIRE_OPEN_TRUNC, 0x0010);
        assert_eq!(WIRE_OPEN_APPEND, 0x0020);
        assert_eq!(WIRE_OPEN_DIRECTORY, 0x0040);
        assert_eq!(WIRE_OPEN_NOFOLLOW, 0x0080);
        assert_eq!(WIRE_OPEN_MASK, 0x00ff);
    }

    #[test]
    fn linux_flags_round_trip() {
        let cases = [
            O_RDONLY,
            O_WRONLY,
            O_RDWR,
            O_RDONLY | O_DIRECTORY,
            O_RDONLY | O_NOFOLLOW,
            O_WRONLY | O_TRUNC,
            O_WRONLY | O_APPEND,
            O_RDWR | O_CREAT,
            O_RDWR | O_CREAT | O_EXCL,
            O_RDWR | O_CREAT | O_EXCL | O_TRUNC | O_APPEND,
        ];
        for flags in cases {
            let wire = OpenFlags::from_linux(flags).expect("valid flags");
            assert_eq!(wire.to_linux(), flags, "flags {flags:#o}");
        }
    }

    #[test]
    fn unknown_bits_are_rejected() {
        assert_eq!(
            OpenFlags::from_wire(0x0100),
            Err(ProtoError::InvalidOpenFlags)
        );
        assert_eq!(
            OpenFlags::from_wire(WIRE_OPEN_READ | 0x8000),
            Err(ProtoError::InvalidOpenFlags)
        );
        assert_eq!(
            OpenFlags::from_linux(O_RDONLY | 0o2000000),
            Err(ProtoError::InvalidOpenFlags)
        );
    }

    #[test]
    fn invalid_combinations_are_rejected() {
        assert_eq!(OpenFlags::from_wire(0), Err(ProtoError::InvalidOpenFlags));
        assert_eq!(
            OpenFlags::from_wire(WIRE_OPEN_WRITE | WIRE_OPEN_EXCL),
            Err(ProtoError::InvalidOpenFlags)
        );
        assert_eq!(
            OpenFlags::from_wire(WIRE_OPEN_READ | WIRE_OPEN_TRUNC),
            Err(ProtoError::InvalidOpenFlags)
        );
        assert_eq!(
            OpenFlags::from_wire(WIRE_OPEN_READ | WIRE_OPEN_DIRECTORY | WIRE_OPEN_CREATE),
            Err(ProtoError::InvalidOpenFlags)
        );
        assert_eq!(
            OpenFlags::from_wire(WIRE_OPEN_READ | WIRE_OPEN_WRITE | WIRE_OPEN_TRUNC)
                .expect("read-write trunc is valid")
                .to_linux(),
            O_RDWR | O_TRUNC
        );
    }

    #[test]
    fn create_mode_is_0644() {
        let flags = OpenFlags::from_linux(O_RDWR | O_CREAT).expect("flags");
        assert_eq!(flags.create_mode(), Some(0o644));
        assert_eq!(OpenFlags::READ.create_mode(), None);
        assert_eq!(CREATE_FILE_MODE & !DEFAULT_UMASK, 0o644);
    }

    #[test]
    fn accessors_match_the_bits() {
        let flags = OpenFlags::from_linux(O_RDWR | O_CREAT | O_TRUNC | O_APPEND).expect("flags");
        assert!(flags.readable());
        assert!(flags.writable());
        assert!(flags.create());
        assert!(!flags.exclusive());
        assert!(flags.truncate());
        assert!(flags.append());
        assert!(!flags.directory());
        assert!(!flags.nofollow());
    }
}
