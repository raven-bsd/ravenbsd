// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The syscall surface used by the server and client frameworks.
//!
//! [`RtKernel`] uses the real S11 syscalls. Host tests and downstream services use a fake
//! implementation of [`Kernel`] instead, so the server/client state machines are testable without
//! a running kernel.

use raven_abi::errno::Errno;
use raven_abi::types::Handle;
use raven_ipc::protocol::{self, Attachment, MAX_ATTACHMENTS, Opcode, PACKET_SIZE, REPLY_HANDLE};

use crate::sys;

/// The kernel operations used by `raven-scheme`.
///
/// `read` and `write` take an offset because the filesystem protocol models positioned I/O. The
/// current S11 `READ`/`WRITE` syscalls do not carry it, so [`RtKernel`] ignores the offset and the
/// server advances its own cursor; a future kernel revision can use the packet field directly.
pub trait Kernel: Clone {
    /// Register a scheme name owned by the calling process.
    fn scheme_register(&self, name: &str) -> Result<(), Errno> {
        let _ = name;
        Err(Errno::ENOSYS)
    }

    /// Open a scheme path with S11 open flags.
    fn open(&self, path: &str, flags: u32) -> Result<Handle, Errno> {
        let _ = (path, flags);
        Err(Errno::ENOSYS)
    }

    /// Read from a scheme file. The offset is advisory for targets whose kernel carries it.
    fn read(&self, file: Handle, offset: u64, buffer: &mut [u8]) -> Result<usize, Errno> {
        let _ = (file, offset, buffer);
        Err(Errno::ENOSYS)
    }

    /// Write to a scheme file. The offset is advisory for targets whose kernel carries it.
    fn write(&self, file: Handle, offset: u64, buffer: &[u8]) -> Result<usize, Errno> {
        let _ = (file, offset, buffer);
        Err(Errno::ENOSYS)
    }

    /// Close a scheme file.
    fn close(&self, file: Handle) -> Result<(), Errno> {
        let _ = file;
        Err(Errno::ENOSYS)
    }

    /// Duplicate a scheme file with a rights mask.
    fn dup(&self, file: Handle, rights: u32) -> Result<Handle, Errno> {
        let _ = (file, rights);
        Err(Errno::ENOSYS)
    }

    /// Read the 64-byte `FSTAT` record into `out`.
    fn fstat(&self, file: Handle, out: &mut [u8; 64]) -> Result<(), Errno> {
        let _ = (file, out);
        Err(Errno::ENOSYS)
    }

    /// Send a typed `CALL` request and receive the response.
    fn call(&self, file: Handle, request: &[u8], response: &mut [u8]) -> Result<usize, Errno> {
        let _ = (file, request, response);
        Err(Errno::ENOSYS)
    }

    /// Receive one request packet; returns the server-side request handle.
    fn recv(&self, packet: &mut [u8]) -> Result<Handle, Errno> {
        let _ = packet;
        Err(Errno::ENOSYS)
    }

    /// Receive with a timeout in milliseconds.
    ///
    /// The S11 `RECV` syscall is blocking and has no timeout argument. The default implementation
    /// therefore ignores `timeout_ms` and calls [`Kernel::recv`]; fake kernels honor the timeout for
    /// host tests. `u64::MAX` means "wait forever".
    fn recv_timeout(&self, packet: &mut [u8], timeout_ms: u64) -> Result<Option<Handle>, Errno> {
        let _ = timeout_ms;
        self.recv(packet).map(Some)
    }

    /// Send one response packet for a server-side request handle.
    fn send(&self, request: Handle, packet: &[u8]) -> Result<(), Errno> {
        let _ = (request, packet);
        Err(Errno::ENOSYS)
    }

