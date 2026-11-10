// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Packet routing between scheme clients and servers.

use core::sync::atomic::{AtomicU64, Ordering};

use alloc::borrow::ToOwned;
use alloc::sync::Arc;

use raven_abi::errno::Errno;
use raven_abi::types::{Handle, Rights};
use raven_usercopy::UserSlice;

use crate::process::handle::Object;
use crate::process::{self, Process};
use crate::sched;
use crate::usercopy::{copy_from_user, copy_to_user};

use super::capture;
use super::model;
use super::protocol::{self, Opcode, PACKET_SIZE, SchemePacket};
use super::{PendingRequest, RequestBuffers, SchemeFile};
use super::{next_file_id, next_request_id, registry};

/// Open a path on `server` and return a new scheme-file handle.
pub fn open(
    caller: &Arc<Process>,
    server: Arc<Process>,
    path: &str,
    flags: u32,
) -> Result<Handle, Errno> {
    let rights = model::open_rights(flags).ok_or(Errno::EINVAL)?;
    if flags > u32::from(u16::MAX) {
        return Err(Errno::EINVAL);
    }
    let file = Arc::new(SchemeFile {
        id: next_file_id(),
        server: Arc::clone(&server),
        server_cookie: AtomicU64::new(0),
    });
    let handle = caller
        .handles
        .lock()
        .insert(Object::SchemeFile(Arc::clone(&file)), rights)?;
    let mut packet = SchemePacket::request(Opcode::Open, next_request_id(), file.id);
    packet.flags = flags as u16;
    if packet.set_path(path.as_bytes()).is_err() {
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
            file.server_cookie
                .store(response.server_cookie, Ordering::Release);
            Ok(handle)
        }
        Ok((status, _)) => {
            let _ = caller.handles.lock().close(handle);
            Err(status_errno(status))
        }
        Err(error) => {
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
    let mut packet = SchemePacket::request(Opcode::Read, next_request_id(), file.id);
    packet.buffer_len = len;
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
    let mut packet = SchemePacket::request(Opcode::Write, next_request_id(), file.id);
    packet.buffer_len = len;
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

/// Close a scheme file (the local handle is always closed; the packet is best effort).
pub fn close(caller: &Arc<Process>, handle: Handle) -> Result<(), Errno> {
    let file = {
        let handles = caller.handles.lock();
        match handles.get(handle, Rights::NONE)? {
            Object::SchemeFile(file) => Arc::clone(file),
            _ => return Err(Errno::EBADF),
        }
    };
    caller.handles.lock().close(handle)?;
    one_way(&file, Opcode::Close);
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
    one_way(&file, Opcode::Dup);
    Ok(duplicate)
}

/// Stat a scheme file; the server returns inline bytes in the response packet.
pub fn fstat(
    caller: &Arc<Process>,
    file: &Arc<SchemeFile>,
    address: u64,
    len: u64,
) -> Result<(), Errno> {
    let mut packet = SchemePacket::request(Opcode::Fstat, next_request_id(), file.id);
    packet.server_cookie = file.server_cookie.load(Ordering::Acquire);
    let request = PendingRequest::new(
        Arc::clone(caller),
        Arc::clone(&file.server),
        Some(Arc::clone(file)),
        Opcode::Fstat,
        packet,
        RequestBuffers::default(),
    );
    let (status, _) = submit(&request).and_then(|_| wait(caller, &request))?;
    if status != 0 {
        return Err(status_errno(status));
    }
    let response = *request.response.lock();
    let bytes = response.data_bytes();
    let length = usize::try_from(len).unwrap_or(usize::MAX).min(bytes.len());
    let destination = UserSlice::new(address, length as u64)?;
    copy_to_user(destination, bytes.get(..length).unwrap_or(&[]))?;
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
    let mut packet = SchemePacket::request(Opcode::Call, next_request_id(), file.id);
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
    let packet = protocol::decode(&bytes).map_err(|_| Errno::EPROTO)?;
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
    if request.opcode == Opcode::Open
        && packet.status == 0
        && let Some(file) = request.file.as_ref()
    {
        file.server_cookie
            .store(packet.server_cookie, Ordering::Release);
    }
    let status = packet.status;
    let result = match request.opcode {
        Opcode::Read | Opcode::Write | Opcode::Call => packet.buffer_len,
        _ => u64::from(packet.data_len),
    };
    request.complete(status, result, Some(packet));
    caller
        .active_requests
        .lock()
        .retain(|item| !Arc::ptr_eq(item, &request));
    let _ = caller.handles.lock().close(request_handle);
    Ok(())
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
    let destination = UserSlice::new(packet_address, PACKET_SIZE as u64)?;
    if let Err(error) = copy_to_user(destination, &bytes) {
        request.fail(error);
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
    // Hold the server state lock while enqueueing: `notify_exit` marks the process Exited under
    // the same lock and only then drains the queue, so a request cannot be pushed into a dead
    // server's queue after the drain.
    let state = request.server.state.lock();
    if *state != process::ProcessState::Running {
        return Err(Errno::EIO);
    }
    request
        .client
        .client_requests
        .lock()
        .push(Arc::clone(request));
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

fn one_way(file: &Arc<SchemeFile>, opcode: Opcode) {
    if !registry::owner_running(&file.server) {
        return;
    }
    let mut packet = SchemePacket::request(opcode, next_request_id(), file.id);
    packet.server_cookie = file.server_cookie.load(Ordering::Acquire);
    let request = PendingRequest::new(
        process::current().unwrap_or_else(|| Arc::clone(&file.server)),
        Arc::clone(&file.server),
        Some(Arc::clone(file)),
        opcode,
        packet,
        RequestBuffers::default(),
    );
    if submit(&request).is_ok() {
        // The server will complete the request through SEND; if it never does, server exit
        // completes it.
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
