// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Socket requests and responses for the `net:` scheme server.
//!
//! The scheme protocol mirrors the kernel's S11 operations. `OPEN` carries the scheme path,
//! `READ`/`WRITE` move stream bytes or one datagram, and `CALL` carries the typed socket control
//! operations. Attached handles (from `OPEN` and `ACCEPT`) travel through the kernel's handle
//! transfer, never inside a message; only the numeric socket id is encoded.
//!
//! Every message starts with a little-endian `u16` protocol version. A message with another
//! version is rejected with [`DecodeError`], never reinterpreted.

use core::fmt;

use raven_abi::errno::Errno;

use crate::DecodeError;
use crate::wire::{Reader, WireError, Writer};

/// Version of the socket and interface message framing.
pub const PROTOCOL_VERSION: u16 = 1;

/// Request kind: `OPEN`, carrying the scheme path.
pub const KIND_OPEN: u16 = 1;
/// Request kind: `READ`, carrying the maximum number of bytes to return.
pub const KIND_READ: u16 = 2;
/// Request kind: `WRITE`, carrying stream bytes or one datagram.
pub const KIND_WRITE: u16 = 3;
/// Request kind: `CALL`, carrying a typed socket operation.
pub const KIND_CALL: u16 = 4;

/// `CALL` operation: `BIND`.
pub const OP_BIND: u16 = 1;
/// `CALL` operation: `LISTEN`.
pub const OP_LISTEN: u16 = 2;
/// `CALL` operation: `ACCEPT`.
pub const OP_ACCEPT: u16 = 3;
/// `CALL` operation: `CONNECT`.
pub const OP_CONNECT: u16 = 4;
/// `CALL` operation: `SHUTDOWN`.
pub const OP_SHUTDOWN: u16 = 5;
/// `CALL` operation: `GETSOCKNAME`.
pub const OP_GETSOCKNAME: u16 = 6;
/// `CALL` operation: `GETPEERNAME`.
pub const OP_GETPEERNAME: u16 = 7;
/// `CALL` operation: `SETSOCKOPT`.
pub const OP_SETSOCKOPT: u16 = 8;
/// `CALL` operation: `GETSOCKOPT`.
pub const OP_GETSOCKOPT: u16 = 9;

/// Maximum length of an `OPEN` scheme path in bytes.
pub const MAX_PATH: usize = 64;
/// Maximum UDP datagram or `READ`/`WRITE` payload in bytes (the IPv4 limitation).
pub const MAX_DATAGRAM: usize = 65_535;
/// Largest accepted `LISTEN` backlog.
pub const MAX_BACKLOG: u16 = 4096;

/// Response tag: success without a body.
const TAG_OK: u16 = 0;
/// Response tag: `u32` socket id.
const TAG_SOCKET: u16 = 1;
/// Response tag: `READ` result.
const TAG_READ: u16 = 2;
/// Response tag: `WRITE` result.
const TAG_WRITE: u16 = 3;
/// Response tag: endpoint.
const TAG_ENDPOINT: u16 = 4;
/// Response tag: signed integer.
const TAG_VALUE: u16 = 5;
/// Response tag: errno.
const TAG_ERR: u16 = 6;

/// Write flag: a destination endpoint follows.
const WRITE_FLAG_ENDPOINT: u8 = 0x01;
/// Read response flag: a source endpoint follows.
const READ_FLAG_ENDPOINT: u8 = 0x01;

/// A socket endpoint on the wire: four IPv4 octets then a little-endian port.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Endpoint {
    /// IPv4 address octets.
    pub ip: [u8; 4],
    /// Port in host order.
    pub port: u16,
}

impl Endpoint {
    /// Encoded size in bytes.
    pub const LEN: usize = 6;

    fn write(&self, writer: &mut Writer<'_>) -> Result<(), WireError> {
        writer.put_bytes(&self.ip)?;
        writer.put_u16(self.port)
    }

    fn read(reader: &mut Reader<'_>) -> Result<Self, WireError> {
        let ip = reader.get_bytes(4)?;
        Ok(Self {
            ip: [ip[0], ip[1], ip[2], ip[3]],
            port: reader.get_u16()?,
        })
    }
}

