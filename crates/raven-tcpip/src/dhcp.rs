// SPDX-License-Identifier: 0BSD
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! DHCPv4 client (RFC 2131): a sans-IO state machine and a small message codec.
//!
//! The state machine (discover, request, renew, rebind, expiry, retry back-off and the T1/T2
//! derivation) is adapted from smoltcp's `socket::dhcpv4` (0BSD, tier A; see `PROVENANCE.toml`)
//! and rewritten around this crate's types and a millisecond clock. The caller owns the I/O: it
//! feeds received UDP payloads to [`Dhcp::on_packet`], asks [`Dhcp::poll`] what to transmit, and
//! applies [`Event`]s to the interface.

use alloc::vec::Vec;

use crate::Ipv4Addr;

/// UDP port DHCP clients listen on.
pub const CLIENT_PORT: u16 = 68;
/// UDP port DHCP servers listen on.
pub const SERVER_PORT: u16 = 67;

const MAGIC: [u8; 4] = [0x63, 0x82, 0x53, 0x63];
const FIXED_LEN: usize = 236;
const MIN_MESSAGE_LEN: usize = 300;
const MAX_DNS: usize = 3;
const DEFAULT_LEASE_MS: i64 = 120_000;

const OPT_PAD: u8 = 0;
const OPT_SUBNET_MASK: u8 = 1;
const OPT_ROUTER: u8 = 3;
const OPT_DNS: u8 = 6;
const OPT_REQUESTED_IP: u8 = 50;
const OPT_LEASE: u8 = 51;
const OPT_MESSAGE_TYPE: u8 = 53;
const OPT_SERVER_ID: u8 = 54;
const OPT_PARAMETER_LIST: u8 = 55;
const OPT_MAX_SIZE: u8 = 57;
const OPT_RENEW: u8 = 58;
const OPT_REBIND: u8 = 59;
const OPT_CLIENT_ID: u8 = 61;
const OPT_END: u8 = 255;

/// DHCP message type (option 53).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageType {
    /// DHCPDISCOVER.
    Discover,
    /// DHCPOFFER.
    Offer,
    /// DHCPREQUEST.
    Request,
    /// DHCPDECLINE.
    Decline,
    /// DHCPACK.
    Ack,
    /// DHCPNAK.
    Nak,
    /// DHCPRELEASE.
    Release,
}

impl MessageType {
    const fn code(self) -> u8 {
        match self {
            Self::Discover => 1,
            Self::Offer => 2,
            Self::Request => 3,
            Self::Decline => 4,
            Self::Ack => 5,
            Self::Nak => 6,
            Self::Release => 7,
        }
    }

    const fn from_code(code: u8) -> Option<Self> {
        Some(match code {
            1 => Self::Discover,
            2 => Self::Offer,
            3 => Self::Request,
            4 => Self::Decline,
            5 => Self::Ack,
            6 => Self::Nak,
            7 => Self::Release,
            _ => return None,
        })
    }
}

/// Timeouts and retry limits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RetryConfig {
    /// Wait between DISCOVER retransmissions, in milliseconds.
    pub discover_timeout_ms: i64,
    /// First REQUEST timeout; it doubles every two tries.
    pub initial_request_timeout_ms: i64,
    /// REQUEST tries before discovery restarts.
    pub request_retries: u16,
    /// Lower bound between renew or rebind retransmissions.
    pub min_renew_timeout_ms: i64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            discover_timeout_ms: 10_000,
            initial_request_timeout_ms: 5_000,
            request_retries: 5,
            min_renew_timeout_ms: 60_000,
        }
    }
}

/// Absolute lease timers, in the same millisecond epoch as [`Dhcp::poll`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DhcpTimers {
    /// When the client starts renewing (T1).
    pub renew_at: i64,
    /// When the client starts rebinding (T2).
    pub rebind_at: i64,
    /// When the lease expires.
    pub expires_at: i64,
}

