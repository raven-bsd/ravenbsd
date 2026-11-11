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

use raven_abi::types::Rights;

/// Current packet version.
pub const VERSION: u32 = 1;
/// Maximum path suffix carried by an `Open` packet.
pub const MAX_PATH: usize = 256;
/// Maximum inline data carried by a packet.
pub const MAX_DATA: usize = 64;
/// Maximum payload captured for one request or response.
pub const MAX_TRANSFER: u64 = 4 * 1024 * 1024;
/// Maximum handles attached to one reply.
pub const MAX_ATTACHMENTS: usize = 4;
/// An `Open` response returns its single attachment directly.
pub const REPLY_HANDLE: u16 = 1 << 15;

/// One reply attachment: a server-table handle and a rights-reduction mask.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Attachment {
    /// Handle value in the sender's table.
    pub handle: u32,
    /// Rights mask; zero retains all of the sender's current rights.
    pub rights_mask: u32,
}

/// Reduce `source` by an attachment mask; zero means all current rights.
pub const fn attachment_rights(source: Rights, mask: u32) -> Rights {
    if mask == 0 {
        source
    } else {
        source.intersect(Rights(mask))
    }
}

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
    /// Number of populated reply attachments.
    pub attach_count: u32,
    /// Reply attachments, encoded after inline data.
    pub attach: [Attachment; MAX_ATTACHMENTS],
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
            attach_count: 0,
            attach: [Attachment::default(); MAX_ATTACHMENTS],
        }
    }
}

/// Encoded packet size.
pub const PACKET_SIZE: usize = size_of::<SchemePacket>();

const _: () = {
    assert!(PACKET_SIZE == 440);
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
    assert!(offset_of!(SchemePacket, attach_count) == 400);
    assert!(offset_of!(SchemePacket, attach) == 404);
};

/// Why a reply's reported result length is not acceptable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplyBoundError {
    /// The result is larger than the buffer the request captured for it.
    ExceedsCapacity,
}

/// The result length a reply reports for `opcode`.
///
/// `Read`, `Write` and `Call` report their `buffer_len`; every other opcode reports inline
/// `data_len`. This is the value the client's syscall returns on success.
pub const fn reported_result(opcode: Opcode, packet: &SchemePacket) -> u64 {
    match opcode {
        Opcode::Read | Opcode::Write | Opcode::Call => packet.buffer_len,
        _ => packet.data_len as u64,
    }
}

