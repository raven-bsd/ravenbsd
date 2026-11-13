// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! A/AAAA resolver loop with bounded timeouts, one retry and CNAME following.

use alloc::string::String;
use alloc::vec::Vec;

use raven_abi::errno::Errno;

use crate::dns::{self, Address, Answer, DnsError, TYPE_A, TYPE_AAAA};

/// A DNS exchange transport.
pub trait Exchange {
    /// Send one UDP query and return the response length.
    fn query(
        &mut self,
        server: [u8; 4],
        request: &[u8],
        response: &mut [u8],
        timeout_ms: u32,
    ) -> Result<usize, Errno>;
}

/// Why a name could not be resolved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResolveError {
    /// The name does not exist.
    NotFound,
    /// The server answered without the requested address.
    NoData,
    /// Every server and retry timed out.
    Timeout,
    /// A response was malformed.
    Proto,
    /// The transport failed.
    Io(Errno),
}

impl ResolveError {
    /// Map to the service errno.
    pub const fn errno(self) -> Errno {
        match self {
            Self::NotFound => Errno::ENOENT,
            Self::NoData | Self::Proto => Errno::EPROTO,
            Self::Timeout => Errno::ETIMEDOUT,
            Self::Io(error) => error,
        }
    }
}

/// A resolver over a configured server list.
pub struct Resolver {
    servers: Vec<[u8; 4]>,
    timeout_ms: u32,
    attempts: u8,
    next_id: u16,
}

impl Resolver {
    /// Create a resolver.  `attempts` is clamped to at least one.
    pub fn new(servers: Vec<[u8; 4]>, timeout_ms: u32, attempts: u8) -> Self {
        Self {
            servers,
            timeout_ms,
            attempts: attempts.max(1),
            next_id: 1,
        }
    }

    /// Configured servers.
    pub fn servers(&self) -> &[[u8; 4]] {
        &self.servers
    }

    /// Resolve `name` for `qtype` (A or AAAA).
    pub fn resolve(
        &mut self,
        exchange: &mut dyn Exchange,
        name: &str,
        qtype: u16,
    ) -> Result<Vec<(u32, Address)>, ResolveError> {
        if !matches!(qtype, TYPE_A | TYPE_AAAA) {
            return Err(ResolveError::NoData);
        }
        let servers = self.servers.clone();
        if servers.is_empty() {
            return Err(ResolveError::NoData);
        }
        let mut current = String::from(name);
        let mut seen: Vec<String> = Vec::new();
        for _ in 0..8 {
            let key = current.to_ascii_lowercase();
            if seen.iter().any(|item| item == &key) {
                return Err(ResolveError::Proto);
            }
            seen.push(key);
            let id = self.next_id;
            self.next_id = self.next_id.wrapping_add(1).max(1);
            let request = dns::build_query(id, &current, qtype).map_err(|_| ResolveError::Proto)?;
            let mut last = ResolveError::NoData;
            let mut follow: Option<String> = None;
            let mut answers: Vec<(u32, Address)> = Vec::new();
            'servers: for server in &servers {
                for _ in 0..self.attempts {
                    let mut response = [0u8; dns::MAX_PACKET];
                    match exchange.query(*server, &request, &mut response, self.timeout_ms) {
                        Ok(length) => match dns::parse_response(&response[..length], id) {
                            Ok(parsed) => {
                                if parsed.rcode == 3 {
                                    return Err(ResolveError::NotFound);
                                }
                                for answer in &parsed.answers {
                                    match answer {
                                        Answer::Address { ttl, address }
                                            if matches!(
                                                (qtype, address),
                                                (TYPE_A, Address::V4(_))
                                                    | (TYPE_AAAA, Address::V6(_))
                                            ) =>
                                        {
                                            answers.push((*ttl, *address));
                                        }
                                        Answer::Cname(target) => {
                                            follow = Some(target.clone());
                                        }
                                        _ => {}
                                    }
                                }
                                if !answers.is_empty() {
                                    return Ok(answers);
                                }
                                if follow.is_some() {
                                    break 'servers;
                                }
                                last = ResolveError::NoData;
                            }
                            Err(DnsError::ServerError(_)) => {
                                last = ResolveError::NoData;
                                break;
                            }
                            Err(_) => {
                                last = ResolveError::Proto;
                                break;
                            }
                        },
                        Err(Errno::ETIMEDOUT | Errno::EAGAIN) => {
                            last = ResolveError::Timeout;
                        }
                        Err(error) => {
                            last = ResolveError::Io(error);
                            break;
                        }
                    }
                }
            }
            if answers.is_empty() {
                if let Some(target) = follow {
                    current = target;
                    continue;
                }
                return Err(last);
            }
            return Ok(answers);
        }
        Err(ResolveError::Proto)
    }
}

#[cfg(test)]
#[path = "resolver_ut.rs"]
mod tests;
