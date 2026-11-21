// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Bounded packet logging.
//!
//! Donor: `PFLOG_PACKET` and `pf_log_matches()` (`sys/netpfil/pf/pf.c:11628`), with the
//! `PF_LOG_FORCE` path for fail-closed drops. The donor writes to the pflog interface; the
//! 0.1.0 core keeps the same record in a bounded ring so a host test can assert it, and
//! `netd` can drain it to the log device (see `docs/pf.md`).

use alloc::string::String;

use crate::address::IpAddr;
use crate::ast::{Action, Direction, Packet, Protocol};

/// Largest number of retained log records.
pub const MAX_LOG_ENTRIES: usize = 256;

/// One packet-filter log record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogEntry {
    /// Monotonic time in seconds.
    pub time: u64,
    /// Packet direction.
    pub direction: Direction,
    /// Verdict.
    pub action: Action,
    /// Matching rule, when any.
    pub rule: Option<usize>,
    /// `PFRES_*` reason, when the packet was dropped by a failure.
    pub reason: Option<usize>,
    /// Interface name.
    pub interface: String,
    /// Transport protocol.
    pub protocol: Protocol,
    /// Source address.
    pub source: IpAddr,
    /// Source port.
    pub source_port: u16,
    /// Destination address.
    pub destination: IpAddr,
    /// Destination port.
    pub destination_port: u16,
    /// The packet was an IP fragment.
    pub fragment: bool,
}

impl LogEntry {
    /// Build a record from a packet.
    #[must_use]
    pub fn from_packet(
        packet: &Packet<'_>,
        action: Action,
        rule: Option<usize>,
        reason: Option<usize>,
        time: u64,
    ) -> Self {
        Self {
            time,
            direction: packet.direction,
            action,
            rule,
            reason,
            interface: String::from(packet.interface),
            protocol: packet.protocol,
            source: packet.source,
            source_port: packet.source_port,
            destination: packet.destination,
            destination_port: packet.destination_port,
            fragment: packet.fragment,
        }
    }
}

/// A bounded log ring; the oldest record is dropped at [`MAX_LOG_ENTRIES`].
#[derive(Clone, Debug, Default)]
pub struct PacketLog {
    entries: alloc::vec::Vec<LogEntry>,
}

impl PacketLog {
    /// An empty log.
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: alloc::vec::Vec::new(),
        }
    }

    /// Append a record, dropping the oldest at the bound.
    pub fn push(&mut self, entry: LogEntry) {
        if self.entries.len() >= MAX_LOG_ENTRIES {
            self.entries.remove(0);
        }
        self.entries.push(entry);
    }

    /// Records in time order.
    #[must_use]
    pub fn entries(&self) -> &[LogEntry] {
        &self.entries
    }

    /// Number of retained records.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when nothing is retained.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Take every record.
    pub fn drain(&mut self) -> alloc::vec::Vec<LogEntry> {
        core::mem::take(&mut self.entries)
    }
}

#[cfg(test)]
#[path = "log_ut.rs"]
mod tests;
