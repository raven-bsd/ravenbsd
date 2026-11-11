// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Packet routing between scheme clients and servers.

use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use alloc::borrow::ToOwned;
use alloc::sync::Arc;
use alloc::vec::Vec;

use raven_abi::errno::Errno;
use raven_abi::types::{Handle, Rights};
use raven_usercopy::UserSlice;
use spin::Mutex;

use crate::process::handle::Object;
use crate::process::{self, Process};
use crate::sched;
use crate::usercopy::{copy_from_user, copy_to_user};

use super::capture;
use super::model;
use super::protocol::{self, Opcode, PACKET_SIZE, REPLY_HANDLE, SchemePacket, attachment_rights};
use super::{PendingRequest, RequestBuffers, SchemeFile};
use super::{next_file_id, next_request_id, registry};

/// Maximum outstanding client requests one process may hold.
const MAX_CLIENT_REQUESTS: usize = 1024;

/// K66 wire open-flag bits (`raven-proto-fs::WIRE_OPEN_*`). The kernel carries the set and
/// rejects unknown bits; combination checks are the server's typed `OpenFlags` job.
const WIRE_OPEN_MASK: u32 = 0xFF;
/// Read access bit.
const WIRE_OPEN_READ: u32 = 1;
/// Write access bit.
const WIRE_OPEN_WRITE: u32 = 2;
/// Access-mode mask.
const WIRE_OPEN_ACCESS: u32 = WIRE_OPEN_READ | WIRE_OPEN_WRITE;

/// One deferred last-close notification: one `CLOSE` per server reference, delivered when the
/// queue is drained from a shallow server context.
struct CloseNotice {
    client: Arc<Process>,
    server: Arc<Process>,
    id: u64,
    cookie: u64,
    count: u32,
}

/// Last-close notifications produced by `SchemeFile::drop`.
///
/// A drop can run inside the deep server `SEND` path (when an in-flight request was the last
/// holder), so the notice is queued instead of building requests there. `recv` drains it before
/// taking the next request.
static CLOSE_NOTICES: Mutex<Vec<CloseNotice>> = Mutex::new(Vec::new());

/// Open a path on `server` and return a new scheme-file handle.
pub fn open(
    caller: &Arc<Process>,
    server: Arc<Process>,
    path: &str,
    flags: u32,
) -> Result<Handle, Errno> {
    if flags & !WIRE_OPEN_MASK != 0 {
        return Err(Errno::EINVAL);
    }
    // Rights follow only the access bits; O_TRUNC/O_APPEND/O_EXCL/O_DIRECTORY/O_NOFOLLOW travel
    // through the wire flags and are validated by the server's `OpenFlags`. A SchemeFile is a
    // transferable capability (C14), so retain TRANSFER for attachments and cross-process
    // references.
    let rights = model::open_rights(flags & WIRE_OPEN_ACCESS)
        .ok_or(Errno::EINVAL)?
        .union(Rights::TRANSFER);
    let file = Arc::new(SchemeFile {
        id: next_file_id(),
        server: Arc::clone(&server),
        server_cookie: AtomicU64::new(0),
        server_refs: AtomicU32::new(1),
    });
    let handle = caller
        .handles
        .lock()
        .insert(Object::SchemeFile(Arc::clone(&file)), rights)?;
    let mut packet = SchemePacket::request(Opcode::Open, next_request_id(), file.id);
    packet.flags = flags as u16;
    if packet.set_path(path.as_bytes()).is_err() {
        // The server never saw an OPEN, so no CLOSE is owed.
        file.server_refs.store(0, Ordering::Release);
        let _ = caller.handles.lock().close(handle);
        return Err(Errno::ENAMETOOLONG);
    }
    let request = PendingRequest::new(
        Arc::clone(caller),
        server,
        Some(Arc::clone(&file)),
        Opcode::Open,
        packet,
        RequestBuffers::default(),
    );
    let result = submit(&request).and_then(|_| wait(caller, &request));
    match result {
        Ok((0, _)) => {
            let response = *request.response.lock();
            if response.flags & REPLY_HANDLE != 0 && response.attach_count == 1 {
                let attached = response.attach[0].handle;
                let attached = Handle(attached);
                // The attachment replaces this provisional SchemeFile; it owns no server state.
                file.server_refs.store(0, Ordering::Release);
                let _ = caller.handles.lock().close(handle);
                return Ok(attached);
            }
            file.server_cookie
                .store(response.server_cookie, Ordering::Release);
            Ok(handle)
        }
        Ok((status, _)) => {
            // The server rejected the OPEN; it created no description to close.
            file.server_refs.store(0, Ordering::Release);
            let _ = caller.handles.lock().close(handle);
            Err(status_errno(status))
        }
        Err(error) => {
            // The OPEN may have reached the server before the wait failed; send one CLOSE so a
            // description the client cannot see is not leaked. An unknown id is harmless.
            let _ = caller.handles.lock().close(handle);
            Err(error)
        }
    }
}

