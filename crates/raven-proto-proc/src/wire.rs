// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The version-1 `proc:` request and response codec.
//!
//! All integers are little-endian. A four-byte field starts at a multiple of four and an eight-byte
//! field at a multiple of eight; zero padding is inserted before each aligned field. A string is a
//! `u32` byte length, the bytes and padding to four bytes, with no terminating NUL. An argument
//! vector is a `u32` count, a `u32` total byte length, then the NUL-terminated entries packed
//! together, followed by padding to four bytes.

use alloc::vec::Vec;

use raven_abi::errno::Errno;

/// Protocol version.
pub const VERSION: u16 = 1;
/// Largest accepted message.
pub const MAX_MESSAGE: usize = 4 * 1024 * 1024;
/// Largest accepted string.
pub const MAX_STRING: usize = 4096;
/// Largest accepted argument vector.
pub const MAX_ARGS: usize = 32_768;
/// Largest argument/environment byte blob.
pub const ARG_MAX: usize = 131_072;
/// Largest accepted descriptor map.
pub const MAX_FDS: usize = 256;

/// A malformed or unsupported protocol message.
pub type Error = Errno;

/// One protocol operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum Op {
    /// Version handshake.
    Hello = 0x01,
    /// Spawn a new image.
    Spawn = 0x02,
    /// Fork the caller.
    Fork = 0x03,
    /// Replace the caller's image.
    Exec = 0x04,
    /// Wait for a child.
    Wait = 0x05,
    /// Send a signal.
    Kill = 0x06,
    /// Set a process group.
    SetPgid = 0x07,
    /// Create a session.
    SetSid = 0x08,
    /// Read a process group.
    GetPgid = 0x09,
    /// Read a session id.
    GetSid = 0x0A,
    /// Change credentials.
    SetCred = 0x0B,
    /// Read supplementary groups.
    GetGroups = 0x0C,
    /// Read or write a resource limit.
    Rlimit = 0x0D,
    /// Read resource usage.
    Rusage = 0x0E,
    /// Read or write a priority.
    Prio = 0x0F,
    /// Scheduler operation.
    Sched = 0x10,
    /// Set the process title.
    SetTitle = 0x11,
    /// Native `prctl` subset.
    Prctl = 0x12,
    /// Login-name operation.
    Login = 0x13,
    /// Apply a pledge.
    Pledge = 0x14,
    /// Enter capability mode.
    CapEnter = 0x15,
    /// Query session information.
    SessionInfo = 0x16,
}

impl Op {
    /// Decode an operation number.
    #[must_use]
    pub const fn from_u16(value: u16) -> Option<Self> {
        Some(match value {
            0x01 => Self::Hello,
            0x02 => Self::Spawn,
            0x03 => Self::Fork,
            0x04 => Self::Exec,
            0x05 => Self::Wait,
            0x06 => Self::Kill,
            0x07 => Self::SetPgid,
            0x08 => Self::SetSid,
            0x09 => Self::GetPgid,
            0x0A => Self::GetSid,
            0x0B => Self::SetCred,
            0x0C => Self::GetGroups,
            0x0D => Self::Rlimit,
            0x0E => Self::Rusage,
            0x0F => Self::Prio,
            0x10 => Self::Sched,
            0x11 => Self::SetTitle,
            0x12 => Self::Prctl,
            0x13 => Self::Login,
            0x14 => Self::Pledge,
            0x15 => Self::CapEnter,
            0x16 => Self::SessionInfo,
            _ => return None,
        })
    }
}

/// One fd-to-handle pair in a spawn map.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FdMapEntry {
    /// Destination file descriptor.
    pub fd: i32,
    /// Sender handle value.
    pub handle: u32,
}

/// The request header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RequestHeader {
    /// Protocol version.
    pub version: u16,
    /// Operation.
    pub op: Op,
    /// Operation flags.
    pub flags: u32,
    /// Sequence number.
    pub seq: u64,
}

/// The response header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResponseHeader {
    /// Zero or `-errno`.
    pub status: i32,
    /// Reserved.
    pub reserved: u32,
    /// Primary scalar result.
    pub value: i64,
}

