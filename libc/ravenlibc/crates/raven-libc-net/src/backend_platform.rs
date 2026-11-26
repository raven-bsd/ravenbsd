// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `NetBackend` over C55's scheme client.
//!
//! The socket framing of `raven-proto-net` is not exposed by C55's generic `SysBackend`
//! `read`/`write`/`call` shape: a net file's `READ` request carries a length and the response
//! carries a source endpoint, while `SysBackend::read` only supplies a destination buffer. This
//! adapter therefore implements the operations the current C55 API can express (open, raw
//! read/write, typed CALL, close) and reports `ENOSYS` for the parts that need a net-aware
//! scheme client. The gap is listed as a QUESTION in the C63 report; the fake backend and the
//! trait-level tests are unaffected.
use alloc::vec::Vec;

use crate::ErrnoExt;

use raven_libc_abi::errno::Errno;
use raven_libc_abi::poll::{POLLERR, POLLHUP, POLLIN, POLLOUT};
use raven_libc_abi::structs::Pollfd;
use raven_libc_platform::Handle;
use raven_libc_platform::Platform;
use raven_libc_platform::backend::SysBackend;
use raven_libc_platform::lock::Lock;
use raven_libc_platform::scheme::client::open_scheme_path;
use raven_proto_net::{Call, Endpoint, IfaceRequest, IfaceResponse, Response, SocketRequest};

use crate::{CallReply, Caps, NetBackend, ReadDone, Sock, WaitItem};

/// The kernel `OPEN` access bits: read and write. A socket file is always both.
const WIRE_READ_WRITE: u16 = 3;

/// A `NetBackend` over one C55 platform.
pub struct PlatformBackend<'a, B: SysBackend, L: Lock> {
    platform: &'a Platform<B, L>,
}

/// Send `Call::Event` and return the client notification handle the server attached.
///
/// The attached `CALL` reply carries the 6-byte [`Response::Ok`] inline and then one
/// `(handle, rights_mask)` pair; the kernel installs that handle in the caller's table.
pub fn event_handle<B: SysBackend>(backend: &B, sock: Sock) -> Result<Handle, Errno> {
    let request = SocketRequest::Call(Call::Event);
    let mut encoded = [0u8; 64];
    let used = request.encode(&mut encoded).map_err(|_| Errno::EINVAL)?;
    let mut response = [0u8; 16];
    let _ = backend
        .call(
            Handle(sock.0),
            encoded.get(..used).unwrap_or(&[]),
            &mut response,
        )
        .map_err(|error| Errno(error.0))?;
    let bytes: [u8; 4] = response
        .get(6..10)
        .ok_or(Errno::EPROTO)?
        .try_into()
        .map_err(|_| Errno::EPROTO)?;
    let value = u32::from_le_bytes(bytes);
    if value == 0 || value == u32::MAX {
        return Err(Errno::EPROTO);
    }
    Ok(Handle(value))
}

impl<'a, B: SysBackend, L: Lock> PlatformBackend<'a, B, L> {
    /// Wrap one platform.
    #[must_use]
    pub fn new(platform: &'a Platform<B, L>) -> Self {
        Self { platform }
    }
}

