// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Sender-side SACK loss recovery (RFC 6675) driven by the live ACK path.
//!
//! The receiver already reports SACK blocks; this module keeps the sender's view of them as
//! absolute sequence ranges, builds the [`SackScoreboard`] on demand and retransmits every hole
//! the scoreboard calls lost while the congestion window leaves room, instead of one segment
//! per round trip.

use alloc::vec::Vec;

use raven_tcpip_wire::tcp::TcpFlags;

use crate::device::Device;
use crate::sack::{NextKind, SackScoreboard};
use crate::seq::{seq_ge, seq_gt, seq_lt};
use crate::socket::Kind;
use crate::tcp_data::advertised_window;
use crate::tcp_state::TcpConnection;
use crate::{SocketId, Stack};

/// Most SACKed ranges the sender remembers.
const MAX_SACKED_RANGES: usize = 64;

/// Most retransmissions one ACK may trigger.
const MAX_BURST: usize = 64;

/// Merge reported SACK blocks into the connection's ranges.
///
/// Returns whether bytes that were not already SACKed are now covered. Blocks at or below
/// `snd_una` (DSACK) and blocks beyond `snd_nxt` are ignored.
pub(crate) fn merge_sack_blocks(connection: &mut TcpConnection, blocks: &[(u32, u32)]) -> bool {
    if blocks.is_empty() {
        return false;
    }
    let una = connection.snd_una;
    let nxt = connection.highest_sent();
    let before = sacked_bytes(&connection.sacked);
    for &(left, right) in blocks {
        let left = if seq_lt(left, una) { una } else { left };
        let right = if seq_gt(right, nxt) { nxt } else { right };
        if !seq_gt(right, left) || !seq_gt(right, una) {
            continue;
        }
        connection.sacked.push((left, right));
    }
    connection
        .sacked
        .sort_unstable_by_key(|(start, _)| start.wrapping_sub(una));
    let mut merged: Vec<(u32, u32)> = Vec::with_capacity(connection.sacked.len());
    for &(start, end) in &connection.sacked {
        if let Some(last) = merged.last_mut()
            && seq_ge(last.1, start)
        {
            if seq_gt(end, last.1) {
                last.1 = end;
            }
            continue;
        }
        merged.push((start, end));
    }
    merged.truncate(MAX_SACKED_RANGES);
    connection.sacked = merged;
    sacked_bytes(&connection.sacked) > before
}

/// Drop the SACKed ranges that the cumulative ACK now covers.
pub(crate) fn prune_sacked(connection: &mut TcpConnection, ack: u32) {
    connection.sacked.retain(|(_, end)| seq_gt(*end, ack));
    for range in &mut connection.sacked {
        if seq_lt(range.0, ack) {
            range.0 = ack;
        }
    }
}

fn sacked_bytes(ranges: &[(u32, u32)]) -> u64 {
    ranges
        .iter()
        .map(|(start, end)| u64::from(end.wrapping_sub(*start)))
        .sum()
}

/// The RFC 6675 scoreboard for a connection, in offsets from `snd_una`.
pub(crate) fn scoreboard(connection: &TcpConnection) -> SackScoreboard {
    let una = connection.snd_una;
    let high_data = connection.snd_nxt.wrapping_sub(una);
    let high_rxt = if seq_gt(connection.high_rxt, una) {
        connection.high_rxt.wrapping_sub(una)
    } else {
        0
    };
    let threshold = if connection.rto_recovery {
        1
    } else {
        connection.reorder_dist.max(1)
    };
    let mut board = SackScoreboard::new(
        high_data,
        high_rxt,
        0,
        u32::from(connection.mss),
        threshold,
        MAX_SACKED_RANGES,
    );
    board.sacked = connection
        .sacked
        .iter()
        .map(|(start, end)| (start.wrapping_sub(una), end.wrapping_sub(una)))
        .collect();
    board
}

/// Bytes the scoreboard counts as still in the network.
pub(crate) fn sack_pipe(connection: &TcpConnection) -> u32 {
    scoreboard(connection).pipe()
}

/// Largest reordering distance, in segments, the loss detector will learn.
const MAX_REORDER_DIST: u32 = 300;

/// How many segments overtook the oldest unacknowledged one, from the SACKed ranges.
pub(crate) fn reorder_distance(connection: &TcpConnection) -> u32 {
    let mss = u32::from(connection.mss).max(1);
    let top = connection
        .sacked
        .last()
        .map_or(0, |(_, end)| end.wrapping_sub(connection.snd_una));
    (top / mss).saturating_add(1)
}

/// Whether the oldest unacknowledged byte is considered lost by the SACK information.
pub(crate) fn head_is_lost(connection: &TcpConnection) -> bool {
    connection.sack_permitted && !connection.sacked.is_empty() && scoreboard(connection).is_lost(0)
}

