// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// Ported from FreeBSD at donor revision 133344405f532bd54ff3629a77d4c4e062f01fdb:
//   sys/netinet/tcp_subr.c    tcp_keyed_hash(), tcp_new_ts_offset(), tcp_new_isn(),
//                             ISN_BYTES_PER_SECOND, ISN_STATIC_INCREMENT,
//                             ISN_RANDOM_INCREMENT, ISN_SECRET_LENGTH
//   sys/crypto/siphash/siphash.c  SipHash-2-4 reference behaviour (BSD-3-Clause,
//                             Copyright (c) 2013 Andre Oppermann)
//
// Copyright (c) 1982, 1986, 1993
//	The Regents of the University of California.  All rights reserved.
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions
// are met:
// 1. Redistributions of source code must retain the above copyright
//    notice, this list of conditions and the following disclaimer.
// 2. Redistributions in binary form must reproduce the above copyright
//    notice, this list of conditions and the following disclaimer in the
//    documentation and/or other materials provided with the distribution.
// 3. Neither the name of the University nor the names of its contributors
//    may be used to endorse or promote products derived from this software
//    without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE REGENTS AND CONTRIBUTORS ``AS IS'' AND
// ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED.  IN NO EVENT SHALL THE REGENTS OR CONTRIBUTORS BE LIABLE
// FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
// DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
// OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
// HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
// LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
// OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
// SUCH DAMAGE.

//! FreeBSD TCP initial sequence number and timestamp-offset generation.
//!
//! This is a sans-IO port of `sys/netinet/tcp_subr.c`: the caller supplies `ticks`, `hz`, the
//! 16-byte secrets and the per-connection random increment. There is no global mutable state and
//! no lock. `IsnState::next_isn` implements the RFC 1948/6528 keyed hash plus the monotonic
//! 1 MiB/s offset; `IsnState::ts_offset` implements `tcp_new_ts_offset`.
//!
//! SipHash-2-4 is implemented from the published specification and reference vectors, so the
//! crate needs no external dependency.

use crate::IpAddr;
use crate::seq::seq_gt;

/// FreeBSD `ISN_BYTES_PER_SECOND`: the monotonic offset rate.
pub const ISN_BYTES_PER_SECOND: u32 = 1_048_576;

/// FreeBSD `ISN_STATIC_INCREMENT`: the fixed offset added for every connection.
pub const ISN_STATIC_INCREMENT: u32 = 4_096;

/// FreeBSD `ISN_RANDOM_INCREMENT`: the per-connection random increment mask.
pub const ISN_RANDOM_INCREMENT: u32 = 4_095;

/// FreeBSD `ISN_SECRET_LENGTH` / `SIPHASH_KEY_LENGTH`: the SipHash key length.
pub const ISN_SECRET_LENGTH: usize = 16;

/// Maximum bytes in the keyed-hash input: two ports plus two IPv6 addresses.
const HASH_INPUT_MAX: usize = 2 + 2 + 16 + 16;

/// A TCP connection 4-tuple in FreeBSD `in_conninfo` field order.
///
/// The keyed hash covers the remote port, local port, remote address and local address, in that
/// order, exactly as `tcp_keyed_hash` updates the SipHash context.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IsnConn {
    /// Local address (`inc_laddr`).
    pub local: IpAddr,
    /// Local port (`inc_lport`), in host byte order.
    pub local_port: u16,
    /// Remote address (`inc_faddr`).
    pub remote: IpAddr,
    /// Remote port (`inc_fport`), in host byte order.
    pub remote_port: u16,
}

impl IsnConn {
    /// Build a 4-tuple.
    #[must_use]
    pub const fn new(local: IpAddr, local_port: u16, remote: IpAddr, remote_port: u16) -> Self {
        Self {
            local,
            local_port,
            remote,
            remote_port,
        }
    }

    /// The bytes hashed by `tcp_keyed_hash`, in FreeBSD order.
    fn hash_input(&self) -> ([u8; HASH_INPUT_MAX], usize) {
        let mut bytes = [0u8; HASH_INPUT_MAX];
        bytes[0..2].copy_from_slice(&self.remote_port.to_be_bytes());
        bytes[2..4].copy_from_slice(&self.local_port.to_be_bytes());
        let mut end = 4;
        match self.remote {
            IpAddr::V4(address) => {
                bytes[end..end + 4].copy_from_slice(&address.octets());
                end += 4;
            }
            IpAddr::V6(address) => {
                bytes[end..end + 16].copy_from_slice(&address.octets());
                end += 16;
            }
        }
        match self.local {
            IpAddr::V4(address) => {
                bytes[end..end + 4].copy_from_slice(&address.octets());
                end += 4;
            }
            IpAddr::V6(address) => {
                bytes[end..end + 16].copy_from_slice(&address.octets());
                end += 16;
            }
        }
        (bytes, end)
    }
}