/// An address lease and the parameters that came with it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lease {
    /// The leased address.
    pub address: Ipv4Addr,
    /// Prefix length from the subnet mask.
    pub prefix_len: u8,
    /// Default gateway, when the server gave one.
    pub router: Option<Ipv4Addr>,
    /// Up to three DNS servers.
    pub dns: Vec<Ipv4Addr>,
    /// Address the server answered from.
    pub server: Ipv4Addr,
    /// Server identifier to echo in requests.
    pub server_id: Ipv4Addr,
}

/// A change the interface must apply.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// A lease was acquired or its parameters changed.
    Configured(Lease),
    /// The lease expired or was refused; the interface address must be dropped.
    Deconfigured,
}

/// What the client wants transmitted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outgoing {
    /// DISCOVER or REQUEST.
    pub kind: MessageType,
    /// Transaction id.
    pub xid: u32,
    /// Client address field: the leased address when renewing or rebinding.
    pub ciaddr: Ipv4Addr,
    /// Requested-address option (selecting state).
    pub requested: Option<Ipv4Addr>,
    /// Server-identifier option (selecting state).
    pub server_id: Option<Ipv4Addr>,
    /// Unicast destination for a renewal, `None` for a broadcast.
    pub unicast_to: Option<Ipv4Addr>,
}

#[derive(Clone, Debug)]
enum State {
    Discovering {
        retry_at: i64,
    },
    Requesting {
        retry_at: i64,
        retry: u16,
        server: Ipv4Addr,
        server_id: Ipv4Addr,
        requested: Ipv4Addr,
    },
    Renewing {
        lease: Lease,
        renew_at: i64,
        rebind_at: i64,
        rebinding: bool,
        expires_at: i64,
    },
}

/// The DHCPv4 client state machine.
#[derive(Clone, Debug)]
pub struct Dhcp {
    mac: [u8; 6],
    state: State,
    xid: u32,
    rng: u64,
    retry: RetryConfig,
    events: Vec<Event>,
}

/// A parsed server reply.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Reply {
    kind: Option<MessageType>,
    xid: u32,
    your_ip: Ipv4Addr,
    subnet_mask: Option<Ipv4Addr>,
    router: Option<Ipv4Addr>,
    dns: Vec<Ipv4Addr>,
    server_id: Option<Ipv4Addr>,
    lease_s: Option<u32>,
    renew_s: Option<u32>,
    rebind_s: Option<u32>,
}

impl Dhcp {
    /// A client for hardware address `mac`; `seed` makes transaction ids reproducible.
    #[must_use]
    pub fn new(mac: [u8; 6], seed: u64) -> Self {
        Self {
            mac,
            state: State::Discovering { retry_at: 0 },
            xid: 0,
            rng: seed | 1,
            retry: RetryConfig::default(),
            events: Vec::new(),
        }
    }

    /// Replace the retry configuration.
    pub fn set_retry_config(&mut self, retry: RetryConfig) {
        self.retry = retry;
    }

    /// The client's hardware address.
    #[must_use]
    pub const fn mac(&self) -> [u8; 6] {
        self.mac
    }

    /// The active lease, once acquired.
    #[must_use]
    pub fn lease(&self) -> Option<&Lease> {
        match &self.state {
            State::Renewing { lease, .. } => Some(lease),
            _ => None,
        }
    }

    /// The active lease's absolute T1/T2/expiry timers, once acquired.
    #[must_use]
    pub fn timers(&self) -> Option<DhcpTimers> {
        match &self.state {
            State::Renewing {
                renew_at,
                rebind_at,
                expires_at,
                ..
            } => Some(DhcpTimers {
                renew_at: *renew_at,
                rebind_at: *rebind_at,
                expires_at: *expires_at,
            }),
            _ => None,
        }
    }

    /// Restart discovery, for example when the link comes back in another network.
    pub fn reset(&mut self) {
        if matches!(self.state, State::Renewing { .. }) {
            self.events.push(Event::Deconfigured);
        }
        self.state = State::Discovering { retry_at: 0 };
    }

