// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! MP_CAPABLE key negotiation and MP_JOIN HMAC authentication (RFC 8684 3.1-3.2).

use crate::Error;
use crate::crypto::{hmac_sha256, tag_eq, token};
use crate::option::MptcpOption;

/// MP_CAPABLE capability flags. Bit H (HMAC-SHA256) is mandatory; B and D-G are reserved.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MpCapableFlags {
    /// A: checksum required. Either side may require it.
    pub checksum_required: bool,
    /// C: the sender will not accept new subflows to this address/port.
    pub deny_join_id0: bool,
}

impl MpCapableFlags {
    /// Decode the flags octet, rejecting reserved or unsupported crypto bits.
    pub fn from_byte(byte: u8) -> Result<Self, Error> {
        // A=0x80, B=0x40, C=0x20, D-G=0x1e, H=0x01.
        if byte & 0x40 != 0 || byte & 0x1e != 0 || byte & 0x01 == 0 {
            return Err(Error::InvalidFlags);
        }
        Ok(Self {
            checksum_required: byte & 0x80 != 0,
            deny_join_id0: byte & 0x20 != 0,
        })
    }

    /// Encode the flags octet with H always set.
    #[must_use]
    pub fn to_byte(self) -> u8 {
        let mut byte = 0x01;
        if self.checksum_required {
            byte |= 0x80;
        }
        if self.deny_join_id0 {
            byte |= 0x20;
        }
        byte
    }
}

/// The role of a local endpoint in the MP_CAPABLE exchange.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Role {
    Initiator,
    Responder,
}

/// MP_CAPABLE handshake state for one connection.
///
/// The caller supplies the local 64-bit key; the state machine validates the peer's version,
/// flags and keys and produces the next option. It never derives keys from a clock or a global
/// RNG: key generation is the caller's responsibility (RFC 8684 section 3.1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MpCapableHandshake {
    role: Role,
    local_key: u64,
    remote_key: Option<u64>,
    remote_flags: MpCapableFlags,
    checksum_required: bool,
}

impl MpCapableHandshake {
    /// Start an active opener with `local_key`.
    #[must_use]
    pub fn initiator(local_key: u64, checksum_required: bool) -> Self {
        Self {
            role: Role::Initiator,
            local_key,
            remote_key: None,
            remote_flags: MpCapableFlags::default(),
            checksum_required,
        }
    }

    /// Start a passive opener with `local_key`.
    #[must_use]
    pub fn responder(local_key: u64, checksum_required: bool) -> Self {
        Self {
            role: Role::Responder,
            local_key,
            remote_key: None,
            remote_flags: MpCapableFlags::default(),
            checksum_required,
        }
    }

    /// The local key.
    #[must_use]
    pub fn local_key(&self) -> u64 {
        self.local_key
    }

    /// The peer key once exchanged.
    #[must_use]
    pub fn remote_key(&self) -> Option<u64> {
        self.remote_key
    }

    /// Whether checksums were required by either side.
    #[must_use]
    pub fn checksum_required(&self) -> bool {
        self.checksum_required
    }

    /// Build the initial SYN option (length 4, no key).
    #[must_use]
    pub fn syn(&self) -> MptcpOption {
        MptcpOption::MpCapable {
            version: crate::option::VERSION,
            flags: MpCapableFlags {
                checksum_required: self.checksum_required,
                deny_join_id0: false,
            },
            sender_key: None,
            receiver_key: None,
            data_level_length: None,
            checksum: None,
        }
    }

    /// Process the peer's SYN and build the SYN/ACK (length 12, local key).
    pub fn accept_syn(&mut self, option: &MptcpOption) -> Result<MptcpOption, Error> {
        if self.role != Role::Responder {
            return Err(Error::InvalidFlags);
        }
        let (version, flags, sender, receiver, data) = split_capable(option)?;
        if version != crate::option::VERSION {
            return Err(Error::UnsupportedVersion);
        }
        if sender.is_some() || receiver.is_some() || data.is_some() {
            return Err(Error::BadLength);
        }
        self.remote_flags = flags;
        self.checksum_required |= flags.checksum_required;
        Ok(MptcpOption::MpCapable {
            version: crate::option::VERSION,
            flags: MpCapableFlags {
                checksum_required: self.checksum_required,
                deny_join_id0: false,
            },
            sender_key: Some(self.local_key),
            receiver_key: None,
            data_level_length: None,
            checksum: None,
        })
    }