impl<B: SysBackend, L: Lock> NetBackend for PlatformBackend<'_, B, L> {
    fn caps(&self) -> Caps {
        Caps::default()
    }

    fn open(&self, path: &str) -> Result<Sock, Errno> {
        let handle = open_scheme_path(self.platform.backend(), path.as_bytes(), WIRE_READ_WRITE)
            .map_err(|error| Errno(error.0))?;
        Ok(Sock(handle.0))
    }

    fn read(&self, s: Sock, buf: &mut [u8]) -> Result<ReadDone, Errno> {
        let count = self
            .platform
            .backend()
            .read(raven_libc_platform::Handle(s.0), buf)
            .map_err(|error| Errno(error.0))?;
        Ok(ReadDone {
            len: count,
            from: None,
            truncated: false,
        })
    }

    fn write(&self, s: Sock, data: &[u8], _to: Option<Endpoint>) -> Result<usize, Errno> {
        self.platform
            .backend()
            .write(raven_libc_platform::Handle(s.0), data)
            .map_err(|error| Errno(error.0))
    }

    fn call(&self, s: Sock, c: Call) -> Result<CallReply, Errno> {
        let request = SocketRequest::Call(c);
        let mut encoded = [0u8; 64];
        let used = request.encode(&mut encoded).map_err(|_| Errno::EINVAL)?;
        let mut response_bytes = [0u8; 64];
        let count = self
            .platform
            .backend()
            .call(
                raven_libc_platform::Handle(s.0),
                encoded.get(..used).unwrap_or(&[]),
                &mut response_bytes,
            )
            .map_err(|error| Errno(error.0))?;
        match Response::decode(response_bytes.get(..count).unwrap_or(&[]))
            .map_err(|_| Errno::EPROTO)?
        {
            Response::Ok => Ok(CallReply::Ok),
            Response::Endpoint(endpoint) => Ok(CallReply::Endpoint(endpoint)),
            Response::Value(value) => Ok(CallReply::Value(value)),
            Response::Socket { id } => {
                // `ACCEPT` returns a server-side socket id; materialise the client-side scheme
                // file (`net:accept/<id>`) so the new fd owns a real handle.
                //
                // Donor divergence (recorded for review): NetBSD `uipc_socket.c:sonewconn` and
                // Illumos `sockcommon_sops.c:so_accept` return the accepted socket object
                // directly and the file references it (`struct file.f_socket`/`sonode`); the
                // fd's `write` reaches that object's send queue (`sys_socket.c:soo_write` ->
                // `uipc_socket.c:sosend`). ABI v1 has no server-side SchemeFile-create, so K230
                // re-opens the server id as `net:accept/<id>`; `netd` maps it to the same
                // `SocketId`, keeping one socket object and one `tx`/`snd_*` state.
                let path = alloc::format!("{}{id}", raven_proto_net::path::ACCEPT_PREFIX);
                let handle =
                    open_scheme_path(self.platform.backend(), path.as_bytes(), WIRE_READ_WRITE)
                        .map_err(|error| Errno(error.0))?;
                Ok(CallReply::Accepted(Sock(handle.0)))
            }
            Response::Err(error) => Err(Errno(error.0)),
            _ => Err(Errno::EPROTO),
        }
    }

    fn mptcp_control(&self, s: Sock, request: &[u8], reply: &mut [u8]) -> Result<usize, Errno> {
        self.platform
            .backend()
            .call(raven_libc_platform::Handle(s.0), request, reply)
            .map_err(|error| Errno(error.0))
    }

    fn close(&self, s: Sock) {
        let _ = self
            .platform
            .backend()
            .close(raven_libc_platform::Handle(s.0));
    }

    fn wait(&self, items: &mut [WaitItem], timeout_ms: Option<u64>) -> Result<usize, Errno> {
        let mut fds = Vec::with_capacity(items.len());
        let mut owners = Vec::with_capacity(items.len());
        for (index, item) in items.iter_mut().enumerate() {
            item.got_read = false;
            item.got_write = false;
            item.got_err = false;
            item.got_hup = false;
            let Some(fd) = self.platform.fd_for_socket_handle(Handle(item.sock.0)) else {
                continue;
            };
            let mut events = 0i16;
            if item.want_read {
                events |= POLLIN as i16;
            }
            if item.want_write {
                events |= POLLOUT as i16;
            }
            if events == 0 {
                continue;
            }
            fds.push(Pollfd {
                fd,
                events,
                revents: 0,
            });
            owners.push(index);
        }
        if fds.is_empty() {
            return Ok(0);
        }
        let timeout = match timeout_ms {
            None => -1,
            Some(ms) => i32::try_from(ms).unwrap_or(i32::MAX),
        };
        let _ = raven_libc_platform::poll::poll(self.platform, &mut fds, timeout)
            .map_err(|error| Errno(error.0))?;
        let mut ready = 0usize;
        for (position, index) in owners.into_iter().enumerate() {
            let Some(entry) = fds.get(position) else {
                continue;
            };
            let Some(item) = items.get_mut(index) else {
                continue;
            };
            let revents = i32::from(entry.revents);
            item.got_read = revents & POLLIN != 0;
            item.got_write = revents & POLLOUT != 0;
            item.got_err = revents & POLLERR != 0;
            item.got_hup = revents & POLLHUP != 0;
            if item.got_read || item.got_write || item.got_err || item.got_hup {
                ready = ready.saturating_add(1);
            }
        }
        Ok(ready)
    }

    fn iface(&self, req: IfaceRequest) -> Result<IfaceResponse, Errno> {
        let handle = open_scheme_path(self.platform.backend(), b"net:", WIRE_READ_WRITE)
            .map_err(|error| Errno(error.0))?;
        let mut request = [0u8; 64];
        let used = req.encode(&mut request).map_err(|_| Errno::EINVAL)?;
        let mut response = [0u8; 256];
        let count = self
            .platform
            .backend()
            .call(handle, request.get(..used).unwrap_or(&[]), &mut response)
            .map_err(|error| Errno(error.0))?;
        let decoded = IfaceResponse::decode(response.get(..count).unwrap_or(&[]))
            .map_err(|_| Errno::EPROTO)?;
        let _ = self
            .platform
            .backend()
            .close(raven_libc_platform::Handle(handle.0));
        Ok(decoded)
    }
}

#[cfg(test)]
#[path = "backend_platform_ut.rs"]
mod tests;