/// Socket option ids used by `SETSOCKOPT`/`GETSOCKOPT`.
///
/// These are protocol ids, not Linux `SOL_SOCKET` numbers; `ralibc` maps its BSD constants onto
/// them. `SO_ERROR` is read-only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum SockOpt {
    /// `SO_REUSEADDR`.
    ReuseAddr = 1,
    /// `SO_RCVBUF`.
    RcvBuf = 2,
    /// `SO_SNDBUF`.
    SndBuf = 3,
    /// `TCP_NODELAY`.
    TcpNoDelay = 4,
    /// `SO_ERROR`.
    Error = 5,
}

impl SockOpt {
    /// Wire code.
    pub const fn code(self) -> u16 {
        self as u16
    }

    /// Decode a wire code.
    pub const fn from_code(code: u16) -> Option<Self> {
        match code {
            1 => Some(Self::ReuseAddr),
            2 => Some(Self::RcvBuf),
            3 => Some(Self::SndBuf),
            4 => Some(Self::TcpNoDelay),
            5 => Some(Self::Error),
            _ => None,
        }
    }

    /// Whether `SETSOCKOPT` accepts this option.
    pub const fn is_writable(self) -> bool {
        !matches!(self, Self::Error)
    }
}

/// Argument of `SHUTDOWN`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Shutdown {
    /// Stop reading.
    Read = 0,
    /// Stop writing.
    Write = 1,
    /// Stop both directions.
    Both = 2,
}

impl Shutdown {
    /// Decode a wire code.
    pub const fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::Read),
            1 => Some(Self::Write),
            2 => Some(Self::Both),
            _ => None,
        }
    }

    /// Wire code.
    pub const fn code(self) -> u8 {
        self as u8
    }
}

/// A typed socket control operation carried by `CALL`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Call {
    /// Bind to a local endpoint.
    Bind(Endpoint),
    /// Start listening with a backlog.
    Listen {
        /// Requested backlog, `1..=MAX_BACKLOG`.
        backlog: u16,
    },
    /// Accept a completed connection; the kernel attaches the new handle.
    Accept,
    /// Connect to a remote endpoint.
    Connect(Endpoint),
    /// Shut down one or both directions.
    Shutdown {
        /// Direction to shut down.
        how: Shutdown,
    },
    /// Return the local endpoint.
    GetSockName,
    /// Return the peer endpoint.
    GetPeerName,
    /// Set a socket option.
    SetSockOpt {
        /// Option to set.
        opt: SockOpt,
        /// New value.
        value: i32,
    },
    /// Read a socket option.
    GetSockOpt {
        /// Option to read.
        opt: SockOpt,
    },
}

/// A request sent to `netd`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SocketRequest<'a> {
    /// Open a socket for a `net:` path.
    Open {
        /// Scheme path, `1..=MAX_PATH` bytes.
        path: &'a str,
    },
    /// Read at most `len` bytes from the socket handle.
    Read {
        /// Maximum byte count, `0..=MAX_DATAGRAM`.
        len: u32,
    },
    /// Write one stream segment or datagram.
    Write {
        /// Payload, at most `MAX_DATAGRAM` bytes.
        data: &'a [u8],
        /// Datagram destination; absent for TCP and for a connected UDP socket.
        to: Option<Endpoint>,
    },
    /// Run a typed socket operation.
    Call(Call),
}

