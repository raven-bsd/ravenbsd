// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Scheme packet wire format.
//!
//! This file is compiled twice: as part of the kernel (`crate::ipc::protocol`) and as part of the
//! pure `raven-ipc` host-test crate. It therefore depends only on `core` and `alloc`, never on
//! kernel internals.
//!
//! The packet is a fixed `#[repr(C)]` record. The codec writes every field explicitly in
//! little-endian order at the field's `offset_of!` position, so the wire image matches the
//! structure without any unsafe pointer casts. Lengths are validated before they are used.

#![allow(clippy::indexing_slicing)] // Every offset is checked against `PACKET_SIZE` first.
#![allow(clippy::arithmetic_side_effects)] // Field offsets are compile-time constants < PACKET_SIZE.

use alloc::vec::Vec;
use core::mem::{offset_of, size_of};

/// Current packet version.
pub const VERSION: u32 = 1;
/// Maximum path suffix carried by an `Open` packet.
pub const MAX_PATH: usize = 256;
/// Maximum inline data carried by a packet.
pub const MAX_DATA: usize = 64;
/// Maximum payload captured for one request or response.
pub const MAX_TRANSFER: u64 = 4 * 1024 * 1024;

/// Packet opcode.
#[repr(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Opcode {
    /// Open a path on a scheme.
    Open = 1,
    /// Read from a scheme file.
    Read = 2,
    /// Write to a scheme file.
    Write = 3,
    /// Close a scheme file.
    Close = 4,
    /// Duplicate a scheme file handle (informational packet).
    Dup = 5,
    /// Stat a scheme file.
    Fstat = 6,
    /// Typed RPC escape hatch.
    Call = 7,
}

impl Opcode {
    /// Raw wire value.
    pub const fn as_u16(self) -> u16 {
        self as u16
    }

    /// Parse a wire value.
    pub const fn from_u16(value: u16) -> Option<Self> {
        Some(match value {
            1 => Self::Open,
            2 => Self::Read,
            3 => Self::Write,
            4 => Self::Close,
            5 => Self::Dup,
            6 => Self::Fstat,
            7 => Self::Call,
            _ => return None,
        })
    }
}

/// One request or response packet.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SchemePacket {
    /// `VERSION`.
    pub version: u32,
    /// Operation.
    pub opcode: u16,
    /// Operation flags (open flags for `Open`).
    pub flags: u16,
    /// Kernel-assigned request id; the server echoes it in the response.
    pub request_id: u64,
    /// Kernel-assigned scheme-file id.
    pub file_id: u64,
    /// Opaque server cookie returned by the `Open` response.
    pub server_cookie: u64,
    /// Captured client buffer address in the server's address space.
    pub buffer_addr: u64,
    /// Captured client buffer length.
    pub buffer_len: u64,
    /// Second captured buffer address (`Call` response).
    pub response_addr: u64,
    /// Second captured buffer length (`Call` response).
    pub response_len: u64,
    /// File offset for offset-aware operations.
    pub offset: u64,
    /// Response status: 0 or a positive errno.
    pub status: i32,
    /// Path length for `Open`; inline data length for other opcodes.
    pub data_len: u32,
    /// Path suffix for `Open`.
    pub path: [u8; MAX_PATH],
    /// Small inline payload.
    pub data: [u8; MAX_DATA],
}

impl Default for SchemePacket {
    fn default() -> Self {
        Self {
            version: VERSION,
            opcode: 0,
            flags: 0,
            request_id: 0,
            file_id: 0,
            server_cookie: 0,
            buffer_addr: 0,
            buffer_len: 0,
            response_addr: 0,
            response_len: 0,
            offset: 0,
            status: 0,
            data_len: 0,
            path: [0; MAX_PATH],
            data: [0; MAX_DATA],
        }
    }
}

/// Encoded packet size.
pub const PACKET_SIZE: usize = size_of::<SchemePacket>();

const _: () = {
    assert!(PACKET_SIZE == 400);
    assert!(offset_of!(SchemePacket, version) == 0);
    assert!(offset_of!(SchemePacket, opcode) == 4);
    assert!(offset_of!(SchemePacket, flags) == 6);
    assert!(offset_of!(SchemePacket, request_id) == 8);
    assert!(offset_of!(SchemePacket, file_id) == 16);
    assert!(offset_of!(SchemePacket, server_cookie) == 24);
    assert!(offset_of!(SchemePacket, buffer_addr) == 32);
    assert!(offset_of!(SchemePacket, buffer_len) == 40);
    assert!(offset_of!(SchemePacket, response_addr) == 48);
    assert!(offset_of!(SchemePacket, response_len) == 56);
    assert!(offset_of!(SchemePacket, offset) == 64);
    assert!(offset_of!(SchemePacket, status) == 72);
    assert!(offset_of!(SchemePacket, data_len) == 76);
    assert!(offset_of!(SchemePacket, path) == 80);
    assert!(offset_of!(SchemePacket, data) == 336);
};

