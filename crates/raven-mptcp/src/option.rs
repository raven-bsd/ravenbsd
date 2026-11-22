// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! RFC 8684 MPTCP option encoding and total decoding.
//!
//! One TCP option kind (30) carries all MPTCP messages; the high nibble of the first payload
//! byte is the subtype. Each subtype has a strict length/layout below. Decoders validate the
//! length before touching any field and never panic on arbitrary input.

use alloc::vec::Vec;

use crate::Error;
use crate::dss::{self, DataSequenceSignal};
use crate::handshake::MpCapableFlags;

/// IANA TCP option kind for MPTCP.
pub const KIND: u8 = 30;

/// MP_CAPABLE subtype.
pub const SUBTYPE_MP_CAPABLE: u8 = 0;
/// MP_JOIN subtype.
pub const SUBTYPE_MP_JOIN: u8 = 1;
/// DSS subtype.
pub const SUBTYPE_DSS: u8 = 2;
/// ADD_ADDR subtype.
pub const SUBTYPE_ADD_ADDR: u8 = 3;
/// REMOVE_ADDR subtype.
pub const SUBTYPE_REMOVE_ADDR: u8 = 4;
/// MP_PRIO subtype.
pub const SUBTYPE_MP_PRIO: u8 = 5;
/// MP_FAIL subtype.
pub const SUBTYPE_MP_FAIL: u8 = 6;
/// MP_FASTCLOSE subtype.
pub const SUBTYPE_MP_FASTCLOSE: u8 = 7;
/// MP_TCPRST subtype.
pub const SUBTYPE_MP_TCPRST: u8 = 8;

/// The only MPTCP version implemented here (RFC 8684 v1).
pub const VERSION: u8 = 1;

/// Maximum number of Address IDs in one REMOVE_ADDR (Linux `MPTCP_RM_IDS_MAX`).
pub const MAX_REMOVE_IDS: usize = 8;

/// An IP address carried by ADD_ADDR.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Address {
    /// An IPv4 address in network byte order.
    V4([u8; 4]),
    /// An IPv6 address in network byte order.
    V6([u8; 16]),
}

impl Address {
    /// Encoded length in octets.
    #[must_use]
    pub fn wire_len(self) -> usize {
        match self {
            Self::V4(_) => 4,
            Self::V6(_) => 16,
        }
    }
}

/// The Address IDs of a REMOVE_ADDR option (1..=8).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoveIds {
    /// Non-empty, at most [`MAX_REMOVE_IDS`] IDs.
    pub ids: Vec<u8>,
}

impl RemoveIds {
    /// Build a validated list; an empty or oversized list is [`Error::BadLength`].
    pub fn new(ids: Vec<u8>) -> Result<Self, Error> {
        if ids.is_empty() || ids.len() > MAX_REMOVE_IDS {
            return Err(Error::BadLength);
        }
        Ok(Self { ids })
    }
}

/// MP_TCPRST reason codes (RFC 8684 table 4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TcpRstReason {
    /// 0x00: unspecified error.
    Unspecified,
    /// 0x01: MPTCP-specific error.
    Mptcp,
    /// 0x02: lack of resources.
    LackOfResources,
    /// 0x03: administratively prohibited.
    AdministrativelyProhibited,
    /// 0x04: too much outstanding data.
    TooMuchOutstandingData,
    /// 0x05: unacceptable performance.
    UnacceptablePerformance,
    /// 0x06: middlebox interference.
    MiddleboxInterference,
    /// A future or private code point; preserved but not interpreted.
    Other(u8),
}

impl TcpRstReason {
    /// The wire code.
    #[must_use]
    pub fn code(self) -> u8 {
        match self {
            Self::Unspecified => 0x00,
            Self::Mptcp => 0x01,
            Self::LackOfResources => 0x02,
            Self::AdministrativelyProhibited => 0x03,
            Self::TooMuchOutstandingData => 0x04,
            Self::UnacceptablePerformance => 0x05,
            Self::MiddleboxInterference => 0x06,
            Self::Other(code) => code,
        }
    }

