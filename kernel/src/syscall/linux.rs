// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Linux-personality trap delivery (S41).
//!
//! The kernel captures the raw syscall frame and enqueues one scheme-style
//! `CALL` packet on the process's handler. The packet carries a
//! [`LinuxSyscallV1`] in its inline data and the handler replies with a
//! [`LinuxReplyV1`]. The calling thread blocks without holding the process or
//! handler locks; a dead handler leaves a default response packet, which is
//! reported as `-EIO` and terminates the Linux process.

use alloc::sync::Arc;

use raven_abi::errno::Errno;
use raven_abi::types::{Handle, Rights};
#[cfg(target_arch = "aarch64")]
use raven_proto_linux::ARCH_AARCH64;
#[cfg(target_arch = "x86_64")]
use raven_proto_linux::ARCH_X86_64;
use raven_proto_linux::{LinuxReplyV1, LinuxSyscallV1, REPLY_LEN};

use crate::ipc::protocol::{Opcode, SchemePacket};
use crate::ipc::{PendingRequest, RequestBuffers, next_request_id};
use crate::process::{Process, handle::Object};
use crate::sched;
use crate::syscall::SyscallFrame;

/// Bound on a process's outstanding handler requests.
const MAX_HANDLER_REQUESTS: usize = 1024;

#[cfg(target_arch = "x86_64")]
const ARCH: u8 = ARCH_X86_64;
#[cfg(target_arch = "aarch64")]
const ARCH: u8 = ARCH_AARCH64;

/// Install a handler into `target` for `PROC_SET_PERSONALITY`.
///
/// A self-installation already owns the handle; a cross-process installation
/// clones any object kind (the earlier S41 path used the x86-only
/// `clone_transferable` and rejected aarch64 scheme handles).
pub fn install_handler(
    caller: &Arc<Process>,
    target: &Arc<Process>,
    handler: Handle,
) -> Result<Option<Handle>, Errno> {
    if handler == Handle::NONE {
        return Ok(None);
    }
    if Arc::ptr_eq(caller, target) {
        return Ok(Some(handler));
    }
    let (object, rights, parent) = {
        let handles = caller.handles.lock();
        let (entry, rights) = handles.get_with_rights(handler, Rights::TRANSFER)?;
        let object = entry.clone();
        let parent = handles.lineage_node(handler)?;
        (object, rights, parent)
    };
    let (installed, node) = {
        let mut handles = target.handles.lock();
        let installed = handles.insert_derived(object, rights, &parent)?;
        let node = handles.lineage_node(installed).ok();
        (installed, node)
    };
    if let Some(node) = node {
        crate::process::handle::register_lineage(node);
    }
    Ok(Some(installed))
}

/// Deliver one raw syscall frame to the process handler and return its `i64`.
pub fn trap(caller: &Arc<Process>, frame: &SyscallFrame) -> Result<isize, Errno> {
    let state = *caller.personality.lock();
    let handler = state.handler.ok_or(Errno::EIO)?;
    let server = handler_server(caller, handler)?;
    if Arc::ptr_eq(&server, caller) {
        // A process cannot serve its own blocking trap.
        return Err(Errno::EIO);
    }

    let request = LinuxSyscallV1::new(
        ARCH,
        frame.rax,
        [
            frame.rdi, frame.rsi, frame.rdx, frame.r10, frame.r8, frame.r9,
        ],
    );
    let mut request_bytes = [0u8; 64];
    let request_len = request
        .encode(&mut request_bytes)
        .map_err(|_| Errno::EPROTO)?;
    let mut packet = SchemePacket::request(Opcode::Call, next_request_id(), 0);
    let data = packet.data.get_mut(..request_len).ok_or(Errno::EPROTO)?;
    data.copy_from_slice(request_bytes.get(..request_len).ok_or(Errno::EPROTO)?);
    packet.data_len = u32::try_from(request_len).map_err(|_| Errno::EPROTO)?;
    // The reply value travels in the response inline data. `Call` reports its
    // result length from `buffer_len`; asking for 8 bytes keeps the router's
    // reply-bound check satisfied without a captured response buffer.
    packet.buffer_len = 8;
    packet.response_len = REPLY_LEN as u64;

    let pending = PendingRequest::new(
        Arc::clone(caller),
        Arc::clone(&server),
        None,
        Opcode::Call,
        packet,
        RequestBuffers::default(),
    );
    submit(caller, &server, &pending)?;
    wait(caller, &pending)?;

    let response = *pending.response.lock();
    if response.opcode != Opcode::Call.as_u16() || response.status != 0 {
        // A dead handler completes the request with no response and `EIO`;
        // a live handler error status becomes the Linux syscall's `-errno`.
        if response.opcode == 0 {
            return Err(Errno::EIO);
        }
        return Ok(isize::try_from(response.status)
            .unwrap_or(0)
            .saturating_neg());
    }
    let reply = match LinuxReplyV1::decode(response.data_bytes()) {
        Ok(reply) => reply,
        Err(_) => {
            return Ok(isize::from(Errno::EPROTO.0 as i16).saturating_neg());
        }
    };
    Ok(reply.value as isize)
}

fn handler_server(caller: &Arc<Process>, handler: Handle) -> Result<Arc<Process>, Errno> {
    let handles = caller.handles.lock();
    match handles.get(handler, Rights::NONE)? {
        Object::SchemeFile(file) => Ok(Arc::clone(&file.server)),
        Object::SchemeServer(server) => server.owner.upgrade().ok_or(Errno::EIO),
        _ => Err(Errno::EIO),
    }
}

fn submit(
    caller: &Arc<Process>,
    server: &Arc<Process>,
    request: &Arc<PendingRequest>,
) -> Result<(), Errno> {
    let state = server.state.lock();
    if *state != crate::process::ProcessState::Running {
        return Err(Errno::EIO);
    }
    {
        let mut requests = caller.client_requests.lock();
        if requests.len() >= MAX_HANDLER_REQUESTS {
            return Err(Errno::EAGAIN);
        }
        requests.push(Arc::clone(request));
    }
    server.scheme_queue.lock().push_back(Arc::clone(request));
    drop(state);
    server.scheme_waiters.wake(server.id, 0, usize::MAX);
    crate::event::notify_all();
    Ok(())
}

fn wait(caller: &Arc<Process>, request: &Arc<PendingRequest>) -> Result<(), Errno> {
    loop {
        if request.done.load(core::sync::atomic::Ordering::Acquire) {
            break;
        }
        let token = sched::begin_block()?;
        if request.done.load(core::sync::atomic::Ordering::Acquire) {
            sched::end_block(token);
            break;
        }
        let queued = request.waiters.enqueue_if(request.id, 0, || {
            Ok(!request.done.load(core::sync::atomic::Ordering::Acquire))
        })?;
        if queued {
            let _ = sched::block_current();
        }
        sched::end_block(token);
    }
    caller
        .client_requests
        .lock()
        .retain(|item| !Arc::ptr_eq(item, request));
    Ok(())
}