/// Read from a scheme file.
pub fn read(
    caller: &Arc<Process>,
    file: &Arc<SchemeFile>,
    address: u64,
    len: u64,
) -> Result<usize, Errno> {
    if len == 0 {
        return Ok(0);
    }
    ensure_server(&file.server)?;
    let capture = capture::create(caller, &file.server, address, len, true)?;
    let mut packet = SchemePacket::request(Opcode::Read, next_request_id(), file.server_id());
    packet.buffer_len = len;
    packet.offset = 0;
    packet.server_cookie = file.server_cookie.load(Ordering::Acquire);
    let request = PendingRequest::new(
        Arc::clone(caller),
        Arc::clone(&file.server),
        Some(Arc::clone(file)),
        Opcode::Read,
        packet,
        RequestBuffers {
            buffer: Some(capture),
            response: None,
        },
    );
    let (status, result) = submit(&request).and_then(|_| wait(caller, &request))?;
    if status != 0 {
        return Err(status_errno(status));
    }
    Ok(usize::try_from(result).unwrap_or(usize::MAX))
}

/// Write to a scheme file.
pub fn write(
    caller: &Arc<Process>,
    file: &Arc<SchemeFile>,
    address: u64,
    len: u64,
) -> Result<usize, Errno> {
    if len == 0 {
        return Ok(0);
    }
    ensure_server(&file.server)?;
    let capture = capture::create(caller, &file.server, address, len, false)?;
    let mut packet = SchemePacket::request(Opcode::Write, next_request_id(), file.server_id());
    packet.buffer_len = len;
    packet.offset = 0;
    packet.server_cookie = file.server_cookie.load(Ordering::Acquire);
    let request = PendingRequest::new(
        Arc::clone(caller),
        Arc::clone(&file.server),
        Some(Arc::clone(file)),
        Opcode::Write,
        packet,
        RequestBuffers {
            buffer: Some(capture),
            response: None,
        },
    );
    let (status, result) = submit(&request).and_then(|_| wait(caller, &request))?;
    if status != 0 {
        return Err(status_errno(status));
    }
    Ok(usize::try_from(result).unwrap_or(usize::MAX))
}

/// Close a scheme file (the local handle is always closed; the server sees the CLOSE only when
/// the last kernel reference goes away).
pub fn close(caller: &Arc<Process>, handle: Handle) -> Result<(), Errno> {
    let file = {
        let handles = caller.handles.lock();
        match handles.get(handle, Rights::NONE)? {
            Object::SchemeFile(file) => Arc::clone(file),
            _ => return Err(Errno::EBADF),
        }
    };
    crate::event::on_handle_close(caller, handle);
    caller.handles.lock().close(handle)?;
    if Arc::strong_count(&file) == 1 {
        // No other handle, transfer or in-flight request references this open-file description.
        // Build the CLOSE requests here, in the shallow close-syscall context, so they are
        // queued before the caller's next request.
        let references = file.server_refs.swap(0, Ordering::AcqRel);
        if references > 0 {
            close_notify(
                &file.server,
                file.server_id(),
                file.server_cookie.load(Ordering::Acquire),
                references,
            );
        }
    }
    Ok(())
}