    /// Send a reply with S25 handle attachments.
    ///
    /// `attachments` are handles in this server's table; the kernel validates
    /// `Rights::TRANSFER`, installs reduced duplicates in the client's table and
    /// writes `[handle, rights_mask]` pairs after the inline data.  A kernel
    /// that does not implement attachments accepts the empty slice (a normal
    /// [`Kernel::send`]) and returns `ENOSYS` for a non-empty one.
    fn send_attach(
        &self,
        request: Handle,
        packet: &[u8],
        attachments: &[Attachment],
    ) -> Result<(), Errno> {
        if attachments.is_empty() {
            self.send(request, packet)
        } else {
            let _ = (request, packet);
            Err(Errno::ENOSYS)
        }
    }
}

/// The real S11 syscall implementation.
#[derive(Clone, Copy, Debug, Default)]
pub struct RtKernel;

impl Kernel for RtKernel {
    fn scheme_register(&self, name: &str) -> Result<(), Errno> {
        sys::scheme_register(name)
    }

    fn open(&self, path: &str, flags: u32) -> Result<Handle, Errno> {
        sys::open(path, flags)
    }

    fn read(&self, file: Handle, _offset: u64, buffer: &mut [u8]) -> Result<usize, Errno> {
        sys::read(file, buffer)
    }

    fn write(&self, file: Handle, _offset: u64, buffer: &[u8]) -> Result<usize, Errno> {
        sys::write(file, buffer)
    }

    fn close(&self, file: Handle) -> Result<(), Errno> {
        sys::close(file)
    }

    fn dup(&self, file: Handle, rights: u32) -> Result<Handle, Errno> {
        sys::dup(file, rights)
    }

    fn fstat(&self, file: Handle, out: &mut [u8; 64]) -> Result<(), Errno> {
        sys::fstat(file, out)
    }

    fn call(&self, file: Handle, request: &[u8], response: &mut [u8]) -> Result<usize, Errno> {
        sys::call(file, request, response)
    }

    fn recv(&self, packet: &mut [u8]) -> Result<Handle, Errno> {
        sys::recv(packet)
    }

    fn send(&self, request: Handle, packet: &[u8]) -> Result<(), Errno> {
        sys::send(request, packet)
    }

    fn send_attach(
        &self,
        request: Handle,
        packet: &[u8],
        attachments: &[Attachment],
    ) -> Result<(), Errno> {
        if attachments.is_empty() {
            return sys::send(request, packet);
        }
        let bytes = apply_attachments(packet, attachments)?;
        sys::send(request, &bytes)
    }
}

/// Apply the S25 attachment layout to an encoded reply packet.
///
/// The rules are shared by [`RtKernel`] and the host `FakeKernel`, so tests see
/// the same packet a kernel would validate.
pub(crate) fn apply_attachments(
    packet: &[u8],
    attachments: &[Attachment],
) -> Result<[u8; PACKET_SIZE], Errno> {
    if attachments.len() > MAX_ATTACHMENTS || packet.len() != PACKET_SIZE {
        return Err(Errno::E2BIG);
    }
    let mut bytes = [0u8; PACKET_SIZE];
    bytes.copy_from_slice(packet);
    let mut decoded = protocol::decode(&bytes).map_err(|_| Errno::EPROTO)?;
    decoded.validate(true).map_err(|_| Errno::EPROTO)?;
    let opcode = Opcode::from_u16(decoded.opcode).ok_or(Errno::EPROTO)?;
    if opcode == Opcode::Open {
        if attachments.len() != 1 {
            return Err(Errno::E2BIG);
        }
        decoded.flags |= REPLY_HANDLE;
    } else if opcode == Opcode::Call {
        let pairs = u64::try_from(attachments.len())
            .map_err(|_| Errno::E2BIG)?
            .checked_mul(8)
            .ok_or(Errno::E2BIG)?;
        decoded.buffer_len = u64::from(decoded.data_len)
            .checked_add(pairs)
            .ok_or(Errno::E2BIG)?;
    } else {
        return Err(Errno::E2BIG);
    }
    decoded.attach_count = u32::try_from(attachments.len()).map_err(|_| Errno::E2BIG)?;
    for (index, attachment) in attachments.iter().enumerate() {
        decoded.attach[index] = *attachment;
    }
    protocol::encode(&decoded, &mut bytes).map_err(|_| Errno::EPROTO)?;
    Ok(bytes)
}
