// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The `SOL_MPTCP` userland control encoding.
//!
//! The guest socket is a thin `net:` client: the kernel MPTCP state lives in `netd`, so the
//! Linux-shaped `setsockopt`/`getsockopt` requests are encoded here and sent over the scheme
//! `CALL` path. The wire stays inside the existing request header (`u16` version, `u16`
//! `KIND_CALL`); each MPTCP operation uses its own `OP_MPTCP_*` opcode, followed by its payload.
//! The server side is MPTCP2c2 (`netd`/`raven-netcore`).
//!
//! Reply tags are additive: success is `TAG_OK`, a structured record uses
//! [`TAG_MPTCP_INFO`]/[`TAG_MPTCP_SUBFLOWS`] with a `u16` length and the raw C-layout bytes,
//! and an error uses the existing `TAG_ERR` + `u16` errno.

use raven_libc_abi::errno::Errno;

use crate::ErrnoExt;

/// Linux `SOL_MPTCP`.
pub const SOL_MPTCP: i32 = 284;
/// Linux `IPPROTO_MPTCP`.
pub const IPPROTO_MPTCP: i32 = 262;
/// `getsockopt(SOL_MPTCP, MPTCP_INFO)`.
pub const MPTCP_INFO: i32 = 1;
/// `getsockopt(SOL_MPTCP, MPTCP_SUBFLOW_ADDRS)`.
pub const MPTCP_SUBFLOW_ADDRS: i32 = 2;
/// `setsockopt(SOL_MPTCP, MPTCP_PM_CMD_ADD_ADDR)`.
pub const MPTCP_PM_CMD_ADD_ADDR: i32 = 1;
/// `setsockopt(SOL_MPTCP, MPTCP_PM_CMD_DEL_ADDR)`.
pub const MPTCP_PM_CMD_DEL_ADDR: i32 = 2;
/// `setsockopt(SOL_MPTCP, MPTCP_PM_CMD_SET_FLAGS)`.
pub const MPTCP_PM_CMD_SET_FLAGS: i32 = 7;
/// Backup flag in [`PmAddr::flags`].
pub const MPTCP_PM_ADDR_FLAG_BACKUP: u16 = 1;

/// MPTCP request opcode for `ADD_ADDR` inside a `KIND_CALL` frame.
pub const OP_MPTCP_ADD_ADDR: u16 = 12;
/// MPTCP request opcode for `DEL_ADDR`.
pub const OP_MPTCP_DEL_ADDR: u16 = 13;
/// MPTCP request opcode for `SET_FLAGS` (backup/unbackup).
pub const OP_MPTCP_SET_FLAGS: u16 = 14;
/// MPTCP request opcode for `MPTCP_INFO`.
pub const OP_MPTCP_INFO: u16 = 15;
/// MPTCP request opcode for `MPTCP_SUBFLOW_ADDRS`.
pub const OP_MPTCP_SUBFLOWS: u16 = 16;
/// Header version used by `raven-proto-net`.
const PROTOCOL_VERSION: u16 = 1;
/// Request kind `CALL`.
const KIND_CALL: u16 = 4;
/// Response tag: success.
const TAG_OK: u16 = 0;
/// Response tag: error number.
const TAG_ERR: u16 = 6;
/// Response tag: `MPTCP_INFO` record.
const TAG_MPTCP_INFO: u16 = 8;
/// Response tag: subflow-address table.
const TAG_MPTCP_SUBFLOWS: u16 = 9;
/// C-layout `struct mptcp_pm_addr` used by the guest probe.
pub const PM_ADDR_LEN: usize = 12;
/// C-layout `struct mptcp_info` used by the guest probe.
pub const INFO_LEN: usize = 12;
/// One subflow record: id, backup flag, local/remote IPv4 and ports.
pub const SUBFLOW_LEN: usize = 16;

/// One path-manager address, matching the C `struct mptcp_pm_addr`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PmAddr {
    /// Address family (`AF_INET`).
    pub family: u8,
    /// Address id.
    pub id: u8,
    /// Flags (bit 0 is backup).
    pub flags: u16,
    /// IPv4 address in network byte order.
    pub addr: [u8; 4],
    /// Port in network byte order.
    pub port: u16,
}

impl PmAddr {
    /// Decode the C layout; the two trailing bytes are reserved and ignored.
    pub fn decode(bytes: &[u8]) -> Result<Self, Errno> {
        let family = *bytes.first().ok_or(Errno::EINVAL)?;
        let id = *bytes.get(1).ok_or(Errno::EINVAL)?;
        let flags = u16::from_le_bytes([
            *bytes.get(2).ok_or(Errno::EINVAL)?,
            *bytes.get(3).ok_or(Errno::EINVAL)?,
        ]);
        let addr = [
            *bytes.get(4).ok_or(Errno::EINVAL)?,
            *bytes.get(5).ok_or(Errno::EINVAL)?,
            *bytes.get(6).ok_or(Errno::EINVAL)?,
            *bytes.get(7).ok_or(Errno::EINVAL)?,
        ];
        let port = u16::from_le_bytes([
            *bytes.get(8).ok_or(Errno::EINVAL)?,
            *bytes.get(9).ok_or(Errno::EINVAL)?,
        ]);
        Ok(Self {
            family,
            id,
            flags,
            addr,
            port,
        })
    }

