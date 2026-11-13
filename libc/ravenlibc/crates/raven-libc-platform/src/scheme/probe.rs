// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Fd-kind probing for handles adopted from another process.
//!
//! The cheapest method the protocol allows is the tty `GET_MODE` call: a tty answers, every
//! other server returns `ENOSYS`/`EINVAL`. An OFD caches the answer, so the probe runs at most
//! once per OFD (trap 21).

use raven_abi::errno::Errno;
use raven_abi::types::Handle;
use raven_proto_tty::{self, Request};

use crate::backend::SysBackend;
use crate::fd::ofd::OfdKind;

/// Probe a handle: `Tty` on a decoded `GET_MODE` answer, `Unknown` otherwise.
pub fn probe_kind<B: SysBackend>(backend: &B, handle: Handle) -> OfdKind {
    let mut request = [0u8; raven_proto_tty::CALL_LEN];
    raven_proto_tty::encode_request(Request::GetMode, &mut request);
    let mut response = [0u8; raven_proto_tty::CALL_LEN];
    match backend.call(handle, &request, &mut response) {
        Ok(count)
            if count >= raven_proto_tty::CALL_LEN
                && raven_proto_tty::decode_response(response.get(..count).unwrap_or(&[]))
                    .is_ok() =>
        {
            OfdKind::Tty
        }
        Err(Errno::ENOSYS) | Err(Errno::EINVAL) | Err(Errno::ENOTTY) | Ok(_) => OfdKind::Unknown,
        Err(_) => OfdKind::Unknown,
    }
}