/// A decoded request.
#[allow(clippy::large_enum_variant)] // spawn carries the fixed 384-byte signal masks once
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    /// Version handshake.
    Hello(HelloRequest),
    /// Spawn a new image.
    Spawn(SpawnRequest),
    /// Wait for a child.
    Wait(WaitRequest),
    /// Send a signal.
    Kill(KillRequest),
    /// Set a process group.
    SetPgid(SetPgidRequest),
    /// Read a process group.
    GetPgid(GetPgidRequest),
    /// Read a session id.
    GetSid(GetSidRequest),
    /// A known operation this server does not implement.
    Other(Op),
}

impl Request {
    /// The operation code.
    #[must_use]
    pub const fn op(&self) -> Op {
        match self {
            Self::Hello(_) => Op::Hello,
            Self::Spawn(_) => Op::Spawn,
            Self::Wait(_) => Op::Wait,
            Self::Kill(_) => Op::Kill,
            Self::SetPgid(_) => Op::SetPgid,
            Self::GetPgid(_) => Op::GetPgid,
            Self::GetSid(_) => Op::GetSid,
            Self::Other(op) => *op,
        }
    }
}

macro_rules! request_struct {
    ($name:ident { $($field:ident: $ty:ty),* $(,)? }) => {
        /// One protocol request body.
        #[allow(missing_docs)] // field names are the documented wire table
        #[derive(Clone, Debug, PartialEq, Eq)]
        pub struct $name {
            $(pub $field: $ty,)*
        }
    };
}

request_struct!(HelloRequest {
    client_version: u32,
});
request_struct!(SpawnRequest {
    spawn_flags: u32,
    pgroup: i32,
    sigdefault: [u8; 128],
    sigmask: [u8; 128],
    ignored: [u8; 128],
    sched_policy: i32,
    sched_prio: i32,
    umask: u32,
    tcpgrp_fd: i32,
    path: Vec<u8>,
    cwd: Vec<u8>,
    argv: Vec<Vec<u8>>,
    envp: Vec<Vec<u8>>,
    fdmap: Vec<FdMapEntry>,
});
request_struct!(WaitRequest {
    idtype: u32,
    id: i64,
    options: u32,
});
request_struct!(KillRequest {
    pid: i64,
    sig: u32,
    flags: u32,
    value: Option<u64>,
});
request_struct!(SetPgidRequest {
    pid: i32,
    pgid: i32,
});
request_struct!(GetPgidRequest { pid: i32 });
request_struct!(GetSidRequest { pid: i32 });

/// A decoded response body.
#[allow(clippy::large_enum_variant)] // fixed 276-byte wait body; boxing would allocate per wait
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Body {
    /// No body; use the header value.
    Empty,
    /// A wait result.
    Wait {
        /// POSIX wait status.
        status: i32,
        /// Opaque `siginfo_t`.
        siginfo: [u8; 128],
        /// Opaque `rusage`.
        rusage: [u8; 144],
    },
    /// Supplementary groups.
    Groups(Vec<u32>),
    /// A hello result.
    Hello {
        /// Server protocol version.
        server_version: u32,
        /// Server pid.
        pid: i32,
    },
    /// A login-name result.
    Login(Vec<u8>),
}

/// One decoded response.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Response {
    /// Zero or `-errno`.
    pub status: i32,
    /// Primary scalar result.
    pub value: i64,
    /// Operation-specific body.
    pub body: Body,
}

impl Response {
    /// A successful response with a scalar value.
    #[must_use]
    pub const fn ok(value: i64) -> Self {
        Self {
            status: 0,
            value,
            body: Body::Empty,
        }
    }

    /// A successful response with a body.
    #[must_use]
    pub const fn body(value: i64, body: Body) -> Self {
        Self {
            status: 0,
            value,
            body,
        }
    }

    /// An error response.
    #[must_use]
    pub const fn error(error: Errno) -> Self {
        Self {
            status: -(error.0 as i32),
            value: 0,
            body: Body::Empty,
        }
    }
}

impl ResponseHeader {
    /// Decode a bare 16-byte response header.
    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() != 16 {
            return Err(Errno::EINVAL);
        }
        let status = read_i32(bytes, 0)?;
        let reserved = read_u32(bytes, 4)?;
        let value = read_i64(bytes, 8)?;
        if !(-4095..=0).contains(&status) {
            return Err(Errno::EPROTO);
        }
        Ok(Self {
            status,
            reserved,
            value,
        })
    }
}

