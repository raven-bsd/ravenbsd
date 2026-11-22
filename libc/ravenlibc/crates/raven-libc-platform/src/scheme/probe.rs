// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Fd-kind probing for handles adopted from another process.
//!
//! The cheapest method the protocol allows is the tty `GET_MODE` call: a tty answers, every
//! other server returns `ENOSYS`/`EINVAL`. When it does not answer, the filesystem `FSTAT`
//! record still carries the file type; a netd socket reports `S_IFSOCK`, so an adopted socket
//! keeps the `OfdKind::Socket` blocking-read policy instead of degrading to `Unknown`. An OFD
//! caches the answer, so the probe runs at most once per OFD (trap 21).

use raven_abi::errno::Errno;
use raven_abi::types::Handle;
use raven_proto_fs::{S_IFMT, S_IFSOCK};
use raven_proto_net::{Call, SocketRequest};
use raven_proto_tty::{self, Request};

use crate::backend::SysBackend;
use crate::fd::ofd::OfdKind;

/// Probe a handle: `Tty` on a decoded `GET_MODE` answer, `Socket` on an `S_IFSOCK` `FSTAT`
/// record, `Unknown` otherwise.
pub fn probe_kind<B: SysBackend>(backend: &B, handle: Handle) -> OfdKind {
    if tty_probe(backend, handle) {
        return OfdKind::Tty;
    }
    if socket_probe(backend, handle) {
        return OfdKind::Socket;
    }
    OfdKind::Unknown
}

/// Ask the scheme server for the readiness event of an adopted socket.
///
/// The event handle is a separate capability: the transferred data handle does not carry it, and
/// a blocking read on a scheme socket needs the event as its wake hint. Best-effort callers keep
/// the fd even when the server does not provide one.
pub(crate) fn socket_event<B: SysBackend>(backend: &B, handle: Handle) -> Result<Handle, Errno> {
    let request = SocketRequest::Call(Call::Event);
    let mut encoded = [0u8; 64];
    let used = request.encode(&mut encoded).map_err(|_| Errno::EINVAL)?;
    let mut response = [0u8; 16];
    let _ = backend.call(
        handle,
        encoded.get(..used).unwrap_or(&[]),
        &mut response,
    )?;
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

fn tty_probe<B: SysBackend>(backend: &B, handle: Handle) -> bool {
    let mut request = [0u8; raven_proto_tty::CALL_LEN];
    raven_proto_tty::encode_request(Request::GetMode, &mut request);
    let mut response = [0u8; raven_proto_tty::CALL_LEN];
    matches!(
        backend.call(handle, &request, &mut response),
        Ok(count)
            if count >= raven_proto_tty::CALL_LEN
                && raven_proto_tty::decode_response(response.get(..count).unwrap_or(&[])).is_ok()
    )
}

fn socket_probe<B: SysBackend>(backend: &B, handle: Handle) -> bool {
    let mut record = [0u8; raven_proto_fs::STAT_LEN];
    if backend.fstat(handle, &mut record).is_err() {
        return false;
    }
    match raven_proto_fs::Stat::decode(&record) {
        Ok(stat) => stat.mode & S_IFMT == S_IFSOCK,
        Err(_) => false,
    }
}
