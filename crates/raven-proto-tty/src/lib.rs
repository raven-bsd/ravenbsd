// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Terminal control protocol for `consd` (K36).
//!
//! The console server speaks the S11 `CALL` opcode. This crate defines the 12-byte request and
//! response bodies that travel inside the captured request and response buffers:
//!
//! ```text
//! offset  size  field
//! 0       2     version (little endian, 1)
//! 2       2     op (little endian)
//! 4       4     arg0 (little endian)
//! 8       4     arg1 (little endian)
//! ```
//!
//! `SET_MODE` carries the new mode in `arg0` (0 canonical, 1 raw) and the echo switch in `arg1`
//! (0 off, 1 on) and answers with the applied settings; `GET_MODE` answers with the current ones;
//! `FLUSH` answers with the number of discarded input bytes in `arg0`. Unknown versions and ops
//! are protocol errors the server maps to `-ENOSYS`.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub use raven_tty_core::{Mode, Settings};

/// Protocol version carried in every request and response.
pub const VERSION: u16 = 1;

/// Encoded length of one request or response body, in bytes.
pub const CALL_LEN: usize = 12;

/// `SET_MODE(mode, echo)`.
pub const OP_SET_MODE: u16 = 1;
/// `GET_MODE`.
pub const OP_GET_MODE: u16 = 2;
/// `FLUSH`.
pub const OP_FLUSH: u16 = 3;

const CANONICAL: u32 = 0;
const RAW: u32 = 1;

/// A typed control request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Request {
    /// Switch mode and echo.
    SetMode {
        /// Requested mode and echo switch.
        settings: Settings,
    },
    /// Ask for the current settings.
    GetMode,
    /// Discard buffered input.
    Flush,
}

/// A typed control response.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Response {
    /// The applied or current settings.
    Mode(Settings),
    /// The number of input bytes discarded by `FLUSH`.
    Flushed {
        /// Discarded bytes.
        bytes: u32,
    },
}

/// A malformed or unsupported control message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TtyError {
    /// The message is shorter than [`CALL_LEN`].
    Truncated,
    /// The version field is not [`VERSION`].
    Version(u16),
    /// The op field is not one of the defined operations.
    UnknownOp(u16),
    /// The mode value is neither 0 nor 1.
    InvalidMode(u32),
    /// The echo value is neither 0 nor 1.
    InvalidEcho(u32),
}

impl TtyError {
    /// Stable human-readable description.
    pub fn describe(&self) -> &'static str {
        match self {
            Self::Truncated => "truncated control message",
            Self::Version(_) => "unsupported protocol version",
            Self::UnknownOp(_) => "unknown control operation",
            Self::InvalidMode(_) => "invalid terminal mode",
            Self::InvalidEcho(_) => "invalid echo switch",
        }
    }
}

/// Encode a request into `out`.
pub fn encode_request(request: Request, out: &mut [u8; CALL_LEN]) {
    let (op, arg0, arg1) = match request {
        Request::SetMode { settings } => (
            OP_SET_MODE,
            mode_value(settings.mode),
            u32::from(settings.echo),
        ),
        Request::GetMode => (OP_GET_MODE, 0, 0),
        Request::Flush => (OP_FLUSH, 0, 0),
    };
    out[..2].copy_from_slice(&VERSION.to_le_bytes());
    out[2..4].copy_from_slice(&op.to_le_bytes());
    out[4..8].copy_from_slice(&arg0.to_le_bytes());
    out[8..12].copy_from_slice(&arg1.to_le_bytes());
}

/// Decode a request body.
pub fn decode_request(input: &[u8]) -> Result<Request, TtyError> {
    let (op, arg0, arg1) = header(input)?;
    match op {
        OP_SET_MODE => Ok(Request::SetMode {
            settings: settings_from(arg0, arg1)?,
        }),
        OP_GET_MODE => Ok(Request::GetMode),
        OP_FLUSH => Ok(Request::Flush),
        other => Err(TtyError::UnknownOp(other)),
    }
}

/// Encode a response into `out`.
pub fn encode_response(response: Response, out: &mut [u8; CALL_LEN]) {
    let (op, arg0, arg1) = match response {
        Response::Mode(settings) => (
            OP_GET_MODE,
            mode_value(settings.mode),
            u32::from(settings.echo),
        ),
        Response::Flushed { bytes } => (OP_FLUSH, bytes, 0),
    };
    out[..2].copy_from_slice(&VERSION.to_le_bytes());
    out[2..4].copy_from_slice(&op.to_le_bytes());
    out[4..8].copy_from_slice(&arg0.to_le_bytes());
    out[8..12].copy_from_slice(&arg1.to_le_bytes());
}

/// Decode a response body.
pub fn decode_response(input: &[u8]) -> Result<Response, TtyError> {
    let (op, arg0, arg1) = header(input)?;
    match op {
        OP_GET_MODE | OP_SET_MODE => Ok(Response::Mode(settings_from(arg0, arg1)?)),
        OP_FLUSH => Ok(Response::Flushed { bytes: arg0 }),
        other => Err(TtyError::UnknownOp(other)),
    }
}

fn header(input: &[u8]) -> Result<(u16, u32, u32), TtyError> {
    if input.len() < CALL_LEN {
        return Err(TtyError::Truncated);
    }
    let version = u16::from_le_bytes([input[0], input[1]]);
    if version != VERSION {
        return Err(TtyError::Version(version));
    }
    let op = u16::from_le_bytes([input[2], input[3]]);
    let arg0 = u32::from_le_bytes([input[4], input[5], input[6], input[7]]);
    let arg1 = u32::from_le_bytes([input[8], input[9], input[10], input[11]]);
    Ok((op, arg0, arg1))
}

fn settings_from(mode: u32, echo: u32) -> Result<Settings, TtyError> {
    let mode = match mode {
        CANONICAL => Mode::Canonical,
        RAW => Mode::Raw,
        other => return Err(TtyError::InvalidMode(other)),
    };
    let echo = match echo {
        0 => false,
        1 => true,
        other => return Err(TtyError::InvalidEcho(other)),
    };
    Ok(Settings { mode, echo })
}

fn mode_value(mode: Mode) -> u32 {
    match mode {
        Mode::Canonical => CANONICAL,
        Mode::Raw => RAW,
    }
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