/// Measure one encoded request.
pub fn measure_request(request: &Request) -> Result<usize, Error> {
    let mut writer = Writer::new(None);
    write_request(request, 0, &mut writer)?;
    Ok(writer.position())
}

/// Encode one request into `out`.
pub fn encode_request(request: &Request, seq: u64, out: &mut [u8]) -> Result<usize, Error> {
    let length = measure_request(request)?;
    if length > out.len() {
        return Err(Errno::E2BIG);
    }
    let mut writer = Writer::new(Some(out));
    write_request(request, seq, &mut writer)?;
    Ok(writer.position())
}

fn write_request(request: &Request, seq: u64, writer: &mut Writer<'_>) -> Result<(), Error> {
    writer.u16(VERSION)?;
    writer.u16(request.op() as u16)?;
    writer.u32(0)?;
    writer.u64(seq)?;
    match request {
        Request::Hello(value) => writer.u32(value.client_version),
        Request::Spawn(value) => {
            writer.u32(value.spawn_flags)?;
            writer.i32(value.pgroup)?;
            writer.bytes(&value.sigdefault)?;
            writer.bytes(&value.sigmask)?;
            writer.bytes(&value.ignored)?;
            writer.i32(value.sched_policy)?;
            writer.i32(value.sched_prio)?;
            writer.u32(value.umask)?;
            writer.i32(value.tcpgrp_fd)?;
            writer.string(&value.path)?;
            writer.string(&value.cwd)?;
            writer.argv(&value.argv)?;
            writer.argv(&value.envp)?;
            writer.fdmap(&value.fdmap)
        }
        Request::Wait(value) => {
            writer.u32(value.idtype)?;
            writer.i64(value.id)?;
            writer.u32(value.options)
        }
        Request::Kill(value) => {
            writer.i64(value.pid)?;
            writer.u32(value.sig)?;
            writer.u32(value.flags)?;
            match value.value {
                Some(extra) => writer.u64(extra),
                None => Ok(()),
            }
        }
        Request::SetPgid(value) => {
            writer.i32(value.pid)?;
            writer.i32(value.pgid)
        }
        Request::GetPgid(value) => writer.i32(value.pid),
        Request::GetSid(value) => writer.i32(value.pid),
        Request::Other(_) => Err(Errno::ENOSYS),
    }
}

/// Decode one request.
pub fn decode_request(input: &[u8]) -> Result<(RequestHeader, Request), Error> {
    if input.len() > MAX_MESSAGE {
        return Err(Errno::E2BIG);
    }
    let mut reader = Reader::new(input);
    let version = reader.u16()?;
    let op_number = reader.u16()?;
    let flags = reader.u32()?;
    let seq = reader.u64()?;
    if version != VERSION {
        return Err(Errno::EPROTO);
    }
    let op = Op::from_u16(op_number).ok_or(Errno::EINVAL)?;
    let request = match op {
        Op::Hello => Request::Hello(HelloRequest {
            client_version: reader.u32()?,
        }),
        Op::Spawn => Request::Spawn(SpawnRequest {
            spawn_flags: reader.u32()?,
            pgroup: reader.i32()?,
            sigdefault: reader.bytes_array()?,
            sigmask: reader.bytes_array()?,
            ignored: reader.bytes_array()?,
            sched_policy: reader.i32()?,
            sched_prio: reader.i32()?,
            umask: reader.u32()?,
            tcpgrp_fd: reader.i32()?,
            path: reader.string()?,
            cwd: reader.string()?,
            argv: reader.argv()?,
            envp: reader.argv()?,
            fdmap: reader.fdmap()?,
        }),
        Op::Wait => Request::Wait(WaitRequest {
            idtype: reader.u32()?,
            id: reader.i64()?,
            options: reader.u32()?,
        }),
        Op::Kill => {
            let pid = reader.i64()?;
            let sig = reader.u32()?;
            let flags = reader.u32()?;
            let value = if flags & 1 != 0 {
                Some(reader.u64()?)
            } else {
                None
            };
            Request::Kill(KillRequest {
                pid,
                sig,
                flags,
                value,
            })
        }
        Op::SetPgid => Request::SetPgid(SetPgidRequest {
            pid: reader.i32()?,
            pgid: reader.i32()?,
        }),
        Op::GetPgid => Request::GetPgid(GetPgidRequest { pid: reader.i32()? }),
        Op::GetSid => Request::GetSid(GetSidRequest { pid: reader.i32()? }),
        other => {
            return Ok((
                RequestHeader {
                    version,
                    op,
                    flags,
                    seq,
                },
                Request::Other(other),
            ));
        }
    };
    reader.finish()?;
    Ok((
        RequestHeader {
            version,
            op,
            flags,
            seq,
        },
        request,
    ))
}

