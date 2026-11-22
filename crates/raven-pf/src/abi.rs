// SPDX-License-Identifier: BSD-2-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// Portions ported from FreeBSD sys/netpfil/pf/pf.h (BSD-2-Clause):
// Copyright (c) 2001 Daniel Hartmeier. All rights reserved.
// Full donor notice and revision: PROVENANCE.toml, docs/pf.md section 13.

//! The pfctl-compatible ABI surface: constants, address representation and status block.
//!
//! Donor: `sys/netpfil/pf/pf.h` at `133344405f532bd54ff3629a77d4c4e062f01fdb`: the
//! `PF_PASS..PF_RT` action enum, `PF_OP_*`, `PF_POOL_*`, `PF_LOG_*`, `PF_ADDR_*`,
//! `PFTM_*` indices and `PFTM_*_VAL` defaults, `PFRES_*` reasons and `PFRES_NAMES`, and
//! `struct pf_addr` / `struct pf_status`.
//!
//! Deviation D-pf-abi-ioctl: the pruned donor tree has no `PFOP_*` ioctl numbers (they live
//! in upstream `usr.sbin/pfctl`/`net/pfvar.h`, absent here), so this module exposes the
//! `pf.h` data surface only. The ioctl transport is planned for K274 together with the
//! userspace client.

use crate::address::{IpAddr, Ipv4Addr, Ipv6Addr};

/// Donor `enum { PF_INOUT, PF_IN, PF_OUT }`.
pub const PF_INOUT: u8 = 0;
/// Inbound direction.
pub const PF_IN: u8 = 1;
/// Outbound direction.
pub const PF_OUT: u8 = 2;

/// Donor `enum { PF_PASS, PF_DROP, ... PF_RT }`.
pub const PF_PASS: u8 = 0;
/// Blocked by a rule.
pub const PF_DROP: u8 = 1;
/// Scrub.
pub const PF_SCRUB: u8 = 2;
/// No scrub.
pub const PF_NOSCRUB: u8 = 3;
/// Source NAT.
pub const PF_NAT: u8 = 4;
/// No NAT.
pub const PF_NONAT: u8 = 5;
/// Bidirectional NAT.
pub const PF_BINAT: u8 = 6;
/// No bidirectional NAT.
pub const PF_NOBINAT: u8 = 7;
/// Redirect.
pub const PF_RDR: u8 = 8;
/// No redirect.
pub const PF_NORDR: u8 = 9;
/// SYN proxy drop.
pub const PF_SYNPROXY_DROP: u8 = 10;
/// Deferred.
pub const PF_DEFER: u8 = 11;
/// Match-only rule.
pub const PF_MATCH: u8 = 12;
/// Address-family translation.
pub const PF_AFRT: u8 = 13;
/// Route.
pub const PF_RT: u8 = 14;

/// Donor `PF_OP_NONE`.
pub const PF_OP_NONE: u8 = 0;
/// Inclusive range (`a >< b`).
pub const PF_OP_IRG: u8 = 1;
/// Equal.
pub const PF_OP_EQ: u8 = 2;
/// Not equal.
pub const PF_OP_NE: u8 = 3;
/// Less than.
pub const PF_OP_LT: u8 = 4;
/// Less than or equal.
pub const PF_OP_LE: u8 = 5;
/// Greater than.
pub const PF_OP_GT: u8 = 6;
/// Greater than or equal.
pub const PF_OP_GE: u8 = 7;
/// Exclusive range (`a <> b`).
pub const PF_OP_XRG: u8 = 8;
/// Exclusive range (`a >< b` in pfctl spelling).
pub const PF_OP_RRG: u8 = 9;

