// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Connection-level MPTCP meta-socket, driven by simulated subflows.
//!
//! Phase 1 has the wire format; this module is the data plane above it. One [`MetaSocket`]
//! connection spreads bytes over several ordinary TCP `Subflow`s, keys them by data sequence
//! number, reorders the receive side, and re-injects unacknowledged ranges when a path dies.
//! The transport is a tiny [`Subflow`] trait so a deterministic mock can drive all failure modes
//! without a kernel, clock, socket or allocator outside the crate.
//!
//! The state is bounded by the constructor's send/receive caps. Every queue and table is a
//! `Vec`/`VecDeque` with an explicit limit; no peer value is used as an allocation size.

use alloc::boxed::Box;
use alloc::collections::VecDeque;
use alloc::vec::Vec;

use crate::Error;
use crate::dss::Mapping;

/// Live level of one subflow transport.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SubflowReadiness {
    /// The transport has bytes to read.
    pub readable: bool,
    /// The transport can accept a write.
    pub writable: bool,
    /// The transport is terminally closed and its outstanding ranges must be re-injected.
    pub closed: bool,
}

/// A transport that can carry one MPTCP subflow.
///
/// The trait is deliberately small and object-safe: MPTCP2b adapts the real TCP stack to it.
/// `send` returns an error when the transport cannot queue the mapping; the meta-socket leaves
/// the range unsent and retries on the next pump/failover. `readiness` is a live query; the
/// default is a writable, open transport for simple mocks.
pub trait Subflow {
    /// Queue one mapped range on this subflow. `data_fin` is the connection-level DATA_FIN for
    /// this range (the mapping's `data_len` may be zero).
    fn send(&mut self, mapping: Mapping, payload: &[u8], data_fin: bool) -> Result<(), Error>;

    /// Current live level of the transport.
    fn readiness(&mut self) -> SubflowReadiness {
        SubflowReadiness {
            readable: false,
            writable: true,
            closed: false,
        }
    }

    /// Close the transport. The meta-socket keeps the handle for terminal state.
    fn close(&mut self) {}
}

struct SubflowEntry {
    id: u32,
    backup: bool,
    alive: bool,
    ssn: u32,
    transport: Box<dyn Subflow>,
}

struct SendRange {
    dsn: u64,
    data: Vec<u8>,
    fin: bool,
    sent_on: Option<u32>,
}

struct RecvRange {
    dsn: u64,
    data: Vec<u8>,
    fin: bool,
}

/// One connection-level MPTCP socket.
pub struct MetaSocket {
    mss: usize,
    send_cap: usize,
    rcv_cap: usize,
    window: u64,
    subflows: Vec<SubflowEntry>,
    send: Vec<SendRange>,
    snd_una: u64,
    snd_nxt: u64,
    rcv_nxt: u64,
    reorder: Vec<RecvRange>,
    delivered: VecDeque<u8>,
    peer_fin: bool,
    send_closed: bool,
    closed: bool,
    fail_at: Option<u64>,
    aggregation: bool,
    scheduler_cursor: usize,
}

impl MetaSocket {
    /// Create a socket with `mss`, a connection send-buffer cap, a receive-memory cap and a
    /// connection-level receive window.
    #[must_use]
    pub fn new(mss: usize, send_cap: usize, rcv_cap: usize, window: u64) -> Self {
        Self {
            mss: mss.max(1),
            send_cap,
            rcv_cap,
            window,
            subflows: Vec::new(),
            send: Vec::new(),
            snd_una: 0,
            snd_nxt: 0,
            rcv_nxt: 0,
            reorder: Vec::new(),
            delivered: VecDeque::new(),
            peer_fin: false,
            send_closed: false,
            closed: false,
            fail_at: None,
            aggregation: false,
            scheduler_cursor: 0,
        }
    }

    /// Add a subflow. IDs are unique and caller-assigned.
    pub fn add_subflow(
        &mut self,
        id: u32,
        backup: bool,
        transport: Box<dyn Subflow>,
    ) -> Result<(), Error> {
        if self.subflows.iter().any(|entry| entry.id == id) {
            return Err(Error::Duplicate);
        }
        self.subflows.push(SubflowEntry {
            id,
            backup,
            alive: true,
            ssn: 0,
            transport,
        });
        self.pump();
        Ok(())
    }

    /// Mark a subflow alive/dead. Marking it dead requeues every range sent on it, exactly once,
    /// and immediately pumps the range onto the remaining active path.
    pub fn set_alive(&mut self, id: u32, alive: bool) -> Result<(), Error> {
        let entry = self
            .subflows
            .iter_mut()
            .find(|entry| entry.id == id)
            .ok_or(Error::NotFound)?;
        let was = entry.alive;
        entry.alive = alive;
        if was && !alive {
            for range in &mut self.send {
                if range.sent_on == Some(id) {
                    range.sent_on = None;
                }
            }
        }
        self.pump();
        Ok(())
    }

