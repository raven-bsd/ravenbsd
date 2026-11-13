// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The live DHCP lease as `resolvd` consumes it.
//!
//! `netd` owns the interface address, prefix, router and the RFC 2131 timers;
//! this module only turns the wire record into the DNS configuration and the
//! next refresh deadline. It never invents a lease field.

use alloc::vec::Vec;

use raven_proto_net::{DHCP_TIMER_UNKNOWN, DhcpLeaseInfo};

use crate::config::Config;

/// Refresh at least this often when the lease exposes no timer.
pub const DEFAULT_REFRESH_MS: u32 = 60_000;
/// Never schedule a refresh closer than this.
pub const MIN_REFRESH_MS: u32 = 100;
/// Safety margin subtracted from the earliest lease timer.
pub const MAX_MARGIN_MS: u32 = 1_000;

/// A live lease with its relative timers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lease {
    /// Leased address.
    pub addr: [u8; 4],
    /// Prefix length.
    pub prefix: u8,
    /// Default router.
    pub router: Option<[u8; 4]>,
    /// DNS servers, in server order.
    pub dns: Vec<[u8; 4]>,
    /// Milliseconds until renewal, or [`DHCP_TIMER_UNKNOWN`].
    pub renew_ms: u32,
    /// Milliseconds until rebinding, or [`DHCP_TIMER_UNKNOWN`].
    pub rebind_ms: u32,
    /// Milliseconds until expiry, or [`DHCP_TIMER_UNKNOWN`].
    pub expires_ms: u32,
}

impl From<DhcpLeaseInfo> for Lease {
    fn from(info: DhcpLeaseInfo) -> Self {
        Self {
            addr: info.addr,
            prefix: info.prefix,
            router: info.router,
            dns: info.dns,
            renew_ms: info.renew_ms,
            rebind_ms: info.rebind_ms,
            expires_ms: info.expires_ms,
        }
    }
}

impl Lease {
    /// The resolv.conf a lease produces.
    pub fn config(&self) -> Config {
        Config {
            servers: self.dns.clone(),
        }
    }

    /// Whether the lease carries at least one usable DNS server.
    pub fn has_dns(&self) -> bool {
        !self.dns.is_empty()
    }

    /// Whether two leases describe the same applied configuration.
    ///
    /// The timers are deliberately excluded: a refresh that only moves the
    /// timers must not rewrite the resolver configuration.
    pub fn same_config(&self, other: &Self) -> bool {
        self.addr == other.addr
            && self.prefix == other.prefix
            && self.router == other.router
            && self.dns == other.dns
    }

    /// Milliseconds until the next lease refresh.
    ///
    /// The earliest live timer wins, minus a small margin so the query runs
    /// before the stack changes state. An all-unknown lease falls back to
    /// [`DEFAULT_REFRESH_MS`].
    pub fn refresh_delay_ms(&self) -> u32 {
        let mut earliest: Option<u32> = None;
        for value in [self.renew_ms, self.rebind_ms, self.expires_ms] {
            if value == DHCP_TIMER_UNKNOWN {
                continue;
            }
            earliest = Some(match earliest {
                Some(current) => current.min(value),
                None => value,
            });
        }
        match earliest {
            Some(value) => {
                let margin = (value / 10).min(MAX_MARGIN_MS);
                value.saturating_sub(margin).max(MIN_REFRESH_MS)
            }
            None => DEFAULT_REFRESH_MS,
        }
    }
}

#[cfg(test)]
#[path = "lease_ut.rs"]
mod tests;
