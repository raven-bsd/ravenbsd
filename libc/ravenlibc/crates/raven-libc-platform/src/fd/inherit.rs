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
//! 4   2  version = 5
//! 6   2  count
//! 8  53*count  entries { fd: i32, handle: u32, kind: u8, fd_flags: u8,
//!                        status: u32, path_off: u32, event: u32, readiness: u32,
//!                        socket domain/type/proto/state, endpoint flags, two IPv4
//!                        endpoints, pty_master: u8 }
//! ..  string table: NUL-terminated paths; path_off 0xFFFF_FFFF means none
//! ```
//!
//! Version 1 (18-byte entries, no event/readiness) is still decoded: `raven-spawn` publishes
//! that form for ordinary inherited stdio. Version 5 adds the tty `pty_master` role, a property
//! of the open file description that the donor keeps across `fdcopy` and `execve`; the libc
//! `execve` path uses version 5 so the rebuilt table keeps the role and its sticky master EOF.
//! Versions 2-4 still decode with `pty_master` false.

use alloc::vec::Vec;

use raven_abi::errno::Errno;
use raven_abi::types::Handle;

use crate::fd::ofd::{OfdKind, SocketEndpoint, SocketMeta};

/// Magic at the start of the block.
pub const FDMAP_MAGIC: [u8; 4] = *b"RFDM";
/// Current block version (adds the tty `pty_master` role).
pub const FDMAP_VERSION: u16 = 5;
/// Version 4, with socket endpoint metadata but without the tty master role.
pub const FDMAP_VERSION_V4: u16 = 4;
/// Version 3, with socket identity but without the endpoint metadata.
pub const FDMAP_VERSION_V3: u16 = 3;
/// The previous version, without socket identity.
pub const FDMAP_VERSION_V2: u16 = 2;
/// The legacy version `raven-spawn` publishes.
pub const FDMAP_VERSION_V1: u16 = 1;
/// Fixed header length.
pub const HEADER_LEN: usize = 8;
/// Version-1 entry length (also the prefix of a version-2 entry).
pub const ENTRY_LEN: usize = 18;
/// Version-2 entry length, with the event handle and readiness mask.
pub const ENTRY_LEN_V2: usize = 26;
/// Version-3 entry length: version 2 plus domain/type/proto/state.
pub const ENTRY_LEN_V3: usize = 39;
/// Version-4 entry length: version 3 plus endpoint flags and two IPv4 endpoints.
pub const ENTRY_LEN_V4: usize = 52;
/// Current entry length: version 4 plus the tty `pty_master` role.
pub const ENTRY_LEN_V5: usize = 53;
/// No-path marker in `path_off`.
pub const NO_PATH: u32 = u32::MAX;