    fn encode(&self, out: &mut [u8]) -> Result<usize, Errno> {
        let slot = out.get_mut(..PM_ADDR_LEN).ok_or(Errno::ENOBUFS)?;
        slot.fill(0);
        *slot.get_mut(0).ok_or(Errno::ENOBUFS)? = self.family;
        *slot.get_mut(1).ok_or(Errno::ENOBUFS)? = self.id;
        slot.get_mut(2..4)
            .ok_or(Errno::ENOBUFS)?
            .copy_from_slice(&self.flags.to_le_bytes());
        slot.get_mut(4..8)
            .ok_or(Errno::ENOBUFS)?
            .copy_from_slice(&self.addr);
        slot.get_mut(8..10)
            .ok_or(Errno::ENOBUFS)?
            .copy_from_slice(&self.port.to_le_bytes());
        Ok(PM_ADDR_LEN)
    }
}

/// One MPTCP control request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Control {
    /// Add one path.
    AddAddr(PmAddr),
    /// Remove one path by id.
    DelAddr(u8),
    /// Set/clear the backup flag on a path.
    SetFlags(PmAddr),
    /// Read the connection state.
    Info,
    /// Read the per-subflow table.
    Subflows,
}

impl Control {
    /// Encode the full scheme `CALL` request and return its byte count.
    pub fn encode(&self, out: &mut [u8]) -> Result<usize, Errno> {
        const HEADER: usize = 6;
        let header = out.get_mut(..HEADER).ok_or(Errno::ENOBUFS)?;
        header
            .get_mut(..2)
            .ok_or(Errno::ENOBUFS)?
            .copy_from_slice(&PROTOCOL_VERSION.to_le_bytes());
        header
            .get_mut(2..4)
            .ok_or(Errno::ENOBUFS)?
            .copy_from_slice(&KIND_CALL.to_le_bytes());
        let (opcode, used) = match self {
            Self::AddAddr(addr) => {
                let used = addr.encode(out.get_mut(HEADER..).ok_or(Errno::ENOBUFS)?)?;
                (OP_MPTCP_ADD_ADDR, HEADER.saturating_add(used))
            }
            Self::DelAddr(id) => {
                *out.get_mut(HEADER).ok_or(Errno::ENOBUFS)? = *id;
                (OP_MPTCP_DEL_ADDR, HEADER.saturating_add(1))
            }
            Self::SetFlags(addr) => {
                let used = addr.encode(out.get_mut(HEADER..).ok_or(Errno::ENOBUFS)?)?;
                (OP_MPTCP_SET_FLAGS, HEADER.saturating_add(used))
            }
            Self::Info => (OP_MPTCP_INFO, HEADER),
            Self::Subflows => (OP_MPTCP_SUBFLOWS, HEADER),
        };
        out.get_mut(4..6)
            .ok_or(Errno::ENOBUFS)?
            .copy_from_slice(&opcode.to_le_bytes());
        Ok(used)
    }
}

/// Decode a scheme reply and copy its structured payload into `out`.
///
/// Returns the payload length. `TAG_OK` is a zero-length success; `TAG_ERR` maps to the
/// server error; the MPTCP tags carry the exact C-layout bytes the caller asked for.
pub fn decode_reply(reply: &[u8], want: i32, out: &mut [u8]) -> Result<usize, Errno> {
    let tag = u16::from_le_bytes([
        *reply.first().ok_or(Errno::EPROTO)?,
        *reply.get(1).ok_or(Errno::EPROTO)?,
    ]);
    match tag {
        TAG_OK => Ok(0),
        TAG_ERR => {
            let error = u16::from_le_bytes([
                *reply.get(2).ok_or(Errno::EPROTO)?,
                *reply.get(3).ok_or(Errno::EPROTO)?,
            ]);
            Err(Errno(error))
        }
        TAG_MPTCP_INFO | TAG_MPTCP_SUBFLOWS => {
            let expected = if want == MPTCP_INFO {
                TAG_MPTCP_INFO
            } else {
                TAG_MPTCP_SUBFLOWS
            };
            if tag != expected {
                return Err(Errno::EPROTO);
            }
            let length = usize::from(u16::from_le_bytes([
                *reply.get(2).ok_or(Errno::EPROTO)?,
                *reply.get(3).ok_or(Errno::EPROTO)?,
            ]));
            let end = 4usize.saturating_add(length);
            let payload = reply.get(4..end).ok_or(Errno::EPROTO)?;
            let target = out.get_mut(..length).ok_or(Errno::EMSGSIZE)?;
            target.copy_from_slice(payload);
            Ok(length)
        }
        _ => Err(Errno::EPROTO),
    }
}

#[cfg(test)]
#[path = "mptcp_ut.rs"]
mod tests;
