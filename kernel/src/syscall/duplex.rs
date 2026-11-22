// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Raw duplex endpoint syscalls (`0x110..0x114`).
//!
//! The numbers stay kernel-owned until the reviewer promotes the contract into `raven-abi`.
//! Both endpoints live in the caller's existing handle table; there is no userspace server.
//! `duplex_sendmsg`/`duplex_recvmsg` carry `SCM_RIGHTS` handles and `duplex_peercred` exposes the
//! kernel-stamped creator identity.

use alloc::sync::Arc;
use alloc::vec::Vec;

use raven_abi::errno::Errno;
use raven_abi::types::{Handle, Rights};
use raven_usercopy::UserSlice;

use crate::process::duplex::{self, ControlRecord, PendingHandle};
use crate::process::{self, handle::Object};
use crate::syscall::SyscallFrame;
use crate::usercopy::{copy_from_user, copy_to_user};

/// Raw `duplex_pair(out: *mut [u32; 2], flags: u32) -> 0`.
pub const DUPLEX_PAIR: usize = 0x110;
/// Diagnostic flag for `duplex_pair`: write [`DuplexStats`] instead of creating a pair.
///
/// The raw duplex numbers are kernel-owned and not yet in `raven-abi`; this evidence flag lets the
/// K496 ownership matrix observe live pairs/endpoints, queued handles and queued control records
/// from the guest. It creates no object and is rejected by [`duplex::create`] when it reaches the
/// normal path.
pub const DUPLEX_DIAGNOSTIC: u32 = 0x8000_0000;
/// Raw `duplex_shutdown(handle: u32, how: u32) -> 0`.
pub const DUPLEX_SHUTDOWN: usize = 0x111;
/// Raw `duplex_sendmsg(handle: u32, msg: *mut DuplexMessage) -> bytes`.
pub const DUPLEX_SENDMSG: usize = 0x112;
/// Raw `duplex_recvmsg(handle: u32, msg: *mut DuplexMessage) -> bytes`.
pub const DUPLEX_RECVMSG: usize = 0x113;
/// Raw `duplex_peercred(handle: u32, out: *mut DuplexCred) -> 0`.
pub const DUPLEX_PEERCRED: usize = 0x114;

/// `SOL_SOCKET` control level.
const SOL_SOCKET: u32 = 1;
/// `SCM_RIGHTS` control type.
const SCM_RIGHTS: u32 = 1;
/// Bytes in one `DuplexMessage`.
const MESSAGE_BYTES: usize = 40;
/// Bytes in one control header.
const CMSG_BYTES: usize = 16;
/// Bytes in `DuplexCred`.
const CRED_BYTES: usize = 16;

/// Both endpoints carry data access plus duplication and transfer.
const END_RIGHTS: Rights =
    Rights(Rights::READ.0 | Rights::WRITE.0 | Rights::DUP.0 | Rights::TRANSFER.0);

/// User-visible duplex message header.
#[repr(C)]
struct DuplexMessage {
    /// Data pointer.
    data: u64,
    /// Data length for send; capacity for receive.
    data_len: u64,
    /// Control pointer.
    control: u64,
    /// Control length for send; capacity on entry and bytes used on return for receive.
    control_len: u64,
    /// `DUPLEX_MSG_NOSIGNAL` may be set for send; receive accepts zero.
    flags: u32,
    /// Number of installed handles on receive.
    handle_count: u32,
}

const _: () = assert!(core::mem::size_of::<DuplexMessage>() == MESSAGE_BYTES);

/// Ownership counters written by the `DUPLEX_DIAGNOSTIC` query.
#[repr(C)]
struct DuplexStats {
    /// Live pair objects.
    pairs: u64,
    /// Live endpoint objects.
    ends: u64,
    /// Handles waiting in a control record or datagram message.
    pending: u64,
    /// Queued stream control records.
    controls: u64,
}

/// Bytes in `DuplexStats`.
const STATS_BYTES: usize = 32;

const _: () = assert!(core::mem::size_of::<DuplexStats>() == STATS_BYTES);

/// User-visible peer credentials.
#[repr(C)]
struct DuplexCred {
    /// Creator pid.
    pid: u64,
    /// Creator real uid.
    uid: u32,
    /// Creator real gid.
    gid: u32,
}

const _: () = assert!(core::mem::size_of::<DuplexCred>() == CRED_BYTES);

/// One control record header: `len, level, type, count` followed by `count` handle ids.
#[repr(C)]
struct ControlHeader {
    len: u32,
    level: u32,
    kind: u32,
    count: u32,
}