impl<'a> SocketRequest<'a> {
    /// Encode the request into `out` and return the byte count.
    ///
    /// Returns [`WireError::Truncated`] when `out` is too small and [`WireError::TooLong`] or
    /// [`WireError::Invalid`] when a field violates a protocol limit.
    pub fn encode(&self, out: &mut [u8]) -> Result<usize, WireError> {
        let mut writer = Writer::new(out);
        writer.put_u16(PROTOCOL_VERSION)?;
        match self {
            Self::Open { path } => {
                if path.is_empty() || path.len() > MAX_PATH {
                    return Err(WireError::TooLong);
                }
                if crate::path::parse(path).is_err() {
                    return Err(WireError::Invalid);
                }
                writer.put_u16(KIND_OPEN)?;
                writer.put_u16(path.len() as u16)?;
                writer.put_bytes(path.as_bytes())?;
            }
            Self::Read { len } => {
                if *len > MAX_DATAGRAM as u32 {
                    return Err(WireError::TooLong);
                }
                writer.put_u16(KIND_READ)?;
                writer.put_u32(*len)?;
            }
            Self::Write { data, to } => {
                if data.len() > MAX_DATAGRAM {
                    return Err(WireError::TooLong);
                }
                writer.put_u16(KIND_WRITE)?;
                writer.put_u8(u8::from(to.is_some()))?;
                writer.put_u32(data.len() as u32)?;
                if let Some(endpoint) = to {
                    endpoint.write(&mut writer)?;
                }
                writer.put_bytes(data)?;
            }
            Self::Call(call) => {
                writer.put_u16(KIND_CALL)?;
                call.write(&mut writer)?;
            }
        }
        Ok(writer.position())
    }

    /// Decode one request. The entire slice must belong to the request.
    pub fn decode(buffer: &'a [u8]) -> Result<SocketRequest<'a>, DecodeError> {
        let mut reader = Reader::new(buffer);
        let version = reader.get_u16().map_err(DecodeError::from)?;
        if version != PROTOCOL_VERSION {
            return Err(DecodeError::version());
        }
        let kind = reader.get_u16().map_err(DecodeError::from)?;
        let request = match kind {
            KIND_OPEN => {
                let length = reader.get_u16().map_err(DecodeError::from)? as usize;
                if length == 0 || length > MAX_PATH {
                    return Err(DecodeError::length());
                }
                let bytes = reader.get_bytes(length).map_err(DecodeError::from)?;
                let path = core::str::from_utf8(bytes).map_err(|_| DecodeError::malformed())?;
                if crate::path::parse(path).is_err() {
                    return Err(DecodeError::malformed());
                }
                Self::Open { path }
            }
            KIND_READ => {
                let len = reader.get_u32().map_err(DecodeError::from)?;
                if len > MAX_DATAGRAM as u32 {
                    return Err(DecodeError::length());
                }
                Self::Read { len }
            }
            KIND_WRITE => {
                let flags = reader.get_u8().map_err(DecodeError::from)?;
                if flags & !WRITE_FLAG_ENDPOINT != 0 {
                    return Err(DecodeError::malformed());
                }
                let length = reader.get_u32().map_err(DecodeError::from)? as usize;
                if length > MAX_DATAGRAM {
                    return Err(DecodeError::length());
                }
                let to = if flags & WRITE_FLAG_ENDPOINT != 0 {
                    Some(Endpoint::read(&mut reader).map_err(DecodeError::from)?)
                } else {
                    None
                };
                let data = reader.get_bytes(length).map_err(DecodeError::from)?;
                Self::Write { data, to }
            }
            KIND_CALL => Self::Call(Call::read(&mut reader)?),
            _ => return Err(DecodeError::tag()),
        };
        if !reader.is_empty() {
            return Err(DecodeError::trailing());
        }
        Ok(request)
    }
}

impl Call {
    fn write(&self, writer: &mut Writer<'_>) -> Result<(), WireError> {
        match self {
            Self::Bind(endpoint) => {
                writer.put_u16(OP_BIND)?;
                endpoint.write(writer)
            }
            Self::Listen { backlog } => {
                if *backlog == 0 || *backlog > MAX_BACKLOG {
                    return Err(WireError::Invalid);
                }
                writer.put_u16(OP_LISTEN)?;
                writer.put_u16(*backlog)
            }
            Self::Accept => writer.put_u16(OP_ACCEPT),
            Self::Connect(endpoint) => {
                writer.put_u16(OP_CONNECT)?;
                endpoint.write(writer)
            }
            Self::Shutdown { how } => {
                writer.put_u16(OP_SHUTDOWN)?;
                writer.put_u8(how.code())
            }
            Self::GetSockName => writer.put_u16(OP_GETSOCKNAME),
            Self::GetPeerName => writer.put_u16(OP_GETPEERNAME),
            Self::SetSockOpt { opt, value } => {
                if !opt.is_writable() {
                    return Err(WireError::Invalid);
                }
                if *value < 0 {
                    return Err(WireError::Invalid);
                }
                writer.put_u16(OP_SETSOCKOPT)?;
                writer.put_u16(opt.code())?;
                writer.put_i32(*value)
            }
            Self::GetSockOpt { opt } => {
                writer.put_u16(OP_GETSOCKOPT)?;
                writer.put_u16(opt.code())
            }
        }
    }

