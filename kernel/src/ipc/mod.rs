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

use core::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, AtomicU64, Ordering};

use alloc::string::String;
use alloc::sync::{Arc, Weak};

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
    /// Kernel-assigned file id (also the `file_id` of the `OPEN` packet).
    pub id: u64,
    /// Owning server process.
    pub server: Arc<Process>,
    /// Server file id returned by the server's `Open` response.
    pub server_cookie: AtomicU64,
    /// Server-side references this kernel object accounts for: one successful `OPEN` plus one
    /// `DUP` notification per duplicate. `CLOSE` is delivered for this many references only when
    /// the last kernel handle (and in-flight request) is gone.
    pub server_refs: AtomicU32,
}

impl SchemeFile {
    /// The server file id carried on the wire after `OPEN`.
    ///
    /// The server keys its open-file description by this value. A zero cookie falls back to the
    /// kernel id for servers that do not return one.
    pub fn server_id(&self) -> u64 {
        let cookie = self.server_cookie.load(Ordering::Acquire);
        if cookie != 0 { cookie } else { self.id }
    }
}

impl Drop for SchemeFile {
    fn drop(&mut self) {
        let references = self.server_refs.swap(0, Ordering::AcqRel);
        if references == 0 {
            return;
        }
        router::defer_close(
            &self.server,
            self.server_id(),
            self.server_cookie.load(Ordering::Acquire),
            references,
        );
    }
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
    fn copy_back(&mut self) {
        if let Some(capture) = self.buffer.as_mut() {
            capture.copy_back();
        }
        if let Some(capture) = self.response.as_mut() {
            capture.copy_back();
        }
    }

    fn cancel_copy_back(&mut self) {
        if let Some(capture) = self.buffer.as_mut() {
            capture.cancel_copy_back();
        }
        if let Some(capture) = self.response.as_mut() {
            capture.cancel_copy_back();
        }
    }

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
        if self.cancelled.load(Ordering::Acquire) {
            // The client is gone. Keep the copy-backed frames mapped until the server retires
            // the request; a server still touching the buffer must see live kernel-owned memory.
            self.buffers.lock().cancel_copy_back();
        } else {
            self.buffers.lock().copy_back();
            self.buffers.lock().release();
        }
        if self
            .done
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }
        // Drop the completed request from the client's outstanding list. One-way requests
        // (CLOSE/DUP) have no waiter to remove them, so without this a long-lived client would
        // accumulate every completed notification in kernel memory.
        self.client
            .client_requests
            .lock()
            .retain(|item| item.id != self.id);
        self.waiters.wake_all();
    }

    /// Release the captured buffers after the server is done with them.
    ///
    /// Idempotent.  A cancelled request deliberately keeps its captures mapped until the server
    /// retires or skips it, because the server may still touch the buffer.  Once that happens the
    /// server-side release must free them even if a dead client thread's abandoned syscall frame
    /// still holds this request's `Arc` (the S40 teardown case).
    pub fn release_captures(&self) {
        self.buffers.lock().release();
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

/// Pop one queued server request without holding the lock across completion.
fn pop_queued(
    queue: &Mutex<alloc::collections::VecDeque<Arc<PendingRequest>>>,
) -> Option<Arc<PendingRequest>> {
    queue.lock().pop_front()
}

/// Pop one tracked request without holding the lock across completion.
fn pop_request(
    requests: &Mutex<alloc::vec::Vec<Arc<PendingRequest>>>,
) -> Option<Arc<PendingRequest>> {
    requests.lock().pop()
}

/// Fail pending requests and remove the registry entries of an exiting process.
///
/// Called from `process::notify_exit` before the address space is released, so capture mappings
/// can still be removed.  Every drain is one request at a time: the teardown runs from the page
/// fault/exception/EXIT path on a bounded kernel stack, and a temporary request `Vec` here was
/// part of the S40 stack overflow.  The lock is released before `fail`/`cancel` because completion
/// touches the same client list and the request waits.
pub fn on_process_exit(process: &Arc<Process>) {
    // S38/S39: the process-exit close hook runs on every architecture.  Clear the dying
    // process's event registrations before its handle table is replaced, so no registration
    // survives the keys it was registered against.
    crate::event::on_process_exit(process);
    // S33 leak evidence: the exiting process's outstanding client requests. A completed request
    // is removed from the list, so a long OPEN/CLOSE/DUP loop leaves this at zero; the fixture
    // prints it for the loop client. Gated by a boot command-line flag like s24/s25.
    if crate::memory::paging::cmdline()
        .split_ascii_whitespace()
        .any(|token| token == "s33-ipccheck")
    {
        let outstanding = process.client_requests.lock().len();
        crate::kprintln!("s33: {} outstanding={}", process.path, outstanding);
    }

    registry::on_process_exit(process);

    while let Some(request) = pop_queued(&process.scheme_queue) {
        request.fail(Errno::EIO);
        request.release_captures();
    }

    while let Some(request) = pop_request(&process.active_requests) {
        request.fail(Errno::EIO);
        request.release_captures();
    }

    while let Some(request) = pop_request(&process.client_requests) {
        if matches!(request.opcode, Opcode::Close | Opcode::Dup) {
            // One-way notifications must still reach the server after the client is gone: the
            // server releases the open-file description on the last CLOSE, and DUP updates its
            // reference count. They are the client's last words, not cancellable work.
            continue;
        }
        request.cancel();
    }

    process.scheme_waiters.wake_all();

    // S40 evidence: every teardown invariant in one line. The faulting client may still have a
    // capture held by a server that has not retired the cancelled request yet; by the controller's
    // final exit (after the FIFO barrier) all counters are zero.
    if crate::memory::paging::cmdline()
        .split_ascii_whitespace()
        .any(|token| token == "s40-teardowncheck")
    {
        crate::kprintln!(
            "s40: exit {} client={} active={} queued={} wait={} scheme={} captures={}",
            process.path,
            process.client_requests.lock().len(),
            process.active_requests.lock().len(),
            process.scheme_queue.lock().len(),
            process.waiters.len(),
            process.scheme_waiters.len(),
            capture::frames_in_use()
        );
    }

    // S32 leak probe: capture frames still owned after the process's requests were drained.
    // Zero means every copy-backed capture was released exactly once.
    if crate::memory::paging::cmdline()
        .split_ascii_whitespace()
        .any(|token| token == "s32-capturecheck")
    {
        let stats = crate::memory::frame::stats();
        crate::kprintln!(
            "s32: capture frames={} free_bytes={}",
            capture::frames_in_use(),
            stats.free_bytes
        );
    }
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
