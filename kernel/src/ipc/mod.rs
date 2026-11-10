// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Kernel-side scheme IPC: registry, per-process namespaces, packet routing and capture.
//!
//! A process registers a scheme name with [`registry::register`]. Clients look the name up in
//! their namespace, get a [`SchemeFile`] handle and submit request packets to the owning process.
//! The owner receives packets through `RECV`, replies through `SEND`, and the client's request
//! buffer is mapped into the owner's address space for the lifetime of the request (zero copy).
//!
//! Server death is handled in [`on_process_exit`], called by `process::notify_exit` before the
//! address space is released. Pending requests are completed with `EIO`, registry entries are
//! removed, and captures are unmapped while the server's page tables still exist.

pub mod capture;
pub mod model;
pub mod protocol;
pub mod registry;
pub mod router;

use core::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering};

use alloc::string::String;
use alloc::sync::{Arc, Weak};
use alloc::vec::Vec;

use raven_abi::errno::Errno;
use raven_abi::types::Rights;
use spin::Mutex;

use crate::process::Process;
use crate::sched::wait::WaitQueue;

use self::capture::Capture;
use self::protocol::{Opcode, SchemePacket};

/// A registered scheme name owned by one process.
pub struct SchemeServer {
    /// Registered name.
    pub name: String,
    /// Owning process (weak; the process owns the strong server object).
    pub owner: Weak<Process>,
    /// Set when the owner exits.
    pub dead: AtomicBool,
}

impl SchemeServer {
    /// True while the owner exists and has not exited.
    pub fn is_live(&self) -> bool {
        if self.dead.load(Ordering::Acquire) {
            return false;
        }
        let Some(owner) = self.owner.upgrade() else {
            return false;
        };
        *owner.state.lock() == crate::process::ProcessState::Running
    }
}

/// One open scheme file.
pub struct SchemeFile {
    /// Kernel-assigned file id (also the packet `file_id`).
    pub id: u64,
    /// Owning server process.
    pub server: Arc<Process>,
    /// Opaque cookie returned by the server's `Open` response.
    pub server_cookie: AtomicU64,
}

/// Captures owned by one request.
#[derive(Default)]
pub struct RequestBuffers {
    /// Primary request/response buffer.
    pub buffer: Option<Capture>,
    /// Second buffer for `Call` responses.
    pub response: Option<Capture>,
}

impl RequestBuffers {
    fn release(&mut self) {
        self.buffer.take();
        self.response.take();
    }
}

/// One in-flight request.
pub struct PendingRequest {
    /// Kernel request id echoed by the server.
    pub id: u64,
    /// Client process.
    pub client: Arc<Process>,
    /// Server process.
    pub server: Arc<Process>,
    /// Scheme file, when the request operates on one.
    pub file: Option<Arc<SchemeFile>>,
    /// Operation.
    pub opcode: Opcode,
    /// Request packet template.
    pub packet: Mutex<SchemePacket>,
    /// Captured buffers, released when the request completes.
    pub buffers: Mutex<RequestBuffers>,
    /// Server's response packet.
    pub response: Mutex<SchemePacket>,
    /// Set when the request completed or was cancelled.
    pub done: AtomicBool,
    /// Set when the client exited while the request was in flight.
    pub cancelled: AtomicBool,
    /// Response status (0 or positive errno).
    pub status: AtomicI32,
    /// Result length reported by the server.
    pub result_len: AtomicU64,
    /// Client waiters.
    pub waiters: WaitQueue,
}

impl PendingRequest {
    /// Create a request. `packet` already carries the request fields; capture addresses are
    /// filled in when the server receives the request.
    pub fn new(
        client: Arc<Process>,
        server: Arc<Process>,
        file: Option<Arc<SchemeFile>>,
        opcode: Opcode,
        packet: SchemePacket,
        buffers: RequestBuffers,
    ) -> Arc<Self> {
        Arc::new(Self {
            id: packet.request_id,
            client,
            server,
            file,
            opcode,
            packet: Mutex::new(packet),
            buffers: Mutex::new(buffers),
            response: Mutex::new(SchemePacket::default()),
            done: AtomicBool::new(false),
            cancelled: AtomicBool::new(false),
            status: AtomicI32::new(0),
            result_len: AtomicU64::new(0),
            waiters: WaitQueue::new(),
        })
    }

    /// Fill the capture addresses into the packet returned by `RECV`.
    pub fn deliverable_packet(&self) -> SchemePacket {
        let mut packet = *self.packet.lock();
        let buffers = self.buffers.lock();
        if let Some(buffer) = buffers.buffer.as_ref() {
            packet.buffer_addr = buffer.address();
        }
        if let Some(response) = buffers.response.as_ref() {
            packet.response_addr = response.address();
        }
        packet
    }

    /// Complete the request, release captures and wake the client.
    pub fn complete(&self, status: i32, result_len: u64, response: Option<SchemePacket>) {
        if self.done.load(Ordering::Acquire) {
            return;
        }
        self.status.store(status, Ordering::Release);
        self.result_len.store(result_len, Ordering::Release);
        if let Some(response) = response {
            *self.response.lock() = response;
        }
        self.buffers.lock().release();
        if self
            .done
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }
        self.waiters.wake_all();
    }

    /// Fail the request with `errno`.
    pub fn fail(&self, error: Errno) {
        self.complete(i32::from(error.0), 0, None);
    }

    /// Cancel a request whose client is exiting.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        self.fail(Errno::EIO);
        self.server.scheme_waiters.wake_all();
    }
}

static NEXT_REQUEST_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_FILE_ID: AtomicU64 = AtomicU64::new(1);

/// Allocate a new request id.
pub fn next_request_id() -> u64 {
    NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed)
}

/// Allocate a new scheme-file id.
pub fn next_file_id() -> u64 {
    NEXT_FILE_ID.fetch_add(1, Ordering::Relaxed)
}

/// Fail pending requests and remove the registry entries of an exiting process.
///
/// Called from `process::notify_exit` before the address space is released, so capture mappings
/// can still be removed.
pub fn on_process_exit(process: &Arc<Process>) {
    registry::on_process_exit(process);

    let queued: Vec<Arc<PendingRequest>> = {
        let mut queue = process.scheme_queue.lock();
        queue.drain(..).collect()
    };
    for request in queued {
        request.fail(Errno::EIO);
    }

    let active: Vec<Arc<PendingRequest>> = {
        let mut active = process.active_requests.lock();
        active.drain(..).collect()
    };
    for request in active {
        request.fail(Errno::EIO);
    }

    let client: Vec<Arc<PendingRequest>> = {
        let mut client = process.client_requests.lock();
        client.drain(..).collect()
    };
    for request in client {
        request.cancel();
    }

    process.scheme_waiters.wake_all();
}

/// Look up a scheme-file object through a handle with the required rights.
pub fn file_from_handle(
    process: &Arc<Process>,
    handle: raven_abi::types::Handle,
    rights: Rights,
) -> Result<Arc<SchemeFile>, Errno> {
    let handles = process.handles.lock();
    match handles.get(handle, rights)? {
        crate::process::handle::Object::SchemeFile(file) => Ok(Arc::clone(file)),
        _ => Err(Errno::EBADF),
    }
}