    /// Set/clear the backup flag. A backup subflow is selected only when no regular path is
    /// alive; changing the flag immediately lets pending data use the new primary.
    pub fn set_backup(&mut self, id: u32, backup: bool) -> Result<(), Error> {
        let entry = self
            .subflows
            .iter_mut()
            .find(|entry| entry.id == id)
            .ok_or(Error::NotFound)?;
        entry.backup = backup;
        self.pump();
        Ok(())
    }

    /// The selected active subflow: a regular alive path first, otherwise an alive backup.
    #[must_use]
    pub fn active(&self) -> Option<u32> {
        self.subflows
            .iter()
            .find(|entry| entry.alive && !entry.backup)
            .or_else(|| self.subflows.iter().find(|entry| entry.alive))
            .map(|entry| entry.id)
    }

    /// Enable or disable aggregation across every regular path.
    ///
    /// Off is the default: one active path plus idle backups. On distributes new ranges across
    /// all alive regular paths; backups still wait until no regular path is alive.
    pub fn set_aggregation(&mut self, enabled: bool) {
        self.aggregation = enabled;
        self.scheduler_cursor = 0;
        self.pump();
    }

    /// Whether aggregation is enabled.
    #[must_use]
    pub fn aggregation(&self) -> bool {
        self.aggregation
    }

    /// Remove one subflow, requeue its unacked ranges and close its transport.
    pub fn remove_subflow(&mut self, id: u32) -> Result<(), Error> {
        let index = self
            .subflows
            .iter()
            .position(|entry| entry.id == id)
            .ok_or(Error::NotFound)?;
        let mut entry = self.subflows.remove(index);
        entry.transport.close();
        for range in &mut self.send {
            if range.sent_on == Some(id) {
                range.sent_on = None;
            }
        }
        self.pump();
        Ok(())
    }

    /// Unacknowledged bytes last sent on `id`.
    #[must_use]
    pub fn bytes_in_flight(&self, id: u32) -> usize {
        self.send
            .iter()
            .filter(|range| range.sent_on == Some(id))
            .map(|range| range.data.len())
            .sum()
    }

    /// Delivered bytes or a peer DATA_FIN are readable.
    #[must_use]
    pub fn readable(&self) -> bool {
        !self.delivered.is_empty() || self.peer_fin
    }

    /// The connection can accept new application bytes.
    #[must_use]
    pub fn writable(&self) -> bool {
        !self.closed && !self.send_closed && self.send_queue() < self.send_cap
    }

    /// Close every transport. Used by the socket wrapper after the DATA_FIN path.
    pub fn close_paths(&mut self) {
        for entry in &mut self.subflows {
            entry.transport.close();
            entry.alive = false;
        }
    }

    /// Accept application bytes into the connection send buffer and pump them.
    ///
    /// Returns the accepted prefix (which may be short when the send cap is reached); an empty
    /// result for non-empty input is [`Error::WouldBlock`].
    pub fn send(&mut self, data: &[u8]) -> Result<usize, Error> {
        if self.closed || self.send_closed {
            return Err(Error::Closed);
        }
        let mut accepted = 0;
        while accepted < data.len() {
            let used = self.send_queue();
            let available = self.send_cap.saturating_sub(used);
            if available == 0 {
                break;
            }
            let length = self.mss.min(data.len() - accepted).min(available);
            let chunk = data[accepted..accepted + length].to_vec();
            self.send.push(SendRange {
                dsn: self.snd_nxt,
                data: chunk,
                fin: false,
                sent_on: None,
            });
            self.snd_nxt = self.snd_nxt.wrapping_add(length as u64);
            accepted += length;
        }
        if accepted == 0 && !data.is_empty() {
            return Err(Error::WouldBlock);
        }
        self.pump();
        Ok(accepted)
    }

    /// Send every range that has not been assigned to a live subflow. Returns the number sent.
    ///
    /// Only the active path is used: backup paths stay idle until the primary is dead or their
    /// priority is changed. A `send` error leaves the range pending for the next pump. The
    /// transport's live `closed` level is re-read first, so a real connection that died is
    /// requeued without a separate notification.
    pub fn pump(&mut self) -> usize {
        self.refresh_dead();
        self.pump_ranges()
    }