    /// Map a wire code.
    #[must_use]
    pub fn from_code(code: u8) -> Self {
        match code {
            0x00 => Self::Unspecified,
            0x01 => Self::Mptcp,
            0x02 => Self::LackOfResources,
            0x03 => Self::AdministrativelyProhibited,
            0x04 => Self::TooMuchOutstandingData,
            0x05 => Self::UnacceptablePerformance,
            0x06 => Self::MiddleboxInterference,
            other => Self::Other(other),
        }
    }
}

/// A decoded MPTCP option.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MptcpOption {
    /// MP_CAPABLE, with the fields present in the received length.
    MpCapable {
        /// Negotiated version (must be [`VERSION`]).
        version: u8,
        /// Negotiated capability flags.
        flags: MpCapableFlags,
        /// Sender key when the option length is at least 12.
        sender_key: Option<u64>,
        /// Receiver key when the option length is at least 20.
        receiver_key: Option<u64>,
        /// First-data length when the option length is 22 or 24.
        data_level_length: Option<u16>,
        /// Optional mapping checksum on the first data.
        checksum: Option<u16>,
    },
    /// MP_JOIN on the initial SYN.
    MpJoinSyn {
        /// Requested backup path.
        backup: bool,
        /// Sender's Address ID.
        address_id: u8,
        /// Receiver's token.
        token: u32,
        /// Sender's random number.
        rand: u32,
    },
    /// MP_JOIN on the responding SYN/ACK.
    MpJoinSynAck {
        /// Requested backup path.
        backup: bool,
        /// Sender's Address ID.
        address_id: u8,
        /// Truncated HMAC (8 octets).
        hmac: [u8; 8],
        /// Sender's random number.
        rand: u32,
    },
    /// MP_JOIN on the initiator's first ACK.
    MpJoinAck {
        /// Truncated HMAC (20 octets).
        hmac: [u8; 20],
    },
    /// Data Sequence Signal.
    Dss(DataSequenceSignal),
    /// ADD_ADDR with IPv4 or IPv6 address, optional port and optional HMAC.
    AddAddr {
        /// Echo flag (E=1 means the HMAC is absent).
        echo: bool,
        /// Address ID.
        address_id: u8,
        /// Advertised address.
        address: Address,
        /// Optional advertised port.
        port: Option<u16>,
        /// Present when `echo` is false.
        hmac: Option<[u8; 8]>,
    },
    /// REMOVE_ADDR with one or more Address IDs.
    RemoveAddr(RemoveIds),
    /// MP_PRIO on the carrying subflow.
    MpPrio {
        /// Requested backup flag.
        backup: bool,
    },
    /// MP_FAIL with the full 64-bit Data Sequence Number.
    MpFail {
        /// First data sequence number that must be discarded.
        data_seq: u64,
    },
    /// MP_FASTCLOSE with the receiver's key.
    MpFastClose {
        /// Receiver's key.
        receiver_key: u64,
    },
    /// MP_TCPRST with the transient flag and reason code.
    MpTcpRst {
        /// The error is transient.
        transient: bool,
        /// Reason code.
        reason: TcpRstReason,
    },
}

fn push_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_be_bytes());
}

fn push_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_be_bytes());
}

fn push_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_be_bytes());
}

/// Write the four-byte MPTCP option header (`kind`, `length`, subtype/nibble, field).
fn header(out: &mut Vec<u8>, length: u8, subtype: u8, nibble: u8, field: u8) {
    out.push(KIND);
    out.push(length);
    out.push((subtype << 4) | (nibble & 0x0f));
    out.push(field);
}