    fn read(reader: &mut Reader<'_>) -> Result<Self, DecodeError> {
        let op = reader.get_u16().map_err(DecodeError::from)?;
        let call = match op {
            OP_BIND => Self::Bind(Endpoint::read(reader).map_err(DecodeError::from)?),
            OP_LISTEN => {
                let backlog = reader.get_u16().map_err(DecodeError::from)?;
                if backlog == 0 || backlog > MAX_BACKLOG {
                    return Err(DecodeError::length());
                }
                Self::Listen { backlog }
            }
            OP_ACCEPT => Self::Accept,
            OP_CONNECT => Self::Connect(Endpoint::read(reader).map_err(DecodeError::from)?),
            OP_SHUTDOWN => {
                let how = Shutdown::from_code(reader.get_u8().map_err(DecodeError::from)?)
                    .ok_or_else(DecodeError::tag)?;
                Self::Shutdown { how }
            }
            OP_GETSOCKNAME => Self::GetSockName,
            OP_GETPEERNAME => Self::GetPeerName,
            OP_SETSOCKOPT => {
                let opt = SockOpt::from_code(reader.get_u16().map_err(DecodeError::from)?)
                    .ok_or_else(DecodeError::tag)?;
                if !opt.is_writable() {
                    return Err(DecodeError::malformed());
                }
                let value = reader.get_i32().map_err(DecodeError::from)?;
                if value < 0 {
                    return Err(DecodeError::malformed());
                }
                Self::SetSockOpt { opt, value }
            }
            OP_GETSOCKOPT => {
                let opt = SockOpt::from_code(reader.get_u16().map_err(DecodeError::from)?)
                    .ok_or_else(DecodeError::tag)?;
                Self::GetSockOpt { opt }
            }
            _ => return Err(DecodeError::tag()),
        };
        Ok(call)
    }
}

/// A response from `netd`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Response {
    /// Success without a body.
    Ok,
    /// A socket id; the attached handle travelled through the kernel.
    Socket {
        /// Numeric socket id.
        id: u32,
    },
    /// Result of `READ`; `from` is set for UDP datagrams.
    Read {
        /// Byte count.
        len: u32,
        /// Source endpoint for an unconnected UDP socket.
        from: Option<Endpoint>,
    },
    /// Result of `WRITE`.
    Write {
        /// Byte count accepted.
        len: u32,
    },
    /// Endpoint returned by `GETSOCKNAME`/`GETPEERNAME`.
    Endpoint(Endpoint),
    /// Integer returned by `GETSOCKOPT`.
    Value(i32),
    /// Failure carrying the errno the caller must report.
    Err(Errno),
}

impl Response {
    /// Build an error response.
    pub const fn error(errno: Errno) -> Self {
        Self::Err(errno)
    }

    /// Whether this response is an error.
    pub const fn is_error(self) -> bool {
        matches!(self, Self::Err(_))
    }

