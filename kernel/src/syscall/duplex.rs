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

use crate::process::duplex::{self, PendingHandle};
use crate::process::{self, handle::Object};
use crate::syscall::SyscallFrame;
use crate::syscall::basic::require_root;
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
/// Diagnostic flag for `duplex_pair`: run one cycle-collection pass, then write [`DuplexStats`].
///
/// The collector is explicit, never a read/write/send/recv hook. It costs one registry snapshot
/// and a scan of every live pair's queued handles; a pair with a live external endpoint is a root
/// and its receive queues are never cleared.
pub const DUPLEX_COLLECT: u32 = 0x4000_0000;
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
/// Returned in `msg_flags` when ancillary data did not fit (FreeBSD/NetBSD `MSG_CTRUNC`).
const MSG_CTRUNC: u32 = 0x20;
/// Requested full-length receive (FreeBSD/NetBSD `MSG_WAITALL`); refused, see [`recvmsg`].
const MSG_WAITALL: u32 = 0x40;
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
    if flags & DUPLEX_COLLECT != 0 {
        // Collection is a global pass over every live pair's queues, so it crosses handles owned
        // by other processes. The 0.1.0 authority model reserves that operation for root
        // (`require_root`, the same gate as clock/power administration); an unauthorised caller
        // fails closed before any queue is detached.
        require_root()?;
        duplex::collect_cycles();
    }
    if flags & (DUPLEX_COLLECT | DUPLEX_DIAGNOSTIC) != 0 {
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
///
/// A control buffer too small for the whole `SCM_RIGHTS` record is not an error: the data is
/// delivered, the record is dropped and `MSG_CTRUNC` is returned, matching the donor copyout rule
/// (NetBSD `sys/kern/uipc_syscalls.c:867`, FreeBSD `sys/kern/uipc_syscalls.c:1020`). `MSG_WAITALL`
/// is refused with `EOPNOTSUPP`: the raw receive path returns the queued chunk and cannot hold a
/// partial message open, and FreeBSD's 2026 `MSG_WAITALL`-with-ancillary behaviour is a known
/// disagreement, so refusing is the documented rule (no partial-ancillary delivery).
pub fn recvmsg(frame: &SyscallFrame) -> Result<usize, Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let handle = Handle(u32::try_from(frame.rdi).map_err(|_| Errno::EBADF)?);
    let end = endpoint(&caller, handle, Rights::READ)?;
    let mut message = read_message(frame.rsi)?;
    if message.flags & MSG_WAITALL != 0 {
        return Err(Errno::EOPNOTSUPP);
    }
    if message.flags != 0 {
        return Err(Errno::EINVAL);
    }
    let control_capacity = usize::try_from(message.control_len).map_err(|_| Errno::EMSGSIZE)?;
    let data = user_slice(message.data, message.data_len)?;
    // Let the queue hand over every record even when the caller's buffer is smaller; the donor
    // consumes the data and only then truncates the ancillary copy. The queue's own limit is the
    // largest legal send control buffer, so a legal record is never rejected there.
    let (copied, records) = duplex::recv(&end, data, duplex::DUPLEX_CONTROL_MAX)?;
    let (installed, truncated) = install_records(&caller, records, control_capacity)?;
    if let Err(error) = write_control(&message, &installed) {
        close_installed(&caller, &installed);
        return Err(error);
    }
    let control_len = installed.iter().fold(0usize, |total, record| {
        total.saturating_add(control_bytes(record.len()))
    });
    let handle_count = installed
        .iter()
        .fold(0usize, |total, record| total.saturating_add(record.len()));
    message.control_len = u64::try_from(control_len).map_err(|_| Errno::EINVAL)?;
    message.handle_count = u32::try_from(handle_count).map_err(|_| Errno::EMSGSIZE)?;
    message.flags = if truncated { MSG_CTRUNC } else { 0 };
    if let Err(error) = write_message(frame.rsi, &message) {
        // The caller cannot learn the installed handle values, so leaving them open would make
        // them unreachable. Close them exactly like the control write-back failure path.
        close_installed(&caller, &installed);
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
) -> Result<Vec<Vec<PendingHandle>>, Errno> {
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
    let mut total = 0usize;
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
        if count > duplex::DUPLEX_MAX_HANDLES.saturating_sub(total) {
            return Err(Errno::EMSGSIZE);
        }
        // One vector per cmsg header keeps the sender's record boundaries (K521); an empty
        // header carries no rights and is not queued.
        let mut record = Vec::new();
        record.try_reserve_exact(count).map_err(|_| Errno::ENOMEM)?;
        let ids = offset.saturating_add(CMSG_BYTES);
        let mut index = 0usize;
        while index < count {
            let raw = u32_at(&bytes, ids.saturating_add(index.saturating_mul(4)))?;
            let handle = Handle(raw);
            let (object, rights, lineage) =
                crate::syscall::handle_xfer::clone_transferable(caller, handle)?;
            record.push(PendingHandle::new(object, rights, lineage));
            index = index.saturating_add(1);
        }
        if !record.is_empty() {
            records.push(record);
        }
        total = total.saturating_add(count);
        offset = offset.saturating_add(record_len);
    }
    if offset != length {
        return Err(Errno::EINVAL);
    }
    Ok(records)
}

