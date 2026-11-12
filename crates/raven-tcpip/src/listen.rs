// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Listen-path queues, SYN cookies and RFC 6528 ISN generation.

use alloc::collections::VecDeque;

use crate::IpAddr;

/// A minimal half-open record kept instead of a full socket.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HalfOpen {
    /// Peer address.
    pub remote: IpAddr,
    /// Peer port.
    pub remote_port: u16,
    /// Peer initial sequence.
    pub irs: u32,
    /// Our initial sequence.
    pub iss: u32,
}

/// Counters for the listen path.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ListenStats {
    /// SYNs dropped because the half-open queue was full.
    pub half_open_drops: usize,
    /// ACKs dropped because the accept queue was full.
    pub accept_queue_drops: usize,
    /// SYNs dropped by the global listener limit.
    pub global_drops: usize,
    /// Invalid SYN-cookie ACKs.
    pub cookie_invalid: usize,
    /// Accepted connections abandoned before the application read them.
    pub abandoned: usize,
    /// Connections accepted through the cookie path.
    pub cookies_accepted: usize,
}

/// A bounded half-open queue.
#[derive(Clone, Debug)]
pub struct HalfOpenQueue {
    entries: VecDeque<HalfOpen>,
    capacity: usize,
    pub stats: ListenStats,
}

impl HalfOpenQueue {
    /// Create a queue.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            capacity: capacity.max(1),
            stats: ListenStats::default(),
        }
    }

    /// Add a half-open record; returns false and counts a drop when full.
    pub fn push(&mut self, entry: HalfOpen) -> bool {
        if self.entries.len() >= self.capacity {
            self.stats.half_open_drops = self.stats.half_open_drops.saturating_add(1);
            return false;
        }
        self.entries.push_back(entry);
        true
    }

    /// Remove a half-open entry by 4-tuple.
    pub fn remove(&mut self, remote: IpAddr, remote_port: u16) -> Option<HalfOpen> {
        let index = self
            .entries
            .iter()
            .position(|entry| entry.remote == remote && entry.remote_port == remote_port)?;
        self.entries.remove(index)
    }

    /// Number of half-open entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the queue is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// A bounded accept queue.
#[derive(Clone, Debug)]
pub struct AcceptQueue {
    entries: VecDeque<HalfOpen>,
    capacity: usize,
    pub stats: ListenStats,
}

impl AcceptQueue {
    /// Create a queue.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            capacity: capacity.max(1),
            stats: ListenStats::default(),
        }
    }

    /// Add a completed connection; returns false and counts a drop when full.
    pub fn push(&mut self, entry: HalfOpen) -> bool {
        if self.entries.len() >= self.capacity {
            self.stats.accept_queue_drops = self.stats.accept_queue_drops.saturating_add(1);
            return false;
        }
        self.entries.push_back(entry);
        true
    }

    /// Pop one completed connection.
    pub fn pop(&mut self) -> Option<HalfOpen> {
        self.entries.pop_front()
    }

    /// Number of completed connections.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the queue is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// RFC 6528-style ISN generator.
#[derive(Clone, Debug)]
pub struct IsnGenerator {
    secret: u64,
    deterministic: bool,
}

impl IsnGenerator {
    /// Create from a stack seed. Deterministic mode is for tests only.
    #[must_use]
    pub const fn new(seed: u64, deterministic: bool) -> Self {
        Self {
            secret: seed | 1,
            deterministic,
        }
    }

    /// Generate an ISN for a 4-tuple and clock value.
    #[must_use]
    pub fn generate(
        &self,
        remote: IpAddr,
        remote_port: u16,
        local_port: u16,
        clock_ms: u64,
    ) -> u32 {
        let mut hash = self.secret ^ 0x9e37_79b9_7f4a_7c15;
        hash = mix(hash ^ u64::from(remote_port));
        hash = mix(hash ^ u64::from(local_port));
        match remote {
            IpAddr::V4(address) => {
                for octet in address.octets() {
                    hash = mix(hash ^ u64::from(octet));
                }
            }
            IpAddr::V6(address) => {
                for octet in address.octets() {
                    hash = mix(hash ^ u64::from(octet));
                }
            }
        }
        let clock = if self.deterministic { 0 } else { clock_ms };
        let value = hash ^ clock.rotate_left(17);
        u32::try_from(value & 0xffff_ffff).unwrap_or(0)
    }

    /// Whether deterministic mode is active.
    #[must_use]
    pub const fn is_deterministic(&self) -> bool {
        self.deterministic
    }
}

fn mix(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

/// A rotating SYN-cookie encoder.
#[derive(Clone, Debug)]
pub struct SynCookie {
    secret: [u8; 16],
    rotation: u32,
}

impl SynCookie {
    /// Create from the stack seed.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        let mut secret = [0u8; 16];
        let mut state = seed | 1;
        for chunk in secret.chunks_exact_mut(8) {
            state = mix(state);
            chunk.copy_from_slice(&state.to_le_bytes());
        }
        Self {
            secret,
            rotation: 0,
        }
    }

    /// Rotate the secret, invalidating previous cookies.
    pub fn rotate(&mut self) {
        self.rotation = self.rotation.wrapping_add(1);
        for byte in &mut self.secret {
            *byte ^= (self.rotation & 0xff) as u8;
        }
    }

    /// Encode a cookie as an ISN.
    #[must_use]
    pub fn encode(&self, remote: IpAddr, remote_port: u16, local_port: u16, clock_ms: u64) -> u32 {
        let mut hash = u64::from(self.rotation);
        hash = mix(hash ^ u64::from(remote_port));
        hash = mix(hash ^ u64::from(local_port));
        let bytes = match remote {
            IpAddr::V4(address) => address.octets().to_vec(),
            IpAddr::V6(address) => address.octets().to_vec(),
        };
        for byte in bytes {
            hash = mix(hash ^ u64::from(byte));
        }
        for byte in self.secret {
            hash = mix(hash ^ u64::from(byte));
        }
        let time = u32::try_from(clock_ms / 60_000).unwrap_or(0);
        (hash as u32) ^ time
    }

    /// Validate a returned ACK cookie.
    #[must_use]
    pub fn validate(
        &self,
        cookie: u32,
        remote: IpAddr,
        remote_port: u16,
        local_port: u16,
        clock_ms: u64,
    ) -> bool {
        self.encode(remote, remote_port, local_port, clock_ms) == cookie
    }
}