impl MptcpOption {
    /// Encode this option to a fresh vector, including the TCP kind and length.
    #[must_use]
    pub fn to_vec(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(32);
        match self {
            Self::MpCapable {
                version,
                flags,
                sender_key,
                receiver_key,
                data_level_length,
                checksum,
            } => {
                let length = if sender_key.is_none() {
                    4
                } else if receiver_key.is_none() {
                    12
                } else if data_level_length.is_none() {
                    20
                } else if checksum.is_none() {
                    22
                } else {
                    24
                };
                header(
                    &mut out,
                    length,
                    SUBTYPE_MP_CAPABLE,
                    *version,
                    flags.to_byte(),
                );
                if let Some(key) = sender_key {
                    push_u64(&mut out, *key);
                }
                if let Some(key) = receiver_key {
                    push_u64(&mut out, *key);
                }
                if let Some(value) = data_level_length {
                    push_u16(&mut out, *value);
                }
                if let Some(sum) = checksum {
                    push_u16(&mut out, *sum);
                }
            }
            Self::MpJoinSyn {
                backup,
                address_id,
                token,
                rand,
            } => {
                header(
                    &mut out,
                    12,
                    SUBTYPE_MP_JOIN,
                    u8::from(*backup),
                    *address_id,
                );
                push_u32(&mut out, *token);
                push_u32(&mut out, *rand);
            }
            Self::MpJoinSynAck {
                backup,
                address_id,
                hmac,
                rand,
            } => {
                header(
                    &mut out,
                    16,
                    SUBTYPE_MP_JOIN,
                    u8::from(*backup),
                    *address_id,
                );
                out.extend_from_slice(hmac);
                push_u32(&mut out, *rand);
            }
            Self::MpJoinAck { hmac } => {
                header(&mut out, 24, SUBTYPE_MP_JOIN, 0, 0);
                out.extend_from_slice(hmac);
            }
            Self::Dss(signal) => {
                let body = signal.encode_body();
                let length = u8::try_from(4 + body.len()).unwrap_or(u8::MAX);
                header(&mut out, length, SUBTYPE_DSS, 0, signal.flags());
                out.extend_from_slice(&body);
            }
            Self::AddAddr {
                echo,
                address_id,
                address,
                port,
                hmac,
            } => {
                let base = 4 + address.wire_len() + if port.is_some() { 2 } else { 0 };
                let length = base + if *echo { 0 } else { 8 };
                let length = u8::try_from(length).unwrap_or(u8::MAX);
                header(
                    &mut out,
                    length,
                    SUBTYPE_ADD_ADDR,
                    u8::from(*echo),
                    *address_id,
                );
                match address {
                    Address::V4(bytes) => out.extend_from_slice(bytes),
                    Address::V6(bytes) => out.extend_from_slice(bytes),
                }
                if let Some(port) = port {
                    push_u16(&mut out, *port);
                }
                if let Some(hmac) = hmac {
                    out.extend_from_slice(hmac);
                }
            }
            Self::RemoveAddr(ids) => {
                let length = u8::try_from(3 + ids.ids.len()).unwrap_or(u8::MAX);
                out.push(KIND);
                out.push(length);
                out.push(SUBTYPE_REMOVE_ADDR << 4);
                out.extend_from_slice(&ids.ids);
            }
            Self::MpPrio { backup } => {
                out.push(KIND);
                out.push(3);
                out.push((SUBTYPE_MP_PRIO << 4) | u8::from(*backup));
            }
            Self::MpFail { data_seq } => {
                header(&mut out, 12, SUBTYPE_MP_FAIL, 0, 0);
                push_u64(&mut out, *data_seq);
            }
            Self::MpFastClose { receiver_key } => {
                header(&mut out, 12, SUBTYPE_MP_FASTCLOSE, 0, 0);
                push_u64(&mut out, *receiver_key);
            }
            Self::MpTcpRst { transient, reason } => {
                header(
                    &mut out,
                    4,
                    SUBTYPE_MP_TCPRST,
                    u8::from(*transient),
                    reason.code(),
                );
            }
        }
        out
    }

    /// Decode one MPTCP option starting at byte 0 of `data`.
    ///
    /// Returns the option and its encoded length. Unknown kinds, unknown subtypes, reserved
    /// bits and length mismatches are errors; the caller must ignore the option and follow the
    /// fallback rules.
    pub fn parse(data: &[u8]) -> Result<(Self, usize), Error> {
        if data.len() < 3 {
            return Err(Error::Truncated);
        }
        if data[0] != KIND {
            return Err(Error::NotMptcp);
        }
        let length = usize::from(data[1]);
        if length < 3 || length > data.len() {
            return Err(Error::BadLength);
        }
        let body = &data[..length];
        let subtype = body[2] >> 4;
        let nibble = body[2] & 0x0f;
        let option = match subtype {
            SUBTYPE_MP_CAPABLE => parse_mp_capable(body, nibble, length)?,
            SUBTYPE_MP_JOIN => parse_mp_join(body, nibble, length)?,
            SUBTYPE_DSS => parse_dss(body, nibble, length)?,
            SUBTYPE_ADD_ADDR => parse_add_addr(body, nibble, length)?,
            SUBTYPE_REMOVE_ADDR => parse_remove_addr(body, nibble, length)?,
            SUBTYPE_MP_PRIO => parse_mp_prio(body, nibble, length)?,
            SUBTYPE_MP_FAIL => parse_mp_fail(body, nibble, length)?,
            SUBTYPE_MP_FASTCLOSE => parse_fastclose(body, nibble, length)?,
            SUBTYPE_MP_TCPRST => parse_tcp_rst(body, nibble, length)?,
            _ => return Err(Error::UnknownSubtype),
        };
        Ok((option, length))
    }
}

