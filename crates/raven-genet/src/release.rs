// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pi 4 release network gate: link up, DHCP lease, gateway ping.
//!
//! The REL4 board gate has three ordered observations and one completion marker:
//!
//! 1. [`MARKER_LINK`] - the debounced link is up at a resolved speed and duplex;
//! 2. [`MARKER_DHCP`] - `netd` applied a DHCP lease (address, prefix, gateway, DNS);
//! 3. [`MARKER_PING`] - `ping <gateway>` received at least one reply;
//! 4. [`MARKER_OK`] - all three observations are present and in order.
//!
//! [`ReleaseGate`] is the host-testable state machine. It rejects an out-of-order step, a
//! non-unicast lease, a ping to an address other than the lease gateway and a ping with no
//! reply. The stable marker lines are consumed by `scripts/pi4-net-evidence.sh`; a board result
//! remains UNVERIFIED until the owner supplies the serial transcript.

use core::fmt;

use crate::link::LinkEvent;
use crate::phy::{Duplex, Speed};

/// Stable marker for the debounced link-up line.
pub const MARKER_LINK: &str = "GENET-NET-LINK";
/// Stable marker for the applied DHCP lease.
pub const MARKER_DHCP: &str = "GENET-NET-DHCP";
/// Stable marker for the gateway ping result.
pub const MARKER_PING: &str = "GENET-NET-PING";
/// Stable marker for a complete release network gate.
pub const MARKER_OK: &str = "GENET-NET-OK";

/// Every marker the evidence script requires, in transcript order.
pub const MARKERS: [&str; 4] = [MARKER_LINK, MARKER_DHCP, MARKER_PING, MARKER_OK];

/// One stage of the release network gate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    /// Waiting for the debounced link-up event.
    Link,
    /// Waiting for the DHCP lease.
    Dhcp,
    /// Waiting for the gateway ping.
    Ping,
    /// All observations are present.
    Complete,
}

impl fmt::Display for Stage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Link => "link",
            Self::Dhcp => "dhcp",
            Self::Ping => "ping",
            Self::Complete => "complete",
        };
        formatter.write_str(name)
    }
}

/// A validated DHCP lease.
///
/// Addresses are host-order byte arrays (`192.168.1.23` is `[192, 168, 1, 23]`). The
/// constructor rejects the zero, broadcast, multicast and loopback ranges, a zero prefix, a
/// zero/broadcast/multicast/loopback gateway and a gateway equal to the lease address.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lease {
    /// Lease address.
    pub address: [u8; 4],
    /// Prefix length, 1..=32.
    pub prefix: u8,
    /// Default gateway.
    pub gateway: [u8; 4],
    /// Optional DNS server from the lease.
    pub dns: Option<[u8; 4]>,
}

impl Lease {
    /// Validate and construct a lease.
    pub fn new(
        address: [u8; 4],
        prefix: u8,
        gateway: [u8; 4],
        dns: Option<[u8; 4]>,
    ) -> Result<Self, ReleaseError> {
        if !usable_unicast(address)
            || !usable_unicast(gateway)
            || address == gateway
            || prefix == 0
            || prefix > 32
            || dns.is_some_and(|server| !usable_unicast(server))
        {
            return Err(ReleaseError::InvalidLease);
        }
        Ok(Self {
            address,
            prefix,
            gateway,
            dns,
        })
    }
}

/// The result of `ping <gateway>`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PingReport {
    /// Destination address.
    pub target: [u8; 4],
    /// Echo requests sent.
    pub transmitted: u16,
    /// Echo replies received.
    pub received: u16,
    /// Minimum or average round-trip time reported by `ping`.
    pub rtt_ms: u32,
}

/// Why a release gate step was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReleaseError {
    /// A step was observed before its predecessor.
    WrongStage {
        /// The stage the step requires.
        expected: Stage,
        /// The stage the gate is in.
        actual: Stage,
    },
    /// The debounced link went down before the gate completed.
    LinkDown,
    /// The lease address, prefix, gateway or DNS server is not usable.
    InvalidLease,
    /// The ping target is not the lease gateway.
    TargetIsNotGateway {
        /// The address the probe used.
        target: [u8; 4],
        /// The gateway from the lease.
        gateway: [u8; 4],
    },
    /// The ping sent no requests or reported impossible counts.
    InvalidPing {
        /// Requests sent.
        transmitted: u16,
        /// Replies received.
        received: u16,
    },
    /// The ping sent requests but received no reply.
    NoReply,
}

impl fmt::Display for ReleaseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongStage { expected, actual } => {
                write!(formatter, "expected stage {expected}, got {actual}")
            }
            Self::LinkDown => formatter.write_str("link went down before the gate completed"),
            Self::InvalidLease => {
                formatter.write_str("lease is not a usable unicast configuration")
            }
            Self::TargetIsNotGateway { target, gateway } => write!(
                formatter,
                "ping target {}.{}.{}.{} is not the lease gateway {}.{}.{}.{}",
                target[0],
                target[1],
                target[2],
                target[3],
                gateway[0],
                gateway[1],
                gateway[2],
                gateway[3]
            ),
            Self::InvalidPing {
                transmitted,
                received,
            } => write!(
                formatter,
                "invalid ping counts: transmitted={transmitted} received={received}"
            ),
            Self::NoReply => formatter.write_str("ping received no reply"),
        }
    }
}

/// A complete release network observation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReleaseTranscript {
    /// Resolved link speed.
    pub speed: Speed,
    /// Resolved link duplex.
    pub duplex: Duplex,
    /// Applied DHCP lease.
    pub lease: Lease,
    /// Gateway ping result.
    pub ping: PingReport,
}