/// Caller-supplied entropy for one `next_isn` call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IsnEntropy {
    /// The 16-byte secret used when a reseed is due.
    pub secret: [u8; ISN_SECRET_LENGTH],
    /// The `arc4random()` value; only the low 12 bits are used.
    pub increment: u32,
}

impl IsnEntropy {
    /// Build an entropy bundle.
    #[must_use]
    pub const fn new(secret: [u8; ISN_SECRET_LENGTH], increment: u32) -> Self {
        Self { secret, increment }
    }
}

/// The FreeBSD ISN generator state (`isn_secret`, `isn_last`, `isn_last_reseed`,
/// `isn_offset`, `isn_offset_old`), without globals or locks.
#[derive(Clone, Debug)]
pub struct IsnState {
    hz: u64,
    secret: [u8; ISN_SECRET_LENGTH],
    ts_secret: [u8; ISN_SECRET_LENGTH],
    last_reseed: u64,
    last_tick: u64,
    offset: u32,
    offset_old: u32,
    reseed_interval_secs: u64,
    ts_offset_per_conn: bool,
}

impl IsnState {
    /// Create the per-VNET state with an initial secret.
    ///
    /// `hz` is the tick rate (ticks per second); a zero value is clamped to one. The first
    /// `next_isn` call still performs FreeBSD's first-use reseed because `last_reseed` starts at
    /// zero, using the `IsnEntropy::secret` supplied by the caller.
    #[must_use]
    pub fn new(
        hz: u64,
        secret: [u8; ISN_SECRET_LENGTH],
        ts_secret: [u8; ISN_SECRET_LENGTH],
    ) -> Self {
        Self {
            hz: hz.max(1),
            secret,
            ts_secret,
            last_reseed: 0,
            last_tick: 0,
            offset: 0,
            offset_old: 0,
            reseed_interval_secs: 0,
            ts_offset_per_conn: true,
        }
    }

    /// Replace the ISN secret and mark the state reseeded at `ticks`.
    pub fn reseed(&mut self, ticks: u64, secret: [u8; ISN_SECRET_LENGTH]) {
        self.secret = secret;
        self.last_reseed = ticks;
    }

    /// FreeBSD `tcp_isn_reseed_interval`: seconds between reseeds, 0 disables them.
    pub fn set_reseed_interval_secs(&mut self, seconds: u64) {
        self.reseed_interval_secs = seconds;
    }

    /// FreeBSD `tcp_ts_offset_per_conn`: hash the full 4-tuple or zero the two ports.
    pub fn set_ts_offset_per_conn(&mut self, enabled: bool) {
        self.ts_offset_per_conn = enabled;
    }

    /// Whether FreeBSD `tcp_new_isn` would reseed on this call.
    #[must_use]
    pub fn reseed_due(&self, ticks: u64) -> bool {
        self.last_reseed == 0
            || (self.reseed_interval_secs > 0
                && self
                    .last_reseed
                    .saturating_add(self.reseed_interval_secs.saturating_mul(self.hz))
                    < ticks)
    }

    /// FreeBSD `tcp_new_isn`: keyed hash plus the monotonic offset.
    #[must_use]
    pub fn next_isn(&mut self, conn: &IsnConn, ticks: u64, entropy: IsnEntropy) -> u32 {
        if self.reseed_due(ticks) {
            self.secret = entropy.secret;
            self.last_reseed = ticks;
        }

        let mut new_isn = keyed_hash(conn, &self.secret);
        self.offset = self
            .offset
            .wrapping_add(ISN_STATIC_INCREMENT)
            .wrapping_add(entropy.increment & ISN_RANDOM_INCREMENT);
        if ticks != self.last_tick {
            let elapsed = ticks
                .saturating_sub(self.last_tick)
                .min(u64::from(u32::MAX));
            let per_tick = u64::from(ISN_BYTES_PER_SECOND) / self.hz;
            let projected = self
                .offset_old
                .wrapping_add(per_tick.saturating_mul(elapsed) as u32);
            if seq_gt(projected, self.offset) {
                self.offset = projected;
            }
            self.offset_old = self.offset;
            self.last_tick = ticks;
        }
        new_isn = new_isn.wrapping_add(self.offset);
        new_isn
    }