const _: () = assert!(core::mem::size_of::<ControlHeader>() == CMSG_BYTES);

/// `duplex_pair(out, flags)`.
pub fn pair(frame: &SyscallFrame) -> Result<usize, Errno> {
    let flags = u32::try_from(frame.rsi).map_err(|_| Errno::EINVAL)?;
    if flags & DUPLEX_DIAGNOSTIC != 0 {
        return stats(frame);
    }
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let destination = UserSlice::new(frame.rdi, 8)?;
    let (left, right) = duplex::create(flags)?;
    let (left_handle, right_handle) = {
        let mut handles = caller.handles.lock();
        let left_handle = handles.insert(Object::Duplex(left), END_RIGHTS)?;
        match handles.insert(Object::Duplex(right), END_RIGHTS) {
            Ok(right_handle) => (left_handle, right_handle),
            Err(error) => {
                let _ = handles.close(left_handle);
                return Err(error);
            }
        }
    };
    let mut bytes = [0u8; 8];
    let (left, right) = bytes.split_at_mut(4);
    left.copy_from_slice(&left_handle.0.to_le_bytes());
    right.copy_from_slice(&right_handle.0.to_le_bytes());
    if let Err(error) = copy_to_user(destination, &bytes) {
        // The caller never saw the handles; close both so the endpoints cannot leak.
        let mut handles = caller.handles.lock();
        let _ = handles.close(right_handle);
        let _ = handles.close(left_handle);
        return Err(error);
    }
    Ok(0)
}

/// `duplex_pair(out, DUPLEX_DIAGNOSTIC)` evidence query.
fn stats(frame: &SyscallFrame) -> Result<usize, Errno> {
    let snapshot = duplex::stats();
    let values = [
        snapshot.pairs,
        snapshot.ends,
        snapshot.pending,
        snapshot.controls,
    ];
    let mut bytes = [0u8; STATS_BYTES];
    for (chunk, value) in bytes.chunks_exact_mut(8).zip(values) {
        chunk.copy_from_slice(&value.to_le_bytes());
    }
    copy_to_user(UserSlice::new(frame.rdi, STATS_BYTES as u64)?, &bytes)?;
    Ok(0)
}

/// `duplex_shutdown(handle, how)`.
pub fn shutdown(frame: &SyscallFrame) -> Result<usize, Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let handle = Handle(u32::try_from(frame.rdi).map_err(|_| Errno::EBADF)?);
    let how = u8::try_from(frame.rsi).map_err(|_| Errno::EINVAL)?;
    let needed = match how {
        duplex::SHUT_RD => Rights::READ,
        duplex::SHUT_WR => Rights::WRITE,
        duplex::SHUT_RDWR => Rights::READ.union(Rights::WRITE),
        _ => return Err(Errno::EINVAL),
    };
    let end = endpoint(&caller, handle, needed)?;
    duplex::shutdown(&end, how)?;
    Ok(0)
}

/// `duplex_sendmsg(handle, msg)`.
pub fn sendmsg(frame: &SyscallFrame) -> Result<usize, Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let handle = Handle(u32::try_from(frame.rdi).map_err(|_| Errno::EBADF)?);
    let end = endpoint(&caller, handle, Rights::WRITE)?;
    let message = read_message(frame.rsi)?;
    if message.flags & !duplex::DUPLEX_MSG_NOSIGNAL != 0 {
        return Err(Errno::EINVAL);
    }
    let data = user_slice(message.data, message.data_len)?;
    let records = parse_control(&caller, message.control, message.control_len)?;
    duplex::send(&end, data, records, message.flags)
}