/// Why a packet could not be decoded or validated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PacketError {
    /// Fewer than [`PACKET_SIZE`] bytes.
    TooShort,
    /// Unknown version.
    BadVersion,
    /// Unknown opcode.
    BadOpcode,
    /// A length field exceeds its array or the transfer limit.
    BadLength,
    /// A response status is outside the errno range.
    BadStatus,
}

/// Encode a packet into `out`, which must be at least [`PACKET_SIZE`] bytes.
pub fn encode(packet: &SchemePacket, out: &mut [u8]) -> Result<(), PacketError> {
    if out.len() < PACKET_SIZE {
        return Err(PacketError::TooShort);
    }
    put_u32(out, offset_of!(SchemePacket, version), packet.version);
    put_u16(out, offset_of!(SchemePacket, opcode), packet.opcode);
    put_u16(out, offset_of!(SchemePacket, flags), packet.flags);
    put_u64(out, offset_of!(SchemePacket, request_id), packet.request_id);
    put_u64(out, offset_of!(SchemePacket, file_id), packet.file_id);
    put_u64(
        out,
        offset_of!(SchemePacket, server_cookie),
        packet.server_cookie,
    );
    put_u64(
        out,
        offset_of!(SchemePacket, buffer_addr),
        packet.buffer_addr,
    );
    put_u64(out, offset_of!(SchemePacket, buffer_len), packet.buffer_len);
    put_u64(
        out,
        offset_of!(SchemePacket, response_addr),
        packet.response_addr,
    );
    put_u64(
        out,
        offset_of!(SchemePacket, response_len),
        packet.response_len,
    );
    put_u64(out, offset_of!(SchemePacket, offset), packet.offset);
    put_i32(out, offset_of!(SchemePacket, status), packet.status);
    put_u32(out, offset_of!(SchemePacket, data_len), packet.data_len);
    put_bytes(out, offset_of!(SchemePacket, path), &packet.path);
    put_bytes(out, offset_of!(SchemePacket, data), &packet.data);
    Ok(())
}

/// Decode a packet from `input`, which must be at least [`PACKET_SIZE`] bytes.
pub fn decode(input: &[u8]) -> Result<SchemePacket, PacketError> {
    if input.len() < PACKET_SIZE {
        return Err(PacketError::TooShort);
    }
    let mut path = [0u8; MAX_PATH];
    path.copy_from_slice(
        input
            .get(offset_of!(SchemePacket, path)..offset_of!(SchemePacket, path) + MAX_PATH)
            .ok_or(PacketError::TooShort)?,
    );
    let mut data = [0u8; MAX_DATA];
    data.copy_from_slice(
        input
            .get(offset_of!(SchemePacket, data)..offset_of!(SchemePacket, data) + MAX_DATA)
            .ok_or(PacketError::TooShort)?,
    );
    Ok(SchemePacket {
        version: get_u32(input, offset_of!(SchemePacket, version))?,
        opcode: get_u16(input, offset_of!(SchemePacket, opcode))?,
        flags: get_u16(input, offset_of!(SchemePacket, flags))?,
        request_id: get_u64(input, offset_of!(SchemePacket, request_id))?,
        file_id: get_u64(input, offset_of!(SchemePacket, file_id))?,
        server_cookie: get_u64(input, offset_of!(SchemePacket, server_cookie))?,
        buffer_addr: get_u64(input, offset_of!(SchemePacket, buffer_addr))?,
        buffer_len: get_u64(input, offset_of!(SchemePacket, buffer_len))?,
        response_addr: get_u64(input, offset_of!(SchemePacket, response_addr))?,
        response_len: get_u64(input, offset_of!(SchemePacket, response_len))?,
        offset: get_u64(input, offset_of!(SchemePacket, offset))?,
        status: get_i32(input, offset_of!(SchemePacket, status))?,
        data_len: get_u32(input, offset_of!(SchemePacket, data_len))?,
        path,
        data,
    })
}

impl SchemePacket {
    /// Build a request packet.
    pub fn request(opcode: Opcode, request_id: u64, file_id: u64) -> Self {
        Self {
            version: VERSION,
            opcode: opcode.as_u16(),
            request_id,
            file_id,
            ..Self::default()
        }
    }