    /// The next pending interface change.
    pub fn take_event(&mut self) -> Option<Event> {
        if self.events.is_empty() {
            None
        } else {
            Some(self.events.remove(0))
        }
    }

    /// The time the client next needs [`Self::poll`], in milliseconds.
    #[must_use]
    pub fn poll_at(&self) -> i64 {
        match &self.state {
            State::Discovering { retry_at } | State::Requesting { retry_at, .. } => *retry_at,
            State::Renewing {
                renew_at,
                rebind_at,
                rebinding,
                expires_at,
                ..
            } => {
                let next = if *rebinding {
                    *rebind_at
                } else {
                    (*renew_at).min(*rebind_at)
                };
                next.min(*expires_at)
            }
        }
    }

    fn next_xid(&mut self) -> u32 {
        let mut value = self.rng;
        value ^= value << 13;
        value ^= value >> 7;
        value ^= value << 17;
        self.rng = value;
        (value >> 16) as u32
    }

    /// Advance timers and return the message to transmit now, if any.
    pub fn poll(&mut self, now: i64) -> Option<Outgoing> {
        match self.state.clone() {
            State::Discovering { retry_at } => {
                if now < retry_at {
                    return None;
                }
                let xid = self.next_xid();
                self.xid = xid;
                self.state = State::Discovering {
                    retry_at: now.saturating_add(self.retry.discover_timeout_ms),
                };
                Some(Outgoing {
                    kind: MessageType::Discover,
                    xid,
                    ciaddr: Ipv4Addr::UNSPECIFIED,
                    requested: None,
                    server_id: None,
                    unicast_to: None,
                })
            }
            State::Requesting {
                retry_at,
                retry,
                server,
                server_id,
                requested,
            } => {
                if now < retry_at {
                    return None;
                }
                if retry >= self.retry.request_retries {
                    self.reset();
                    return None;
                }
                let _ = server;
                self.state = State::Requesting {
                    retry_at: now.saturating_add(
                        self.retry.initial_request_timeout_ms << u32::from(retry / 2),
                    ),
                    retry: retry + 1,
                    server,
                    server_id,
                    requested,
                };
                Some(Outgoing {
                    kind: MessageType::Request,
                    xid: self.xid,
                    ciaddr: Ipv4Addr::UNSPECIFIED,
                    requested: Some(requested),
                    server_id: Some(server_id),
                    unicast_to: None,
                })
            }
            State::Renewing {
                lease,
                renew_at,
                rebind_at,
                rebinding,
                expires_at,
            } => {
                if expires_at <= now {
                    self.reset();
                    return None;
                }
                if now < renew_at || (rebinding && now < rebind_at) {
                    return None;
                }
                let rebinding = rebinding || now >= rebind_at;
                let xid = self.next_xid();
                self.xid = xid;
                let min = self.retry.min_renew_timeout_ms;
                let (renew_at, rebind_at) = if rebinding {
                    (
                        renew_at,
                        now.saturating_add(min.max((expires_at - now) / 2)),
                    )
                } else {
                    (
                        now.saturating_add(min.max((rebind_at - now) / 2).min(rebind_at - now)),
                        rebind_at,
                    )
                };
                let out = Outgoing {
                    kind: MessageType::Request,
                    xid,
                    ciaddr: lease.address,
                    requested: None,
                    server_id: None,
                    unicast_to: if rebinding { None } else { Some(lease.server) },
                };
                self.state = State::Renewing {
                    lease,
                    renew_at,
                    rebind_at,
                    rebinding,
                    expires_at,
                };
                Some(out)
            }
        }
    }