    /// FreeBSD `tcp_new_ts_offset`: keyed hash for the timestamp option offset.
    #[must_use]
    pub fn ts_offset(&self, conn: &IsnConn) -> u32 {
        if self.ts_offset_per_conn {
            keyed_hash(conn, &self.ts_secret)
        } else {
            let mut shared = *conn;
            shared.local_port = 0;
            shared.remote_port = 0;
            keyed_hash(&shared, &self.ts_secret)
        }
    }

    /// The current monotonic ISN offset.
    #[must_use]
    pub const fn offset(&self) -> u32 {
        self.offset
    }

    /// The tick of the last offset projection.
    #[must_use]
    pub const fn last_tick(&self) -> u64 {
        self.last_tick
    }

    /// The tick of the last secret reseed.
    #[must_use]
    pub const fn last_reseed(&self) -> u64 {
        self.last_reseed
    }
}

/// FreeBSD `tcp_keyed_hash`: SipHash-2-4 over the 4-tuple, folded to 32 bits.
fn keyed_hash(conn: &IsnConn, key: &[u8; ISN_SECRET_LENGTH]) -> u32 {
    let (bytes, len) = conn.hash_input();
    let hash = siphash24(key, &bytes[..len]);
    (hash as u32) ^ ((hash >> 32) as u32)
}

/// FreeBSD's keyed hash checks `len >= SIPHASH_KEY_LENGTH`; expose the same negative test hook.
#[cfg(test)]
fn keyed_hash_slice(conn: &IsnConn, key: &[u8]) -> Option<u32> {
    if key.len() < ISN_SECRET_LENGTH {
        return None;
    }
    let key: &[u8; ISN_SECRET_LENGTH] = key[..ISN_SECRET_LENGTH].try_into().ok()?;
    Some(keyed_hash(conn, key))
}

/// SipHash-2-4 with a 16-byte key and the 64-bit digest.
fn siphash24(key: &[u8; ISN_SECRET_LENGTH], message: &[u8]) -> u64 {
    let k0 = u64::from_le_bytes([
        key[0], key[1], key[2], key[3], key[4], key[5], key[6], key[7],
    ]);
    let k1 = u64::from_le_bytes([
        key[8], key[9], key[10], key[11], key[12], key[13], key[14], key[15],
    ]);
    let mut v = [
        k0 ^ 0x736f_6d65_7073_6575,
        k1 ^ 0x646f_7261_6e64_6f6d,
        k0 ^ 0x6c79_6765_6e65_7261,
        k1 ^ 0x7465_6462_7974_6573,
    ];

    let mut chunks = message.chunks_exact(8);
    for chunk in &mut chunks {
        let block = u64::from_le_bytes([
            chunk[0], chunk[1], chunk[2], chunk[3], chunk[4], chunk[5], chunk[6], chunk[7],
        ]);
        v[3] ^= block;
        sip_rounds(&mut v, 2);
        v[0] ^= block;
    }

    let tail = chunks.remainder();
    let mut last = (message.len() as u64 & 0xff) << 56;
    for (index, byte) in tail.iter().enumerate() {
        last |= u64::from(*byte) << (8 * index);
    }
    v[3] ^= last;
    sip_rounds(&mut v, 2);
    v[0] ^= last;
    v[2] ^= 0xff;
    sip_rounds(&mut v, 4);
    v[0] ^ v[1] ^ v[2] ^ v[3]
}

fn sip_rounds(v: &mut [u64; 4], rounds: usize) {
    for _ in 0..rounds {
        v[0] = v[0].wrapping_add(v[1]);
        v[1] = v[1].rotate_left(13);
        v[1] ^= v[0];
        v[0] = v[0].rotate_left(32);
        v[2] = v[2].wrapping_add(v[3]);
        v[3] = v[3].rotate_left(16);
        v[3] ^= v[2];
        v[0] = v[0].wrapping_add(v[3]);
        v[3] = v[3].rotate_left(21);
        v[3] ^= v[0];
        v[2] = v[2].wrapping_add(v[1]);
        v[1] = v[1].rotate_left(17);
        v[1] ^= v[2];
        v[2] = v[2].rotate_left(32);
    }
}

#[cfg(test)]
#[path = "isn_ut.rs"]
mod tests;