    /// Encode the response into `out` and return the byte count.
    pub fn encode(&self, out: &mut [u8]) -> Result<usize, WireError> {
        let mut writer = Writer::new(out);
        writer.put_u16(PROTOCOL_VERSION)?;
        match self {
            Self::Ok => {
                writer.put_u16(0)?;
                writer.put_u16(TAG_OK)?;
            }
            Self::Socket { id } => {
                writer.put_u16(0)?;
                writer.put_u16(TAG_SOCKET)?;
                writer.put_u32(*id)?;
            }
            Self::Read { len, from } => {
                if *len > MAX_DATAGRAM as u32 {
                    return Err(WireError::TooLong);
                }
                writer.put_u16(0)?;
                writer.put_u16(TAG_READ)?;
                writer.put_u8(u8::from(from.is_some()))?;
                writer.put_u32(*len)?;
                if let Some(endpoint) = from {
                    endpoint.write(&mut writer)?;
                }
            }
            Self::Write { len } => {
                if *len > MAX_DATAGRAM as u32 {
                    return Err(WireError::TooLong);
                }
                writer.put_u16(0)?;
                writer.put_u16(TAG_WRITE)?;
                writer.put_u32(*len)?;
            }
            Self::Endpoint(endpoint) => {
                writer.put_u16(0)?;
                writer.put_u16(TAG_ENDPOINT)?;
                endpoint.write(&mut writer)?;
            }
            Self::Value(value) => {
                writer.put_u16(0)?;
                writer.put_u16(TAG_VALUE)?;
                writer.put_i32(*value)?;
            }
            Self::Err(errno) => {
                writer.put_u16(errno.0)?;
                writer.put_u16(TAG_ERR)?;
            }
        }
        Ok(writer.position())
    }

    /// Decode one response. The entire slice must belong to the response.
    pub fn decode(buffer: &[u8]) -> Result<Self, DecodeError> {
        let mut reader = Reader::new(buffer);
        let version = reader.get_u16().map_err(DecodeError::from)?;
        if version != PROTOCOL_VERSION {
            return Err(DecodeError::version());
        }
        let status = reader.get_u16().map_err(DecodeError::from)?;
        let tag = reader.get_u16().map_err(DecodeError::from)?;
        let response = match tag {
            TAG_OK => {
                if status != 0 {
                    return Err(DecodeError::malformed());
                }
                Self::Ok
            }
            TAG_SOCKET => {
                if status != 0 {
                    return Err(DecodeError::malformed());
                }
                Self::Socket {
                    id: reader.get_u32().map_err(DecodeError::from)?,
                }
            }
            TAG_READ => {
                if status != 0 {
                    return Err(DecodeError::malformed());
                }
                let flags = reader.get_u8().map_err(DecodeError::from)?;
                if flags & !READ_FLAG_ENDPOINT != 0 {
                    return Err(DecodeError::malformed());
                }
                let len = reader.get_u32().map_err(DecodeError::from)?;
                if len > MAX_DATAGRAM as u32 {
                    return Err(DecodeError::length());
                }
                let from = if flags & READ_FLAG_ENDPOINT != 0 {
                    Some(Endpoint::read(&mut reader).map_err(DecodeError::from)?)
                } else {
                    None
                };
                Self::Read { len, from }
            }
            TAG_WRITE => {
                if status != 0 {
                    return Err(DecodeError::malformed());
                }
                let len = reader.get_u32().map_err(DecodeError::from)?;
                if len > MAX_DATAGRAM as u32 {
                    return Err(DecodeError::length());
                }
                Self::Write { len }
            }
            TAG_ENDPOINT => {
                if status != 0 {
                    return Err(DecodeError::malformed());
                }
                Self::Endpoint(Endpoint::read(&mut reader).map_err(DecodeError::from)?)
            }
            TAG_VALUE => {
                if status != 0 {
                    return Err(DecodeError::malformed());
                }
                Self::Value(reader.get_i32().map_err(DecodeError::from)?)
            }
            TAG_ERR => {
                if status == 0 {
                    return Err(DecodeError::malformed());
                }
                Self::Err(Errno(status))
            }
            _ => return Err(DecodeError::tag()),
        };
        if !reader.is_empty() {
            return Err(DecodeError::trailing());
        }
        Ok(response)
    }
}

impl fmt::Display for SockOpt {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::ReuseAddr => "SO_REUSEADDR",
            Self::RcvBuf => "SO_RCVBUF",
            Self::SndBuf => "SO_SNDBUF",
            Self::TcpNoDelay => "TCP_NODELAY",
            Self::Error => "SO_ERROR",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOCAL: Endpoint = Endpoint {
        ip: [10, 0, 0, 1],
        port: 8080,
    };

    fn encode_request(request: SocketRequest<'_>) -> Vec<u8> {
        let mut buffer = [0u8; 256];
        let length = request.encode(&mut buffer).unwrap();
        buffer[..length].to_vec()
    }