/// Donor `PFTM_*` indices.
pub const PFTM_TCP_FIRST_PACKET: usize = 0;
/// No response yet.
pub const PFTM_TCP_OPENING: usize = 1;
/// Established.
pub const PFTM_TCP_ESTABLISHED: usize = 2;
/// Half closed.
pub const PFTM_TCP_CLOSING: usize = 3;
/// Got both FINs.
pub const PFTM_TCP_FIN_WAIT: usize = 4;
/// Got a RST.
pub const PFTM_TCP_CLOSED: usize = 5;
/// First UDP packet.
pub const PFTM_UDP_FIRST_PACKET: usize = 6;
/// Unidirectional UDP.
pub const PFTM_UDP_SINGLE: usize = 7;
/// Bidirectional UDP.
pub const PFTM_UDP_MULTIPLE: usize = 8;
/// First ICMP packet.
pub const PFTM_ICMP_FIRST_PACKET: usize = 9;
/// ICMP error reply.
pub const PFTM_ICMP_ERROR_REPLY: usize = 10;
/// First other-protocol packet.
pub const PFTM_OTHER_FIRST_PACKET: usize = 11;
/// Unidirectional other flow.
pub const PFTM_OTHER_SINGLE: usize = 12;
/// Bidirectional other flow.
pub const PFTM_OTHER_MULTIPLE: usize = 13;
/// Fragment lifetime.
pub const PFTM_FRAG: usize = 14;
/// Expiry interval.
pub const PFTM_INTERVAL: usize = 15;
/// Adaptive start.
pub const PFTM_ADAPTIVE_START: usize = 16;
/// Adaptive end.
pub const PFTM_ADAPTIVE_END: usize = 17;
/// Source tracking.
pub const PFTM_SRC_NODE: usize = 18;
/// Allowed timestamp difference.
pub const PFTM_TS_DIFF: usize = 19;
/// Legacy `pf_rule.timeout[]` bound.
pub const PFTM_OLD_MAX: usize = 20;
/// Real `PFTM_*` bound (includes the SCTP entries).
pub const PFTM_MAX: usize = 25;
/// Purge marker.
pub const PFTM_PURGE: usize = 26;
/// Unlinked marker.
pub const PFTM_UNLINKED: usize = 27;

/// Donor `PFTM_*_VAL` defaults for indices `0..PFTM_OLD_MAX`.
pub const PFTM_DEFAULT: [u32; PFTM_OLD_MAX] = [
    120,
    30,
    24 * 60 * 60,
    15 * 60,
    45,
    90,
    60,
    30,
    60,
    20,
    10,
    60,
    30,
    60,
    60,
    10,
    0,
    0,
    0,
    30,
];

/// Donor `enum { PF_NOPFROUTE, PF_FASTROUTE, PF_ROUTETO, PF_DUPTO, PF_REPLYTO }`.
pub const PF_NOPFROUTE: u8 = 0;
/// Fast route.
pub const PF_FASTROUTE: u8 = 1;
/// Route to.
pub const PF_ROUTETO: u8 = 2;
/// Duplicate to.
pub const PF_DUPTO: u8 = 3;
/// Reply to.
pub const PF_REPLYTO: u8 = 4;

/// Donor `PF_POOL_NONE`.
pub const PF_POOL_NONE: u8 = 0;
/// Bitmask pool.
pub const PF_POOL_BITMASK: u8 = 1;
/// Random pool.
pub const PF_POOL_RANDOM: u8 = 2;
/// Source-hash pool.
pub const PF_POOL_SRCHASH: u8 = 3;
/// Round-robin pool.
pub const PF_POOL_ROUNDROBIN: u8 = 4;
/// Sticky-address option.
pub const PF_POOL_STICKYADDR: u8 = 0x20;
/// Pool-kind mask.
pub const PF_POOL_IDMASK: u8 = 0x0f;

/// Donor `PF_ADDR_ADDRMASK`.
pub const PF_ADDR_ADDRMASK: u8 = 0;
/// No-route address.
pub const PF_ADDR_NOROUTE: u8 = 1;
/// Dynamic interface address.
pub const PF_ADDR_DYNIFTL: u8 = 2;
/// Table reference.
pub const PF_ADDR_TABLE: u8 = 3;
/// Urpf-failed address.
pub const PF_ADDR_URPFFAILED: u8 = 4;
/// Address range.
pub const PF_ADDR_RANGE: u8 = 5;
/// Empty address.
pub const PF_ADDR_NONE: u8 = 6;

/// Log this packet.
pub const PF_LOG: u8 = 0x01;
/// Log all packets of a state.
pub const PF_LOG_ALL: u8 = 0x02;
/// Log with user priority.
pub const PF_LOG_USER: u8 = 0x04;
/// Force logging (fail-closed events).
pub const PF_LOG_FORCE: u8 = 0x08;
/// Log matching rules.
pub const PF_LOG_MATCHES: u8 = 0x10;

/// Donor `PFRULE_RETURNRST`.
pub const PFRULE_RETURNRST: u32 = 0x0000_0001;
/// Donor `PFRULE_FRAGMENT`.
pub const PFRULE_FRAGMENT: u32 = 0x0000_0002;
/// Donor `PFRULE_RETURNICMP`.
pub const PFRULE_RETURNICMP: u32 = 0x0000_0004;
/// Donor `PFRULE_RETURN`.
pub const PFRULE_RETURN: u32 = 0x0000_0008;
/// Donor `PFRULE_NOSYNC`.
pub const PFRULE_NOSYNC: u32 = 0x0000_0010;
/// Donor `PFRULE_SRCTRACK`.
pub const PFRULE_SRCTRACK: u32 = 0x0000_0020;
/// Donor `PFRULE_RULESRCTRACK`.
pub const PFRULE_RULESRCTRACK: u32 = 0x0000_0040;
/// Donor `PFRULE_IFBOUND`.
pub const PFRULE_IFBOUND: u32 = 0x0001_0000;
/// Donor `PFRULE_STATESLOPPY`.
pub const PFRULE_STATESLOPPY: u32 = 0x0002_0000;
/// Donor `PFRULE_AFTO`.
pub const PFRULE_AFTO: u32 = 0x0020_0000;
/// Donor `PFRULE_ONCE`.
pub const PFRULE_ONCE: u32 = 0x0040_0000;