    /// Process a UDP payload received on the client port from `source`.
    pub fn on_packet(&mut self, now: i64, source: Ipv4Addr, payload: &[u8]) {
        let Some(reply) = parse(payload, self.mac) else {
            return;
        };
        if reply.xid != self.xid {
            return;
        }
        let Some(server_id) = reply.server_id else {
            return;
        };
        match (self.state.clone(), reply.kind) {
            (State::Discovering { .. }, Some(MessageType::Offer)) => {
                if !is_unicast(reply.your_ip) {
                    return;
                }
                self.state = State::Requesting {
                    retry_at: now,
                    retry: 0,
                    server: source,
                    server_id,
                    requested: reply.your_ip,
                };
            }
            (State::Requesting { server, .. }, Some(MessageType::Ack)) => {
                if let Some((lease, renew_at, rebind_at, expires_at)) =
                    lease_from(now, &reply, server, server_id)
                {
                    self.events.push(Event::Configured(lease.clone()));
                    self.state = State::Renewing {
                        lease,
                        renew_at,
                        rebind_at,
                        rebinding: false,
                        expires_at,
                    };
                }
            }
            (State::Renewing { lease, .. }, Some(MessageType::Ack)) => {
                if let Some((fresh, renew_at, rebind_at, expires_at)) =
                    lease_from(now, &reply, lease.server, server_id)
                {
                    if fresh != lease {
                        self.events.push(Event::Configured(fresh.clone()));
                    }
                    self.state = State::Renewing {
                        lease: fresh,
                        renew_at,
                        rebind_at,
                        rebinding: false,
                        expires_at,
                    };
                }
            }
            (State::Requesting { .. }, Some(MessageType::Nak))
            | (State::Renewing { .. }, Some(MessageType::Nak)) => self.reset(),
            _ => {}
        }
    }
}

fn is_unicast(address: Ipv4Addr) -> bool {
    !address.is_unspecified() && !address.is_broadcast() && !address.is_multicast()
}

fn prefix_len(mask: Ipv4Addr) -> Option<u8> {
    let value = u32::from_be_bytes(mask.octets());
    let ones = value.leading_ones();
    (value.checked_shl(ones).unwrap_or(0) == 0).then(|| u8::try_from(ones).unwrap_or(32))
}

fn lease_from(
    now: i64,
    reply: &Reply,
    server: Ipv4Addr,
    server_id: Ipv4Addr,
) -> Option<(Lease, i64, i64, i64)> {
    let prefix = prefix_len(reply.subnet_mask?)?;
    if !is_unicast(reply.your_ip) {
        return None;
    }
    let lease_ms = reply
        .lease_s
        .map_or(DEFAULT_LEASE_MS, |seconds| i64::from(seconds) * 1000);
    let dns: Vec<Ipv4Addr> = reply
        .dns
        .iter()
        .copied()
        .filter(|address| is_unicast(*address))
        .take(MAX_DNS)
        .collect();
    let lease = Lease {
        address: reply.your_ip,
        prefix_len: prefix,
        router: reply.router,
        dns,
        server,
        server_id,
    };
    let t1 = reply.renew_s.map(|seconds| i64::from(seconds) * 1000);
    let t2 = reply.rebind_s.map(|seconds| i64::from(seconds) * 1000);
    // RFC 2131: T1 defaults to 0.5 and T2 to 0.875 of the lease; the order must be
    // T1 < T2 < lease, otherwise the defaults apply.
    let (renew, rebind) = match (t1, t2) {
        (Some(t1), Some(t2)) if t1 < t2 && t2 < lease_ms => (t1, t2),
        (Some(t1), None) if t1 < lease_ms => (t1, t1 + (lease_ms - t1) * 3 / 4),
        (None, Some(t2)) if t2 < lease_ms => ((lease_ms / 2).min(t2), t2),
        _ => (lease_ms / 2, lease_ms * 7 / 8),
    };
    Some((
        lease,
        now.saturating_add(renew),
        now.saturating_add(rebind),
        now.saturating_add(lease_ms),
    ))
}