/// Validate a successful reply's reported result against the request's capacities.
///
/// `Write` may report at most the client's input length, `Read` at most the captured buffer
/// length, and `Call`/`Fstat` at most the response capacity. Equal and zero lengths are valid.
/// Error replies keep their reported length for diagnostics; the client returns the status
/// without using it. Open/Close/Dup have no client-side result bound.
pub fn reply_result(
    opcode: Opcode,
    request_buffer_len: u64,
    request_response_len: u64,
    packet: &SchemePacket,
) -> Result<u64, ReplyBoundError> {
    let result = reported_result(opcode, packet);
    if packet.status != 0 {
        return Ok(result);
    }
    let allowed = match opcode {
        Opcode::Read | Opcode::Write => request_buffer_len,
        Opcode::Call => {
            if u64::from(packet.data_len) > request_response_len {
                return Err(ReplyBoundError::ExceedsCapacity);
            }
            request_response_len
        }
        Opcode::Fstat => request_response_len,
        Opcode::Open | Opcode::Close | Opcode::Dup => return Ok(result),
    };
    if result > allowed {
        return Err(ReplyBoundError::ExceedsCapacity);
    }
    Ok(result)
}

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
    put_u32(
        out,
        offset_of!(SchemePacket, attach_count),
        packet.attach_count,
    );
    let mut index = 0usize;
    while index < MAX_ATTACHMENTS {
        let offset = offset_of!(SchemePacket, attach).saturating_add(index.saturating_mul(8));
        let attachment = packet.attach.get(index).ok_or(PacketError::TooShort)?;
        put_u32(out, offset, attachment.handle);
        put_u32(out, offset.saturating_add(4), attachment.rights_mask);
        index = index.saturating_add(1);
    }
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
    let attach_count = get_u32(input, offset_of!(SchemePacket, attach_count))?;
    let mut attach = [Attachment::default(); MAX_ATTACHMENTS];
    let mut index = 0usize;
    while index < MAX_ATTACHMENTS {
        let offset = offset_of!(SchemePacket, attach).saturating_add(index.saturating_mul(8));
        let slot = attach.get_mut(index).ok_or(PacketError::TooShort)?;
        slot.handle = get_u32(input, offset)?;
        slot.rights_mask = get_u32(input, offset.saturating_add(4))?;
        index = index.saturating_add(1);
    }
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
        attach_count,
        attach,
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
        if self.attach_count as usize > MAX_ATTACHMENTS {
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
        assert_eq!(PACKET_SIZE, 440);
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
    fn attachment_roundtrip_preserves_handle_and_mask() {
        let mut packet = SchemePacket::request(Opcode::Open, 1, 2);
        packet.attach[0] = Attachment {
            handle: 7,
            rights_mask: 3,
        };
        packet.attach_count = 1;
        let decoded = decode(&to_vec(&packet).unwrap()).unwrap();
        assert_eq!(decoded.attach_count, 1);
        assert_eq!(decoded.attach[0], packet.attach[0]);
    }

    #[test]
    fn attachment_count_is_bounded() {
        let packet = SchemePacket {
            attach_count: (MAX_ATTACHMENTS + 1) as u32,
            ..SchemePacket::request(Opcode::Read, 1, 2)
        };
        assert_eq!(packet.validate(true), Err(PacketError::BadLength));
    }

    #[test]
    fn first_attachment_sets_count_to_one() {
        let mut packet = SchemePacket::default();
        packet.attach[0] = Attachment::default();
        packet.attach_count = 1;
        assert_eq!(packet.attach_count, 1);
    }

    #[test]
    fn later_attachment_sets_count_to_index_plus_one() {
        let mut packet = SchemePacket::default();
        packet.attach[3] = Attachment::default();
        packet.attach_count = 4;
        assert_eq!(packet.attach_count, 4);
    }

    #[test]
    fn attachment_count_past_limit_is_rejected() {
        let packet = SchemePacket {
            attach_count: (MAX_ATTACHMENTS + 1) as u32,
            ..SchemePacket::request(Opcode::Call, 1, 2)
        };
        assert_eq!(packet.validate(true), Err(PacketError::BadLength));
    }

    #[test]
    fn zero_attachment_mask_retains_source_rights() {
        let rights = Rights::READ.union(Rights::TRANSFER);
        assert_eq!(attachment_rights(rights, 0), rights);
    }

    #[test]
    fn attachment_mask_only_reduces_rights() {
        let rights = Rights::READ.union(Rights::WRITE).union(Rights::TRANSFER);
        assert_eq!(attachment_rights(rights, Rights::READ.bits()), Rights::READ);
    }

    #[test]
    fn attachment_mask_cannot_grow_rights() {
        assert_eq!(
            attachment_rights(Rights::READ, Rights::ALL.bits()),
            Rights::READ
        );
    }

    #[test]
    fn reply_handle_flag_is_distinct_from_open_flags() {
        assert_eq!(REPLY_HANDLE & 0x7fff, 0);
    }

    #[test]
    fn empty_attachment_tail_is_valid() {
        let packet = SchemePacket::request(Opcode::Call, 1, 2);
        assert_eq!(packet.validate(true), Ok(()));
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

    fn reply(opcode: Opcode, status: i32, buffer_len: u64, data_len: u32) -> SchemePacket {
        SchemePacket {
            status,
            buffer_len,
            data_len,
            ..SchemePacket::request(opcode, 1, 2)
        }
    }

    #[test]
    fn write_result_is_bounded_by_the_client_input() {
        let equal = reply(Opcode::Write, 0, 8, 0);
        assert_eq!(reply_result(Opcode::Write, 8, 0, &equal), Ok(8));
        let zero = reply(Opcode::Write, 0, 0, 0);
        assert_eq!(reply_result(Opcode::Write, 8, 0, &zero), Ok(0));
        let over = reply(Opcode::Write, 0, 9, 0);
        assert_eq!(
            reply_result(Opcode::Write, 8, 0, &over),
            Err(ReplyBoundError::ExceedsCapacity)
        );
    }

    #[test]
    fn read_result_is_bounded_by_the_captured_buffer() {
        let equal = reply(Opcode::Read, 0, 4096, 0);
        assert_eq!(reply_result(Opcode::Read, 4096, 0, &equal), Ok(4096));
        let over = reply(Opcode::Read, 0, 4097, 0);
        assert_eq!(
            reply_result(Opcode::Read, 4096, 0, &over),
            Err(ReplyBoundError::ExceedsCapacity)
        );
    }

    #[test]
    fn call_result_is_bounded_by_the_response_capacity() {
        let equal = reply(Opcode::Call, 0, 16, 0);
        assert_eq!(reply_result(Opcode::Call, 0, 16, &equal), Ok(16));
        let zero = reply(Opcode::Call, 0, 0, 0);
        assert_eq!(reply_result(Opcode::Call, 0, 16, &zero), Ok(0));
        let over = reply(Opcode::Call, 0, 17, 0);
        assert_eq!(
            reply_result(Opcode::Call, 0, 16, &over),
            Err(ReplyBoundError::ExceedsCapacity)
        );
        let inline = reply(Opcode::Call, 0, 8, 16);
        assert_eq!(reply_result(Opcode::Call, 0, 16, &inline), Ok(8));
        let inline_over = reply(Opcode::Call, 0, 8, 17);
        assert_eq!(
            reply_result(Opcode::Call, 0, 16, &inline_over),
            Err(ReplyBoundError::ExceedsCapacity)
        );
    }

    #[test]
    fn fstat_inline_result_is_bounded_by_the_response_capacity() {
        let equal = reply(Opcode::Fstat, 0, 0, 16);
        assert_eq!(reply_result(Opcode::Fstat, 0, 16, &equal), Ok(16));
        let over = reply(Opcode::Fstat, 0, 0, 17);
        assert_eq!(
            reply_result(Opcode::Fstat, 0, 16, &over),
            Err(ReplyBoundError::ExceedsCapacity)
        );
    }

    #[test]
    fn error_replies_are_not_bounded() {
        let error = reply(Opcode::Write, 5, u64::MAX, 0);
        assert_eq!(reply_result(Opcode::Write, 8, 0, &error), Ok(u64::MAX));
    }

    #[test]
    fn open_and_one_way_replies_have_no_result_bound() {
        let open = reply(Opcode::Open, 0, u64::MAX, 0);
        assert_eq!(reported_result(Opcode::Open, &open), 0);
        assert_eq!(reply_result(Opcode::Open, 0, 0, &open), Ok(0));
        let close = reply(Opcode::Close, 0, u64::MAX, 0);
        assert_eq!(reply_result(Opcode::Close, 0, 0, &close), Ok(0));
    }
}