/// `duplex_recvmsg(handle, msg)`.
pub fn recvmsg(frame: &SyscallFrame) -> Result<usize, Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let handle = Handle(u32::try_from(frame.rdi).map_err(|_| Errno::EBADF)?);
    let end = endpoint(&caller, handle, Rights::READ)?;
    let mut message = read_message(frame.rsi)?;
    // The donor flag is refused explicitly (FreeBSD unix-stream restarts on MSG_WAITALL while
    // externalizing ancillary data, `uipc_usrreq.c:1445,1739`); unknown bits stay EINVAL.
    if message.flags & duplex::MSG_WAITALL != 0 {
        return Err(Errno::EOPNOTSUPP);
    }
    if message.flags != 0 {
        return Err(Errno::EINVAL);
    }
    let control_capacity = usize::try_from(message.control_len).map_err(|_| Errno::EMSGSIZE)?;
    let data = user_slice(message.data, message.data_len)?;
    let (copied, records) = duplex::recv(&end, data)?;
    // NetBSD/FreeBSD deliver the data and the largest prefix of complete records that fits, set
    // MSG_CTRUNC, and release the records that did not fit. A SCM_RIGHTS record is never
    // partially copied (NetBSD copyout_msg_control 867-872).
    let (records, truncated) = fit_control(records, control_capacity);
    let installed = install_handles(&caller, records)?;
    let control_len = match write_control(&message, &installed) {
        Ok(bytes) => bytes,
        Err(error) => {
            close_records(&caller, &installed);
            return Err(error);
        }
    };
    message.control_len = u64::try_from(control_len).map_err(|_| Errno::EINVAL)?;
    message.handle_count = u32::try_from(installed.iter().map(Vec::len).sum::<usize>())
        .map_err(|_| Errno::EMSGSIZE)?;
    message.flags = if truncated { duplex::MSG_CTRUNC } else { 0 };
    if let Err(error) = write_message(frame.rsi, &message) {
        // The caller cannot learn the installed handle values, so leaving them open would make
        // them unreachable. Close them exactly like the control write-back failure path.
        close_records(&caller, &installed);
        return Err(error);
    }
    Ok(copied)
}

/// `duplex_peercred(handle, out)`.
pub fn peercred(frame: &SyscallFrame) -> Result<usize, Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let handle = Handle(u32::try_from(frame.rdi).map_err(|_| Errno::EBADF)?);
    let end = endpoint(&caller, handle, Rights::NONE)?;
    let credentials = end.credentials();
    let destination = UserSlice::new(frame.rsi, CRED_BYTES as u64)?;
    let mut bytes = [0u8; CRED_BYTES];
    bytes
        .get_mut(0..8)
        .ok_or(Errno::EINVAL)?
        .copy_from_slice(&credentials.pid.to_le_bytes());
    bytes
        .get_mut(8..12)
        .ok_or(Errno::EINVAL)?
        .copy_from_slice(&credentials.uid.to_le_bytes());
    bytes
        .get_mut(12..16)
        .ok_or(Errno::EINVAL)?
        .copy_from_slice(&credentials.gid.to_le_bytes());
    copy_to_user(destination, &bytes)?;
    Ok(0)
}

fn endpoint(
    caller: &Arc<process::Process>,
    handle: Handle,
    rights: Rights,
) -> Result<Arc<duplex::DuplexEnd>, Errno> {
    match caller.handles.lock().get(handle, rights)? {
        Object::Duplex(end) => Ok(Arc::clone(end)),
        _ => Err(Errno::EBADF),
    }
}

fn user_slice(pointer: u64, length: u64) -> Result<UserSlice, Errno> {
    if length == 0 {
        // No dereference happens for an empty range, so a null pointer is acceptable.
        UserSlice::new_in(0, 0, 0, 1)
    } else {
        UserSlice::new(pointer, length)
    }
}

fn read_message(address: u64) -> Result<DuplexMessage, Errno> {
    let mut bytes = [0u8; MESSAGE_BYTES];
    copy_from_user(&mut bytes, UserSlice::new(address, MESSAGE_BYTES as u64)?)?;
    Ok(DuplexMessage {
        data: u64_at(&bytes, 0)?,
        data_len: u64_at(&bytes, 8)?,
        control: u64_at(&bytes, 16)?,
        control_len: u64_at(&bytes, 24)?,
        flags: u32_at(&bytes, 32)?,
        handle_count: u32_at(&bytes, 36)?,
    })
}

fn write_message(address: u64, message: &DuplexMessage) -> Result<(), Errno> {
    let mut bytes = [0u8; MESSAGE_BYTES];
    bytes
        .get_mut(0..8)
        .ok_or(Errno::EINVAL)?
        .copy_from_slice(&message.data.to_le_bytes());
    bytes
        .get_mut(8..16)
        .ok_or(Errno::EINVAL)?
        .copy_from_slice(&message.data_len.to_le_bytes());
    bytes
        .get_mut(16..24)
        .ok_or(Errno::EINVAL)?
        .copy_from_slice(&message.control.to_le_bytes());
    bytes
        .get_mut(24..32)
        .ok_or(Errno::EINVAL)?
        .copy_from_slice(&message.control_len.to_le_bytes());
    bytes
        .get_mut(32..36)
        .ok_or(Errno::EINVAL)?
        .copy_from_slice(&message.flags.to_le_bytes());
    bytes
        .get_mut(36..40)
        .ok_or(Errno::EINVAL)?
        .copy_from_slice(&message.handle_count.to_le_bytes());
    copy_to_user(UserSlice::new(address, MESSAGE_BYTES as u64)?, &bytes)
}