/// Donor `PFRES_MATCH`: blocked by a rule.
pub const PFRES_MATCH: usize = 0;
/// Bad offset.
pub const PFRES_BADOFF: usize = 1;
/// Fragment.
pub const PFRES_FRAG: usize = 2;
/// Short packet.
pub const PFRES_SHORT: usize = 3;
/// Normalization failure.
pub const PFRES_NORM: usize = 4;
/// Memory allocation failure.
pub const PFRES_MEMORY: usize = 5;
/// Bad timestamp.
pub const PFRES_TS: usize = 6;
/// Congestion.
pub const PFRES_CONGEST: usize = 7;
/// IP option.
pub const PFRES_IPOPTIONS: usize = 8;
/// Protocol checksum.
pub const PFRES_PROTCKSUM: usize = 9;
/// State mismatch.
pub const PFRES_BADSTATE: usize = 10;
/// State insertion failure.
pub const PFRES_STATEINS: usize = 11;
/// State-table limit (`max_states`).
pub const PFRES_MAXSTATES: usize = 12;
/// Source-node limit.
pub const PFRES_SRCLIMIT: usize = 13;
/// SYN proxy.
pub const PFRES_SYNPROXY: usize = 14;
/// Address mapping failure.
pub const PFRES_MAPFAILED: usize = 15;
/// Translation failure.
pub const PFRES_TRANSLATE: usize = 16;
/// Donor `PFRES_MAX`.
pub const PFRES_MAX: usize = 17;

/// Donor `PFRES_NAMES`.
pub const PFRES_NAMES: [&str; PFRES_MAX] = [
    "match",
    "bad-offset",
    "fragment",
    "short",
    "normalize",
    "memory",
    "bad-timestamp",
    "congestion",
    "ip-option",
    "proto-cksum",
    "state-mismatch",
    "state-insert",
    "state-limit",
    "src-limit",
    "synproxy",
    "map-failed",
    "translate",
];

/// One address in the donor `struct pf_addr` representation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct PfAddr {
    /// Sixteen address bytes; an IPv4 address uses the first four.
    pub addr: [u8; 16],
}

impl PfAddr {
    /// Store an IPv4 or IPv6 address.
    #[must_use]
    pub fn from_ip(address: IpAddr) -> Self {
        let mut addr = [0u8; 16];
        match address {
            IpAddr::V4(v4) => addr[..4].copy_from_slice(&v4.octets()),
            IpAddr::V6(v6) => addr.copy_from_slice(&v6.octets()),
        }
        Self { addr }
    }

    /// The IPv4 view of the first four octets.
    #[must_use]
    pub fn as_v4(&self) -> Ipv4Addr {
        Ipv4Addr::new(self.addr[0], self.addr[1], self.addr[2], self.addr[3])
    }

    /// The IPv6 view.
    #[must_use]
    pub fn as_v6(&self) -> Ipv6Addr {
        let mut octets = [0u8; 16];
        octets.copy_from_slice(&self.addr);
        Ipv6Addr::new(octets)
    }
}

/// A `struct pf_status`-shaped snapshot for the pfctl client.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PfStatus {
    /// Whether the filter is running.
    pub running: bool,
    /// Seconds since the filter started.
    pub since: u64,
    /// Number of live states.
    pub states: u32,
    /// Configured state limit.
    pub max_states: u32,
    /// `PFRES_*` counters.
    pub counters: [u64; PFRES_MAX],
    /// Log inserts.
    pub linserts: u64,
    /// Log deletes.
    pub ldeletes: u64,
    /// Packets passed.
    pub fcounters_pass: u64,
    /// Packets blocked.
    pub fcounters_block: u64,
}

impl Default for PfStatus {
    fn default() -> Self {
        Self {
            running: false,
            since: 0,
            states: 0,
            max_states: 0,
            counters: [0; PFRES_MAX],
            linserts: 0,
            ldeletes: 0,
            fcounters_pass: 0,
            fcounters_block: 0,
        }
    }
}

#[cfg(test)]
#[path = "abi_ut.rs"]
mod tests;
