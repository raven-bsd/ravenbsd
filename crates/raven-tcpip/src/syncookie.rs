// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// Ported from FreeBSD at donor revision 133344405f532bd54ff3629a77d4c4e062f01fdb:
//   sys/netinet/tcp_syncache.c  syncookie_mac(), syncookie_generate(), syncookie_expand(),
//                               syncookie_cmp(), syncookie_reseed(), the syncookies/
//                               syncookies_only policy and the MSS/WSCALE tables
//   sys/netinet/tcp_syncache.h  SYNCOOKIE_SECRET_SIZE, SYNCOOKIE_LIFETIME, union syncookie
// BSD-3-Clause. The kernel callout, locks, VNET sysctls, socket/TCB pointers and mbuf state are
// replaced by explicit secrets, `secmod` and `now`; no globals, no locks, no stack wiring.

//! FreeBSD TCP SYN cookies (RFC 4987).
//!
//! A SYN cookie packs the per-connection state into the 32-bit ISS of the SYN|ACK. The top 24
//! bits are a SipHash-2-4 MAC over the 4-tuple, IRS, cookie flags and a caller-supplied `secmod`;
//! the low 8 bits carry the MSS index, window-scale index, SACK bit and odd/even secret bit.
//! `Syncookie::generate` produces the ISS and `Syncookie::expand` authenticates the returning ACK
//! and reconstructs the MSS/wscale/SACK/window values.
//!
//! The module is sans-IO: the caller supplies both 16-byte rotating secrets, the `secmod` context
//! (FreeBSD passes the syncache-head pointer), the current time and the ACK/SEQ/window values.

use crate::IpAddr;

/// FreeBSD `SYNCOOKIE_SECRET_SIZE`.
pub const SYNCOOKIE_SECRET_SIZE: usize = 16;
/// FreeBSD `SYNCOOKIE_LIFETIME` in milliseconds (15 s).
pub const SYNCOOKIE_LIFETIME_MS: u64 = 15_000;
/// FreeBSD `tcp_sc_msstab`: the encoded MSS table.
pub const MSS_TABLE: [u16; 8] = [216, 536, 1_200, 1_360, 1_400, 1_440, 1_452, 1_460];
/// FreeBSD `tcp_sc_wstab`: the encoded window-scale table.
pub const WSCALE_TABLE: [u8; 8] = [0, 0, 1, 2, 4, 6, 7, 8];
/// The largest TCP window (FreeBSD `TCP_MAXWIN`).
pub const TCP_MAX_WINDOW: u16 = 65_535;

const MAC_INPUT_MAX: usize = 16 + 16 + 2 + 2 + 4 + 1 + 8;

/// A connection 4-tuple hashed into a SYN cookie.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CookieConn {
    /// Local address (`inc_laddr`).
    pub local: IpAddr,
    /// Local port (`inc_lport`).
    pub local_port: u16,
    /// Remote address (`inc_faddr`).
    pub remote: IpAddr,
    /// Remote port (`inc_fport`).
    pub remote_port: u16,
}

impl CookieConn {
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
}

/// The SYN options encoded into the cookie.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CookieRequest {
    /// Peer MSS (`sc_peer_mss`).
    pub peer_mss: u16,
    /// The peer's window scale, or `None` when WSCALE was absent.
    pub requested_s_scale: Option<u8>,
    /// SACK was permitted in the SYN.
    pub sack_permitted: bool,
}

/// The ACK that returns for a SYN|ACK cookie.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CookieAck {
    /// The ACK number (`th_ack`), one past the cookie ISS.
    pub ack: u32,
    /// The sequence number (`th_seq`), one past the client IRS.
    pub seq: u32,
    /// The advertised receive window.
    pub window: u16,
}

/// The state reconstructed from a valid cookie.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExpandedCookie {
    /// Encoded peer MSS (the table value, rounded down).
    pub peer_mss: u16,
    /// Encoded window scale, or `None` when the cookie has no WSCALE.
    pub requested_s_scale: Option<u8>,
    /// Encoded SACK permission.
    pub sack_permitted: bool,
    /// The ACK window carried through the expansion.
    pub window: u16,
    /// The client IRS (`ack`/`seq` minus one).
    pub irs: u32,
    /// The cookie ISS.
    pub iss: u32,
}

/// The syncache values that `syncookie_cmp` compares against the expanded cookie.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SyncacheSnapshot {
    /// Original peer MSS.
    pub peer_mss: u16,
    /// Original window scale.
    pub requested_s_scale: Option<u8>,
    /// Original SACK permission.
    pub sack_permitted: bool,
}

