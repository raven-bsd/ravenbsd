// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! A host-test fake kernel for `raven-scheme` consumers.
//!
//! Enabled by the `testing` feature. It records requests and returns scripted client data; it is
//! not a filesystem and does not model rights beyond handing out fresh handles.

use alloc::collections::VecDeque;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;

use raven_abi::errno::Errno;
use raven_abi::types::Handle;

use crate::kernel::Kernel;
use crate::sync::SpinLock;

struct State {
    registered: Vec<String>,
    next_handle: u32,
    requests: VecDeque<Vec<u8>>,
    sent: Vec<(Handle, Vec<u8>)>,
    open_calls: Vec<(String, u32)>,
    read_data: Vec<u8>,
    write_data: Vec<u8>,
    call_response: Vec<u8>,
    fstat_data: [u8; 64],
    attachments: Vec<(Handle, Handle)>,
    closed: Vec<Handle>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            registered: Vec::new(),
            next_handle: 0,
            requests: VecDeque::new(),
            sent: Vec::new(),
            open_calls: Vec::new(),
            read_data: Vec::new(),
            write_data: Vec::new(),
            call_response: Vec::new(),
            fstat_data: [0u8; 64],
            attachments: Vec::new(),
            closed: Vec::new(),
        }
    }
}

/// A scriptable [`Kernel`] for host tests.
#[derive(Clone)]
pub struct FakeKernel {
    state: Arc<SpinLock<State>>,
}

impl Default for FakeKernel {
    fn default() -> Self {
        Self {
            state: Arc::new(SpinLock::new(State::default())),
        }
    }
}

impl FakeKernel {
    /// Create an empty fake kernel.
    pub fn new() -> Self {
        Self::default()
    }

    /// Queue one encoded server request.
    pub fn push_request(&self, packet: &[u8]) {
        self.state.lock().requests.push_back(packet.to_vec());
    }

    /// Names registered through `SCHEME_REGISTER`.
    pub fn registered(&self) -> Vec<String> {
        self.state.lock().registered.clone()
    }

    /// `(path, flags)` pairs passed to `OPEN`.
    pub fn open_calls(&self) -> Vec<(String, u32)> {
        self.state.lock().open_calls.clone()
    }

    /// Bytes written through `WRITE`.
    pub fn written(&self) -> Vec<u8> {
        self.state.lock().write_data.clone()
    }

    /// Encoded responses passed to `SEND`.
    pub fn sent(&self) -> Vec<Vec<u8>> {
        self.state
            .lock()
            .sent
            .iter()
            .map(|(_, bytes)| bytes.clone())
            .collect()
    }

    /// Request handles closed through `CLOSE`.
    pub fn closed(&self) -> Vec<Handle> {
        self.state.lock().closed.clone()
    }

    /// Attachments recorded by `REPLY_ATTACH`.
    pub fn attachments(&self) -> Vec<(Handle, Handle)> {
        self.state.lock().attachments.clone()
    }

    /// Script the data returned by `READ`.
    pub fn set_read_data(&self, data: &[u8]) {
        self.state.lock().read_data = data.to_vec();
    }

    /// Script the data returned by `CALL`.
    pub fn set_call_response(&self, data: &[u8]) {
        self.state.lock().call_response = data.to_vec();
    }

    /// Script the `FSTAT` record.
    pub fn set_fstat(&self, data: [u8; 64]) {
        self.state.lock().fstat_data = data;
    }
}

impl Kernel for FakeKernel {
    fn scheme_register(&self, name: &str) -> Result<(), Errno> {
        self.state.lock().registered.push(name.to_string());
        Ok(())
    }

    fn open(&self, path: &str, flags: u32) -> Result<Handle, Errno> {
        let mut state = self.state.lock();
        state.open_calls.push((path.to_string(), flags));
        state.next_handle = state.next_handle.wrapping_add(1).max(1);
        Ok(Handle(state.next_handle))
    }

    fn read(&self, _file: Handle, offset: u64, buffer: &mut [u8]) -> Result<usize, Errno> {
        let state = self.state.lock();
        let start = usize::try_from(offset).map_err(|_| Errno::EINVAL)?;
        let available = state.read_data.get(start..).unwrap_or(&[]);
        let length = available.len().min(buffer.len());
        buffer[..length].copy_from_slice(&available[..length]);
        Ok(length)
    }

    fn write(&self, _file: Handle, _offset: u64, buffer: &[u8]) -> Result<usize, Errno> {
        self.state.lock().write_data.extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn close(&self, file: Handle) -> Result<(), Errno> {
        self.state.lock().closed.push(file);
        Ok(())
    }

    fn dup(&self, _file: Handle, _rights: u32) -> Result<Handle, Errno> {
        let mut state = self.state.lock();
        state.next_handle = state.next_handle.wrapping_add(1).max(1);
        Ok(Handle(state.next_handle))
    }

    fn fstat(&self, _file: Handle, out: &mut [u8; 64]) -> Result<(), Errno> {
        *out = self.state.lock().fstat_data;
        Ok(())
    }

    fn call(&self, _file: Handle, _request: &[u8], response: &mut [u8]) -> Result<usize, Errno> {
        let state = self.state.lock();
        let length = state.call_response.len().min(response.len());
        response[..length].copy_from_slice(&state.call_response[..length]);
        Ok(length)
    }

    fn recv_timeout(&self, packet: &mut [u8], _timeout_ms: u64) -> Result<Option<Handle>, Errno> {
        let mut state = self.state.lock();
        let Some(request) = state.requests.pop_front() else {
            return Ok(None);
        };
        let length = request.len().min(packet.len());
        packet[..length].copy_from_slice(&request[..length]);
        state.next_handle = state.next_handle.wrapping_add(1).max(1);
        Ok(Some(Handle(state.next_handle)))
    }

    fn send(&self, request: Handle, packet: &[u8]) -> Result<(), Errno> {
        self.state.lock().sent.push((request, packet.to_vec()));
        Ok(())
    }

    fn reply_attach(&self, request: Handle, handle: Handle) -> Result<(), Errno> {
        self.state.lock().attachments.push((request, handle));
        Ok(())
    }
}