/// Measure one encoded response.
pub fn measure_response(response: &Response) -> Result<usize, Error> {
    let mut writer = Writer::new(None);
    write_response(response, &mut writer)?;
    Ok(writer.position())
}

/// Encode one response into `out`.
pub fn encode_response(response: &Response, out: &mut [u8]) -> Result<usize, Error> {
    let length = measure_response(response)?;
    if length > out.len() {
        return Err(Errno::E2BIG);
    }
    let mut writer = Writer::new(Some(out));
    write_response(response, &mut writer)?;
    Ok(writer.position())
}

fn write_response(response: &Response, writer: &mut Writer<'_>) -> Result<(), Error> {
    writer.i32(response.status)?;
    writer.u32(0)?;
    writer.i64(response.value)?;
    if response.status != 0 {
        return Ok(());
    }
    match &response.body {
        Body::Empty => Ok(()),
        Body::Wait {
            status,
            siginfo,
            rusage,
        } => {
            writer.i32(*status)?;
            writer.bytes(siginfo)?;
            writer.bytes(rusage)
        }
        Body::Groups(groups) => {
            writer.u32(u32::try_from(groups.len()).map_err(|_| Errno::E2BIG)?)?;
            for group in groups {
                writer.u32(*group)?;
            }
            Ok(())
        }
        Body::Hello {
            server_version,
            pid,
        } => {
            writer.u32(*server_version)?;
            writer.i32(*pid)
        }
        Body::Login(name) => writer.string(name),
    }
}

/// Decode one response for `op`.
pub fn decode_response(op: Op, input: &[u8]) -> Result<Response, Error> {
    if input.len() > MAX_MESSAGE || input.len() < 16 {
        return Err(Errno::EINVAL);
    }
    let header = ResponseHeader::decode(input.get(..16).ok_or(Errno::EINVAL)?)?;
    if header.status != 0 {
        if input.len() != 16 {
            return Err(Errno::EINVAL);
        }
        return Ok(Response {
            status: header.status,
            value: header.value,
            body: Body::Empty,
        });
    }
    let mut reader = Reader::new(input.get(16..).ok_or(Errno::EINVAL)?);
    let body = match op {
        Op::Wait => {
            let status = reader.i32()?;
            let mut siginfo = [0u8; 128];
            siginfo.copy_from_slice(reader.bytes(128)?);
            let mut rusage = [0u8; 144];
            rusage.copy_from_slice(reader.bytes(144)?);
            Body::Wait {
                status,
                siginfo,
                rusage,
            }
        }
        Op::GetGroups => {
            let count = reader.u32()? as usize;
            if count > MAX_FDS {
                return Err(Errno::E2BIG);
            }
            let mut groups = Vec::with_capacity(count);
            for _ in 0..count {
                groups.push(reader.u32()?);
            }
            Body::Groups(groups)
        }
        Op::Hello => Body::Hello {
            server_version: reader.u32()?,
            pid: reader.i32()?,
        },
        Op::Login => Body::Login(reader.string()?),
        _ => Body::Empty,
    };
    reader.finish()?;
    Ok(Response {
        status: header.status,
        value: header.value,
        body,
    })
}

struct Writer<'a> {
    out: Option<&'a mut [u8]>,
    pos: usize,
}

impl<'a> Writer<'a> {
    fn new(out: Option<&'a mut [u8]>) -> Self {
        Self { out, pos: 0 }
    }

    fn position(&self) -> usize {
        self.pos
    }

