// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Wire format for the `proc:` process-model scheme.
//!
//! The format is version 1 of the protocol sketched in the libc donor map: a 16-byte request
//! header, aligned little-endian fields, length-prefixed strings and argument blobs, and a
//! 16-byte response header followed by an operation-specific body. The first `procd` server
//! implements the spawn/wait/kill/group subset the current kernel can support; the remaining
//! operations are recognised so the server can answer `ENOSYS` without guessing at a body.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

pub mod wire;
pub use wire::{
    ARG_MAX, Body, Error, FdMapEntry, GetPgidRequest, GetSidRequest, HelloRequest, KillRequest,
    MAX_ARGS, MAX_FDS, MAX_MESSAGE, MAX_STRING, Op, Request, RequestHeader, Response,
    ResponseHeader, SetPgidRequest, SpawnRequest, VERSION, WaitRequest, decode_request,
    decode_response, encode_request, encode_response, measure_request, measure_response,
};