fn parse_mp_capable(body: &[u8], version: u8, length: usize) -> Result<MptcpOption, Error> {
    if !matches!(length, 4 | 12 | 20 | 22 | 24) {
        return Err(Error::BadLength);
    }
    if version != VERSION {
        return Err(Error::UnsupportedVersion);
    }
    let flags = MpCapableFlags::from_byte(body[3])?;
    let mut sender_key = None;
    let mut receiver_key = None;
    let mut data_level_length = None;
    let mut checksum = None;
    if length >= 12 {
        sender_key = Some(read_u64(body, 4)?);
    }
    if length >= 20 {
        receiver_key = Some(read_u64(body, 12)?);
    }
    if length >= 22 {
        data_level_length = Some(read_u16(body, 20)?);
    }
    if length == 24 {
        checksum = Some(read_u16(body, 22)?);
    }
    Ok(MptcpOption::MpCapable {
        version,
        flags,
        sender_key,
        receiver_key,
        data_level_length,
        checksum,
    })
}

fn parse_mp_join(body: &[u8], nibble: u8, length: usize) -> Result<MptcpOption, Error> {
    match length {
        12 => {
            if nibble & !1 != 0 {
                return Err(Error::ReservedNonZero);
            }
            Ok(MptcpOption::MpJoinSyn {
                backup: nibble & 1 != 0,
                address_id: body[3],
                token: read_u32(body, 4)?,
                rand: read_u32(body, 8)?,
            })
        }
        16 => {
            if nibble & !1 != 0 {
                return Err(Error::ReservedNonZero);
            }
            let mut hmac = [0u8; 8];
            hmac.copy_from_slice(&body[4..12]);
            Ok(MptcpOption::MpJoinSynAck {
                backup: nibble & 1 != 0,
                address_id: body[3],
                hmac,
                rand: read_u32(body, 12)?,
            })
        }
        24 => {
            if nibble != 0 || body[3] != 0 {
                return Err(Error::ReservedNonZero);
            }
            let mut hmac = [0u8; 20];
            hmac.copy_from_slice(&body[4..24]);
            Ok(MptcpOption::MpJoinAck { hmac })
        }
        _ => Err(Error::BadLength),
    }
}

fn parse_dss(body: &[u8], nibble: u8, length: usize) -> Result<MptcpOption, Error> {
    if length < 4 {
        return Err(Error::BadLength);
    }
    if nibble != 0 {
        return Err(Error::ReservedNonZero);
    }
    let flags = body[3];
    if flags & !0x1f != 0 {
        return Err(Error::InvalidDssFlags);
    }
    let signal = dss::DataSequenceSignal::parse_body(&body[4..], flags, length)?;
    Ok(MptcpOption::Dss(signal))
}

fn parse_add_addr(body: &[u8], nibble: u8, length: usize) -> Result<MptcpOption, Error> {
    if length < 8 {
        return Err(Error::BadLength);
    }
    if nibble & !1 != 0 {
        return Err(Error::ReservedNonZero);
    }
    let echo = nibble & 1 != 0;
    let address_id = body[3];
    let (v4, port) = match (echo, length) {
        (false, 16) | (true, 8) => (true, false),
        (false, 18) | (true, 10) => (true, true),
        (false, 28) | (true, 20) => (false, false),
        (false, 30) | (true, 22) => (false, true),
        _ => return Err(Error::BadLength),
    };
    let mut offset = 4;
    let address = if v4 {
        let mut bytes = [0u8; 4];
        bytes.copy_from_slice(&body[offset..offset + 4]);
        offset += 4;
        Address::V4(bytes)
    } else {
        let mut bytes = [0u8; 16];
        bytes.copy_from_slice(&body[offset..offset + 16]);
        offset += 16;
        Address::V6(bytes)
    };
    let port = if port {
        let value = read_u16(body, offset)?;
        offset += 2;
        Some(value)
    } else {
        None
    };
    let mut hmac = None;
    if !echo {
        let mut tag = [0u8; 8];
        tag.copy_from_slice(&body[offset..offset + 8]);
        hmac = Some(tag);
    }
    Ok(MptcpOption::AddAddr {
        echo,
        address_id,
        address,
        port,
        hmac,
    })
}

