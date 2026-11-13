// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The proposed `procd` wire protocol.
//!
//! All integers are little-endian. A four-byte field is written at a multiple of four and an
//! eight-byte field at a multiple of eight, with zero padding inserted; byte arrays have
//! alignment one. Strings are a `u32` length, the bytes and zero padding to four bytes, with no
//! NUL. The packing rule is fixed here because the area document leaves it to the implementer.

use alloc::vec::Vec;

use crate::errors::{E2BIG, EINVAL, ENAMETOOLONG, EPROTO, Errno};
use crate::limits::{ARG_MAX, MAX_ARGS};

/// Wire protocol version.
pub const VERSION: u16 = 1;
/// Largest accepted message.
pub const MAX_MESSAGE: usize = 4 * 1024 * 1024;
/// Largest accepted string.
pub const MAX_STRING: usize = 4096;

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

/// One fd-to-handle pair in a spawn or exec map.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FdMapEntry {
    /// Destination file descriptor.
    pub fd: i32,
    /// Sender's handle value.
    pub handle: u32,
}

/// The request header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RequestHeader {
    /// Protocol version.
    pub version: u16,
    /// Operation.
    pub op: u16,
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

impl ResponseHeader {
    /// Decode a bare 16-byte response header.
    pub fn decode(bytes: &[u8]) -> Result<Self, Errno> {
        if bytes.len() != 16 {
            return Err(EINVAL);
        }
        let status = read_i32(bytes, 0)?;
        let reserved = read_u32(bytes, 4)?;
        let value = read_i64(bytes, 8)?;
        if !(-4095..=0).contains(&status) {
            return Err(EPROTO);
        }
        Ok(Self {
            status,
            reserved,
            value,
        })
    }
}