    /// Validate the fixed fields. `response` selects the stricter status check.
    pub fn validate(&self, response: bool) -> Result<(), PacketError> {
        if self.version != VERSION {
            return Err(PacketError::BadVersion);
        }
        if Opcode::from_u16(self.opcode).is_none() {
            return Err(PacketError::BadOpcode);
        }
        if self.opcode == Opcode::Open.as_u16() {
            if self.data_len as usize > MAX_PATH {
                return Err(PacketError::BadLength);
            }
        } else if self.data_len as usize > MAX_DATA {
            return Err(PacketError::BadLength);
        }
        if self.buffer_len > MAX_TRANSFER || self.response_len > MAX_TRANSFER {
            return Err(PacketError::BadLength);
        }
        if response && !(0..=4095).contains(&self.status) {
            return Err(PacketError::BadStatus);
        }
        Ok(())
    }

    /// Path bytes for an `Open` request (used by the host model and diagnostics).
    #[allow(dead_code)]
    pub fn path_bytes(&self) -> &[u8] {
        if self.opcode != Opcode::Open.as_u16() {
            return &[];
        }
        self.path
            .get(..self.data_len.min(MAX_PATH as u32) as usize)
            .unwrap_or(&[])
    }

    /// Inline data bytes for non-`Open` packets.
    pub fn data_bytes(&self) -> &[u8] {
        if self.opcode == Opcode::Open.as_u16() {
            return &[];
        }
        self.data
            .get(..self.data_len.min(MAX_DATA as u32) as usize)
            .unwrap_or(&[])
    }

    /// Set the inline data bytes, rejecting overlong input (host model/diagnostics).
    #[allow(dead_code)]
    pub fn set_data(&mut self, bytes: &[u8]) -> Result<(), PacketError> {
        let length = bytes.len();
        if length > MAX_DATA {
            return Err(PacketError::BadLength);
        }
        self.data.fill(0);
        if let Some(destination) = self.data.get_mut(..length) {
            destination.copy_from_slice(bytes);
        }
        self.data_len = u32::try_from(length).map_err(|_| PacketError::BadLength)?;
        Ok(())
    }

    /// Set the path bytes, rejecting overlong input.
    pub fn set_path(&mut self, bytes: &[u8]) -> Result<(), PacketError> {
        let length = bytes.len();
        if length > MAX_PATH {
            return Err(PacketError::BadLength);
        }
        self.path.fill(0);
        if let Some(destination) = self.path.get_mut(..length) {
            destination.copy_from_slice(bytes);
        }
        self.data_len = u32::try_from(length).map_err(|_| PacketError::BadLength)?;
        Ok(())
    }
}

fn put_bytes(out: &mut [u8], at: usize, bytes: &[u8]) {
    if let Some(destination) = out.get_mut(at..at.saturating_add(bytes.len())) {
        destination.copy_from_slice(bytes);
    }
}

fn put_u16(out: &mut [u8], at: usize, value: u16) {
    put_bytes(out, at, &value.to_le_bytes());
}

fn put_u32(out: &mut [u8], at: usize, value: u32) {
    put_bytes(out, at, &value.to_le_bytes());
}

fn put_u64(out: &mut [u8], at: usize, value: u64) {
    put_bytes(out, at, &value.to_le_bytes());
}

fn put_i32(out: &mut [u8], at: usize, value: i32) {
    put_bytes(out, at, &value.to_le_bytes());
}

