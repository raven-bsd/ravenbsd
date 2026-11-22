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
//!
//! `GET_TERMIOS`/`SET_TERMIOS` carry the complete 60-byte ABI record field by field in a
//! [`TERMIOS_LEN`]-byte body (K444). Every field is little-endian with a fixed offset; no struct
//! memory is sent, so the ABI padding bytes never travel. `GET_WINSIZE`/`SET_WINSIZE` carry the
//! four `struct winsize` fields in a [`WINSIZE_LEN`]-byte body (K455).

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub use raven_tty_core::{Mode, Settings, Termios, Winsize};

/// Protocol version carried in every request and response.
pub const VERSION: u16 = 1;

/// Encoded length of one short request or response body, in bytes.
pub const CALL_LEN: usize = 12;

/// Encoded length of a `GET_TERMIOS`/`SET_TERMIOS` body: the 12-byte header followed by the
/// four flag words, `c_line`, `c_cc[32]` and the two speeds.
pub const TERMIOS_LEN: usize = CALL_LEN + 4 * 4 + 1 + 32 + 4 + 4;

/// Encoded length of a `GET_WINSIZE`/`SET_WINSIZE` body: the 12-byte header and four little-endian
/// `u16` fields.
pub const WINSIZE_LEN: usize = CALL_LEN + 4 * 2;

/// `SET_MODE(mode, echo)`.
pub const OP_SET_MODE: u16 = 1;
/// `GET_MODE`.
pub const OP_GET_MODE: u16 = 2;
/// `FLUSH`.
pub const OP_FLUSH: u16 = 3;
/// `GET_WRITE`.
pub const OP_GET_WRITE: u16 = 4;
/// `SET_WRITE(allowed)`.
pub const OP_SET_WRITE: u16 = 5;
/// `GET_TERMIOS`.
pub const OP_GET_TERMIOS: u16 = 6;
/// `SET_TERMIOS`.
pub const OP_SET_TERMIOS: u16 = 7;
/// `GET_WINSIZE`.
pub const OP_GET_WINSIZE: u16 = 8;
/// `SET_WINSIZE`.
pub const OP_SET_WINSIZE: u16 = 9;

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

/// A complete-record request (`GET_TERMIOS`/`SET_TERMIOS`).
///
/// Kept separate from [`Request`] so the short-body server matches (`consd`, `login`, `talk`) do
/// not have to grow match arms for a call they never issue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TermiosRequest {
    /// Ask for the complete terminal attribute record.
    Get,
    /// Install the complete terminal attribute record.
    Set {
        /// Requested attributes.
        termios: Termios,
    },
}

/// A window-size request (`GET_WINSIZE`/`SET_WINSIZE`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WinsizeRequest {
    /// Ask for the current window size.
    Get,
    /// Install the requested window size.
    Set {
        /// Requested geometry.
        winsize: Winsize,
    },
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
    /// The write-permission value is neither 0 nor 1.
    InvalidWrite(u32),
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
            Self::InvalidWrite(_) => "invalid write-permission switch",
        }
    }
}

/// A terminal write-permission request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriteRequest {
    /// Ask for the current permission.
    Get,
    /// Allow or refuse terminal writes.
    Set {
        /// Requested write permission.
        allowed: bool,
    },
}

/// Encode a write-permission request.
pub fn encode_write_request(request: WriteRequest, out: &mut [u8; CALL_LEN]) {
    let (op, arg0) = match request {
        WriteRequest::Get => (OP_GET_WRITE, 0),
        WriteRequest::Set { allowed } => (OP_SET_WRITE, u32::from(allowed)),
    };
    out[..2].copy_from_slice(&VERSION.to_le_bytes());
    out[2..4].copy_from_slice(&op.to_le_bytes());
    out[4..8].copy_from_slice(&arg0.to_le_bytes());
    out[8..12].copy_from_slice(&0u32.to_le_bytes());
}