/// Duplicate a scheme-file handle.
pub fn dup(caller: &Arc<Process>, handle: Handle, mask: u32) -> Result<Handle, Errno> {
    let file = {
        let handles = caller.handles.lock();
        match handles.get(handle, Rights::NONE)? {
            Object::SchemeFile(file) => Arc::clone(file),
            _ => return Err(Errno::EBADF),
        }
    };
    let duplicate = caller.handles.lock().dup(handle, Rights(mask))?;
    let mut packet = SchemePacket::request(Opcode::Dup, next_request_id(), file.server_id());
    packet.server_cookie = file.server_cookie.load(Ordering::Acquire);
    let request = PendingRequest::new(
        Arc::clone(caller),
        Arc::clone(&file.server),
        Some(Arc::clone(&file)),
        Opcode::Dup,
        packet,
        RequestBuffers::default(),
    );
    // One server reference per duplicate. The notification must be enqueued; a failed submit
    // rolls the duplicate back instead of leaving the server's count short.
    file.server_refs.fetch_add(1, Ordering::AcqRel);
    if let Err(error) = submit(&request) {
        file.server_refs.fetch_sub(1, Ordering::AcqRel);
        let _ = caller.handles.lock().close(duplicate);
        return Err(error);
    }
    Ok(duplicate)
}

/// Stat a scheme file; the server returns inline bytes in the response packet.
pub fn fstat(
    caller: &Arc<Process>,
    file: &Arc<SchemeFile>,
    address: u64,
    len: u64,
) -> Result<(), Errno> {
    let response = if len > 0 {
        Some(capture::create(caller, &file.server, address, len, true)?)
    } else {
        None
    };
    let mut packet = SchemePacket::request(Opcode::Fstat, next_request_id(), file.server_id());
    packet.server_cookie = file.server_cookie.load(Ordering::Acquire);
    packet.response_len = len;
    let request = PendingRequest::new(
        Arc::clone(caller),
        Arc::clone(&file.server),
        Some(Arc::clone(file)),
        Opcode::Fstat,
        packet,
        RequestBuffers {
            buffer: None,
            response,
        },
    );
    let (status, _) = submit(&request).and_then(|_| wait(caller, &request))?;
    if status != 0 {
        return Err(status_errno(status));
    }
    Ok(())
}

/// Typed call with an optional response buffer.
pub fn call(
    caller: &Arc<Process>,
    file: &Arc<SchemeFile>,
    request_address: u64,
    request_len: u64,
    response_address: u64,
    response_len: u64,
) -> Result<usize, Errno> {
    ensure_server(&file.server)?;
    let request_capture = if request_len > 0 {
        Some(capture::create(
            caller,
            &file.server,
            request_address,
            request_len,
            false,
        )?)
    } else {
        None
    };
    let response_capture = if response_len > 0 {
        Some(capture::create(
            caller,
            &file.server,
            response_address,
            response_len,
            true,
        )?)
    } else {
        None
    };
    let mut packet = SchemePacket::request(Opcode::Call, next_request_id(), file.server_id());
    packet.buffer_len = request_len;
    packet.response_len = response_len;
    packet.server_cookie = file.server_cookie.load(Ordering::Acquire);
    let request = PendingRequest::new(
        Arc::clone(caller),
        Arc::clone(&file.server),
        Some(Arc::clone(file)),
        Opcode::Call,
        packet,
        RequestBuffers {
            buffer: request_capture,
            response: response_capture,
        },
    );
    let (status, result) = submit(&request).and_then(|_| wait(caller, &request))?;
    if status != 0 {
        return Err(status_errno(status));
    }
    Ok(usize::try_from(result).unwrap_or(usize::MAX))
}