/// FreeBSD `syncookies`/`syncookies_only` policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SyncookiePolicy {
    /// `net.inet.tcp.syncookies`: use cookies when the syncache overflows.
    pub enabled: bool,
    /// `net.inet.tcp.syncookies_only`: never store a syncache entry.
    pub only: bool,
}

impl Default for SyncookiePolicy {
    fn default() -> Self {
        Self {
            enabled: true,
            only: false,
        }
    }
}

impl SyncookiePolicy {
    /// FreeBSD `tcp_syncookies || tcp_syncookies_only` on a paused/overflowed syncache.
    #[must_use]
    pub const fn cookie_on_overflow(&self, syncache_paused: bool) -> bool {
        self.only || (self.enabled && syncache_paused)
    }

    /// Whether the syncache may store an entry (false in `syncookies_only` mode).
    #[must_use]
    pub const fn syncache_allowed(&self) -> bool {
        !self.only
    }
}

/// The two rotating 16-byte secrets and the current odd/even bit.
#[derive(Clone, Debug)]
pub struct SyncookieSecret {
    keys: [[u8; SYNCOOKIE_SECRET_SIZE]; 2],
    oddeven: u8,
}

impl SyncookieSecret {
    /// Create the two secrets with the even key active.
    #[must_use]
    pub const fn new(even: [u8; SYNCOOKIE_SECRET_SIZE], odd: [u8; SYNCOOKIE_SECRET_SIZE]) -> Self {
        Self {
            keys: [even, odd],
            oddeven: 0,
        }
    }

    /// The current parity bit.
    #[must_use]
    pub const fn parity(&self) -> u8 {
        self.oddeven & 1
    }

    /// The active key.
    #[must_use]
    pub fn key(&self) -> &[u8; SYNCOOKIE_SECRET_SIZE] {
        self.key_for_parity(self.parity())
    }

    /// The key selected by a cookie's odd/even bit.
    #[must_use]
    pub fn key_for_parity(&self, parity: u8) -> &[u8; SYNCOOKIE_SECRET_SIZE] {
        &self.keys[usize::from(parity & 1)]
    }

    /// Replace the inactive key, then flip the parity, as `syncookie_reseed` does.
    pub fn reseed(&mut self, key: [u8; SYNCOOKIE_SECRET_SIZE]) {
        let next = 1 - self.parity();
        self.keys[usize::from(next)] = key;
        self.oddeven = self.oddeven.wrapping_add(1);
    }
}

/// A sans-IO SYN cookie generator/verifier.
#[derive(Clone, Debug)]
pub struct Syncookie {
    secret: SyncookieSecret,
    last_reseed_ms: u64,
    reseed_interval_ms: u64,
}

impl Syncookie {
    /// Create with two initial secrets and the 15 s reseed interval.
    #[must_use]
    pub fn new(
        now_ms: u64,
        even: [u8; SYNCOOKIE_SECRET_SIZE],
        odd: [u8; SYNCOOKIE_SECRET_SIZE],
    ) -> Self {
        Self::with_interval(now_ms, even, odd, SYNCOOKIE_LIFETIME_MS)
    }

    /// Create with an explicit reseed interval.
    #[must_use]
    pub fn with_interval(
        now_ms: u64,
        even: [u8; SYNCOOKIE_SECRET_SIZE],
        odd: [u8; SYNCOOKIE_SECRET_SIZE],
        reseed_interval_ms: u64,
    ) -> Self {
        Self {
            secret: SyncookieSecret::new(even, odd),
            last_reseed_ms: now_ms,
            reseed_interval_ms,
        }
    }

    /// The rotating secrets.
    #[must_use]
    pub fn secret(&self) -> &SyncookieSecret {
        &self.secret
    }

    /// Whether the FreeBSD callout would reseed at `now`.
    #[must_use]
    pub fn reseed_due(&self, now_ms: u64) -> bool {
        now_ms >= self.last_reseed_ms.saturating_add(self.reseed_interval_ms)
    }

    /// Rotate the inactive secret and mark the reseed at `now`.
    pub fn reseed(&mut self, now_ms: u64, key: [u8; SYNCOOKIE_SECRET_SIZE]) {
        self.secret.reseed(key);
        self.last_reseed_ms = now_ms;
    }

