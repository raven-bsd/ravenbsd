// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The `FdMap` inheritance block (doc 11 section 3.5).
//!
//! The parent writes one self-describing block into a memory object; the address reaches the
//! child as `AT_RAVEN_FDMAP` (PG-50). The child rebuilds the table from it before `main`.
//! Layout, little endian:
//!
//! ```text
//! 0   4  magic "RFDM"
//! 4   2  version = 1
//! 6   2  count
//! 8  18*count  entries { fd: i32, handle: u32, kind: u8, fd_flags: u8,
//!                        status: u32, path_off: u32 }
//! ..  string table: NUL-terminated paths; path_off 0xFFFF_FFFF means none
//! ```

use alloc::vec::Vec;

use raven_abi::errno::Errno;
use raven_abi::types::Handle;

use crate::fd::ofd::OfdKind;

/// Magic at the start of the block.
pub const FDMAP_MAGIC: [u8; 4] = *b"RFDM";
/// Current block version.
pub const FDMAP_VERSION: u16 = 1;
/// Fixed header length.
pub const HEADER_LEN: usize = 8;
/// Fixed entry length.
pub const ENTRY_LEN: usize = 18;
/// No-path marker in `path_off`.
pub const NO_PATH: u32 = u32::MAX;

/// One inherited descriptor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FdMapEntry {
    /// Descriptor number in the child.
    pub fd: i32,
    /// Handle already transferred into the child.
    pub handle: u32,
    /// OFD kind.
    pub kind: OfdKind,
    /// `FD_CLOEXEC` and reserved bits.
    pub fd_flags: u8,
    /// Access mode and status bits.
    pub status: u32,
    /// Absolute path, if known.
    pub path: Option<Vec<u8>>,
}

impl FdMapEntry {
    /// Build an entry without a path.
    pub fn new(fd: i32, handle: u32, kind: OfdKind, fd_flags: u8, status: u32) -> Self {
        Self {
            fd,
            handle,
            kind,
            fd_flags,
            status,
            path: None,
        }
    }
}

/// A decoded inheritance block.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FdMap {
    /// Entries in wire order.
    pub entries: Vec<FdMapEntry>,
}

impl FdMap {
    /// An empty block (a process started without one, O-5).
    pub fn empty() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Encode the block, including the string table.
    pub fn encode(&self) -> Result<Vec<u8>, Errno> {
        let count = u16::try_from(self.entries.len()).map_err(|_| Errno::E2BIG)?;
        let mut strings = Vec::new();
        let mut offsets = Vec::with_capacity(self.entries.len());
        for entry in &self.entries {
            match &entry.path {
                Some(path) => {
                    if path.is_empty() || path.len() > 4095 || path.contains(&0) {
                        return Err(Errno::EINVAL);
                    }
                    let offset = u32::try_from(strings.len()).map_err(|_| Errno::E2BIG)?;
                    offsets.push(offset);
                    strings.extend_from_slice(path);
                    strings.push(0);
                }
                None => offsets.push(NO_PATH),
            }
        }
        let mut out = Vec::with_capacity(
            HEADER_LEN + ENTRY_LEN.saturating_mul(self.entries.len()) + strings.len(),
        );
        out.extend_from_slice(&FDMAP_MAGIC);
        out.extend_from_slice(&FDMAP_VERSION.to_le_bytes());
        out.extend_from_slice(&count.to_le_bytes());
        for (entry, offset) in self.entries.iter().zip(offsets) {
            out.extend_from_slice(&entry.fd.to_le_bytes());
            out.extend_from_slice(&entry.handle.to_le_bytes());
            out.push(kind_byte(entry.kind));
            out.push(entry.fd_flags);
            out.extend_from_slice(&entry.status.to_le_bytes());
            out.extend_from_slice(&offset.to_le_bytes());
        }
        out.extend_from_slice(&strings);
        Ok(out)
    }