/// Receive the next request packet; returns a server-side request handle.
pub fn recv(caller: &Arc<Process>, packet_address: u64, capacity: u64) -> Result<Handle, Errno> {
    if capacity < PACKET_SIZE as u64 {
        return Err(Errno::EINVAL);
    }
    if caller.schemes.lock().is_empty() {
        return Err(Errno::EINVAL);
    }
    loop {
        // Last-close notifications can be produced by a drop in the deep SEND path; submit them
        // here, where the stack is shallow, before looking for the next request.
        drain_close_notices();
        let request = {
            let mut queue = caller.scheme_queue.lock();
            queue.pop_front()
        };
        if let Some(request) = request {
            if request.cancelled.load(Ordering::Acquire) || request.done.load(Ordering::Acquire) {
                continue;
            }
            return deliver(caller, request, packet_address);
        }

        let token = sched::begin_block()?;
        let queued = {
            let mut queue = caller.scheme_queue.lock();
            if let Some(request) = queue.pop_front() {
                drop(queue);
                sched::end_block(token);
                if request.cancelled.load(Ordering::Acquire) || request.done.load(Ordering::Acquire)
                {
                    continue;
                }
                return deliver(caller, request, packet_address);
            }
            let queued = caller.scheme_waiters.enqueue_if(caller.id, 0, || Ok(true));
            drop(queue);
            queued
        };
        match queued {
            Ok(true) => {
                let _ = sched::block_current();
            }
            Ok(false) => {}
            Err(error) => {
                sched::end_block(token);
                return Err(error);
            }
        }
        sched::end_block(token);
    }
}

/// Send a response for a server-side request handle.
pub fn send(
    caller: &Arc<Process>,
    request_handle: Handle,
    packet_address: u64,
    length: u64,
) -> Result<(), Errno> {
    if length < PACKET_SIZE as u64 {
        return Err(Errno::EINVAL);
    }
    let mut bytes = [0u8; PACKET_SIZE];
    let source = UserSlice::new(packet_address, PACKET_SIZE as u64)?;
    copy_from_user(&mut bytes, source)?;
    let mut packet = protocol::decode(&bytes).map_err(|_| Errno::EPROTO)?;
    packet.validate(true).map_err(|_| Errno::EPROTO)?;

    let request = {
        let handles = caller.handles.lock();
        match handles.get(request_handle, Rights::WRITE)? {
            Object::SchemeRequest(request) => Arc::clone(request),
            _ => return Err(Errno::EBADF),
        }
    };
    if !Arc::ptr_eq(&request.server, caller) {
        return Err(Errno::EACCES);
    }
    if request.done.load(Ordering::Acquire) {
        return Err(Errno::EIO);
    }
    if packet.request_id != request.id || packet.opcode != request.opcode.as_u16() {
        return Err(Errno::EPROTO);
    }
    let bounds = {
        let request_packet = request.packet.lock();
        protocol::reply_result(
            request.opcode,
            request_packet.buffer_len,
            request_packet.response_len,
            &packet,
        )
    };
    let result = match bounds {
        Ok(result) => result,
        Err(_) => {
            // Strict policy: a successful reply that claims more than the request capacity is a
            // protocol error. Complete the client, otherwise it would wait forever.
            request.fail(Errno::EPROTO);
            retire(caller, &request, request_handle);
            return Err(Errno::EPROTO);
        }
    };
    let installed = match install_attachments(caller, &request, &mut packet) {
        Ok(installed) => installed,
        Err(error) => {
            request.fail(error);
            retire(caller, &request, request_handle);
            return Err(error);
        }
    };
    if let Err(error) = write_reply_buffer(&request, &packet) {
        rollback_attachments(&request.client, &installed);
        request.fail(error);
        retire(caller, &request, request_handle);
        return Err(error);
    }
    if request.opcode == Opcode::Open
        && packet.status == 0
        && let Some(file) = request.file.as_ref()
    {
        file.server_cookie
            .store(packet.server_cookie, Ordering::Release);
    }
    let status = packet.status;
    request.complete(status, result, Some(packet));
    retire(caller, &request, request_handle);
    Ok(())
}

/// Remove a completed request from the server's accounting and close its request handle.
fn retire(server: &Arc<Process>, request: &Arc<PendingRequest>, request_handle: Handle) {
    server
        .active_requests
        .lock()
        .retain(|item| item.id != request.id);
    let _ = server.handles.lock().close(request_handle);
}

