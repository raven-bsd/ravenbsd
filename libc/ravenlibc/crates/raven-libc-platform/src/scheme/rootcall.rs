// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The cached `file:/` root handle.
//!
//! Path control operations (`mkdir`, `unlink`, `chmod`, `statfs`, ...) are `CALL`s on a root
//! handle. One handle is cached per process and reopened once on `EIO` after a `vfsd` restart;
//! a killed server does not invalidate ordinary fds (doc 11 section 3.12).

use core::cell::RefCell;

use raven_abi::errno::Errno;
use raven_abi::types::Handle;
use raven_proto_fs::{Call, CallRequest, CallResponse};

use crate::backend::SysBackend;
use crate::xlat;

/// The root-handle cache.
pub struct RootCall {
    handle: RefCell<Option<Handle>>,
}

impl Default for RootCall {
    fn default() -> Self {
        Self::new()
    }
}

impl RootCall {
    /// An empty cache.
    pub const fn new() -> Self {
        Self {
            handle: RefCell::new(None),
        }
    }

    /// The cached handle, opening `file:/` on first use.
    pub fn ensure<B: SysBackend>(&self, backend: &B) -> Result<Handle, Errno> {
        if let Some(handle) = *self.handle.borrow() {
            return Ok(handle);
        }
        let handle = backend.open(
            b"file:/",
            u32::from(raven_proto_fs::WIRE_OPEN_READ | raven_proto_fs::WIRE_OPEN_DIRECTORY),
        )?;
        *self.handle.borrow_mut() = Some(handle);
        Ok(handle)
    }

    /// Forget the handle and close it if possible.
    pub fn invalidate<B: SysBackend>(&self, backend: &B) {
        if let Some(handle) = self.handle.borrow_mut().take() {
            let _ = backend.close(handle);
        }
    }

    /// Send one call, reopening the root once on `EIO`.
    pub fn call<B: SysBackend>(&self, backend: &B, call: Call<'_>) -> Result<(), Errno> {
        let mut request =
            [0u8; raven_proto_fs::CALL_HEADER_LEN + 2 * (2 + raven_proto_fs::MAX_PATH)];
        let encoded = CallRequest::new(call)
            .encode(&mut request)
            .map_err(|error| error.errno())?;
        let op = request_op(&request);
        let mut response = [0u8; 8];
        let handle = self.ensure(backend)?;
        let count = match backend.call(handle, request.get(..encoded).unwrap_or(&[]), &mut response)
        {
            Err(Errno::EIO) => {
                self.invalidate(backend);
                let handle = self.ensure(backend)?;
                backend.call(handle, request.get(..encoded).unwrap_or(&[]), &mut response)?
            }
            other => other?,
        };
        let decoded = CallResponse::decode(op, response.get(..count).unwrap_or(&[]))
            .map_err(|error| error.errno())?;
        xlat::call_status(decoded.status())
    }
}

fn request_op(request: &[u8]) -> u16 {
    match request.get(2..4) {
        Some(bytes) => u16::from_le_bytes([bytes[0], bytes[1]]),
        None => 0,
    }
}

#[cfg(test)]
#[path = "rootcall_ut.rs"]
mod tests;