/// Encode a client message, padded to the BOOTP minimum.
#[must_use]
pub fn encode(out: &Outgoing, mac: [u8; 6], mtu: u16) -> Vec<u8> {
    let mut message = alloc::vec![0u8; FIXED_LEN];
    message[0] = 1;
    message[1] = 1;
    message[2] = 6;
    message[4..8].copy_from_slice(&out.xid.to_be_bytes());
    // Ask for broadcast replies until an address is configured: the reply could not be
    // delivered to an address the interface does not have yet.
    if out.ciaddr.is_unspecified() {
        message[10] = 0x80;
    }
    message[12..16].copy_from_slice(&out.ciaddr.octets());
    message[28..34].copy_from_slice(&mac);
    message.extend_from_slice(&MAGIC);
    message.extend_from_slice(&[OPT_MESSAGE_TYPE, 1, out.kind.code()]);
    message.extend_from_slice(&[OPT_CLIENT_ID, 7, 1]);
    message.extend_from_slice(&mac);
    if let Some(requested) = out.requested {
        message.extend_from_slice(&[OPT_REQUESTED_IP, 4]);
        message.extend_from_slice(&requested.octets());
    }
    if let Some(server_id) = out.server_id {
        message.extend_from_slice(&[OPT_SERVER_ID, 4]);
        message.extend_from_slice(&server_id.octets());
    }
    message.extend_from_slice(&[OPT_PARAMETER_LIST, 3, OPT_SUBNET_MASK, OPT_ROUTER, OPT_DNS]);
    message.extend_from_slice(&[OPT_MAX_SIZE, 2]);
    message.extend_from_slice(&mtu.saturating_sub(28).to_be_bytes());
    message.push(OPT_END);
    if message.len() < MIN_MESSAGE_LEN {
        message.resize(MIN_MESSAGE_LEN, 0);
    }
    message
}

fn address_at(bytes: &[u8]) -> Option<Ipv4Addr> {
    let octets: [u8; 4] = bytes.get(..4)?.try_into().ok()?;
    Some(Ipv4Addr::new(octets))
}

fn parse(payload: &[u8], mac: [u8; 6]) -> Option<Reply> {
    if payload.len() < FIXED_LEN + MAGIC.len()
        || payload[0] != 2
        || payload[1] != 1
        || payload[2] != 6
        || payload[FIXED_LEN..FIXED_LEN + 4] != MAGIC
        || payload[28..34] != mac
    {
        return None;
    }
    let mut reply = Reply {
        xid: u32::from_be_bytes(payload[4..8].try_into().ok()?),
        your_ip: address_at(&payload[16..])?,
        ..Reply::default()
    };
    let mut options = &payload[FIXED_LEN + MAGIC.len()..];
    while let Some((&code, rest)) = options.split_first() {
        match code {
            OPT_PAD => {
                options = rest;
                continue;
            }
            OPT_END => break,
            _ => {}
        }
        let (&length, rest) = rest.split_first()?;
        let length = usize::from(length);
        let value = rest.get(..length)?;
        options = &rest[length..];
        match code {
            OPT_MESSAGE_TYPE if length == 1 => reply.kind = MessageType::from_code(value[0]),
            OPT_SUBNET_MASK if length == 4 => reply.subnet_mask = address_at(value),
            OPT_ROUTER if length >= 4 => reply.router = address_at(value),
            OPT_DNS => {
                reply.dns = value
                    .chunks_exact(4)
                    .filter_map(address_at)
                    .take(MAX_DNS)
                    .collect();
            }
            OPT_SERVER_ID if length == 4 => reply.server_id = address_at(value),
            OPT_LEASE if length == 4 => {
                reply.lease_s = Some(u32::from_be_bytes(value.try_into().ok()?));
            }
            OPT_RENEW if length == 4 => {
                reply.renew_s = Some(u32::from_be_bytes(value.try_into().ok()?));
            }
            OPT_REBIND if length == 4 => {
                reply.rebind_s = Some(u32::from_be_bytes(value.try_into().ok()?));
            }
            _ => {}
        }
    }
    Some(reply)
}

#[cfg(test)]
#[path = "dhcp_ut.rs"]
mod tests;