fn parse_remove_addr(body: &[u8], nibble: u8, length: usize) -> Result<MptcpOption, Error> {
    if nibble != 0 {
        return Err(Error::ReservedNonZero);
    }
    if !(4..=3 + MAX_REMOVE_IDS).contains(&length) {
        return Err(Error::BadLength);
    }
    Ok(MptcpOption::RemoveAddr(RemoveIds {
        ids: body[3..].to_vec(),
    }))
}

fn parse_mp_prio(_body: &[u8], nibble: u8, length: usize) -> Result<MptcpOption, Error> {
    if length != 3 {
        return Err(Error::BadLength);
    }
    if nibble & !1 != 0 {
        return Err(Error::ReservedNonZero);
    }
    Ok(MptcpOption::MpPrio {
        backup: nibble & 1 != 0,
    })
}

fn parse_mp_fail(body: &[u8], nibble: u8, length: usize) -> Result<MptcpOption, Error> {
    if length != 12 {
        return Err(Error::BadLength);
    }
    if nibble != 0 || body[3] != 0 {
        return Err(Error::ReservedNonZero);
    }
    Ok(MptcpOption::MpFail {
        data_seq: read_u64(body, 4)?,
    })
}

fn parse_fastclose(body: &[u8], nibble: u8, length: usize) -> Result<MptcpOption, Error> {
    if length != 12 {
        return Err(Error::BadLength);
    }
    if nibble != 0 || body[3] != 0 {
        return Err(Error::ReservedNonZero);
    }
    Ok(MptcpOption::MpFastClose {
        receiver_key: read_u64(body, 4)?,
    })
}

fn parse_tcp_rst(body: &[u8], nibble: u8, length: usize) -> Result<MptcpOption, Error> {
    if length != 4 {
        return Err(Error::BadLength);
    }
    if nibble & !1 != 0 {
        return Err(Error::ReservedNonZero);
    }
    Ok(MptcpOption::MpTcpRst {
        transient: nibble & 1 != 0,
        reason: TcpRstReason::from_code(body[3]),
    })
}

fn read_u16(data: &[u8], offset: usize) -> Result<u16, Error> {
    let bytes = data.get(offset..offset + 2).ok_or(Error::Truncated)?;
    Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
}

fn read_u32(data: &[u8], offset: usize) -> Result<u32, Error> {
    let bytes = data.get(offset..offset + 4).ok_or(Error::Truncated)?;
    Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn read_u64(data: &[u8], offset: usize) -> Result<u64, Error> {
    let bytes = data.get(offset..offset + 8).ok_or(Error::Truncated)?;
    let mut value = [0u8; 8];
    value.copy_from_slice(bytes);
    Ok(u64::from_be_bytes(value))
}

/// Decode every MPTCP option in a TCP option block.
///
/// EOL stops parsing; NOP and non-MPTCP options are skipped with their declared length. A
/// malformed non-MPTCP length, a malformed MPTCP option or an unknown MPTCP subtype is an error.
pub fn parse_options(data: &[u8]) -> Result<Vec<MptcpOption>, Error> {
    let mut options = Vec::new();
    let mut offset = 0;
    while offset < data.len() {
        match data[offset] {
            0 => break,
            1 => offset += 1,
            KIND => {
                let (option, used) = MptcpOption::parse(&data[offset..])?;
                options.push(option);
                offset += used;
            }
            _ => {
                if offset + 1 >= data.len() {
                    return Err(Error::Truncated);
                }
                let length = usize::from(data[offset + 1]);
                if length < 2 || offset + length > data.len() {
                    return Err(Error::BadLength);
                }
                offset += length;
            }
        }
    }
    Ok(options)
}

#[cfg(test)]
#[path = "option_ut.rs"]
mod tests;
