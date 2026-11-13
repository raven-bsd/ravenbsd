// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Scheme clients: the typed `CALL` encoders and the `OPEN` path helper.
//!
//! A status is decoded exactly once: a scheme packet status is a positive errno `0..=4095`, a
//! `CALL` response status is `0` or a negative errno `-4095..=0`, and an undecodable reply is
//! `EPROTO`; an unknown version or op is `ENOSYS` (via [`raven_proto_fs::ProtoError::errno`]).

use raven_abi::errno::Errno;
use raven_proto_fs::{Call, CallRequest, CallResponse, Stat as WireStat};

use crate::backend::SysBackend;
use crate::xlat;

/// Encode one filesystem control call into `out`; returns the used length.
pub fn encode_fs_call(call: Call<'_>, out: &mut [u8]) -> Result<usize, Errno> {
    CallRequest::new(call)
        .encode(out)
        .map_err(|error| error.errno())
}

/// Decode a filesystem control response for `op`; `Ok(None)` means the status was non-zero and
/// has already been converted to the matching errno.
pub fn decode_fs_response(op: u16, input: &[u8]) -> Result<Option<CallResponse<'_>>, Errno> {
    let response = CallResponse::decode(op, input).map_err(|error| error.errno())?;
    xlat::call_status(response.status())?;
    Ok(Some(response))
}

/// Send one filesystem control call on `handle` and validate the status.
pub fn fs_call<B: SysBackend>(
    backend: &B,
    handle: raven_abi::types::Handle,
    call: Call<'_>,
) -> Result<(), Errno> {
    let mut request = [0u8; raven_proto_fs::CALL_HEADER_LEN + 2 * (2 + raven_proto_fs::MAX_PATH)];
    let encoded = encode_fs_call(call, &mut request)?;
    let mut response = [0u8; 4 + 2 + raven_proto_fs::MAX_PATH];
    let count = backend.call(handle, request.get(..encoded).unwrap_or(&[]), &mut response)?;
    let op = call_op(&call);
    let decoded = CallResponse::decode(op, response.get(..count).unwrap_or(&[]))
        .map_err(|error| error.errno())?;
    xlat::call_status(decoded.status())
}

/// The wire op for a call (kept here so a new op has one place to update).
pub const fn call_op(call: &Call<'_>) -> u16 {
    call.op()
}

/// Open an already-qualified scheme path (`file:/...`, `tty:console`, `net:tcp`, ...).
pub fn open_scheme_path<B: SysBackend>(
    backend: &B,
    scheme_path: &[u8],
    wire_flags: u16,
) -> Result<raven_abi::types::Handle, Errno> {
    backend.open(scheme_path, u32::from(wire_flags))
}

/// Decode a 64-byte `FSTAT` record.
pub fn fstat_record(record: &[u8; 64]) -> Result<WireStat, Errno> {
    WireStat::decode(record).map_err(|error| error.errno())
}