fn parse_control(
    caller: &Arc<process::Process>,
    pointer: u64,
    length: u64,
) -> Result<Vec<ControlRecord>, Errno> {
    let length = usize::try_from(length).map_err(|_| Errno::EMSGSIZE)?;
    if length == 0 {
        return Ok(Vec::new());
    }
    if length > duplex::DUPLEX_CONTROL_MAX {
        return Err(Errno::EMSGSIZE);
    }
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(length).map_err(|_| Errno::ENOMEM)?;
    bytes.resize(length, 0);
    copy_from_user(&mut bytes, UserSlice::new(pointer, length as u64)?)?;

    let mut records = Vec::new();
    let mut total_handles = 0usize;
    let mut offset = 0usize;
    while offset < length {
        let remaining = length.saturating_sub(offset);
        if remaining < CMSG_BYTES {
            return Err(Errno::EINVAL);
        }
        let header_len = u32_at(&bytes, offset)?;
        let level = u32_at(&bytes, offset.saturating_add(4))?;
        let kind = u32_at(&bytes, offset.saturating_add(8))?;
        let count = u32_at(&bytes, offset.saturating_add(12))?;
        let count = usize::try_from(count).map_err(|_| Errno::EMSGSIZE)?;
        let record_len = usize::try_from(header_len).map_err(|_| Errno::EINVAL)?;
        if record_len < CMSG_BYTES
            || !record_len.is_multiple_of(4)
            || record_len > remaining
            || record_len != CMSG_BYTES.saturating_add(count.saturating_mul(4))
        {
            return Err(Errno::EINVAL);
        }
        if level != SOL_SOCKET || kind != SCM_RIGHTS {
            return Err(Errno::EINVAL);
        }
        if count > duplex::DUPLEX_MAX_HANDLES.saturating_sub(total_handles) {
            return Err(Errno::EMSGSIZE);
        }
        let ids = offset.saturating_add(CMSG_BYTES);
        let mut handles = Vec::new();
        let mut index = 0usize;
        while index < count {
            let raw = u32_at(&bytes, ids.saturating_add(index.saturating_mul(4)))?;
            let handle = Handle(raw);
            let (object, rights, lineage) =
                crate::syscall::handle_xfer::clone_transferable(caller, handle)?;
            handles.push(PendingHandle::new(object, rights, lineage));
            index = index.saturating_add(1);
        }
        total_handles = total_handles.saturating_add(count);
        records.push(ControlRecord::new(handles));
        offset = offset.saturating_add(record_len);
    }
    if offset != length {
        return Err(Errno::EINVAL);
    }
    Ok(records)
}

/// Keep the largest prefix of complete records that fits `capacity`.
///
/// A record is never split: NetBSD `copyout_msg_control` breaks before a `SCM_RIGHTS` record
/// that does not fit (`MSG_CTRUNC` 867-872), and FreeBSD disposes the whole excess external
/// control chain. Records after the first that does not fit are released too.
fn fit_control(records: Vec<ControlRecord>, capacity: usize) -> (Vec<ControlRecord>, bool) {
    let mut kept = Vec::new();
    let mut used = 0usize;
    let mut truncated = false;
    for record in records {
        let size = record.wire_len();
        if !truncated && used.saturating_add(size) <= capacity {
            used = used.saturating_add(size);
            kept.push(record);
        } else {
            truncated = true;
        }
    }
    (kept, truncated)
}

fn install_handles(
    caller: &Arc<process::Process>,
    records: Vec<ControlRecord>,
) -> Result<Vec<Vec<u32>>, Errno> {
    let mut installed: Vec<Vec<u32>> = Vec::new();
    let mut flat: Vec<u32> = Vec::new();
    for record in records {
        let mut handles = Vec::new();
        for pending in record.into_handles() {
            let (object, rights, lineage) = pending.into_parts();
            let inserted = {
                let mut table = caller.handles.lock();
                table.insert_derived(object, rights, &lineage)
            };
            match inserted {
                Ok(handle) => {
                    let node = caller.handles.lock().lineage_node(handle).ok();
                    if let Some(node) = node {
                        // `insert_derived` stores the parent's *effective* rights, so a bit
                        // revoked between the send and this install is otherwise
                        // indistinguishable from a bit that was never granted (C17 wants EPERM,
                        // not EACCES). Register first, then copy the parent's revoked mask onto
                        // the child; a revoke after this point reaches the registered child
                        // through the normal descendant walk.
                        crate::process::handle::register_lineage(Arc::clone(&node));
                        let revoked = lineage.revoked();
                        if revoked != Rights::NONE {
                            node.revoke(revoked);
                        }
                    }
                    flat.push(handle.0);
                    handles.push(handle.0);
                }
                Err(error) => {
                    close_installed(caller, &flat);
                    return Err(error);
                }
            }
        }
        installed.push(handles);
    }
    Ok(installed)
}