fn install_attachments(
    server: &Arc<Process>,
    request: &Arc<PendingRequest>,
    packet: &mut SchemePacket,
) -> Result<[Option<Handle>; 4], Errno> {
    let count = usize::try_from(packet.attach_count).map_err(|_| Errno::EPROTO)?;
    let mut source = [None, None, None, None];
    let mut index = 0usize;
    while index < count {
        let attachment = packet.attach.get(index).ok_or(Errno::EPROTO)?;
        let slot = source.get_mut(index).ok_or(Errno::EPROTO)?;
        *slot = Some(crate::syscall::handle_xfer::clone_transferable(
            server,
            Handle(attachment.handle),
        )?);
        index = index.saturating_add(1);
    }
    let mut installed = [None; 4];
    let mut client = request.client.handles.lock();
    index = 0;
    while index < count {
        let (object, rights) = source
            .get_mut(index)
            .and_then(Option::take)
            .ok_or(Errno::EPROTO)?;
        let attachment = packet.attach.get_mut(index).ok_or(Errno::EPROTO)?;
        match client.insert(object, attachment_rights(rights, attachment.rights_mask)) {
            Ok(handle) => {
                attachment.handle = handle.0;
                let slot = installed.get_mut(index).ok_or(Errno::EPROTO)?;
                *slot = Some(handle);
            }
            Err(error) => {
                for handle in installed.iter().flatten() {
                    let _ = client.close(*handle);
                }
                return Err(error);
            }
        }
        index = index.saturating_add(1);
    }
    Ok(installed)
}

/// Put a non-Open reply's inline data and attachment pairs in the client reply buffer while its
/// capture is still mapped into the server. `SEND` rolls back newly installed handles if this
/// fails, so no unreachable client capabilities are left behind.
fn write_reply_buffer(request: &Arc<PendingRequest>, packet: &SchemePacket) -> Result<(), Errno> {
    let count = usize::try_from(packet.attach_count).map_err(|_| Errno::EPROTO)?;
    if request.opcode == Opcode::Open {
        if count == 0 {
            return Ok(());
        }
        if packet.flags & REPLY_HANDLE != 0 && count == 1 {
            return Ok(());
        }
        return Err(Errno::E2BIG);
    }

    let (capture_address, mut data_len, capacity, copy_data) = {
        let request_packet = request.packet.lock();
        let buffers = request.buffers.lock();
        match request.opcode {
            Opcode::Read => (
                buffers.buffer.as_ref().map(capture::Capture::address),
                packet.buffer_len,
                request_packet.buffer_len,
                false,
            ),
            // A `Call` reply without attachments keeps the S11 semantics: the server already
            // wrote the response buffer itself, so inline data is not kernel-delivered.
            Opcode::Call => (
                buffers.response.as_ref().map(capture::Capture::address),
                if count == 0 {
                    0
                } else {
                    u64::from(packet.data_len)
                },
                request_packet.response_len,
                true,
            ),
            Opcode::Fstat => (
                buffers.response.as_ref().map(capture::Capture::address),
                u64::from(packet.data_len),
                request_packet.response_len,
                true,
            ),
            Opcode::Write | Opcode::Close | Opcode::Dup => {
                if count == 0 {
                    return Ok(());
                }
                return Err(Errno::E2BIG);
            }
            Opcode::Open => return Err(Errno::EPROTO),
        }
    };
    // Fstat without a captured buffer keeps the S11 truncation semantics: no bytes delivered.
    if request.opcode == Opcode::Fstat && capture_address.is_none() {
        data_len = 0;
    }
    let pairs_len = u64::try_from(count)
        .map_err(|_| Errno::E2BIG)?
        .checked_mul(8)
        .ok_or(Errno::E2BIG)?;
    let total = data_len.checked_add(pairs_len).ok_or(Errno::E2BIG)?;
    if total > capacity {
        return Err(Errno::E2BIG);
    }
    if total == 0 {
        return Ok(());
    }
    let start = capture_address.ok_or(Errno::E2BIG)?;
    if copy_data && data_len > 0 {
        let bytes = packet.data_bytes();
        copy_to_user(UserSlice::new(start, data_len)?, bytes)?;
    }
    if count == 0 {
        return Ok(());
    }
    let mut pairs = [0u8; 32];
    let mut index = 0usize;
    while index < count {
        let attachment = packet.attach.get(index).ok_or(Errno::EPROTO)?;
        let offset = index.saturating_mul(8);
        let handle = attachment.handle.to_le_bytes();
        let mask = attachment.rights_mask.to_le_bytes();
        let handle_destination = pairs
            .get_mut(offset..offset.saturating_add(4))
            .ok_or(Errno::EPROTO)?;
        handle_destination.copy_from_slice(&handle);
        let mask_destination = pairs
            .get_mut(offset.saturating_add(4)..offset.saturating_add(8))
            .ok_or(Errno::EPROTO)?;
        mask_destination.copy_from_slice(&mask);
        index = index.saturating_add(1);
    }
    let destination = start.checked_add(data_len).ok_or(Errno::E2BIG)?;
    let length = usize::try_from(pairs_len).map_err(|_| Errno::E2BIG)?;
    copy_to_user(
        UserSlice::new(destination, pairs_len)?,
        pairs.get(..length).ok_or(Errno::EPROTO)?,
    )
}

