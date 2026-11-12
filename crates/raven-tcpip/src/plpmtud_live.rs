// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Live PLPMTUD (RFC 4821 / RFC 8899) for TCP, driven by the send path and the stack timer.
//!
//! The search runs in payload-size space: the base is the segment payload the connection
//! already uses, the ceiling is what the interface and the peer allow. A probe is an ordinary
//! data segment larger than the current payload that the send path emits in place of a normal
//! segment when the window, the congestion window and the queue can carry it. An ACK raises
//! the payload size; a retransmission timeout while a probe is outstanding lowers the search
//! ceiling and is not treated as congestion.

use crate::Instant;
use crate::pmtu::{Plpmtud, PlpmtudState};
use crate::seq::{seq_ge, seq_gt, seq_lt};
use crate::tcp_state::{TcpConnection, TcpState};

/// Most probes in one search.
const PROBE_BUDGET: usize = 24;

/// Losses of one probe size before it is declared too large (RFC 8899 MAX_PROBES).
const PROBE_ATTEMPTS: u8 = 3;

/// Wait before retrying a probe that the window, congestion window or queue could not carry.
const RETRY_MS: i64 = 200;

/// Consecutive timeouts of full-size segments that suggest a black hole.
pub(crate) const BLACKHOLE_TIMEOUTS: u8 = 3;

/// The payload ceiling the search may probe up to.
fn ceiling(connection: &TcpConnection) -> u32 {
    let overhead = if connection.timestamps { 12 } else { 0 };
    u32::from(
        connection
            .plp_ceiling
            .min(connection.peer_mss)
            .saturating_sub(overhead),
    )
}

/// The probe payload to send instead of a normal segment now, if one is due.
///
/// `room` is the sendable byte count (window, congestion window and queued data combined).
/// On `Some((size, reprobe))` the probe is recorded as outstanding at `snd_nxt + size`;
/// `reprobe` is set when the probe opens a periodic re-search.
pub(crate) fn probe_due(
    connection: &mut TcpConnection,
    now_ms: i64,
    interval: i64,
    room: u32,
) -> Option<(u32, bool)> {
    if connection.state != TcpState::Established || connection.close_requested {
        return None;
    }
    let top = ceiling(connection);
    if !connection.plp_decided {
        connection.plp_decided = true;
        if u32::from(connection.mss) < top {
            let mut search = Plpmtud::new(u32::from(connection.mss), top, PROBE_BUDGET);
            search.start();
            connection.plp = Some(search);
            connection.plp_next_ms = now_ms;
        }
    }
    if connection.plp_probe.is_some() {
        return None;
    }
    let mut reprobe = false;
    let search = connection.plp.as_mut()?;
    match search.state {
        PlpmtudState::SearchComplete => {
            if search.current >= top
                || !search.reprobe_due(connection.plp_next_ms, now_ms, interval)
            {
                return None;
            }
            search.start();
            reprobe = true;
        }
        PlpmtudState::Searching if now_ms >= connection.plp_next_ms => {}
        _ => return None,
    }
    if room < top {
        connection.plp_next_ms = now_ms.saturating_add(RETRY_MS);
        return None;
    }
    let Some(size) = search.next_probe() else {
        if search.state == PlpmtudState::Error {
            connection.plp_next_ms = i64::MAX;
        } else {
            connection.plp_next_ms = now_ms;
            connection.mss = u16::try_from(search.current).unwrap_or(connection.mss);
        }
        return None;
    };
    connection.plp_probe = Some((connection.snd_nxt.wrapping_add(size), size));
    connection.plp_next_ms = now_ms;
    Some((size, reprobe))
}

/// When the stack next needs to look at this connection's path-MTU search.
pub(crate) fn next_deadline(
    connection: &TcpConnection,
    interval: i64,
    enabled: bool,
) -> Option<Instant> {
    if !enabled || connection.state != TcpState::Established || connection.plp_probe.is_some() {
        return None;
    }
    let search = connection.plp.as_ref()?;
    let at = match search.state {
        PlpmtudState::Searching => connection.plp_next_ms,
        PlpmtudState::SearchComplete if u32::from(connection.mss) < ceiling(connection) => {
            connection.plp_next_ms.saturating_add(interval)
        }
        _ => return None,
    };
    Some(Instant::from_millis(at))
}

/// Account an acknowledged probe: raise the segment payload to the confirmed size.
pub(crate) fn plpmtud_on_ack(connection: &mut TcpConnection, ack: u32, now_ms: i64) {
    let Some((end, size)) = connection.plp_probe else {
        return;
    };
    if !seq_ge(ack, end) {
        return;
    }
    connection.plp_probe = None;
    connection.plp_attempts = 0;
    connection.plp_next_ms = now_ms;
    if let Some(search) = &mut connection.plp {
        search.on_probe_ack(size);
        connection.mss = u16::try_from(search.current).unwrap_or(connection.mss);
    }
}

/// A retransmission of `[seq, seq + len)`: if it covers the probe, the probe was lost.
///
/// An ACK that follows must not be taken as proof that the probe size passes.
pub(crate) fn plpmtud_on_retransmit(connection: &mut TcpConnection, seq: u32, len: u32) -> bool {
    let Some((end, size)) = connection.plp_probe else {
        return false;
    };
    let start = end.wrapping_sub(size);
    let stop = seq.wrapping_add(len);
    if !seq_lt(seq, end) || !seq_gt(stop, start) {
        return false;
    }
    connection.plp_probe = None;
    probe_failed(connection, size);
    true
}

/// Count a lost probe; the size is declared too large only after repeated losses, so random
/// loss does not shrink the search.
fn probe_failed(connection: &mut TcpConnection, size: u32) {
    connection.plp_attempts = connection.plp_attempts.saturating_add(1);
    if connection.plp_attempts < PROBE_ATTEMPTS {
        return;
    }
    connection.plp_attempts = 0;
    if let Some(search) = &mut connection.plp {
        search.on_probe_loss(size);
    }
}

/// A timeout while a probe is outstanding: shrink the ceiling and report whether it was a probe.
///
/// Everything in flight is sent again as new data, so the lost probe is replaced by segments
/// of the current, smaller payload.
pub(crate) fn plpmtud_on_timeout(connection: &mut TcpConnection, now_ms: i64) -> bool {
    let Some((end, size)) = connection.plp_probe else {
        return false;
    };
    connection.plp_probe = None;
    if seq_ge(connection.snd_una, end) {
        return false;
    }
    connection.plp_next_ms = now_ms;
    probe_failed(connection, size);
    connection.snd_nxt = connection.snd_una;
    connection.rto_deadline = None;
    connection.sacked.clear();
    connection.sack_recovery = false;
    connection.rto_recovery = false;
    connection.dupack_count = 0;
    true
}

/// Repeated timeouts of full-size segments: fall back to the floor and probe upward again.
pub(crate) fn plpmtud_blackhole(connection: &mut TcpConnection, now_ms: i64) -> bool {
    let floor = connection.mss_floor;
    if connection.mss <= floor || connection.plp.is_some() {
        return false;
    }
    let mut search = Plpmtud::new(u32::from(floor), u32::from(connection.mss), PROBE_BUDGET);
    search.start();
    connection.plp = Some(search);
    connection.plp_decided = true;
    connection.plp_next_ms = now_ms;
    connection.mss = floor;
    true
}

#[cfg(test)]
#[path = "plpmtud_live_ut.rs"]
mod tests;