/// Install every record that fits in `capacity`, in order, and report whether a record was lost.
///
/// Donor rule (NetBSD `copyout_msg_control`, FreeBSD `uipc_syscalls.c`): whole records only, the
/// first one that does not fit stops the copy and sets `MSG_CTRUNC`; the data was already
/// delivered by the queue. Dropping the remaining records releases their pending handles.
fn install_records(
    caller: &Arc<process::Process>,
    records: Vec<Vec<PendingHandle>>,
    capacity: usize,
) -> Result<(Vec<Vec<u32>>, bool), Errno> {
    let mut installed = Vec::new();
    let mut used = 0usize;
    let mut truncated = false;
    for record in records {
        let needed = control_bytes(record.len());
        let Some(next) = used.checked_add(needed) else {
            truncated = true;
            break;
        };
        if next > capacity {
            truncated = true;
            break;
        }
        match install_handles(caller, record) {
            Ok(handles) => {
                used = next;
                installed.push(handles);
            }
            Err(error) => {
                close_installed(caller, &installed);
                return Err(error);
            }
        }
    }
    Ok((installed, truncated))
}

fn install_handles(
    caller: &Arc<process::Process>,
    handles: Vec<PendingHandle>,
) -> Result<Vec<u32>, Errno> {
    let mut installed = Vec::new();
    for pending in handles {
        let (object, rights, lineage) = pending.into_parts();
        let inserted = {
            let mut table = caller.handles.lock();
            table.insert_derived(object, rights, &lineage)
        };
        match inserted {
            Ok(handle) => {
                let node = caller.handles.lock().lineage_node(handle).ok();
                if let Some(node) = node {
                    // `insert_derived` stores the parent's *effective* rights, so a bit revoked
                    // between the send and this install is otherwise indistinguishable from a bit
                    // that was never granted (C17 wants EPERM, not EACCES). Register first, then
                    // copy the parent's revoked mask onto the child; a revoke after this point
                    // reaches the registered child through the normal descendant walk.
                    crate::process::handle::register_lineage(Arc::clone(&node));
                    let revoked = lineage.revoked();
                    if revoked != Rights::NONE {
                        node.revoke(revoked);
                    }
                }
                installed.push(handle.0);
            }
            Err(error) => {
                close_handles(caller, &installed);
                return Err(error);
            }
        }
    }
    Ok(installed)
}

fn close_handles(caller: &Arc<process::Process>, handles: &[u32]) {
    let mut table = caller.handles.lock();
    for raw in handles {
        let _ = table.close(Handle(*raw));
    }
}

fn close_installed(caller: &Arc<process::Process>, installed: &[Vec<u32>]) {
    for record in installed {
        close_handles(caller, record);
    }
}

fn write_control(message: &DuplexMessage, installed: &[Vec<u32>]) -> Result<(), Errno> {
    let total = installed.iter().fold(0usize, |bytes, record| {
        bytes.saturating_add(control_bytes(record.len()))
    });
    if total == 0 {
        return Ok(());
    }
    if total > usize::try_from(message.control_len).map_err(|_| Errno::EMSGSIZE)? {
        return Err(Errno::EMSGSIZE);
    }
    let mut buffer = Vec::new();
    buffer.try_reserve_exact(total).map_err(|_| Errno::ENOMEM)?;
    buffer.resize(total, 0);
    let mut base = 0usize;
    for record in installed {
        let bytes = control_bytes(record.len());
        if bytes == 0 {
            continue;
        }
        let header = ControlHeader {
            len: u32::try_from(bytes).map_err(|_| Errno::EMSGSIZE)?,
            level: SOL_SOCKET,
            kind: SCM_RIGHTS,
            count: u32::try_from(record.len()).map_err(|_| Errno::EMSGSIZE)?,
        };
        buffer
            .get_mut(base..base.saturating_add(4))
            .ok_or(Errno::EINVAL)?
            .copy_from_slice(&header.len.to_le_bytes());
        buffer
            .get_mut(base.saturating_add(4)..base.saturating_add(8))
            .ok_or(Errno::EINVAL)?
            .copy_from_slice(&header.level.to_le_bytes());
        buffer
            .get_mut(base.saturating_add(8)..base.saturating_add(12))
            .ok_or(Errno::EINVAL)?
            .copy_from_slice(&header.kind.to_le_bytes());
        buffer
            .get_mut(base.saturating_add(12)..base.saturating_add(16))
            .ok_or(Errno::EINVAL)?
            .copy_from_slice(&header.count.to_le_bytes());
        for (index, handle) in record.iter().enumerate() {
            let offset = base
                .saturating_add(CMSG_BYTES)
                .saturating_add(index.saturating_mul(4));
            buffer
                .get_mut(offset..offset.saturating_add(4))
                .ok_or(Errno::EINVAL)?
                .copy_from_slice(&handle.to_le_bytes());
        }
        base = base.saturating_add(bytes);
    }
    copy_to_user(UserSlice::new(message.control, total as u64)?, &buffer)
}

fn control_bytes(handles: usize) -> usize {
    if handles == 0 {
        0
    } else {
        CMSG_BYTES.saturating_add(handles.saturating_mul(4))
    }
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