fn rollback_attachments(client: &Arc<Process>, installed: &[Option<Handle>; 4]) {
    let mut handles = client.handles.lock();
    for handle in installed.iter().flatten() {
        let _ = handles.close(*handle);
    }
}

fn deliver(
    caller: &Arc<Process>,
    request: Arc<PendingRequest>,
    packet_address: u64,
) -> Result<Handle, Errno> {
    let packet = request.deliverable_packet();
    let mut bytes = [0u8; PACKET_SIZE];
    if protocol::encode(&packet, &mut bytes).is_err() {
        request.fail(Errno::EPROTO);
        return Err(Errno::EPROTO);
    }
    let destination = match UserSlice::new(packet_address, PACKET_SIZE as u64) {
        Ok(destination) => destination,
        Err(error) => {
            // K2: the request was already removed from the server queue. Complete the client
            // with EPROTO instead of leaving it blocked forever on an invalid destination.
            request.fail(Errno::EPROTO);
            return Err(error);
        }
    };
    if let Err(error) = copy_to_user(destination, &bytes) {
        request.fail(Errno::EPROTO);
        return Err(error);
    }
    let handle = match caller
        .handles
        .lock()
        .insert(Object::SchemeRequest(Arc::clone(&request)), Rights::WRITE)
    {
        Ok(handle) => handle,
        Err(error) => {
            request.fail(error);
            return Err(error);
        }
    };
    caller.active_requests.lock().push(Arc::clone(&request));
    Ok(handle)
}

fn submit(request: &Arc<PendingRequest>) -> Result<(), Errno> {
    submit_checked(request, true)
}

/// Enqueue one close notification without the client-request cap: the last-close handoff must
/// not be dropped because the dying process has many other requests in flight.
fn submit_close(request: &Arc<PendingRequest>) -> Result<(), Errno> {
    submit_checked(request, false)
}

fn submit_checked(request: &Arc<PendingRequest>, bounded: bool) -> Result<(), Errno> {
    // Hold the server state lock while enqueueing: `notify_exit` marks the process Exited under
    // the same lock and only then drains the queue, so a request cannot be pushed into a dead
    // server's queue after the drain.
    let state = request.server.state.lock();
    if *state != process::ProcessState::Running {
        return Err(Errno::EIO);
    }
    // Bound the client's outstanding list independently of the delivery queue. A client that
    // loops one-way CLOSE/DUP against a slow server cannot grow kernel memory without limit.
    {
        let mut client_requests = request.client.client_requests.lock();
        if bounded && client_requests.len() >= MAX_CLIENT_REQUESTS {
            return Err(Errno::EAGAIN);
        }
        client_requests.push(Arc::clone(request));
    }
    request
        .server
        .scheme_queue
        .lock()
        .push_back(Arc::clone(request));
    drop(state);
    request
        .server
        .scheme_waiters
        .wake(request.server.id, 0, usize::MAX);
    crate::event::notify_all();
    Ok(())
}

fn ensure_server(server: &Arc<Process>) -> Result<(), Errno> {
    let state = server.state.lock();
    if *state == process::ProcessState::Running {
        Ok(())
    } else {
        Err(Errno::EIO)
    }
}