impl<D: Device> Stack<D> {
    /// The ACK that ended a fast retransmit echoes a timestamp from before the retransmission:
    /// the original segment arrived, so the loss was reordering. Undo the window reduction and
    /// raise the tolerated reordering distance.
    pub(crate) fn undo_spurious_recovery(&mut self, socket: SocketId, distance: u32) {
        let Ok(record) = self.sockets.record_mut(socket) else {
            return;
        };
        let Kind::Tcp(connection) = &mut record.kind else {
            return;
        };
        let Some(mut saved) = connection.undo.take() else {
            return;
        };
        self.counters.spurious_retransmits = self.counters.spurious_retransmits.wrapping_add(1);
        if distance > connection.reorder_dist {
            connection.reorder_dist = distance.min(MAX_REORDER_DIST);
            self.counters.reordering_window_growth =
                self.counters.reordering_window_growth.wrapping_add(1);
        }
        if let Some((cwnd, ssthresh)) = saved.restore() {
            connection.cc.undo(cwnd, ssthresh);
            self.counters.spurious_undo = self.counters.spurious_undo.wrapping_add(1);
        }
        connection.sack_recovery = false;
        connection.rto_recovery = false;
        connection.sacked.clear();
    }

    /// Retransmit what the scoreboard calls lost while the window has room.
    pub(crate) fn tcp_sack_recover(&mut self, socket: SocketId) {
        for _ in 0..MAX_BURST {
            let Some(plan) = self.next_sack_retransmit(socket) else {
                return;
            };
            let (payload, local, remote, port, seq, ack, window, options) = plan;
            if !options.is_empty() {
                self.option_override = Some(options);
            }
            let sent = self.send_tcp_packet(
                local,
                remote.0,
                port,
                remote.1,
                TcpFlags::from_bits(TcpFlags::ACK.bits() | TcpFlags::PSH.bits()),
                seq,
                ack,
                window,
                &payload,
                false,
            );
            self.option_override = None;
            if sent.is_err() {
                return;
            }
            self.counters.retrans_segments = self.counters.retrans_segments.wrapping_add(1);
            self.counters.retrans_bytes = self
                .counters
                .retrans_bytes
                .wrapping_add(u64::try_from(payload.len()).unwrap_or(0));
        }
    }

    #[allow(clippy::type_complexity)]
    fn next_sack_retransmit(
        &mut self,
        socket: SocketId,
    ) -> Option<(
        Vec<u8>,
        crate::IpAddr,
        (crate::IpAddr, u16),
        u16,
        u32,
        u32,
        u16,
        Vec<u8>,
    )> {
        let now_ms = u32::try_from(self.now.as_millis()).unwrap_or(u32::MAX);
        let record = self.sockets.record_mut(socket).ok()?;
        let window = advertised_window(record);
        let tx_len = record.tx.len();
        let Kind::Tcp(connection) = &mut record.kind else {
            return None;
        };
        if !connection.sack_recovery || connection.snd_una == connection.snd_nxt {
            return None;
        }
        let mss = u32::from(connection.mss);
        let una = connection.snd_una;
        let mut board = scoreboard(connection);
        if board.pipe().saturating_add(mss) > connection.cc.cwnd() {
            return None;
        }
        let (offset, length) = if connection.sacked.is_empty()
            && !seq_gt(connection.high_rxt, una)
            && seq_lt(una, connection.recover_seq)
        {
            (0u32, mss)
        } else {
            let result = board.next_seg(false, u32::MAX);
            let next = result.next_seg;
            if next.kind != NextKind::Retransmit || next.rule != 1 || next.start < 0 {
                return None;
            }
            (u32::try_from(next.start).ok()?, next.len)
        };
        let start = usize::try_from(offset).ok()?;
        if start >= tx_len {
            return None;
        }
        let take = usize::try_from(length).ok()?.min(tx_len - start);
        if take == 0 {
            return None;
        }
        connection.high_rxt = una
            .wrapping_add(offset)
            .wrapping_add(u32::try_from(take).ok()?);
        connection.retransmits_total = connection.retransmits_total.saturating_add(1);
        let probe_lost = crate::plpmtud_live::plpmtud_on_retransmit(
            connection,
            una.wrapping_add(offset),
            u32::try_from(take).ok()?,
        );
        if probe_lost {
            self.counters.plpmtud_losses = self.counters.plpmtud_losses.wrapping_add(1);
        }
        if let Some(sample) = &mut connection.rtt_sample {
            sample.2 = true;
        }
        let remote = connection.remote?;
        let local = connection.local;
        let port = connection.local_port;
        let ack = connection.rcv_nxt;
        let seq = una.wrapping_add(offset);
        let options = crate::tcp_state::timestamp_options_public(connection, now_ms);
        let payload = crate::tcp_data::queue_range(&record.tx, start, take);
        Some((payload, local, remote, port, seq, ack, window, options))
    }
}

#[cfg(test)]
#[path = "sack_send_ut.rs"]
mod tests;