impl fmt::Display for ReleaseTranscript {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            formatter,
            "{MARKER_LINK} speed={} duplex={}",
            speed_mbps(self.speed),
            duplex_name(self.duplex)
        )?;
        write!(
            formatter,
            "{MARKER_DHCP} addr={}/{} gateway={}",
            Ipv4(self.lease.address),
            self.lease.prefix,
            Ipv4(self.lease.gateway)
        )?;
        match self.lease.dns {
            Some(server) => writeln!(formatter, " dns={}", Ipv4(server))?,
            None => writeln!(formatter, " dns=none")?,
        }
        writeln!(
            formatter,
            "{MARKER_PING} target={} tx={} rx={} rtt_ms={}",
            Ipv4(self.ping.target),
            self.ping.transmitted,
            self.ping.received,
            self.ping.rtt_ms
        )?;
        writeln!(formatter, "{MARKER_OK}")
    }
}

/// The ordered release network gate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReleaseGate {
    stage: Stage,
    link: Option<(Speed, Duplex)>,
    lease: Option<Lease>,
    ping: Option<PingReport>,
}

impl ReleaseGate {
    /// A gate waiting for the link-up event.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            stage: Stage::Link,
            link: None,
            lease: None,
            ping: None,
        }
    }

    /// The current stage.
    #[must_use]
    pub const fn stage(self) -> Stage {
        self.stage
    }

    /// The accepted link speed and duplex, if any.
    #[must_use]
    pub const fn link(self) -> Option<(Speed, Duplex)> {
        self.link
    }

    /// The accepted lease, if any.
    #[must_use]
    pub const fn lease(self) -> Option<Lease> {
        self.lease
    }

    /// The accepted ping report, if any.
    #[must_use]
    pub const fn ping(self) -> Option<PingReport> {
        self.ping
    }

    /// Feed one debounced link event.
    ///
    /// `Up` and `Changed` are accepted only in [`Stage::Link`]; a `Down` event is rejected with
    /// [`ReleaseError::LinkDown`].
    pub fn link_event(&mut self, event: LinkEvent) -> Result<(), ReleaseError> {
        let (speed, duplex) = match event {
            LinkEvent::Down => return Err(ReleaseError::LinkDown),
            LinkEvent::Up { speed, duplex } | LinkEvent::Changed { speed, duplex } => {
                (speed, duplex)
            }
        };
        if self.stage != Stage::Link {
            return Err(ReleaseError::WrongStage {
                expected: Stage::Link,
                actual: self.stage,
            });
        }
        self.link = Some((speed, duplex));
        self.stage = Stage::Dhcp;
        Ok(())
    }

    /// Record the applied DHCP lease.
    pub fn dhcp_bound(&mut self, lease: Lease) -> Result<(), ReleaseError> {
        if self.stage != Stage::Dhcp {
            return Err(ReleaseError::WrongStage {
                expected: Stage::Dhcp,
                actual: self.stage,
            });
        }
        self.lease = Some(lease);
        self.stage = Stage::Ping;
        Ok(())
    }

    /// Record the gateway ping result.
    pub fn ping_reply(&mut self, report: PingReport) -> Result<(), ReleaseError> {
        if self.stage != Stage::Ping {
            return Err(ReleaseError::WrongStage {
                expected: Stage::Ping,
                actual: self.stage,
            });
        }
        let Some(lease) = self.lease else {
            return Err(ReleaseError::InvalidLease);
        };
        if report.target != lease.gateway {
            return Err(ReleaseError::TargetIsNotGateway {
                target: report.target,
                gateway: lease.gateway,
            });
        }
        if report.transmitted == 0 || report.received > report.transmitted {
            return Err(ReleaseError::InvalidPing {
                transmitted: report.transmitted,
                received: report.received,
            });
        }
        if report.received == 0 {
            return Err(ReleaseError::NoReply);
        }
        self.ping = Some(report);
        self.stage = Stage::Complete;
        Ok(())
    }

    /// The stable evidence transcript once every stage has been observed.
    #[must_use]
    pub fn transcript(self) -> Option<ReleaseTranscript> {
        let (speed, duplex) = self.link?;
        let lease = self.lease?;
        let ping = self.ping?;
        if self.stage != Stage::Complete {
            return None;
        }
        Some(ReleaseTranscript {
            speed,
            duplex,
            lease,
            ping,
        })
    }
}

impl Default for ReleaseGate {
    fn default() -> Self {
        Self::new()
    }
}

/// Numeric link speed in Mbit/s.
#[must_use]
pub const fn speed_mbps(speed: Speed) -> u16 {
    match speed {
        Speed::Ten => 10,
        Speed::Hundred => 100,
        Speed::Thousand => 1000,
    }
}

/// Lower-case duplex name used in the evidence line.
#[must_use]
pub const fn duplex_name(duplex: Duplex) -> &'static str {
    match duplex {
        Duplex::Half => "half",
        Duplex::Full => "full",
    }
}

fn usable_unicast(address: [u8; 4]) -> bool {
    let value = u32::from_be_bytes(address);
    value != 0 && value != u32::MAX && address[0] != 127 && (address[0] & 0xf0) != 0xe0
}

struct Ipv4([u8; 4]);

impl fmt::Display for Ipv4 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}.{}.{}.{}",
            self.0[0], self.0[1], self.0[2], self.0[3]
        )
    }
}

#[cfg(test)]
#[path = "release_ut.rs"]
mod tests;