    /// FreeBSD `syncookie_generate`: build the 32-bit ISS for a SYN|ACK.
    #[must_use]
    pub fn generate(
        &self,
        conn: &CookieConn,
        irs: u32,
        request: &CookieRequest,
        secmod: u64,
    ) -> u32 {
        let mss_idx = mss_index(request.peer_mss);
        let wscale_idx = request.requested_s_scale.map_or(0, wscale_index);
        let cookie = (mss_idx << 5)
            | (wscale_idx << 2)
            | (u8::from(request.sack_permitted) << 1)
            | self.secret.parity();
        let hash = mac(conn, irs, cookie, self.secret.key(), secmod);
        let low = cookie ^ (hash >> 24) as u8;
        (hash & !0xff) | u32::from(low)
    }

    /// FreeBSD `syncookie_expand`: authenticate the returning ACK.
    ///
    /// Returns `None` when the MAC does not cover the ACK/SEQ, when the tuple differs or when the
    /// encoded cookie is not authenticated by the selected secret. A zero window is legal; the
    /// window is carried through unchanged.
    #[must_use]
    pub fn expand(
        &self,
        conn: &CookieConn,
        ack: &CookieAck,
        secmod: u64,
    ) -> Option<ExpandedCookie> {
        let acknowledged = ack.ack.wrapping_sub(1);
        let sequence = ack.seq.wrapping_sub(1);
        let cookie = (acknowledged as u8) ^ ((acknowledged >> 24) as u8);
        let key = self.secret.key_for_parity(cookie & 1);
        let hash = mac(conn, sequence, cookie, key, secmod);
        if (acknowledged & !0xff) != (hash & !0xff) {
            return None;
        }

        let mss_idx = usize::from((cookie >> 5) & 7);
        let wscale_idx = usize::from((cookie >> 2) & 7);
        Some(ExpandedCookie {
            peer_mss: MSS_TABLE[mss_idx],
            requested_s_scale: (wscale_idx > 0).then(|| WSCALE_TABLE[wscale_idx]),
            sack_permitted: cookie & 0b10 != 0,
            window: ack.window,
            irs: sequence,
            iss: acknowledged,
        })
    }
}

/// FreeBSD `syncookie_cmp`: compare the reconstructed and stored syncache values.
#[must_use]
pub fn syncookie_cmp(expanded: &ExpandedCookie, syncache: &SyncacheSnapshot) -> bool {
    expanded.peer_mss == syncache.peer_mss
        && expanded.requested_s_scale == syncache.requested_s_scale
        && expanded.sack_permitted == syncache.sack_permitted
}

fn mss_index(mss: u16) -> u8 {
    let mut index = MSS_TABLE.len() - 1;
    while index > 0 && MSS_TABLE[index] > mss {
        index -= 1;
    }
    u8::try_from(index).unwrap_or(0)
}

fn wscale_index(scale: u8) -> u8 {
    let mut index = WSCALE_TABLE.len() - 1;
    while index > 0 && WSCALE_TABLE[index] > scale {
        index -= 1;
    }
    u8::try_from(index).unwrap_or(0)
}

fn mac(
    conn: &CookieConn,
    irs: u32,
    cookie: u8,
    key: &[u8; SYNCOOKIE_SECRET_SIZE],
    secmod: u64,
) -> u32 {
    let mut bytes = [0u8; MAC_INPUT_MAX];
    let mut end = 0;
    match conn.remote {
        IpAddr::V4(address) => {
            bytes[end..end + 4].copy_from_slice(&address.octets());
            end += 4;
        }
        IpAddr::V6(address) => {
            bytes[end..end + 16].copy_from_slice(&address.octets());
            end += 16;
        }
    }
    match conn.local {
        IpAddr::V4(address) => {
            bytes[end..end + 4].copy_from_slice(&address.octets());
            end += 4;
        }
        IpAddr::V6(address) => {
            bytes[end..end + 16].copy_from_slice(&address.octets());
            end += 16;
        }
    }
    bytes[end..end + 2].copy_from_slice(&conn.remote_port.to_be_bytes());
    end += 2;
    bytes[end..end + 2].copy_from_slice(&conn.local_port.to_be_bytes());
    end += 2;
    bytes[end..end + 4].copy_from_slice(&irs.to_le_bytes());
    end += 4;
    bytes[end] = cookie;
    end += 1;
    bytes[end..end + 8].copy_from_slice(&secmod.to_le_bytes());
    end += 8;

    let hash = siphash24(key, &bytes[..end]);
    (hash as u32) ^ ((hash >> 32) as u32)
}

// SipHash-2-4, kept local so K304 stays inside its Owns (isn.rs belongs to K278).
fn siphash24(key: &[u8; SYNCOOKIE_SECRET_SIZE], message: &[u8]) -> u64 {
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
#[path = "syncookie_ut.rs"]
mod tests;