/// Decode a write-permission request.
pub fn decode_write_request(input: &[u8]) -> Result<WriteRequest, TtyError> {
    let (op, arg0, _) = header(input)?;
    match op {
        OP_GET_WRITE => Ok(WriteRequest::Get),
        OP_SET_WRITE => Ok(WriteRequest::Set {
            allowed: write_value(arg0)?,
        }),
        other => Err(TtyError::UnknownOp(other)),
    }
}

/// Encode a write-permission response.
pub fn encode_write_response(allowed: bool, out: &mut [u8; CALL_LEN]) {
    out[..2].copy_from_slice(&VERSION.to_le_bytes());
    out[2..4].copy_from_slice(&OP_GET_WRITE.to_le_bytes());
    out[4..8].copy_from_slice(&u32::from(allowed).to_le_bytes());
    out[8..12].copy_from_slice(&0u32.to_le_bytes());
}

/// Decode a write-permission response.
pub fn decode_write_response(input: &[u8]) -> Result<bool, TtyError> {
    let (op, arg0, _) = header(input)?;
    match op {
        OP_GET_WRITE | OP_SET_WRITE => write_value(arg0),
        other => Err(TtyError::UnknownOp(other)),
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

/// Encode `GET_TERMIOS` into a [`TERMIOS_LEN`] body.
pub fn encode_get_termios_request(out: &mut [u8; TERMIOS_LEN]) {
    put_header(OP_GET_TERMIOS, out);
    out[CALL_LEN..].fill(0);
}

/// Encode `SET_TERMIOS` into a [`TERMIOS_LEN`] body.
pub fn encode_set_termios_request(termios: &Termios, out: &mut [u8; TERMIOS_LEN]) {
    put_header(OP_SET_TERMIOS, out);
    put_termios(termios, out);
}

/// Encode a `GET_TERMIOS`/`SET_TERMIOS` answer into a [`TERMIOS_LEN`] body.
pub fn encode_termios_response(termios: &Termios, out: &mut [u8; TERMIOS_LEN]) {
    put_header(OP_GET_TERMIOS, out);
    put_termios(termios, out);
}

fn put_header(op: u16, out: &mut [u8]) {
    out[..2].copy_from_slice(&VERSION.to_le_bytes());
    out[2..4].copy_from_slice(&op.to_le_bytes());
    out[4..8].copy_from_slice(&0u32.to_le_bytes());
    out[8..12].copy_from_slice(&0u32.to_le_bytes());
}

fn put_termios(termios: &Termios, out: &mut [u8; TERMIOS_LEN]) {
    out[12..16].copy_from_slice(&termios.iflag.to_le_bytes());
    out[16..20].copy_from_slice(&termios.oflag.to_le_bytes());
    out[20..24].copy_from_slice(&termios.cflag.to_le_bytes());
    out[24..28].copy_from_slice(&termios.lflag.to_le_bytes());
    out[28] = termios.line;
    out[29..61].copy_from_slice(&termios.cc);
    out[61..65].copy_from_slice(&termios.ispeed.to_le_bytes());
    out[65..69].copy_from_slice(&termios.ospeed.to_le_bytes());
}

fn termios_from(input: &[u8]) -> Result<Termios, TtyError> {
    if input.len() < TERMIOS_LEN {
        return Err(TtyError::Truncated);
    }
    let mut cc = [0u8; 32];
    cc.copy_from_slice(&input[29..61]);
    Ok(Termios {
        iflag: u32::from_le_bytes([input[12], input[13], input[14], input[15]]),
        oflag: u32::from_le_bytes([input[16], input[17], input[18], input[19]]),
        cflag: u32::from_le_bytes([input[20], input[21], input[22], input[23]]),
        lflag: u32::from_le_bytes([input[24], input[25], input[26], input[27]]),
        line: input[28],
        cc,
        ispeed: u32::from_le_bytes([input[61], input[62], input[63], input[64]]),
        ospeed: u32::from_le_bytes([input[65], input[66], input[67], input[68]]),
    })
}

/// Encode `GET_WINSIZE` into a [`WINSIZE_LEN`] body.
pub fn encode_get_winsize_request(out: &mut [u8; WINSIZE_LEN]) {
    put_header(OP_GET_WINSIZE, out);
    out[CALL_LEN..].fill(0);
}

/// Encode `SET_WINSIZE` into a [`WINSIZE_LEN`] body.
pub fn encode_set_winsize_request(winsize: &Winsize, out: &mut [u8; WINSIZE_LEN]) {
    put_header(OP_SET_WINSIZE, out);
    put_winsize(winsize, out);
}

/// Encode a `GET_WINSIZE`/`SET_WINSIZE` answer into a [`WINSIZE_LEN`] body.
pub fn encode_winsize_response(winsize: &Winsize, out: &mut [u8; WINSIZE_LEN]) {
    put_header(OP_GET_WINSIZE, out);
    put_winsize(winsize, out);
}

fn put_winsize(winsize: &Winsize, out: &mut [u8; WINSIZE_LEN]) {
    out[12..14].copy_from_slice(&winsize.row.to_le_bytes());
    out[14..16].copy_from_slice(&winsize.col.to_le_bytes());
    out[16..18].copy_from_slice(&winsize.xpixel.to_le_bytes());
    out[18..20].copy_from_slice(&winsize.ypixel.to_le_bytes());
}

/// Decode a `GET_WINSIZE`/`SET_WINSIZE` body.
pub fn decode_winsize_request(input: &[u8]) -> Result<WinsizeRequest, TtyError> {
    let (op, _, _) = header(input)?;
    match op {
        OP_GET_WINSIZE => {
            if input.len() < WINSIZE_LEN {
                return Err(TtyError::Truncated);
            }
            Ok(WinsizeRequest::Get)
        }
        OP_SET_WINSIZE => Ok(WinsizeRequest::Set {
            winsize: winsize_from(input)?,
        }),
        other => Err(TtyError::UnknownOp(other)),
    }
}

/// Decode a `GET_WINSIZE`/`SET_WINSIZE` answer body.
pub fn decode_winsize_response(input: &[u8]) -> Result<Winsize, TtyError> {
    let (op, _, _) = header(input)?;
    match op {
        OP_GET_WINSIZE | OP_SET_WINSIZE => winsize_from(input),
        other => Err(TtyError::UnknownOp(other)),
    }
}

fn winsize_from(input: &[u8]) -> Result<Winsize, TtyError> {
    if input.len() < WINSIZE_LEN {
        return Err(TtyError::Truncated);
    }
    Ok(Winsize {
        row: u16::from_le_bytes([input[12], input[13]]),
        col: u16::from_le_bytes([input[14], input[15]]),
        xpixel: u16::from_le_bytes([input[16], input[17]]),
        ypixel: u16::from_le_bytes([input[18], input[19]]),
    })
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

/// Decode a `GET_TERMIOS`/`SET_TERMIOS` body.
pub fn decode_termios_request(input: &[u8]) -> Result<TermiosRequest, TtyError> {
    let (op, _, _) = header(input)?;
    match op {
        OP_GET_TERMIOS => {
            if input.len() < TERMIOS_LEN {
                return Err(TtyError::Truncated);
            }
            Ok(TermiosRequest::Get)
        }
        OP_SET_TERMIOS => Ok(TermiosRequest::Set {
            termios: termios_from(input)?,
        }),
        other => Err(TtyError::UnknownOp(other)),
    }
}

/// Decode a `GET_TERMIOS`/`SET_TERMIOS` answer body.
pub fn decode_termios_response(input: &[u8]) -> Result<Termios, TtyError> {
    let (op, _, _) = header(input)?;
    match op {
        OP_GET_TERMIOS | OP_SET_TERMIOS => termios_from(input),
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

fn write_value(value: u32) -> Result<bool, TtyError> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        other => Err(TtyError::InvalidWrite(other)),
    }
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