    /// Decode and validate one complete block.
    pub fn decode(input: &[u8]) -> Result<Self, Errno> {
        if input.len() < HEADER_LEN {
            return Err(Errno::EPROTO);
        }
        if input.get(..4) != Some(&FDMAP_MAGIC) {
            return Err(Errno::EPROTO);
        }
        let version = read_u16(input, 4).ok_or(Errno::EPROTO)?;
        if version != FDMAP_VERSION {
            return Err(Errno::EPROTO);
        }
        let count = usize::from(read_u16(input, 6).ok_or(Errno::EPROTO)?);
        let entries_len = ENTRY_LEN.checked_mul(count).ok_or(Errno::EPROTO)?;
        let strings_start = HEADER_LEN.checked_add(entries_len).ok_or(Errno::EPROTO)?;
        let strings = input.get(strings_start..).ok_or(Errno::EPROTO)?;
        let mut entries = Vec::with_capacity(count);
        let mut index = 0usize;
        while index < count {
            let base = HEADER_LEN + index.saturating_mul(ENTRY_LEN);
            let record = input.get(base..base + ENTRY_LEN).ok_or(Errno::EPROTO)?;
            let fd = read_i32(record, 0).ok_or(Errno::EPROTO)?;
            let handle = read_u32(record, 4).ok_or(Errno::EPROTO)?;
            let kind = kind_from_byte(*record.get(8).ok_or(Errno::EPROTO)?);
            let fd_flags = *record.get(9).ok_or(Errno::EPROTO)?;
            let status = read_u32(record, 10).ok_or(Errno::EPROTO)?;
            let offset = read_u32(record, 14).ok_or(Errno::EPROTO)?;
            let path = if offset == NO_PATH {
                None
            } else {
                let tail = strings.get(offset as usize..).ok_or(Errno::EPROTO)?;
                let end = tail
                    .iter()
                    .position(|&byte| byte == 0)
                    .ok_or(Errno::EPROTO)?;
                Some(tail.get(..end).ok_or(Errno::EPROTO)?.to_vec())
            };
            entries.push(FdMapEntry {
                fd,
                handle,
                kind,
                fd_flags,
                status,
                path,
            });
            index = index.saturating_add(1);
        }
        Ok(Self { entries })
    }
}

fn kind_byte(kind: OfdKind) -> u8 {
    match kind {
        OfdKind::File => 0,
        OfdKind::Dir => 1,
        OfdKind::Tty => 2,
        OfdKind::Socket => 3,
        OfdKind::Pipe => 4,
        OfdKind::Epoll => 5,
        OfdKind::Timer => 6,
        OfdKind::EventFd => 7,
        OfdKind::ProcessFd => 8,
        OfdKind::Unknown => 255,
    }
}

fn kind_from_byte(byte: u8) -> OfdKind {
    match byte {
        0 => OfdKind::File,
        1 => OfdKind::Dir,
        2 => OfdKind::Tty,
        3 => OfdKind::Socket,
        4 => OfdKind::Pipe,
        5 => OfdKind::Epoll,
        6 => OfdKind::Timer,
        7 => OfdKind::EventFd,
        8 => OfdKind::ProcessFd,
        _ => OfdKind::Unknown,
    }
}

/// Kinds that C14 declares non-transferable and that must not appear in an inheritance block.
pub const fn transferable(kind: OfdKind) -> bool {
    !matches!(kind, OfdKind::Epoll | OfdKind::Timer | OfdKind::ProcessFd)
}

/// A handle reference used when rebuilding a child table.
pub fn entry_handle(entry: &FdMapEntry) -> Handle {
    Handle(entry.handle)
}

fn read_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    let slice = bytes.get(offset..offset.checked_add(2)?)?;
    Some(u16::from_le_bytes([*slice.first()?, *slice.get(1)?]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    let slice = bytes.get(offset..offset.checked_add(4)?)?;
    Some(u32::from_le_bytes([
        *slice.first()?,
        *slice.get(1)?,
        *slice.get(2)?,
        *slice.get(3)?,
    ]))
}

fn read_i32(bytes: &[u8], offset: usize) -> Option<i32> {
    read_u32(bytes, offset).map(|value| value as i32)
}

#[cfg(test)]
#[path = "inherit_ut.rs"]
mod tests;