fn wait(caller: &Arc<Process>, request: &Arc<PendingRequest>) -> Result<(i32, u64), Errno> {
    loop {
        if request.done.load(Ordering::Acquire) {
            break;
        }
        let token = sched::begin_block()?;
        if request.done.load(Ordering::Acquire) {
            sched::end_block(token);
            break;
        }
        let queued = request
            .waiters
            .enqueue_if(request.id, 0, || Ok(!request.done.load(Ordering::Acquire)))?;
        if queued {
            let _ = sched::block_current();
        }
        sched::end_block(token);
    }
    caller
        .client_requests
        .lock()
        .retain(|item| !Arc::ptr_eq(item, request));
    let status = request.status.load(Ordering::Acquire);
    if !model::valid_status(status) {
        return Err(Errno::EPROTO);
    }
    Ok((status, request.result_len.load(Ordering::Acquire)))
}

/// Deliver one `CLOSE` per server reference when the last kernel handle is released.
///
/// Called from an explicit `close` syscall (shallow context); the requests are queued before the
/// caller's next request.
pub(crate) fn close_notify(server: &Arc<Process>, id: u64, cookie: u64, count: u32) {
    let client = process::current().unwrap_or_else(|| Arc::clone(server));
    close_requests(client, server, id, cookie, count);
}

/// Queue a last-close notification for a server-side drain.
///
/// `SchemeFile::drop` can run inside the deep `SEND` path when an in-flight request is the last
/// holder; building requests there overflows the kernel stack. `recv` drains the queue before it
/// takes the next request.
pub(crate) fn defer_close(server: &Arc<Process>, id: u64, cookie: u64, count: u32) {
    if count == 0 || !registry::owner_running(server) {
        return;
    }
    let client = process::current().unwrap_or_else(|| Arc::clone(server));
    CLOSE_NOTICES.lock().push(CloseNotice {
        client,
        server: Arc::clone(server),
        id,
        cookie,
        count,
    });
    server.scheme_waiters.wake_all();
}

/// Submit every queued last-close notification. Called at the top of the `RECV` loop.
pub(crate) fn drain_close_notices() {
    let notices = {
        let mut queue = CLOSE_NOTICES.lock();
        if queue.is_empty() {
            return;
        }
        core::mem::take(&mut *queue)
    };
    for notice in notices {
        close_requests(
            notice.client,
            &notice.server,
            notice.id,
            notice.cookie,
            notice.count,
        );
    }
}

fn close_requests(client: Arc<Process>, server: &Arc<Process>, id: u64, cookie: u64, count: u32) {
    if count == 0 || !registry::owner_running(server) {
        return;
    }
    let mut remaining = count;
    while remaining > 0 {
        let mut packet = SchemePacket::request(Opcode::Close, next_request_id(), id);
        packet.server_cookie = cookie;
        let request = PendingRequest::new(
            Arc::clone(&client),
            Arc::clone(server),
            None,
            Opcode::Close,
            packet,
            RequestBuffers::default(),
        );
        if submit_close(&request).is_err() {
            break;
        }
        remaining = remaining.saturating_sub(1);
    }
}

fn status_errno(status: i32) -> Errno {
    if model::valid_status(status) && status != 0 {
        Errno(status as u16)
    } else {
        Errno::EIO
    }
}

/// Read a user path/name string.
pub fn user_string(
    address: u64,
    length: u64,
    maximum: usize,
) -> Result<alloc::string::String, Errno> {
    let length = usize::try_from(length).map_err(|_| Errno::EINVAL)?;
    if length == 0 {
        return Err(Errno::EINVAL);
    }
    if length > maximum {
        return Err(Errno::ENAMETOOLONG);
    }
    let mut buffer = alloc::vec![0u8; length];
    let source = UserSlice::new(address, u64::try_from(length).map_err(|_| Errno::EINVAL)?)?;
    copy_from_user(&mut buffer, source)?;
    core::str::from_utf8(&buffer)
        .map(|text| text.to_owned())
        .map_err(|_| Errno::EINVAL)
}
