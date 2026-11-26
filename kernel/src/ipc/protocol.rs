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
///
/// C14: the version bumps only when an **existing** field layout changes. The reply-attachment
/// tail and the caller-credential fields are append-only, so the wire stays version 1 with
/// `attach_count == 0` for peers that do not use attachments.
pub const VERSION: u32 = 1;
/// Maximum path suffix carried by an `Open` packet.
pub const MAX_PATH: usize = 256;
/// Maximum inline data carried by a packet.
pub const MAX_DATA: usize = 64;
/// Maximum supplementary groups carried by a packet.
pub const MAX_GROUPS: usize = 16;
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

/// The kernel object vocabulary used by the C14 handle-transfer policy.
///
/// The kernel's `Object` enum maps to this vocabulary in `syscall/handle_xfer.rs`; keeping the
/// policy here makes the allow-list and the rights rule host-testable in `protocol_ut`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectKind {
    /// A process handle.
    Process,
    /// The boot initfs memory object.
    MemObjectInitfs,
    /// An anonymous memory object.
    MemObject,
    /// A namespace handle.
    Namespace,
    /// A thread handle.
    Thread,
    /// A wait-for-many event queue.
    EventQueue,
    /// A monotonic timer event source.
    Timer,
    /// A user-space doorbell event source.
    UserEvent,
    /// The root device capability.
    RootDevice,
    /// A device-memory window.
    MmioRegion,
    /// A read-only boot-time blob.
    BootBlob,
    /// A physically contiguous device buffer.
    DmaBuffer,
    /// A device interrupt source.
    Irq,
    /// An open scheme file.
    SchemeFile,
    /// A registered scheme server capability.
    SchemeServer,
    /// A server-side request handle.
    SchemeRequest,
    /// One end of an in-kernel pipe.
    Pipe,
    /// One end of a kernel duplex endpoint pair.
    Duplex,
}

impl ObjectKind {
    /// Every kind, for the conformance table.
    #[cfg(test)]
    pub const ALL: [Self; 18] = [
        Self::Process,
        Self::MemObjectInitfs,
        Self::MemObject,
        Self::Namespace,
        Self::Thread,
        Self::EventQueue,
        Self::Timer,
        Self::UserEvent,
        Self::RootDevice,
        Self::MmioRegion,
        Self::BootBlob,
        Self::DmaBuffer,
        Self::Irq,
        Self::SchemeFile,
        Self::SchemeServer,
        Self::SchemeRequest,
        Self::Pipe,
        Self::Duplex,
    ];
}

/// C14: only `MemObject`, `SchemeFile` and `SchemeServer` may cross a handle transfer.
pub const fn transferable(kind: ObjectKind) -> bool {
    matches!(
        kind,
        ObjectKind::MemObject
            | ObjectKind::MemObjectInitfs
            | ObjectKind::SchemeFile
            | ObjectKind::SchemeServer
            | ObjectKind::RootDevice
            | ObjectKind::MmioRegion
            | ObjectKind::BootBlob
            | ObjectKind::DmaBuffer
            | ObjectKind::Irq
            | ObjectKind::Pipe
            | ObjectKind::Duplex
            | ObjectKind::Timer
            | ObjectKind::UserEvent
    )
}

/// C14: transferred rights are the source rights AND the caller mask.
///
/// Unlike [`attachment_rights`], a zero mask is not "all current rights": the transfer still
/// succeeds and installs a handle with no rights (`Rights::NONE`).
pub const fn transfer_rights(source: Rights, mask: u32) -> Rights {
    source.intersect(Rights(mask))
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
    /// Move the server-side file cursor (PG-22).
    Seek = 8,
}