    fn encode_response(response: Response) -> Vec<u8> {
        let mut buffer = [0u8; 256];
        let length = response.encode(&mut buffer).unwrap();
        buffer[..length].to_vec()
    }

    #[test]
    fn open_golden_bytes() {
        let bytes = encode_request(SocketRequest::Open {
            path: "net:tcp/10.0.0.2:80",
        });
        assert_eq!(
            bytes,
            [
                0x01, 0x00, // version 1
                0x01, 0x00, // OPEN
                0x13, 0x00, // 19-byte path
                b'n', b'e', b't', b':', b't', b'c', b'p', b'/', b'1', b'0', b'.', b'0', b'.', b'0',
                b'.', b'2', b':', b'8', b'0',
            ]
        );
    }

    #[test]
    fn read_golden_bytes() {
        assert_eq!(
            encode_request(SocketRequest::Read { len: 1500 }),
            [0x01, 0x00, 0x02, 0x00, 0xdc, 0x05, 0x00, 0x00]
        );
    }

    #[test]
    fn write_golden_bytes() {
        assert_eq!(
            encode_request(SocketRequest::Write {
                data: b"hi",
                to: None
            }),
            [
                0x01, 0x00, 0x03, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, b'h', b'i'
            ]
        );
    }

    #[test]
    fn write_with_endpoint_golden_bytes() {
        assert_eq!(
            encode_request(SocketRequest::Write {
                data: b"x",
                to: Some(LOCAL)
            }),
            [
                0x01, 0x00, 0x03, 0x00, 0x01, 0x01, 0x00, 0x00, 0x00, 10, 0, 0, 1, 0x90, 0x1f, b'x'
            ]
        );
    }

    #[test]
    fn bind_and_listen_golden_bytes() {
        assert_eq!(
            encode_request(SocketRequest::Call(Call::Bind(LOCAL))),
            [0x01, 0x00, 0x04, 0x00, 0x01, 0x00, 10, 0, 0, 1, 0x90, 0x1f]
        );
        assert_eq!(
            encode_request(SocketRequest::Call(Call::Listen { backlog: 8 })),
            [0x01, 0x00, 0x04, 0x00, 0x02, 0x00, 0x08, 0x00]
        );
    }

    #[test]
    fn shutdown_and_options_golden_bytes() {
        assert_eq!(
            encode_request(SocketRequest::Call(Call::Shutdown {
                how: Shutdown::Both
            })),
            [0x01, 0x00, 0x04, 0x00, 0x05, 0x00, 0x02]
        );
        assert_eq!(
            encode_request(SocketRequest::Call(Call::SetSockOpt {
                opt: SockOpt::ReuseAddr,
                value: 1
            })),
            [
                0x01, 0x00, 0x04, 0x00, 0x08, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00
            ]
        );
        assert_eq!(
            encode_request(SocketRequest::Call(Call::GetSockOpt {
                opt: SockOpt::Error
            })),
            [0x01, 0x00, 0x04, 0x00, 0x09, 0x00, 0x05, 0x00]
        );
    }

    #[test]
    fn request_roundtrips() {
        for request in [
            SocketRequest::Open {
                path: "net:udp/10.0.0.2:53",
            },
            SocketRequest::Read { len: 0 },
            SocketRequest::Write {
                data: b"payload",
                to: None,
            },
            SocketRequest::Call(Call::Accept),
            SocketRequest::Call(Call::Connect(LOCAL)),
            SocketRequest::Call(Call::GetSockName),
            SocketRequest::Call(Call::GetPeerName),
        ] {
            let bytes = encode_request(request);
            assert_eq!(SocketRequest::decode(&bytes), Ok(request), "{request:?}");
        }
    }

    #[test]
    fn responses_roundtrip() {
        for response in [
            Response::Ok,
            Response::Socket { id: 7 },
            Response::Read {
                len: 10,
                from: Some(LOCAL),
            },
            Response::Read { len: 0, from: None },
            Response::Write { len: 10 },
            Response::Endpoint(LOCAL),
            Response::Value(-1),
            Response::Err(Errno::EAGAIN),
        ] {
            let bytes = encode_response(response);
            assert_eq!(Response::decode(&bytes), Ok(response), "{response:?}");
        }
    }