fn get_u16(input: &[u8], at: usize) -> Result<u16, PacketError> {
    let bytes = input.get(at..at + 2).ok_or(PacketError::TooShort)?;
    Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn get_u32(input: &[u8], at: usize) -> Result<u32, PacketError> {
    let bytes = input.get(at..at + 4).ok_or(PacketError::TooShort)?;
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn get_u64(input: &[u8], at: usize) -> Result<u64, PacketError> {
    let bytes = input.get(at..at + 8).ok_or(PacketError::TooShort)?;
    let mut value = [0u8; 8];
    value.copy_from_slice(bytes);
    Ok(u64::from_le_bytes(value))
}

fn get_i32(input: &[u8], at: usize) -> Result<i32, PacketError> {
    let bytes = input.get(at..at + 4).ok_or(PacketError::TooShort)?;
    Ok(i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

/// Return the packet bytes as a vector (tests and diagnostics).
#[allow(dead_code)]
pub fn to_vec(packet: &SchemePacket) -> Result<Vec<u8>, PacketError> {
    let mut bytes = alloc::vec![0u8; PACKET_SIZE];
    encode(packet, &mut bytes)?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> SchemePacket {
        let mut packet = SchemePacket::request(Opcode::Open, 0x1122_3344_5566_7788, 0x99aa);
        packet.flags = 3;
        packet.server_cookie = 0xdead_beef;
        packet.buffer_addr = 0x7000_0000_0000;
        packet.buffer_len = 1 << 20;
        packet.response_addr = 0x7000_1000_0000;
        packet.response_len = 4096;
        packet.offset = 123;
        packet.set_path(b"echo:hello").unwrap();
        packet
    }

    #[test]
    fn packet_size_and_offsets_are_stable() {
        assert_eq!(PACKET_SIZE, 400);
        assert_eq!(size_of::<Opcode>(), 2);
    }

    #[test]
    fn roundtrip_preserves_every_field() {
        let packet = sample();
        let bytes = to_vec(&packet).unwrap();
        let decoded = decode(&bytes).unwrap();
        assert_eq!(decoded.version, packet.version);
        assert_eq!(decoded.opcode, packet.opcode);
        assert_eq!(decoded.flags, packet.flags);
        assert_eq!(decoded.request_id, packet.request_id);
        assert_eq!(decoded.file_id, packet.file_id);
        assert_eq!(decoded.server_cookie, packet.server_cookie);
        assert_eq!(decoded.buffer_addr, packet.buffer_addr);
        assert_eq!(decoded.buffer_len, packet.buffer_len);
        assert_eq!(decoded.response_addr, packet.response_addr);
        assert_eq!(decoded.response_len, packet.response_len);
        assert_eq!(decoded.offset, packet.offset);
        assert_eq!(decoded.path_bytes(), b"echo:hello");
        assert_eq!(decoded.data_bytes(), b"");
        let mut read = SchemePacket::request(Opcode::Read, 1, 1);
        read.set_data(b"payload").unwrap();
        assert_eq!(read.data_bytes(), b"payload");
        assert_eq!(read.path_bytes(), b"");
    }

    #[test]
    fn short_and_bad_packets_are_rejected() {
        assert_eq!(decode(&[]), Err(PacketError::TooShort));
        assert_eq!(decode(&[0u8; PACKET_SIZE - 1]), Err(PacketError::TooShort));
        let mut bytes = alloc::vec![0u8; PACKET_SIZE];
        let packet = decode(&bytes).unwrap();
        assert_eq!(packet.validate(false), Err(PacketError::BadVersion));
        bytes[0..4].copy_from_slice(&VERSION.to_le_bytes());
        let packet = decode(&bytes).unwrap();
        assert_eq!(packet.validate(false), Err(PacketError::BadOpcode));
    }

    #[test]
    fn length_limits_are_enforced() {
        let mut packet = SchemePacket::request(Opcode::Read, 1, 1);
        packet.buffer_len = MAX_TRANSFER + 1;
        assert_eq!(packet.validate(false), Err(PacketError::BadLength));
        let mut packet = SchemePacket::request(Opcode::Read, 1, 1);
        packet.data_len = (MAX_DATA + 1) as u32;
        assert_eq!(packet.validate(false), Err(PacketError::BadLength));
        let mut packet = SchemePacket::request(Opcode::Open, 1, 1);
        packet.data_len = (MAX_PATH + 1) as u32;
        assert_eq!(packet.validate(false), Err(PacketError::BadLength));
    }

    #[test]
    fn large_read_response_uses_buffer_len() {
        let mut packet = SchemePacket::request(Opcode::Read, 7, 9);
        packet.buffer_len = 1 << 20;
        packet.data_len = 0;
        packet.status = 0;
        assert_eq!(packet.validate(true), Ok(()));
    }

    #[test]
    fn status_range_is_checked_for_responses() {
        let mut packet = SchemePacket::request(Opcode::Call, 1, 1);
        packet.status = 5;
        assert_eq!(packet.validate(true), Ok(()));
        packet.status = -1;
        assert_eq!(packet.validate(true), Err(PacketError::BadStatus));
        packet.status = 4096;
        assert_eq!(packet.validate(true), Err(PacketError::BadStatus));
    }

    #[test]
    fn inline_data_and_path_are_bounded() {
        let mut packet = SchemePacket::default();
        assert_eq!(packet.set_data(b"abcdef"), Ok(()));
        assert_eq!(packet.data_bytes(), b"abcdef");
        assert_eq!(
            packet.set_data(&[0u8; MAX_DATA + 1]),
            Err(PacketError::BadLength)
        );
        assert_eq!(
            packet.set_path(&[b'a'; MAX_PATH + 1]),
            Err(PacketError::BadLength)
        );
    }

    #[test]
    fn decode_never_panics_on_arbitrary_bytes() {
        let mut state = 0x1234_5678_9abc_def0u64;
        let mut bytes = alloc::vec![0u8; PACKET_SIZE + 32];
        for length in 0..=bytes.len() {
            for _ in 0..64 {
                for byte in &mut bytes {
                    state ^= state << 13;
                    state ^= state >> 7;
                    state ^= state << 17;
                    *byte = state as u8;
                }
                let slice = &bytes[..length];
                let _ = decode(slice);
                if let Ok(packet) = decode(slice) {
                    let _ = packet.validate(false);
                    let _ = packet.validate(true);
                    let _ = packet.path_bytes();
                    let _ = packet.data_bytes();
                }
            }
        }
    }
}