/// `Read`/`Write` flag: `packet.offset` is an explicit positional offset.
///
/// Without this bit the server uses its open-file-description cursor and the kernel sends
/// `offset = 0` (the pre-PG-22 behaviour).
pub const OFFSET_EXPLICIT: u16 = 1 << 0;

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
            8 => Self::Seek,
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
    /// Operation flags: open flags for `Open`, `OFFSET_EXPLICIT` for `Read`/`Write`, the
    /// `SEEK_*` whence for `Seek`.
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
    /// Caller real user id (version 2).
    pub uid: u32,
    /// Caller effective user id (version 2).
    pub euid: u32,
    /// Caller real group id (version 2).
    pub gid: u32,
    /// Caller effective group id (version 2).
    pub egid: u32,
    /// Number of valid entries in [`SchemePacket::groups`] (version 2).
    pub group_count: u32,
    /// Caller supplementary groups (version 2).
    pub groups: [u32; MAX_GROUPS],
    /// Length of the per-process root prefix in [`SchemePacket::path`] for a rooted `Open`;
    /// zero means unrooted (K224).
    pub root_len: u32,
    /// Cross-process trace id (`R-OBS-6`); zero means "not traced".
    pub trace_id: u64,
    /// Requested creation mode for `Open` (S731); zero otherwise. The serving filesystem masks
    /// it with the request's authenticated umask before the inode is published.
    pub mode: u32,
    /// The request's authenticated file-creation mask at submit time (S731).
    pub umask: u32,
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
            uid: 0,
            euid: 0,
            gid: 0,
            egid: 0,
            group_count: 0,
            groups: [0; MAX_GROUPS],
            root_len: 0,
            trace_id: 0,
            mode: 0,
            umask: 0,
        }
    }
}

/// Encoded packet size.
pub const PACKET_SIZE: usize = size_of::<SchemePacket>();

const _: () = {
    assert!(PACKET_SIZE == 544);
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
    assert!(offset_of!(SchemePacket, uid) == 436);
    assert!(offset_of!(SchemePacket, euid) == 440);
    assert!(offset_of!(SchemePacket, gid) == 444);
    assert!(offset_of!(SchemePacket, egid) == 448);
    assert!(offset_of!(SchemePacket, group_count) == 452);
    assert!(offset_of!(SchemePacket, groups) == 456);
    assert!(offset_of!(SchemePacket, root_len) == 520);
    assert!(offset_of!(SchemePacket, trace_id) == 528);
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
        Opcode::Fstat => {
            // An FSTAT without a client response buffer is the kernel's internal stat path: the
            // 64-byte record travels in the packet's inline data. Clients that provide a capture
            // keep the historical `data_len <= response_len` bound.
            if request_response_len == 0 {
                if u64::from(packet.data_len) > 64 {
                    return Err(ReplyBoundError::ExceedsCapacity);
                }
                return Ok(0);
            }
            request_response_len
        }
        Opcode::Open | Opcode::Close | Opcode::Dup | Opcode::Seek => return Ok(result),
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
    put_u32(out, offset_of!(SchemePacket, uid), packet.uid);
    put_u32(out, offset_of!(SchemePacket, euid), packet.euid);
    put_u32(out, offset_of!(SchemePacket, gid), packet.gid);
    put_u32(out, offset_of!(SchemePacket, egid), packet.egid);
    put_u32(
        out,
        offset_of!(SchemePacket, group_count),
        packet.group_count,
    );
    let mut index = 0usize;
    while index < MAX_GROUPS {
        let offset = offset_of!(SchemePacket, groups).saturating_add(index.saturating_mul(4));
        let group = packet.groups.get(index).copied().unwrap_or(0);
        put_u32(out, offset, group);
        index = index.saturating_add(1);
    }
    put_u32(out, offset_of!(SchemePacket, root_len), packet.root_len);
    put_u64(out, offset_of!(SchemePacket, trace_id), packet.trace_id);
    put_u32(out, offset_of!(SchemePacket, mode), packet.mode);
    put_u32(out, offset_of!(SchemePacket, umask), packet.umask);
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
    let mut groups = [0u32; MAX_GROUPS];
    let mut index = 0usize;
    while index < MAX_GROUPS {
        let offset = offset_of!(SchemePacket, groups).saturating_add(index.saturating_mul(4));
        let slot = groups.get_mut(index).ok_or(PacketError::TooShort)?;
        *slot = get_u32(input, offset)?;
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
        uid: get_u32(input, offset_of!(SchemePacket, uid))?,
        euid: get_u32(input, offset_of!(SchemePacket, euid))?,
        gid: get_u32(input, offset_of!(SchemePacket, gid))?,
        egid: get_u32(input, offset_of!(SchemePacket, egid))?,
        group_count: get_u32(input, offset_of!(SchemePacket, group_count))?,
        groups,
        root_len: get_u32(input, offset_of!(SchemePacket, root_len))?,
        trace_id: get_u64(input, offset_of!(SchemePacket, trace_id))?,
        mode: get_u32(input, offset_of!(SchemePacket, mode))?,
        umask: get_u32(input, offset_of!(SchemePacket, umask))?,
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
        if self.group_count as usize > MAX_GROUPS {
            return Err(PacketError::BadLength);
        }
        if self.root_len as usize > MAX_PATH {
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
#[path = "protocol_ut.rs"]
mod tests;