    /// Process the peer's SYN/ACK and build the third ACK with both keys.
    pub fn accept_synack(&mut self, option: &MptcpOption) -> Result<MptcpOption, Error> {
        if self.role != Role::Initiator {
            return Err(Error::InvalidFlags);
        }
        let (version, flags, sender, receiver, data) = split_capable(option)?;
        if version != crate::option::VERSION {
            return Err(Error::UnsupportedVersion);
        }
        if receiver.is_some() || data.is_some() {
            return Err(Error::BadLength);
        }
        let remote = sender.ok_or(Error::BadLength)?;
        self.remote_key = Some(remote);
        self.remote_flags = flags;
        self.checksum_required |= flags.checksum_required;
        Ok(MptcpOption::MpCapable {
            version: crate::option::VERSION,
            flags: MpCapableFlags {
                checksum_required: self.checksum_required,
                deny_join_id0: false,
            },
            sender_key: Some(self.local_key),
            receiver_key: Some(remote),
            data_level_length: None,
            checksum: None,
        })
    }

    /// Process the third ACK on the responder and confirm both keys.
    pub fn accept_ack(&mut self, option: &MptcpOption) -> Result<(), Error> {
        if self.role != Role::Responder {
            return Err(Error::InvalidFlags);
        }
        let (_version, _flags, sender, receiver, _data) = split_capable(option)?;
        let remote = sender.ok_or(Error::BadLength)?;
        if receiver != Some(self.local_key) {
            return Err(Error::InvalidFlags);
        }
        self.remote_key = Some(remote);
        Ok(())
    }

    /// Whether the peer key has been received.
    #[must_use]
    pub fn established(&self) -> bool {
        self.remote_key.is_some()
    }
}

/// The decoded MP_CAPABLE fields used by the handshake state machine.
type CapableParts = (
    u8,
    MpCapableFlags,
    Option<u64>,
    Option<u64>,
    Option<(u16, Option<u16>)>,
);

fn split_capable(option: &MptcpOption) -> Result<CapableParts, Error> {
    let MptcpOption::MpCapable {
        version,
        flags,
        sender_key,
        receiver_key,
        data_level_length,
        checksum,
    } = option
    else {
        return Err(Error::InvalidFlags);
    };
    Ok((
        *version,
        *flags,
        *sender_key,
        *receiver_key,
        data_level_length.map(|length| (length, *checksum)),
    ))
}

/// The MP_JOIN SYN: receiver token and initiator nonce.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MpJoinSyn {
    /// Requested backup flag.
    pub backup: bool,
    /// Initiator Address ID.
    pub address_id: u8,
    /// The responder's token.
    pub token: u32,
    /// The initiator nonce.
    pub rand: u32,
}

impl MpJoinSyn {
    /// Encode as an MPTCP option.
    #[must_use]
    pub fn to_option(self) -> MptcpOption {
        MptcpOption::MpJoinSyn {
            backup: self.backup,
            address_id: self.address_id,
            token: self.token,
            rand: self.rand,
        }
    }
}

/// The MP_JOIN SYN/ACK: truncated responder HMAC and nonce.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MpJoinSynAck {
    /// Requested backup flag.
    pub backup: bool,
    /// Responder Address ID.
    pub address_id: u8,
    /// Truncated HMAC (8 octets).
    pub hmac: [u8; 8],
    /// The responder nonce.
    pub rand: u32,
}

impl MpJoinSynAck {
    /// Encode as an MPTCP option.
    #[must_use]
    pub fn to_option(self) -> MptcpOption {
        MptcpOption::MpJoinSynAck {
            backup: self.backup,
            address_id: self.address_id,
            hmac: self.hmac,
            rand: self.rand,
        }
    }
}

/// The MP_JOIN third ACK HMAC.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MpJoinAck {
    /// Truncated HMAC (20 octets).
    pub hmac: [u8; 20],
}

impl MpJoinAck {
    /// Encode as an MPTCP option.
    #[must_use]
    pub fn to_option(self) -> MptcpOption {
        MptcpOption::MpJoinAck { hmac: self.hmac }
    }
}

/// HMAC for the initiator: key `Key-A || Key-B`, message `R-A || R-B`.
#[must_use]
pub fn initiator_hmac(key_a: u64, key_b: u64, rand_a: u32, rand_b: u32) -> [u8; 32] {
    let mut key = [0u8; 16];
    key[..8].copy_from_slice(&key_a.to_be_bytes());
    key[8..].copy_from_slice(&key_b.to_be_bytes());
    let mut message = [0u8; 8];
    message[..4].copy_from_slice(&rand_a.to_be_bytes());
    message[4..].copy_from_slice(&rand_b.to_be_bytes());
    hmac_sha256(&key, &message)
}