fn close_installed(caller: &Arc<process::Process>, installed: &[u32]) {
    for raw in installed {
        let _ = caller.handles.lock().close(Handle(*raw));
    }
}

fn close_records(caller: &Arc<process::Process>, installed: &[Vec<u32>]) {
    for handles in installed {
        close_installed(caller, handles);
    }
}

/// Write one control header per record and return the bytes written.
///
/// Keeping the headers separate is the donor behaviour (NetBSD `unp_externalize`; FreeBSD
/// `unp_externalize` 3778-3925 and `recvmsg` 996-1028); the K496 flattening is gone.
fn write_control(message: &DuplexMessage, installed: &[Vec<u32>]) -> Result<usize, Errno> {
    let bytes = records_bytes(installed);
    if bytes == 0 {
        return Ok(0);
    }
    if bytes > usize::try_from(message.control_len).map_err(|_| Errno::EMSGSIZE)? {
        return Err(Errno::EMSGSIZE);
    }
    let mut buffer = Vec::new();
    buffer.try_reserve_exact(bytes).map_err(|_| Errno::ENOMEM)?;
    buffer.resize(bytes, 0);
    let mut offset = 0usize;
    for handles in installed {
        let record_len = record_bytes(handles.len());
        let header = ControlHeader {
            len: u32::try_from(record_len).map_err(|_| Errno::EMSGSIZE)?,
            level: SOL_SOCKET,
            kind: SCM_RIGHTS,
            count: u32::try_from(handles.len()).map_err(|_| Errno::EMSGSIZE)?,
        };
        buffer
            .get_mut(offset..offset.saturating_add(4))
            .ok_or(Errno::EINVAL)?
            .copy_from_slice(&header.len.to_le_bytes());
        buffer
            .get_mut(offset.saturating_add(4)..offset.saturating_add(8))
            .ok_or(Errno::EINVAL)?
            .copy_from_slice(&header.level.to_le_bytes());
        buffer
            .get_mut(offset.saturating_add(8)..offset.saturating_add(12))
            .ok_or(Errno::EINVAL)?
            .copy_from_slice(&header.kind.to_le_bytes());
        buffer
            .get_mut(offset.saturating_add(12)..offset.saturating_add(16))
            .ok_or(Errno::EINVAL)?
            .copy_from_slice(&header.count.to_le_bytes());
        let mut index = 0usize;
        for handle in handles {
            let id_offset = offset
                .saturating_add(CMSG_BYTES)
                .saturating_add(index.saturating_mul(4));
            buffer
                .get_mut(id_offset..id_offset.saturating_add(4))
                .ok_or(Errno::EINVAL)?
                .copy_from_slice(&handle.to_le_bytes());
            index = index.saturating_add(1);
        }
        offset = offset.saturating_add(record_len);
    }
    copy_to_user(UserSlice::new(message.control, bytes as u64)?, &buffer)?;
    Ok(bytes)
}

fn record_bytes(handles: usize) -> usize {
    CMSG_BYTES.saturating_add(handles.saturating_mul(4))
}

fn records_bytes(installed: &[Vec<u32>]) -> usize {
    installed
        .iter()
        .map(|handles| record_bytes(handles.len()))
        .sum()
}

fn u32_at(bytes: &[u8], offset: usize) -> Result<u32, Errno> {
    let slice = bytes
        .get(offset..offset.saturating_add(4))
        .ok_or(Errno::EINVAL)?;
    let array: [u8; 4] = slice.try_into().map_err(|_| Errno::EINVAL)?;
    Ok(u32::from_le_bytes(array))
}

fn u64_at(bytes: &[u8], offset: usize) -> Result<u64, Errno> {
    let slice = bytes
        .get(offset..offset.saturating_add(8))
        .ok_or(Errno::EINVAL)?;
    let array: [u8; 8] = slice.try_into().map_err(|_| Errno::EINVAL)?;
    Ok(u64::from_le_bytes(array))
}
