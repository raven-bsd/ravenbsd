// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! First-party, transport-independent core of Multipath TCP (MPTCP) v1.
//!
//! This crate is phase 1 of the MPTCP program: the RFC 8684 wire protocol and path state,
//! with no kernel or `raven-tcpip` integration. A caller injects keys, sequence numbers and
//! addresses; the crate never opens a socket, allocates a frame or reads a clock.
//!
//! Implemented from the RFCs:
//!
//! * `MP_CAPABLE` (RFC 8684 section 3.1): key exchange, version and flag negotiation.
//! * `MP_JOIN` (section 3.2): token, nonce, address ID and HMAC-SHA256 authentication.
//! * `DSS` (section 3.3): data ACK, data sequence mapping, DATA_FIN and the TCP-style
//!   mapping checksum.
//! * `ADD_ADDR`, `REMOVE_ADDR`, `MP_PRIO`, `MP_FAIL`, `MP_FASTCLOSE` and `MP_TCPRST`
//!   (sections 3.4-3.7).
//! * A bounded, transport-independent path manager (announce, remove, priority, selection).
//!
//! Clean-room statement: Linux `net/mptcp/` is a behavioural oracle only (GPL, no copy). The
//! named files read were `options.c` (`mptcp_parse_option`, `mptcp_write_options`),
//! `protocol.h` (option lengths/flags), `crypto.c` (token/IDSN/HMAC ordering), and
//! `pm.c`/`pm_netlink.c` (path-manager behaviour). No FreeBSD donor has MPTCP. RFC 8684 is
//! normative and RFC 6824 is consulted for the v0 background; v1 only is implemented.
//!
//! Every decoder is total over arbitrary bytes: malformed, truncated or unknown options return
//! [`Error`], never panic. Options are bounded by the TCP option space and the path table is
//! bounded by [`path::MAX_PATHS`].

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

pub mod crypto;
pub mod dss;
pub mod handshake;
pub mod meta;
pub mod option;
pub mod path;

pub use crypto::{hmac_sha256, initial_data_seq, sha256, token};
pub use dss::{DataSequenceSignal, Mapping};
pub use handshake::{
    MpCapableFlags, MpCapableHandshake, MpJoinAck, MpJoinInitiator, MpJoinResponder, MpJoinSyn,
    MpJoinSynAck,
};
pub use meta::{MetaSocket, Subflow, SubflowReadiness};
pub use option::{Address, MptcpOption, RemoveIds, TcpRstReason};
pub use path::{MAX_PATHS, Path, PathManager};

/// A decoder or protocol-state error.
///
/// The variants are deliberately small and carry no peer-controlled allocation. A caller treats
/// any error as "ignore this option/subflow", which is the RFC 8684 fallback rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// Fewer bytes than the option header or declared length.
    Truncated,
    /// The TCP option kind is not MPTCP (30).
    NotMptcp,
    /// The declared length does not match the subtype layout.
    BadLength,
    /// The subtype is not defined by RFC 8684.
    UnknownSubtype,
    /// The MPTCP version is not supported (only v1 is implemented).
    UnsupportedVersion,
    /// A required MP_CAPABLE crypto flag is missing or a reserved flag is set.
    InvalidFlags,
    /// A reserved field is non-zero.
    ReservedNonZero,
    /// A DSS flag combination is invalid (for example `m` without `M`).
    InvalidDssFlags,
    /// A key, token, address ID or path is already present.
    Duplicate,
    /// The path table is full.
    PathTableFull,
    /// The requested path does not exist.
    NotFound,
    /// The send buffer is full; the caller should retry after an ACK.
    WouldBlock,
    /// The peer exceeded the connection-level receive window.
    WindowViolation,
    /// A queue would exceed its configured bound.
    MemoryLimit,
    /// The connection is closed.
    Closed,
}