    #[test]
    fn response_golden_bytes() {
        assert_eq!(
            encode_response(Response::Socket { id: 0x0102_0304 }),
            [0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x04, 0x03, 0x02, 0x01]
        );
        assert_eq!(
            encode_response(Response::Read {
                len: 2,
                from: Some(LOCAL)
            }),
            [
                0x01, 0x00, 0x00, 0x00, 0x02, 0x00, 0x01, 0x02, 0x00, 0x00, 0x00, 10, 0, 0, 1,
                0x90, 0x1f
            ]
        );
        assert_eq!(
            encode_response(Response::Err(Errno::EAGAIN)),
            [0x01, 0x00, 0x0b, 0x00, 0x06, 0x00]
        );
    }

    #[test]
    fn unknown_version_is_rejected() {
        let mut bytes = encode_request(SocketRequest::Read { len: 1 });
        bytes[0] = 2;
        assert_eq!(SocketRequest::decode(&bytes), Err(DecodeError::version()));
        assert_eq!(Response::decode(&bytes), Err(DecodeError::version()));
    }

    #[test]
    fn unknown_kind_and_tag_are_rejected() {
        let mut bytes = encode_request(SocketRequest::Read { len: 1 });
        bytes[2] = 9;
        assert_eq!(SocketRequest::decode(&bytes), Err(DecodeError::tag()));
        let mut bytes = encode_response(Response::Ok);
        bytes[4] = 9;
        assert_eq!(Response::decode(&bytes), Err(DecodeError::tag()));
    }

    #[test]
    fn truncation_is_rejected_everywhere() {
        let bytes = encode_request(SocketRequest::Call(Call::Bind(LOCAL)));
        for length in 0..bytes.len() {
            assert!(SocketRequest::decode(&bytes[..length]).is_err(), "{length}");
        }
        let bytes = encode_response(Response::Endpoint(LOCAL));
        for length in 0..bytes.len() {
            assert!(Response::decode(&bytes[..length]).is_err(), "{length}");
        }
    }

    #[test]
    fn trailing_bytes_are_rejected() {
        let mut bytes = encode_request(SocketRequest::Read { len: 1 });
        bytes.push(0);
        assert_eq!(SocketRequest::decode(&bytes), Err(DecodeError::trailing()));
        let mut bytes = encode_response(Response::Ok);
        bytes.push(0);
        assert_eq!(Response::decode(&bytes), Err(DecodeError::trailing()));
    }

    #[test]
    fn oversized_lengths_are_rejected() {
        let bytes = [
            0x01, 0x00, 0x02, 0x00, 0x00, 0x00, 0x01, 0x00, // READ 65536
        ];
        assert_eq!(SocketRequest::decode(&bytes), Err(DecodeError::length()));
        let bytes = [
            0x01, 0x00, 0x04, 0x00, 0x02, 0x00, 0x00, 0x00, // LISTEN 0
        ];
        assert_eq!(SocketRequest::decode(&bytes), Err(DecodeError::length()));
    }

    #[test]
    fn read_only_option_cannot_be_set() {
        let mut buffer = [0u8; 32];
        assert_eq!(
            SocketRequest::Call(Call::SetSockOpt {
                opt: SockOpt::Error,
                value: 0
            })
            .encode(&mut buffer),
            Err(WireError::Invalid)
        );
    }

    #[test]
    fn empty_path_is_rejected() {
        let mut buffer = [0u8; 32];
        assert_eq!(
            SocketRequest::Open { path: "" }.encode(&mut buffer),
            Err(WireError::TooLong)
        );
        assert_eq!(
            SocketRequest::Open { path: "file:/x" }.encode(&mut buffer),
            Err(WireError::Invalid)
        );
    }

    #[test]
    fn option_codes_roundtrip() {
        for opt in [
            SockOpt::ReuseAddr,
            SockOpt::RcvBuf,
            SockOpt::SndBuf,
            SockOpt::TcpNoDelay,
            SockOpt::Error,
        ] {
            assert_eq!(SockOpt::from_code(opt.code()), Some(opt));
        }
        assert_eq!(SockOpt::from_code(0), None);
    }
}