/// A decoded response body.
#[allow(clippy::large_enum_variant)] // the fixed 276-byte wait body is read once; boxing it would allocate
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Body {
    /// No body; use the header value.
    Empty,
    /// A wait result.
    Wait {
        /// Raw wait status.
        status: i32,
        /// Opaque `siginfo_t`.
        siginfo: [u8; 128],
        /// Opaque `rusage`.
        rusage: [u8; 144],
    },
    /// Supplementary groups.
    Groups(Vec<u32>),
    /// The old rlimit pair.
    Rlimit {
        /// Old soft limit.
        cur: u64,
        /// Old hard limit.
        max: u64,
    },
    /// A scheduler result.
    Sched {
        /// Primary value.
        value: i32,
        /// Four scheduler parameters.
        param: [i32; 4],
    },
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

/// A protocol request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    /// Version handshake.
    Hello(HelloRequest),
    /// Spawn a new image.
    Spawn(SpawnRequest),
    /// Fork the caller.
    Fork(ForkRequest),
    /// Replace the caller's image.
    Exec(ExecRequest),
    /// Wait for a child.
    Wait(WaitRequest),
    /// Send a signal.
    Kill(KillRequest),
    /// Set a process group.
    SetPgid(SetPgidRequest),
    /// Create a session.
    SetSid,
    /// Read a process group.
    GetPgid(GetPgidRequest),
    /// Read a session id.
    GetSid(GetSidRequest),
    /// Change credentials.
    SetCred(SetCredRequest),
    /// Read supplementary groups.
    GetGroups(GetGroupsRequest),
    /// Read or write a resource limit.
    Rlimit(RlimitRequest),
    /// Read resource usage.
    Rusage(RusageRequest),
    /// Read or write a priority.
    Prio(PrioRequest),
    /// Scheduler operation.
    Sched(SchedRequest),
    /// Set the process title.
    SetTitle(SetTitleRequest),
    /// Native `prctl` subset.
    Prctl(PrctlRequest),
    /// Login-name operation.
    Login(LoginRequest),
    /// Apply a pledge.
    Pledge(PledgeRequest),
    /// Enter capability mode.
    CapEnter,
    /// Query session information.
    SessionInfo(SessionInfoRequest),
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
request_struct!(ForkRequest {
    entry: u64,
    sp: u64,
    arg: u64,
    fork_flags: u32,
});
request_struct!(ExecRequest {
    exec_flags: u32,
    fd_handle: u32,
    path: Vec<u8>,
    cwd: Vec<u8>,
    argv: Vec<Vec<u8>>,
    envp: Vec<Vec<u8>>,
    close_list: Vec<u32>,
    fdmap: Vec<FdMapEntry>,
    sigmask: [u8; 128],
    ignored: [u8; 128],
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
request_struct!(SetCredRequest {
    kind: u32,
    a: i32,
    b: i32,
    c: i32,
    groups: Vec<u32>,
});
request_struct!(GetGroupsRequest { max: u32 });
request_struct!(RlimitRequest {
    pid: i32,
    resource: u32,
    has_new: u32,
    new_cur: u64,
    new_max: u64,
});
request_struct!(RusageRequest { who: i32 });
request_struct!(PrioRequest {
    which: u32,
    who: i32,
    op: u32,
    value: i32,
});
request_struct!(SchedRequest {
    op: u32,
    pid: i32,
    policy: i32,
    param: [i32; 4],
});
request_struct!(SetTitleRequest {
    title: Vec<u8>,
});
request_struct!(PrctlRequest {
    option: u32,
    a: [u64; 4],
});
request_struct!(LoginRequest {
    op: u32,
    name: Vec<u8>,
});
request_struct!(PledgeRequest {
    promises: Vec<u8>,
    execpromises: Vec<u8>,
    flags: u32,
});
request_struct!(SessionInfoRequest { pid: i32 });

impl Request {
    /// The operation code.
    #[must_use]
    pub const fn op(&self) -> Op {
        match self {
            Self::Hello(_) => Op::Hello,
            Self::Spawn(_) => Op::Spawn,
            Self::Fork(_) => Op::Fork,
            Self::Exec(_) => Op::Exec,
            Self::Wait(_) => Op::Wait,
            Self::Kill(_) => Op::Kill,
            Self::SetPgid(_) => Op::SetPgid,
            Self::SetSid => Op::SetSid,
            Self::GetPgid(_) => Op::GetPgid,
            Self::GetSid(_) => Op::GetSid,
            Self::SetCred(_) => Op::SetCred,
            Self::GetGroups(_) => Op::GetGroups,
            Self::Rlimit(_) => Op::Rlimit,
            Self::Rusage(_) => Op::Rusage,
            Self::Prio(_) => Op::Prio,
            Self::Sched(_) => Op::Sched,
            Self::SetTitle(_) => Op::SetTitle,
            Self::Prctl(_) => Op::Prctl,
            Self::Login(_) => Op::Login,
            Self::Pledge(_) => Op::Pledge,
            Self::CapEnter => Op::CapEnter,
            Self::SessionInfo(_) => Op::SessionInfo,
        }
    }
}

/// Measure the encoded request length.
pub fn measure(request: &Request) -> Result<usize, Errno> {
    let mut writer = Writer::new(None);
    write_request(request, 0, &mut writer)?;
    Ok(writer.position())
}

/// Encode one request into `out`.
pub fn encode(request: &Request, seq: u64, out: &mut [u8]) -> Result<usize, Errno> {
    let length = measure(request)?;
    if length > out.len() {
        return Err(E2BIG);
    }
    let mut writer = Writer::new(Some(out));
    write_request(request, seq, &mut writer)?;
    Ok(writer.position())
}

fn write_request(request: &Request, seq: u64, writer: &mut Writer<'_>) -> Result<(), Errno> {
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
        Request::Fork(value) => {
            writer.u64(value.entry)?;
            writer.u64(value.sp)?;
            writer.u64(value.arg)?;
            writer.u32(value.fork_flags)
        }
        Request::Exec(value) => {
            writer.u32(value.exec_flags)?;
            writer.u32(value.fd_handle)?;
            writer.string(&value.path)?;
            writer.string(&value.cwd)?;
            writer.argv(&value.argv)?;
            writer.argv(&value.envp)?;
            writer.u32(u32::try_from(value.close_list.len()).map_err(|_| E2BIG)?)?;
            for handle in &value.close_list {
                writer.u32(*handle)?;
            }
            writer.fdmap(&value.fdmap)?;
            writer.bytes(&value.sigmask)?;
            writer.bytes(&value.ignored)
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
        Request::SetSid => Ok(()),
        Request::GetPgid(value) => writer.i32(value.pid),
        Request::GetSid(value) => writer.i32(value.pid),
        Request::SetCred(value) => {
            writer.u32(value.kind)?;
            writer.i32(value.a)?;
            writer.i32(value.b)?;
            writer.i32(value.c)?;
            writer.u32(u32::try_from(value.groups.len()).map_err(|_| E2BIG)?)?;
            for group in &value.groups {
                writer.u32(*group)?;
            }
            Ok(())
        }
        Request::GetGroups(value) => writer.u32(value.max),
        Request::Rlimit(value) => {
            writer.i32(value.pid)?;
            writer.u32(value.resource)?;
            writer.u32(value.has_new)?;
            writer.u64(value.new_cur)?;
            writer.u64(value.new_max)
        }
        Request::Rusage(value) => writer.i32(value.who),
        Request::Prio(value) => {
            writer.u32(value.which)?;
            writer.i32(value.who)?;
            writer.u32(value.op)?;
            writer.i32(value.value)
        }
        Request::Sched(value) => {
            writer.u32(value.op)?;
            writer.i32(value.pid)?;
            writer.i32(value.policy)?;
            for parameter in &value.param {
                writer.i32(*parameter)?;
            }
            Ok(())
        }
        Request::SetTitle(value) => writer.string(&value.title),
        Request::Prctl(value) => {
            writer.u32(value.option)?;
            for word in &value.a {
                writer.u64(*word)?;
            }
            Ok(())
        }
        Request::Login(value) => {
            writer.u32(value.op)?;
            writer.string(&value.name)
        }
        Request::Pledge(value) => {
            writer.string(&value.promises)?;
            writer.string(&value.execpromises)?;
            writer.u32(value.flags)
        }
        Request::CapEnter => Ok(()),
        Request::SessionInfo(value) => writer.i32(value.pid),
    }
}

/// Decode one response for `op`.
pub fn decode_response(op: Op, bytes: &[u8]) -> Result<(ResponseHeader, Body), Errno> {
    let header = ResponseHeader::decode(bytes.get(..16).ok_or(EINVAL)?)?;
    if header.status != 0 {
        return Err(header.status.checked_neg().ok_or(EPROTO)?);
    }
    let body = bytes.get(16..).ok_or(EINVAL)?;
    let body = match op {
        Op::Wait => {
            if body.len() != 4 + 128 + 144 {
                return Err(EINVAL);
            }
            let status = read_i32(body, 0)?;
            let mut siginfo = [0u8; 128];
            siginfo.copy_from_slice(body.get(4..132).ok_or(EINVAL)?);
            let mut rusage = [0u8; 144];
            rusage.copy_from_slice(body.get(132..276).ok_or(EINVAL)?);
            Body::Wait {
                status,
                siginfo,
                rusage,
            }
        }
        Op::GetGroups => {
            let count = read_u32(body, 0)? as usize;
            let needed = 4usize
                .checked_add(count.checked_mul(4).ok_or(EINVAL)?)
                .ok_or(EINVAL)?;
            if body.len() != needed {
                return Err(EINVAL);
            }
            let mut groups = Vec::with_capacity(count);
            for index in 0..count {
                let offset = index
                    .checked_mul(4)
                    .and_then(|value| value.checked_add(4))
                    .ok_or(EINVAL)?;
                groups.push(read_u32(body, offset)?);
            }
            Body::Groups(groups)
        }
        Op::Rlimit => {
            if body.len() != 16 {
                return Err(EINVAL);
            }
            Body::Rlimit {
                cur: read_u64(body, 0)?,
                max: read_u64(body, 8)?,
            }
        }
        Op::Sched => {
            if body.len() != 20 {
                return Err(EINVAL);
            }
            let mut param = [0i32; 4];
            for (index, slot) in param.iter_mut().enumerate() {
                let offset = index
                    .checked_mul(4)
                    .and_then(|value| value.checked_add(4))
                    .ok_or(EINVAL)?;
                *slot = read_i32(body, offset)?;
            }
            Body::Sched {
                value: read_i32(body, 0)?,
                param,
            }
        }
        Op::Hello => {
            if body.len() != 8 {
                return Err(EINVAL);
            }
            Body::Hello {
                server_version: read_u32(body, 0)?,
                pid: read_i32(body, 4)?,
            }
        }
        Op::Login => {
            let length = read_u32(body, 0)? as usize;
            if length > MAX_STRING {
                return Err(ENAMETOOLONG);
            }
            let padded = length.checked_add(3).ok_or(EINVAL)? & !3usize;
            let expected = 4usize.checked_add(padded).ok_or(EINVAL)?;
            if body.len() != expected {
                return Err(EINVAL);
            }
            let end = 4usize.checked_add(length).ok_or(EINVAL)?;
            Body::Login(body.get(4..end).ok_or(EINVAL)?.to_vec())
        }
        _ => {
            if !body.is_empty() {
                return Err(EINVAL);
            }
            Body::Empty
        }
    };
    Ok((header, body))
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

    fn write(&mut self, bytes: &[u8]) -> Result<(), Errno> {
        let end = self.pos.checked_add(bytes.len()).ok_or(E2BIG)?;
        if end > MAX_MESSAGE {
            return Err(E2BIG);
        }
        if let Some(out) = self.out.as_deref_mut() {
            out.get_mut(self.pos..end)
                .ok_or(E2BIG)?
                .copy_from_slice(bytes);
        }
        self.pos = end;
        Ok(())
    }

    fn align(&mut self, alignment: usize) -> Result<(), Errno> {
        let remainder = self.pos.checked_rem(alignment).ok_or(E2BIG)?;
        if remainder != 0 {
            let padding = alignment.checked_sub(remainder).ok_or(E2BIG)?;
            let zeros = [0u8; 8];
            self.write(zeros.get(..padding).ok_or(E2BIG)?)?;
        }
        Ok(())
    }

    fn u16(&mut self, value: u16) -> Result<(), Errno> {
        self.write(&value.to_le_bytes())
    }

    fn u32(&mut self, value: u32) -> Result<(), Errno> {
        self.align(4)?;
        self.write(&value.to_le_bytes())
    }

    fn i32(&mut self, value: i32) -> Result<(), Errno> {
        self.u32(value as u32)
    }

    fn u64(&mut self, value: u64) -> Result<(), Errno> {
        self.align(8)?;
        self.write(&value.to_le_bytes())
    }

    fn i64(&mut self, value: i64) -> Result<(), Errno> {
        self.u64(value as u64)
    }

    fn bytes(&mut self, value: &[u8]) -> Result<(), Errno> {
        self.write(value)
    }

    fn string(&mut self, value: &[u8]) -> Result<(), Errno> {
        if value.len() > MAX_STRING {
            return Err(ENAMETOOLONG);
        }
        self.u32(u32::try_from(value.len()).map_err(|_| ENAMETOOLONG)?)?;
        self.write(value)?;
        self.align(4)
    }

    fn argv(&mut self, values: &[Vec<u8>]) -> Result<(), Errno> {
        if values.len() > MAX_ARGS {
            return Err(E2BIG);
        }
        self.u32(u32::try_from(values.len()).map_err(|_| E2BIG)?)?;
        let mut length = 0usize;
        for value in values {
            length = length
                .checked_add(value.len())
                .and_then(|total| total.checked_add(1))
                .ok_or(E2BIG)?;
        }
        if length > ARG_MAX {
            return Err(E2BIG);
        }
        self.u32(u32::try_from(length).map_err(|_| E2BIG)?)?;
        for value in values {
            self.write(value)?;
            self.write(&[0])?;
        }
        self.align(4)
    }

    fn fdmap(&mut self, values: &[FdMapEntry]) -> Result<(), Errno> {
        self.u32(u32::try_from(values.len()).map_err(|_| E2BIG)?)?;
        for entry in values {
            self.i32(entry.fd)?;
            self.u32(entry.handle)?;
        }
        Ok(())
    }
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, Errno> {
    let slice = bytes
        .get(offset..offset.checked_add(4).ok_or(EINVAL)?)
        .ok_or(EINVAL)?;
    let array: [u8; 4] = slice.try_into().map_err(|_| EINVAL)?;
    Ok(u32::from_le_bytes(array))
}

fn read_i32(bytes: &[u8], offset: usize) -> Result<i32, Errno> {
    let slice = bytes
        .get(offset..offset.checked_add(4).ok_or(EINVAL)?)
        .ok_or(EINVAL)?;
    let array: [u8; 4] = slice.try_into().map_err(|_| EINVAL)?;
    Ok(i32::from_le_bytes(array))
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, Errno> {
    let slice = bytes
        .get(offset..offset.checked_add(8).ok_or(EINVAL)?)
        .ok_or(EINVAL)?;
    let array: [u8; 8] = slice.try_into().map_err(|_| EINVAL)?;
    Ok(u64::from_le_bytes(array))
}

fn read_i64(bytes: &[u8], offset: usize) -> Result<i64, Errno> {
    let slice = bytes
        .get(offset..offset.checked_add(8).ok_or(EINVAL)?)
        .ok_or(EINVAL)?;
    let array: [u8; 8] = slice.try_into().map_err(|_| EINVAL)?;
    Ok(i64::from_le_bytes(array))
}

#[cfg(test)]
#[path = "wire_ut.rs"]
mod tests;