    fn pump_ranges(&mut self) -> usize {
        let active_only = self.active();
        let candidates: Vec<usize> = if self.aggregation {
            let mut regular: Vec<usize> = self
                .subflows
                .iter()
                .enumerate()
                .filter(|(_, entry)| entry.alive && !entry.backup)
                .map(|(index, _)| index)
                .collect();
            if regular.is_empty() {
                regular = self
                    .subflows
                    .iter()
                    .enumerate()
                    .filter(|(_, entry)| entry.alive)
                    .map(|(index, _)| index)
                    .collect();
            }
            regular
        } else {
            let Some(active) = active_only else {
                return 0;
            };
            self.subflows
                .iter()
                .enumerate()
                .filter(|(_, entry)| entry.id == active && entry.alive)
                .map(|(index, _)| index)
                .collect()
        };
        if candidates.is_empty() {
            return 0;
        }

        let mut sent = 0;
        let mut cursor = self.scheduler_cursor;
        for range in &mut self.send {
            if range.sent_on.is_some() {
                continue;
            }
            let index = candidates[cursor % candidates.len()];
            let length = u16::try_from(range.data.len()).unwrap_or(u16::MAX);
            let ssn = self.subflows[index].ssn;
            let mapping = Mapping {
                data_seq: range.dsn,
                subflow_seq: ssn,
                data_len: length,
                checksum: None,
                dsn_64: true,
            };
            if self.subflows[index]
                .transport
                .send(mapping, &range.data, range.fin)
                .is_err()
            {
                break;
            }
            let consumed = u32::try_from(range.data.len()).unwrap_or(u32::MAX);
            self.subflows[index].ssn = ssn
                .wrapping_add(consumed)
                .wrapping_add(u32::from(range.fin));
            range.sent_on = Some(self.subflows[index].id);
            sent += 1;
            if self.aggregation {
                cursor = cursor.wrapping_add(1);
            }
        }
        self.scheduler_cursor = cursor;
        sent
    }

    /// Re-read every transport's live level and requeue outstanding ranges of a closed path.
    pub fn refresh(&mut self) {
        let _ = self.pump();
    }

    /// Live readiness of one transport.
    pub fn subflow_readiness(&mut self, id: u32) -> Result<SubflowReadiness, Error> {
        let entry = self
            .subflows
            .iter_mut()
            .find(|entry| entry.id == id)
            .ok_or(Error::NotFound)?;
        Ok(entry.transport.readiness())
    }

    fn refresh_dead(&mut self) {
        let mut dead = Vec::new();
        for entry in &mut self.subflows {
            if entry.alive && entry.transport.readiness().closed {
                dead.push(entry.id);
            }
        }
        for id in dead {
            if let Some(entry) = self.subflows.iter_mut().find(|entry| entry.id == id) {
                entry.alive = false;
            }
            for range in &mut self.send {
                if range.sent_on == Some(id) {
                    range.sent_on = None;
                }
            }
        }
    }

    /// Queue and send a DATA_FIN after all queued data.
    pub fn close(&mut self) {
        if self.closed || self.send_closed {
            return;
        }
        self.send_closed = true;
        self.send.push(SendRange {
            dsn: self.snd_nxt,
            data: Vec::new(),
            fin: true,
            sent_on: None,
        });
        self.snd_nxt = self.snd_nxt.wrapping_add(1);
        self.pump();
    }

    /// Deliver a received mapping in order or into the bounded reorder buffer.
    ///
    /// A duplicate or overlapping prefix is trimmed; data beyond the receive window is
    /// [`Error::WindowViolation`]; exceeding the receive memory cap is [`Error::MemoryLimit`].
    pub fn receive(
        &mut self,
        mapping: &Mapping,
        payload: &[u8],
        data_fin: bool,
    ) -> Result<(), Error> {
        if self.closed {
            return Ok(());
        }
        if mapping.data_len as usize != payload.len() {
            return Err(Error::BadLength);
        }

        let mut dsn = mapping.data_seq;
        let mut data = payload;
        let mut data_fin = data_fin;
        if let Some(fail_at) = self.fail_at {
            if dsn >= fail_at {
                return Ok(());
            }
            let keep = usize::try_from(fail_at - dsn).unwrap_or(usize::MAX);
            if keep < data.len() {
                data = &data[..keep];
                data_fin = false;
            }
        }

        if dsn < self.rcv_nxt {
            let skip = usize::try_from(self.rcv_nxt - dsn).unwrap_or(usize::MAX);
            if skip >= data.len() {
                if data_fin && dsn.wrapping_add(data.len() as u64) == self.rcv_nxt {
                    self.peer_fin = true;
                }
                return Ok(());
            }
            data = &data[skip..];
            dsn = self.rcv_nxt;
        }

        if dsn > self.rcv_nxt {
            if dsn > self.rcv_nxt.wrapping_add(self.window) {
                return Err(Error::WindowViolation);
            }
            if self.buffered() + data.len() > self.rcv_cap {
                return Err(Error::MemoryLimit);
            }
            if self
                .reorder
                .iter()
                .any(|range| range.dsn == dsn && range.data.len() == data.len())
            {
                return Ok(());
            }
            self.reorder.push(RecvRange {
                dsn,
                data: data.to_vec(),
                fin: data_fin,
            });
            self.reorder.sort_by_key(|range| range.dsn);
            return Ok(());
        }

        self.deliver(dsn, data, data_fin)?;
        Ok(())
    }