/// HMAC for the responder: key `Key-B || Key-A`, message `R-B || R-A`.
#[must_use]
pub fn responder_hmac(key_a: u64, key_b: u64, rand_a: u32, rand_b: u32) -> [u8; 32] {
    let mut key = [0u8; 16];
    key[..8].copy_from_slice(&key_b.to_be_bytes());
    key[8..].copy_from_slice(&key_a.to_be_bytes());
    let mut message = [0u8; 8];
    message[..4].copy_from_slice(&rand_b.to_be_bytes());
    message[4..].copy_from_slice(&rand_a.to_be_bytes());
    hmac_sha256(&key, &message)
}

/// Initiator side of an MP_JOIN exchange.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MpJoinInitiator {
    key_a: u64,
    key_b: u64,
    rand_a: u32,
    rand_b: Option<u32>,
    address_id: u8,
    backup: bool,
}

impl MpJoinInitiator {
    /// Create the initiator with its local key (`key_a`), the connection's peer key (`key_b`)
    /// and a fresh nonce.
    #[must_use]
    pub fn new(key_a: u64, key_b: u64, rand_a: u32, address_id: u8, backup: bool) -> Self {
        Self {
            key_a,
            key_b,
            rand_a,
            rand_b: None,
            address_id,
            backup,
        }
    }

    /// The SYN option.
    #[must_use]
    pub fn syn(&self) -> MptcpOption {
        MpJoinSyn {
            backup: self.backup,
            address_id: self.address_id,
            token: token(self.key_b),
            rand: self.rand_a,
        }
        .to_option()
    }

    /// Validate the SYN/ACK HMAC and produce the third ACK.
    pub fn accept_synack(&mut self, option: &MptcpOption) -> Result<MptcpOption, Error> {
        let MptcpOption::MpJoinSynAck {
            backup: _,
            address_id: _,
            hmac,
            rand,
        } = option
        else {
            return Err(Error::InvalidFlags);
        };
        let expected = responder_hmac(self.key_a, self.key_b, self.rand_a, *rand);
        if !tag_eq(&expected[..8], hmac) {
            return Err(Error::InvalidFlags);
        }
        self.rand_b = Some(*rand);
        let tag = initiator_hmac(self.key_a, self.key_b, self.rand_a, *rand);
        let mut full = [0u8; 20];
        full.copy_from_slice(&tag[..20]);
        Ok(MpJoinAck { hmac: full }.to_option())
    }

    /// The responder nonce, once the SYN/ACK was accepted.
    #[must_use]
    pub fn rand_b(&self) -> Option<u32> {
        self.rand_b
    }
}

/// Responder side of an MP_JOIN exchange.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MpJoinResponder {
    key_a: u64,
    key_b: u64,
    rand_b: u32,
    rand_a: Option<u32>,
    address_id: u8,
    backup: bool,
}

impl MpJoinResponder {
    /// Create the responder with the connection's peer key (`key_a`), its local key (`key_b`)
    /// and a fresh nonce.
    #[must_use]
    pub fn new(key_a: u64, key_b: u64, rand_b: u32, address_id: u8, backup: bool) -> Self {
        Self {
            key_a,
            key_b,
            rand_b,
            rand_a: None,
            address_id,
            backup,
        }
    }

    /// Validate the SYN token and produce the SYN/ACK HMAC.
    pub fn accept_syn(&mut self, option: &MptcpOption) -> Result<MptcpOption, Error> {
        let MptcpOption::MpJoinSyn {
            backup: _,
            address_id: _,
            token: received_token,
            rand,
        } = option
        else {
            return Err(Error::InvalidFlags);
        };
        if *received_token != token(self.key_b) {
            return Err(Error::NotFound);
        }
        self.rand_a = Some(*rand);
        let tag = responder_hmac(self.key_a, self.key_b, *rand, self.rand_b);
        let mut hmac = [0u8; 8];
        hmac.copy_from_slice(&tag[..8]);
        Ok(MpJoinSynAck {
            backup: self.backup,
            address_id: self.address_id,
            hmac,
            rand: self.rand_b,
        }
        .to_option())
    }

    /// Validate the initiator's third ACK.
    pub fn accept_ack(&self, option: &MptcpOption) -> Result<(), Error> {
        let MptcpOption::MpJoinAck { hmac } = option else {
            return Err(Error::InvalidFlags);
        };
        let rand_a = self.rand_a.ok_or(Error::InvalidFlags)?;
        let tag = initiator_hmac(self.key_a, self.key_b, rand_a, self.rand_b);
        if !tag_eq(&tag[..20], hmac) {
            return Err(Error::InvalidFlags);
        }
        Ok(())
    }

    /// The initiator nonce, once the SYN was accepted.
    #[must_use]
    pub fn rand_a(&self) -> Option<u32> {
        self.rand_a
    }
}

#[cfg(test)]
#[path = "handshake_ut.rs"]
mod tests;