    fn write(&mut self, bytes: &[u8]) -> Result<(), Error> {
        let end = self.pos.checked_add(bytes.len()).ok_or(Errno::E2BIG)?;
        if end > MAX_MESSAGE {
            return Err(Errno::E2BIG);
        }
        if let Some(out) = self.out.as_deref_mut() {
            out.get_mut(self.pos..end)
                .ok_or(Errno::E2BIG)?
                .copy_from_slice(bytes);
        }
        self.pos = end;
        Ok(())
    }

    fn align(&mut self, alignment: usize) -> Result<(), Error> {
        let remainder = self.pos.checked_rem(alignment).ok_or(Errno::E2BIG)?;
        if remainder != 0 {
            let padding = alignment.checked_sub(remainder).ok_or(Errno::E2BIG)?;
            let zeros = [0u8; 8];
            self.write(zeros.get(..padding).ok_or(Errno::E2BIG)?)?;
        }
        Ok(())
    }

    fn u16(&mut self, value: u16) -> Result<(), Error> {
        self.write(&value.to_le_bytes())
    }

    fn u32(&mut self, value: u32) -> Result<(), Error> {
        self.align(4)?;
        self.write(&value.to_le_bytes())
    }

    fn i32(&mut self, value: i32) -> Result<(), Error> {
        self.u32(value as u32)
    }

    fn u64(&mut self, value: u64) -> Result<(), Error> {
        self.align(8)?;
        self.write(&value.to_le_bytes())
    }

    fn i64(&mut self, value: i64) -> Result<(), Error> {
        self.u64(value as u64)
    }

    fn bytes(&mut self, value: &[u8]) -> Result<(), Error> {
        self.write(value)
    }

    fn string(&mut self, value: &[u8]) -> Result<(), Error> {
        if value.len() > MAX_STRING || value.contains(&0) {
            return Err(Errno::ENAMETOOLONG);
        }
        self.u32(u32::try_from(value.len()).map_err(|_| Errno::ENAMETOOLONG)?)?;
        self.write(value)?;
        self.align(4)
    }

    fn argv(&mut self, values: &[Vec<u8>]) -> Result<(), Error> {
        if values.len() > MAX_ARGS {
            return Err(Errno::E2BIG);
        }
        let mut length = 0usize;
        for value in values {
            if value.contains(&0) {
                return Err(Errno::EINVAL);
            }
            length = length
                .checked_add(value.len())
                .and_then(|total| total.checked_add(1))
                .ok_or(Errno::E2BIG)?;
        }
        if length > ARG_MAX {
            return Err(Errno::E2BIG);
        }
        self.u32(u32::try_from(values.len()).map_err(|_| Errno::E2BIG)?)?;
        self.u32(u32::try_from(length).map_err(|_| Errno::E2BIG)?)?;
        for value in values {
            self.write(value)?;
            self.write(&[0])?;
        }
        self.align(4)
    }

    fn fdmap(&mut self, values: &[FdMapEntry]) -> Result<(), Error> {
        if values.len() > MAX_FDS {
            return Err(Errno::E2BIG);
        }
        self.u32(u32::try_from(values.len()).map_err(|_| Errno::E2BIG)?)?;
        for entry in values {
            self.i32(entry.fd)?;
            self.u32(entry.handle)?;
        }
        Ok(())
    }
}