    fn deliver(&mut self, dsn: u64, data: &[u8], fin: bool) -> Result<(), Error> {
        if self.buffered() + data.len() > self.rcv_cap {
            return Err(Error::MemoryLimit);
        }
        self.delivered.extend(data);
        self.rcv_nxt = dsn.wrapping_add(data.len() as u64);
        if fin {
            self.peer_fin = true;
        }
        self.drain_reorder()
    }

    fn drain_reorder(&mut self) -> Result<(), Error> {
        while let Some(index) = self
            .reorder
            .iter()
            .position(|range| range.dsn <= self.rcv_nxt)
        {
            let mut range = self.reorder.remove(index);
            let end = range.dsn.wrapping_add(range.data.len() as u64);
            if end <= self.rcv_nxt {
                continue;
            }
            let skip = usize::try_from(self.rcv_nxt - range.dsn).unwrap_or(usize::MAX);
            range.data.drain(..skip.min(range.data.len()));
            let data = core::mem::take(&mut range.data);
            let consumed = data.len() as u64;
            self.delivered.extend(data);
            self.rcv_nxt = self.rcv_nxt.wrapping_add(consumed);
            if range.fin {
                self.peer_fin = true;
            }
        }
        Ok(())
    }

    /// Copy delivered in-order bytes to `buffer`; returns the count (0 at EOF).
    pub fn read(&mut self, buffer: &mut [u8]) -> usize {
        let count = buffer.len().min(self.delivered.len());
        for target in &mut buffer[..count] {
            if let Some(byte) = self.delivered.pop_front() {
                *target = byte;
            }
        }
        count
    }

    /// Connection-level Data ACK: the next expected data sequence number.
    #[must_use]
    pub fn data_ack(&self) -> u64 {
        self.rcv_nxt
    }

    /// Acknowledge sent data at the connection level and free the prefix.
    pub fn ack(&mut self, data_ack: u64) {
        let ack = data_ack.min(self.snd_nxt);
        if ack <= self.snd_una {
            return;
        }
        self.snd_una = ack;
        while let Some(range) = self.send.first_mut() {
            let end = range.dsn.wrapping_add(range.data.len() as u64);
            if end <= ack {
                self.send.remove(0);
                continue;
            }
            if range.dsn < ack {
                let skip = usize::try_from(ack - range.dsn).unwrap_or(usize::MAX);
                range.data.drain(..skip.min(range.data.len()));
                range.dsn = ack;
            }
            break;
        }
        self.pump();
    }

    /// Discard buffered data at or after `data_seq` (MP_FAIL).
    pub fn on_mp_fail(&mut self, data_seq: u64) {
        self.fail_at = Some(data_seq);
        self.reorder.retain(|range| range.dsn < data_seq);
    }

    /// Abruptly close the whole connection (MP_FASTCLOSE).
    pub fn on_fastclose(&mut self) {
        self.closed = true;
        self.send.clear();
        self.reorder.clear();
        self.delivered.clear();
        self.peer_fin = true;
    }

    /// Bytes queued for transmission (not acknowledged).
    #[must_use]
    pub fn send_queue(&self) -> usize {
        self.send.iter().map(|range| range.data.len()).sum()
    }

    /// Bytes buffered for the application plus out-of-order bytes.
    #[must_use]
    pub fn buffered(&self) -> usize {
        self.delivered.len()
            + self
                .reorder
                .iter()
                .map(|range| range.data.len())
                .sum::<usize>()
    }

    /// Peer DATA_FIN observed.
    #[must_use]
    pub fn peer_fin(&self) -> bool {
        self.peer_fin
    }

    /// The connection was fast-closed.
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.closed
    }

    /// The highest contiguous sent sequence (first unacknowledged).
    #[must_use]
    pub fn snd_una(&self) -> u64 {
        self.snd_una
    }
}

#[cfg(test)]
#[path = "meta_ut.rs"]
mod tests;