/// The environment name used to carry an `FdMap` across `execve`.
///
/// `PROC_EXEC` keeps the kernel handle table but replaces the image (and therefore the libc fd
/// table). The shell serializes the table into this one environment entry; the new image
/// restores it before its first fd use and then removes the entry.
pub const ENV_NAME: &[u8] = b"__RAVEN_FDMAP";

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
    /// Scheme notification event handle, or zero.
    pub event: u32,
    /// The K232 one-shot readiness replay mask.
    pub readiness: u32,
    /// Socket identity for an inherited socket OFD (`FDMAP_VERSION` 3).
    pub socket: Option<SocketMeta>,
    /// The inherited tty is the `pty:` master (`FDMAP_VERSION` 5).
    ///
    /// The role is a property of the open file description, so the donor keeps it across
    /// `fdcopy` and `execve`; this block is the equivalent carrier for the rebuilt libc table.
    pub pty_master: bool,
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
            event: 0,
            readiness: 0,
            socket: None,
            pty_master: false,
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
            HEADER_LEN + ENTRY_LEN_V5.saturating_mul(self.entries.len()) + strings.len(),
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
            out.extend_from_slice(&entry.event.to_le_bytes());
            out.extend_from_slice(&entry.readiness.to_le_bytes());
            let socket = entry.socket.unwrap_or_default();
            out.extend_from_slice(&socket.domain.to_le_bytes());
            out.extend_from_slice(&socket.ty.to_le_bytes());
            out.extend_from_slice(&socket.proto.to_le_bytes());
            out.push(socket.state);
            let flags = u8::from(socket.local.is_some()) | (u8::from(socket.peer.is_some()) << 1);
            out.push(flags);
            for endpoint in [socket.local, socket.peer] {
                let endpoint = endpoint.unwrap_or_default();
                out.extend_from_slice(&endpoint.ip);
                out.extend_from_slice(&endpoint.port.to_le_bytes());
            }
            out.push(u8::from(entry.pty_master));
        }
        out.extend_from_slice(&strings);
        Ok(out)
    }

    /// Encode the block as the lowercase hex value of [`Self::encode`].
    ///
    /// The environment cannot hold NUL bytes; hex is the one text form both sides use.
    pub fn encode_env(&self) -> Result<Vec<u8>, Errno> {
        let bytes = self.encode()?;
        let mut out = Vec::with_capacity(bytes.len().saturating_mul(2));
        for byte in bytes {
            out.push(hex_digit(byte >> 4));
            out.push(hex_digit(byte & 0x0f));
        }
        Ok(out)
    }

    /// Decode the hex value produced by [`Self::encode_env`].
    pub fn decode_env(value: &[u8]) -> Result<Self, Errno> {
        if !value.len().is_multiple_of(2) {
            return Err(Errno::EINVAL);
        }
        let mut bytes = Vec::with_capacity(value.len() / 2);
        let mut index = 0usize;
        while index < value.len() {
            let high = hex_value(*value.get(index).ok_or(Errno::EINVAL)?)?;
            let low = hex_value(*value.get(index.saturating_add(1)).ok_or(Errno::EINVAL)?)?;
            bytes.push((high << 4) | low);
            index = index.saturating_add(2);
        }
        Self::decode(&bytes)
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
        let entry_len = match version {
            FDMAP_VERSION_V1 => ENTRY_LEN,
            FDMAP_VERSION_V2 => ENTRY_LEN_V2,
            FDMAP_VERSION_V3 => ENTRY_LEN_V3,
            FDMAP_VERSION_V4 => ENTRY_LEN_V4,
            FDMAP_VERSION => ENTRY_LEN_V5,
            _ => return Err(Errno::EPROTO),
        };
        let count = usize::from(read_u16(input, 6).ok_or(Errno::EPROTO)?);
        let entries_len = entry_len.checked_mul(count).ok_or(Errno::EPROTO)?;
        let strings_start = HEADER_LEN.checked_add(entries_len).ok_or(Errno::EPROTO)?;
        let strings = input.get(strings_start..).ok_or(Errno::EPROTO)?;
        let mut entries = Vec::with_capacity(count);
        let mut index = 0usize;
        while index < count {
            let base = HEADER_LEN + index.saturating_mul(entry_len);
            let record = input.get(base..base + entry_len).ok_or(Errno::EPROTO)?;
            let fd = read_i32(record, 0).ok_or(Errno::EPROTO)?;
            let handle = read_u32(record, 4).ok_or(Errno::EPROTO)?;
            let kind = kind_from_byte(*record.get(8).ok_or(Errno::EPROTO)?);
            let fd_flags = *record.get(9).ok_or(Errno::EPROTO)?;
            let status = read_u32(record, 10).ok_or(Errno::EPROTO)?;
            let offset = read_u32(record, 14).ok_or(Errno::EPROTO)?;
            let event = if version >= FDMAP_VERSION_V2 {
                read_u32(record, 18).ok_or(Errno::EPROTO)?
            } else {
                0
            };
            let readiness = if version >= FDMAP_VERSION_V2 {
                read_u32(record, 22).ok_or(Errno::EPROTO)?
            } else {
                0
            };
            let socket = if version >= FDMAP_VERSION_V3 {
                let domain = read_i32(record, 26).ok_or(Errno::EPROTO)?;
                let ty = read_i32(record, 30).ok_or(Errno::EPROTO)?;
                let proto = read_i32(record, 34).ok_or(Errno::EPROTO)?;
                let state = *record.get(38).ok_or(Errno::EPROTO)?;
                let (local, peer) = if version >= FDMAP_VERSION_V4 {
                    let flags = *record.get(39).ok_or(Errno::EPROTO)?;
                    let endpoint = |offset: usize| -> Result<SocketEndpoint, Errno> {
                        let ip: [u8; 4] = record
                            .get(offset..offset + 4)
                            .ok_or(Errno::EPROTO)?
                            .try_into()
                            .map_err(|_| Errno::EPROTO)?;
                        let port = u16::from_le_bytes(
                            record
                                .get(offset + 4..offset + 6)
                                .ok_or(Errno::EPROTO)?
                                .try_into()
                                .map_err(|_| Errno::EPROTO)?,
                        );
                        Ok(SocketEndpoint { ip, port })
                    };
                    (
                        if flags & 1 != 0 {
                            Some(endpoint(40)?)
                        } else {
                            None
                        },
                        if flags & 2 != 0 {
                            Some(endpoint(46)?)
                        } else {
                            None
                        },
                    )
                } else {
                    (None, None)
                };
                (domain != 0 || ty != 0 || proto != 0 || state != 0).then_some(SocketMeta {
                    domain,
                    ty,
                    proto,
                    state,
                    local,
                    peer,
                })
            } else {
                None
            };
            let pty_master = if version >= FDMAP_VERSION {
                *record.get(ENTRY_LEN_V4).ok_or(Errno::EPROTO)? != 0
            } else {
                false
            };
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
                event,
                readiness,
                socket,
                pty_master,
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

/// One inherited socket descriptor, exposed to the libc socket layer after `restore_fdmap`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InheritedSocket {
    /// The child descriptor number.
    pub fd: i32,
    /// The OFD arena token.
    pub ofd: u32,
    /// The already-valid kernel handle.
    pub handle: Handle,
    /// `F_GETFL` status bits (the caller derives `O_NONBLOCK`).
    pub status: u32,
    /// The socket identity.
    pub socket: SocketMeta,
}

fn hex_digit(value: u8) -> u8 {
    match value {
        0..=9 => b'0' + value,
        _ => b'a' + value.saturating_sub(10),
    }
}

fn hex_value(digit: u8) -> Result<u8, Errno> {
    match digit {
        b'0'..=b'9' => Ok(digit.saturating_sub(b'0')),
        b'a'..=b'f' => Ok(digit.saturating_sub(b'a').saturating_add(10)),
        b'A'..=b'F' => Ok(digit.saturating_sub(b'A').saturating_add(10)),
        _ => Err(Errno::EINVAL),
    }
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