struct Reader<'a> {
    input: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(input: &'a [u8]) -> Self {
        Self { input, pos: 0 }
    }

    fn remaining(&self) -> usize {
        self.input.len().saturating_sub(self.pos)
    }

    fn align(&mut self, alignment: usize) -> Result<(), Error> {
        let remainder = self.pos.checked_rem(alignment).ok_or(Errno::EINVAL)?;
        if remainder != 0 {
            let padding = alignment.checked_sub(remainder).ok_or(Errno::EINVAL)?;
            let end = self.pos.checked_add(padding).ok_or(Errno::EINVAL)?;
            let slice = self.input.get(self.pos..end).ok_or(Errno::EINVAL)?;
            if slice.iter().any(|byte| *byte != 0) {
                return Err(Errno::EINVAL);
            }
            self.pos = end;
        }
        Ok(())
    }

    fn bytes(&mut self, count: usize) -> Result<&'a [u8], Error> {
        let end = self.pos.checked_add(count).ok_or(Errno::EINVAL)?;
        let slice = self.input.get(self.pos..end).ok_or(Errno::EINVAL)?;
        self.pos = end;
        Ok(slice)
    }

    fn bytes_array(&mut self) -> Result<[u8; 128], Error> {
        let mut array = [0u8; 128];
        array.copy_from_slice(self.bytes(128)?);
        Ok(array)
    }

    fn u16(&mut self) -> Result<u16, Error> {
        let slice = self.bytes(2)?;
        let array: [u8; 2] = slice.try_into().map_err(|_| Errno::EINVAL)?;
        Ok(u16::from_le_bytes(array))
    }

    fn u32(&mut self) -> Result<u32, Error> {
        self.align(4)?;
        let slice = self.bytes(4)?;
        let array: [u8; 4] = slice.try_into().map_err(|_| Errno::EINVAL)?;
        Ok(u32::from_le_bytes(array))
    }

    fn i32(&mut self) -> Result<i32, Error> {
        Ok(self.u32()? as i32)
    }

    fn u64(&mut self) -> Result<u64, Error> {
        self.align(8)?;
        let slice = self.bytes(8)?;
        let array: [u8; 8] = slice.try_into().map_err(|_| Errno::EINVAL)?;
        Ok(u64::from_le_bytes(array))
    }

    fn i64(&mut self) -> Result<i64, Error> {
        Ok(self.u64()? as i64)
    }

    fn string(&mut self) -> Result<Vec<u8>, Error> {
        let length = self.u32()? as usize;
        if length > MAX_STRING {
            return Err(Errno::ENAMETOOLONG);
        }
        let value = self.bytes(length)?.to_vec();
        if value.contains(&0) {
            return Err(Errno::EINVAL);
        }
        self.align(4)?;
        Ok(value)
    }

    fn argv(&mut self) -> Result<Vec<Vec<u8>>, Error> {
        let count = self.u32()? as usize;
        let length = self.u32()? as usize;
        if count > MAX_ARGS || length > ARG_MAX {
            return Err(Errno::E2BIG);
        }
        let blob = self.bytes(length)?;
        self.align(4)?;
        let mut values = Vec::with_capacity(count);
        let mut start = 0usize;
        for _ in 0..count {
            let end = blob
                .get(start..)
                .and_then(|rest| rest.iter().position(|byte| *byte == 0))
                .and_then(|offset| start.checked_add(offset))
                .ok_or(Errno::EINVAL)?;
            let value = blob.get(start..end).ok_or(Errno::EINVAL)?.to_vec();
            values.push(value);
            start = end.checked_add(1).ok_or(Errno::EINVAL)?;
        }
        if start != blob.len() {
            return Err(Errno::EINVAL);
        }
        Ok(values)
    }

    fn fdmap(&mut self) -> Result<Vec<FdMapEntry>, Error> {
        let count = self.u32()? as usize;
        if count > MAX_FDS {
            return Err(Errno::E2BIG);
        }
        let mut entries = Vec::with_capacity(count);
        for _ in 0..count {
            entries.push(FdMapEntry {
                fd: self.i32()?,
                handle: self.u32()?,
            });
        }
        Ok(entries)
    }

    fn finish(&self) -> Result<(), Error> {
        if self.remaining() == 0 {
            Ok(())
        } else {
            Err(Errno::EINVAL)
        }
    }
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, Error> {
    let slice = bytes
        .get(offset..offset.checked_add(4).ok_or(Errno::EINVAL)?)
        .ok_or(Errno::EINVAL)?;
    let array: [u8; 4] = slice.try_into().map_err(|_| Errno::EINVAL)?;
    Ok(u32::from_le_bytes(array))
}

fn read_i32(bytes: &[u8], offset: usize) -> Result<i32, Error> {
    Ok(read_u32(bytes, offset)? as i32)
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, Error> {
    let slice = bytes
        .get(offset..offset.checked_add(8).ok_or(Errno::EINVAL)?)
        .ok_or(Errno::EINVAL)?;
    let array: [u8; 8] = slice.try_into().map_err(|_| Errno::EINVAL)?;
    Ok(u64::from_le_bytes(array))
}

fn read_i64(bytes: &[u8], offset: usize) -> Result<i64, Error> {
    Ok(read_u64(bytes, offset)? as i64)
}

#[cfg(test)]
#[path = "wire_ut.rs"]
mod tests;
