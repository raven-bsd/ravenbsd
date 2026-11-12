// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! TCP data queues, window accounting, SWS, Nagle and delayed ACK.

use alloc::collections::VecDeque;
use alloc::vec::Vec;

use raven_abi::errno::Errno;
use raven_tcpip_wire::tcp::TcpFlags;

use crate::device::Device;
use crate::seq::{seq_ge, seq_gt, seq_lt};
use crate::socket::Kind;
use crate::tcp_state::TcpState;
use crate::{Instant, SocketId, Stack};

impl<D: Device> Stack<D> {
    /// Queue application bytes on a TCP socket and transmit what the window allows.
    pub(crate) fn tcp_send_data(&mut self, socket: SocketId, data: &[u8]) -> Result<usize, Errno> {
        if data.is_empty() {
            return Ok(0);
        }
        let accepted = {
            let record = self.sockets.record_mut(socket)?;
            if record.closed {
                return Err(Errno::EBADF);
            }
            if !matches!(record.kind, Kind::Tcp(_)) {
                return Err(Errno::ENOTCONN);
            }
            if record.tx_shutdown {
                return Err(Errno::EPIPE);
            }
            let room = record.tx_cap.saturating_sub(record.tx.len());
            let accepted = data.len().min(room);
            record.tx.extend(&data[..accepted]);
            accepted
        };
        if accepted == 0 {
            return Err(Errno::EAGAIN);
        }
        if let Ok(record) = self.sockets.record_mut(socket)
            && let Kind::Tcp(connection) = &mut record.kind
        {
            connection.last_activity = self.now;
        }
        self.tcp_try_send(socket)?;
        Ok(accepted)
    }

    /// Read TCP bytes; short buffers get a prefix of the stream.
    pub(crate) fn tcp_recv_data(
        &mut self,
        socket: SocketId,
        buffer: &mut [u8],
    ) -> Result<usize, Errno> {
        if buffer.is_empty() {
            return Ok(0);
        }
        let count = {
            let record = self.sockets.record_mut(socket)?;
            if let Some(error) = record.error.take() {
                return Err(error);
            }
            if !matches!(record.kind, Kind::Tcp(_) | Kind::Icmp { .. }) {
                return Err(Errno::ENOTSOCK);
            }
            let peer_fin = match &record.kind {
                Kind::Tcp(connection) => connection.peer_fin || record.peer_closed,
                _ => record.peer_closed,
            };
            if (record.closed || peer_fin) && record.rx.is_empty() {
                return Ok(0);
            }
            let count = buffer.len().min(record.rx.len());
            let (front, back) = record.rx.as_slices();
            let first = count.min(front.len());
            buffer[..first].copy_from_slice(&front[..first]);
            buffer[first..count].copy_from_slice(&back[..count - first]);
            record.rx.drain(..count);
            count
        };
        if count == 0 {
            let record = self.sockets.record(socket)?;
            let peer_fin = match &record.kind {
                Kind::Tcp(connection) => connection.peer_fin || record.peer_closed,
                _ => record.peer_closed,
            };
            if peer_fin || (record.closed && record.rx.is_empty()) {
                return Ok(0);
            }
            return Err(Errno::EAGAIN);
        }
        // A read opens the receive window; advertise it promptly.
        if let Ok(record) = self.sockets.record_mut(socket)
            && let Kind::Tcp(connection) = &mut record.kind
        {
            connection.last_activity = self.now;
        }
        let _ = self.send_ack_for(socket);
        Ok(count)
    }

    /// Set Nagle behaviour.
    pub(crate) fn tcp_set_nodelay(&mut self, socket: SocketId, on: bool) -> Result<(), Errno> {
        let record = self.sockets.record_mut(socket)?;
        let Kind::Tcp(connection) = &mut record.kind else {
            return Err(Errno::ENOTSOCK);
        };
        connection.nodelay = on;
        if on {
            // Flush what can be sent now.
        }
        Ok(())
    }

    /// Bytes waiting in the transmit queue.
    pub(crate) fn tcp_send_queue_len(&self, socket: SocketId) -> Result<usize, Errno> {
        match &self.sockets.record(socket)?.kind {
            Kind::Tcp(_) => Ok(self.sockets.record(socket)?.tx.len()),
            _ => Err(Errno::ENOTSOCK),
        }
    }

    /// Bytes waiting in the receive queue.
    pub(crate) fn tcp_recv_queue_len(&self, socket: SocketId) -> Result<usize, Errno> {
        match &self.sockets.record(socket)?.kind {
            Kind::Tcp(_) | Kind::Icmp { .. } => Ok(self.sockets.record(socket)?.rx.len()),
            _ => Err(Errno::ENOTSOCK),
        }
    }

    /// Transmit as much queued data as the peer window, SWS and Nagle allow.
    #[allow(clippy::type_complexity)]
    pub(crate) fn tcp_try_send(&mut self, socket: SocketId) -> Result<(), Errno> {
        loop {
            let snapshot = {
                let record = self.sockets.record_mut(socket)?;
                let Kind::Tcp(connection) = &mut record.kind else {
                    return Err(Errno::ENOTSOCK);
                };
                if !matches!(
                    connection.state,
                    TcpState::Established
                        | TcpState::CloseWait
                        | TcpState::FinWait1
                        | TcpState::FinWait2
                ) {
                    return Ok(());
                }
                let inflight = connection.snd_nxt.wrapping_sub(connection.snd_una);
                let queued = u32::try_from(record.tx.len()).unwrap_or(u32::MAX);
                if queued <= inflight {
                    if connection.close_requested && !connection.fin_sent && record.tx.is_empty() {
                        Some(None)
                    } else {
                        None
                    }
                } else {
                    let unsent = queued - inflight;
                    let window = connection.snd_wnd;
                    let cwnd = connection.cc.cwnd();
                    let in_network = if connection.sack_recovery {
                        crate::sack_send::sack_pipe(connection)
                    } else {
                        inflight
                    };
                    if window <= inflight || cwnd <= in_network {
                        if window <= inflight && connection.zero_window_deadline.is_none() {
                            connection.zero_window_deadline = Some(Instant::from_millis(
                                self.now.as_millis().saturating_add(
                                    i64::try_from(connection.zero_window_backoff_ms)
                                        .unwrap_or(i64::MAX),
                                ),
                            ));
                        }
                        None
                    } else {
                        let cwnd_room = cwnd - in_network;
                        let usable = (window - inflight).min(unsent).min(cwnd_room);
                        let mss = u32::from(connection.mss);
                        let cwnd_limited = cwnd_room < window - inflight;
                        let floor = if cwnd_limited {
                            mss
                        } else {
                            mss.max(u32::try_from(record.tx_cap / 2).unwrap_or(0).max(1))
                        };
                        let all_fits = usable == unsent;
                        let window_ok = usable >= floor || all_fits;
                        let nagle_ok = connection.nodelay
                            || inflight == 0
                            || usable >= u32::from(connection.mss);
                        if !window_ok || !nagle_ok {
                            None
                        } else {
                            let mut take = usable.min(u32::from(connection.mss));
                            if self.tunables.plpmtud_enable != 0
                                && let Some((size, reprobe)) = crate::plpmtud_live::probe_due(
                                    connection,
                                    self.now.as_millis(),
                                    i64::try_from(self.tunables.plpmtud_interval)
                                        .unwrap_or(i64::MAX),
                                    usable,
                                )
                            {
                                take = size;
                                self.counters.plpmtud_probes =
                                    self.counters.plpmtud_probes.wrapping_add(1);
                                if reprobe {
                                    self.counters.plpmtud_reprobes =
                                        self.counters.plpmtud_reprobes.wrapping_add(1);
                                }
                            }
                            let start = usize::try_from(inflight).unwrap_or(usize::MAX);
                            let payload = queue_range(&record.tx, start, take as usize);
                            let push = start + payload.len() == record.tx.len();
                            Some(Some((
                                payload,
                                connection.local,
                                connection.remote,
                                connection.local_port,
                                connection.snd_nxt,
                                connection.rcv_nxt,
                                {
                                    let free = record.rx_cap.saturating_sub(record.rx.len());
                                    u16::try_from(
                                        (free >> connection.rcv_wscale).min(usize::from(u16::MAX)),
                                    )
                                    .unwrap_or(u16::MAX)
                                },
                                push,
                                crate::tcp_state::timestamp_options_public(
                                    connection,
                                    u32::try_from(self.now.as_millis()).unwrap_or(0),
                                ),
                            )))
                        }
                    }
                }
            };
            let Some(snapshot) = snapshot else { break };
            let Some((payload, local, remote, local_port, seq, ack, window, push, ts_options)) =
                snapshot
            else {
                // All queued data is sent; emit the deferred FIN.
                let _ = self.tcp_send_deferred_fin(socket);
                break;
            };
            let remote = remote.ok_or(Errno::ENOTCONN)?;
            if !ts_options.is_empty() {
                self.option_override = Some(ts_options);
            }
            let mut flags = TcpFlags::ACK;
            if push {
                flags = TcpFlags::from_bits(flags.bits() | TcpFlags::PSH.bits());
            }
            let cwr = self
                .sockets
                .record(socket)
                .ok()
                .and_then(|record| match &record.kind {
                    Kind::Tcp(connection) => Some(connection.ecn_cwr_pending),
                    _ => None,
                })
                .unwrap_or(false);
            if cwr {
                flags = TcpFlags::from_bits(flags.bits() | TcpFlags::CWR.bits());
            }
            self.send_tcp_packet(
                local, remote.0, local_port, remote.1, flags, seq, ack, window, &payload, false,
            )?;
            let record = self.sockets.record_mut(socket)?;
            if let Kind::Tcp(connection) = &mut record.kind {
                connection.snd_nxt = connection
                    .snd_nxt
                    .wrapping_add(u32::try_from(payload.len()).unwrap_or(0));
                let now_ms = u32::try_from(self.now.as_millis()).unwrap_or(u32::MAX);
                connection.rack.on_sent(connection.snd_nxt, now_ms);
                connection.rack_sent.push((connection.snd_nxt, now_ms));
                if connection.rack_sent.len() > 64 {
                    connection.rack_sent.remove(0);
                }
                if connection.tlp_enabled
                    && connection.snd_una != connection.snd_nxt
                    && connection.rto.has_sample()
                {
                    let srtt = u32::try_from(connection.rto.srtt_ms()).unwrap_or(u32::MAX);
                    let rttvar = u32::try_from(connection.rto.rttvar_ms()).unwrap_or(u32::MAX);
                    connection.tlp.arm(now_ms, srtt, rttvar);
                    let cap =
                        now_ms.saturating_add(srtt.saturating_add(srtt / 2).saturating_add(10));
                    if connection
                        .tlp
                        .deadline
                        .is_some_and(|deadline| deadline > cap)
                    {
                        connection.tlp.deadline = Some(cap);
                    }
                }
                connection.last_activity = self.now;
                self.counters.out_bytes = self
                    .counters
                    .out_bytes
                    .wrapping_add(u64::try_from(payload.len()).unwrap_or(0));
                if cwr {
                    connection.ecn_cwr_pending = false;
                    self.counters.ecn_cwr_sent = self.counters.ecn_cwr_sent.wrapping_add(1);
                }
                if connection.rtt_sample.is_none() {
                    connection.rtt_sample = Some((seq, self.now, false));
                }
                if connection.rto_deadline.is_none() {
                    connection.rto_deadline =
                        Some(Instant::from_millis(self.now.as_millis().saturating_add(
                            i64::try_from(connection.rto.rto_ms()).unwrap_or(i64::MAX),
                        )));
                }
            }
        }
        Ok(())
    }

    fn tcp_send_deferred_fin(&mut self, socket: SocketId) -> Result<(), Errno> {
        let snapshot = {
            let record = self.sockets.record(socket)?;
            let Kind::Tcp(connection) = &record.kind else {
                return Err(Errno::ENOTSOCK);
            };
            if connection.fin_sent || !connection.close_requested {
                return Ok(());
            }
            let remote = connection.remote.ok_or(Errno::ENOTCONN)?;
            (
                connection.local,
                remote,
                connection.local_port,
                connection.snd_nxt,
                connection.rcv_nxt,
                advertised_window(record),
                connection.state,
            )
        };
        let (local, remote, local_port, seq, ack, window, state) = snapshot;
        self.send_tcp_packet(
            local,
            remote.0,
            local_port,
            remote.1,
            TcpFlags::from_bits(TcpFlags::FIN.bits() | TcpFlags::ACK.bits()),
            seq,
            ack,
            window,
            &[],
            false,
        )?;
        let record = self.sockets.record_mut(socket)?;
        if let Kind::Tcp(connection) = &mut record.kind {
            connection.snd_nxt = connection.snd_nxt.wrapping_add(1);
            connection.fin_sent = true;
            connection.state = if state == TcpState::CloseWait {
                TcpState::LastAck
            } else {
                TcpState::FinWait1
            };
        }
        Ok(())
    }

    /// Remove acknowledged bytes and transmit more.
    pub(crate) fn tcp_ack_data(&mut self, socket: SocketId, ack: u32) -> Result<(), Errno> {
        {
            let record = self.sockets.record_mut(socket)?;
            let Kind::Tcp(connection) = &mut record.kind else {
                return Err(Errno::ENOTSOCK);
            };
            if seq_gt(ack, connection.snd_una) {
                if seq_gt(ack, connection.snd_nxt) {
                    connection.snd_nxt = ack;
                }
                if connection.snd_max.is_some_and(|max| seq_ge(ack, max)) {
                    connection.snd_max = None;
                }
                let mut sampled = false;
                if let Some((sample_seq, sent_at, retransmitted)) = connection.rtt_sample
                    && seq_ge(ack, sample_seq)
                {
                    if !retransmitted {
                        let rtt = self.now.as_millis().saturating_sub(sent_at.as_millis());
                        if rtt >= 0 {
                            connection.rto.sample(u64::try_from(rtt).unwrap_or(0));
                            sampled = true;
                        }
                    }
                    connection.rtt_sample = None;
                }
                if sampled {
                    self.counters.rtt_updates = self.counters.rtt_updates.wrapping_add(1);
                } else {
                    self.counters.rtt_no_updates = self.counters.rtt_no_updates.wrapping_add(1);
                }
                let delta = ack.wrapping_sub(connection.snd_una);
                let flight = connection.snd_nxt.wrapping_sub(connection.snd_una);
                let snd_nxt = connection.snd_nxt;
                let mut cc_context = crate::CcContext {
                    mss: u32::from(connection.mss),
                    flight,
                    pipe: flight,
                    rtt: None,
                    snd_nxt,
                    snd_una: connection.snd_una,
                };
                connection.cc.on_ack(
                    &mut cc_context,
                    delta,
                    Some(crate::Duration::from_millis(connection.rto.srtt_ms())),
                );
                let remove = usize::try_from(delta)
                    .unwrap_or(usize::MAX)
                    .min(record.tx.len());
                record.tx.drain(..remove);
                connection.snd_una = ack;
                crate::plpmtud_live::plpmtud_on_ack(connection, ack, self.now.as_millis());
                let now_ms = u32::try_from(self.now.as_millis()).unwrap_or(u32::MAX);
                connection.rack.on_ack(ack, now_ms);
                if connection.tlp_enabled && connection.snd_nxt != ack {
                    let srtt = u32::try_from(connection.rto.srtt_ms()).unwrap_or(u32::MAX);
                    let rttvar = u32::try_from(connection.rto.rttvar_ms()).unwrap_or(u32::MAX);
                    connection.tlp.arm(now_ms, srtt, rttvar);
                    let cap =
                        now_ms.saturating_add(srtt.saturating_add(srtt / 2).saturating_add(10));
                    if connection
                        .tlp
                        .deadline
                        .is_some_and(|deadline| deadline > cap)
                    {
                        connection.tlp.deadline = Some(cap);
                    }
                } else {
                    connection.tlp.disarm();
                }
                connection.tlp_probe_at = None;
                connection.rack_sent.retain(|(end, _)| seq_gt(*end, ack));
                crate::sack_send::prune_sacked(connection, ack);
                if connection.sack_recovery && seq_ge(ack, connection.recover_seq) {
                    connection.sack_recovery = false;
                    connection.rto_recovery = false;
                    connection.sacked.clear();
                }
                if connection.cc.in_recovery() {
                    if crate::seq::seq_ge(ack, connection.recover_seq) {
                        connection.cc.on_recovery_exit();
                    } else {
                        connection.cc.on_partial_ack(delta);
                    }
                }
                if sampled {
                    connection.ecn.on_rtt();
                }
                connection.last_activity = self.now;
                connection.zero_window_backoff_ms = 1000;
                connection.zero_window_deadline = None;
                connection.retransmit_count = 0;
                if connection.snd_nxt == ack {
                    connection.rto_deadline = None;
                } else {
                    if connection.rtt_sample.is_none() {
                        connection.rtt_sample = Some((ack, self.now, false));
                    }
                    connection.rto_deadline =
                        Some(Instant::from_millis(self.now.as_millis().saturating_add(
                            i64::try_from(connection.rto.rto_ms()).unwrap_or(i64::MAX),
                        )));
                }
            }
        }
        self.tcp_sack_recover(socket);
        self.tcp_try_send(socket)
    }

    /// Deliver incoming in-order and out-of-order payload.
    pub(crate) fn tcp_deliver_data(
        &mut self,
        socket: SocketId,
        seq: u32,
        payload: &[u8],
        now: Instant,
    ) {
        if payload.is_empty() {
            return;
        }
        let mut immediate_ack = false;
        {
            let Ok(record) = self.sockets.record_mut(socket) else {
                return;
            };
            let Kind::Tcp(connection) = &mut record.kind else {
                return;
            };
            let mut data_start = 0usize;
            let mut sequence = seq;
            if seq_lt(sequence, connection.rcv_nxt) {
                let skip = usize::try_from(connection.rcv_nxt.wrapping_sub(sequence))
                    .unwrap_or(usize::MAX);
                if skip >= payload.len() {
                    if payload.len() == 1 && record.rx.len() >= record.rx_cap {
                        self.counters.in_zwnd_probes = self.counters.in_zwnd_probes.wrapping_add(1);
                    }
                    self.counters.in_dup_segments = self.counters.in_dup_segments.wrapping_add(1);
                    self.counters.in_dup_bytes = self
                        .counters
                        .in_dup_bytes
                        .wrapping_add(u64::try_from(payload.len()).unwrap_or(0));
                    let _ = self.send_ack_for(socket);
                    return;
                }
                data_start = skip;
                sequence = connection.rcv_nxt;
            }
            if seq_gt(sequence, connection.rcv_nxt) {
                self.counters.in_ooo_segments = self.counters.in_ooo_segments.wrapping_add(1);
                self.counters.in_ooo_bytes = self
                    .counters
                    .in_ooo_bytes
                    .wrapping_add(u64::try_from(payload.len()).unwrap_or(0));
                let new_hole = ooo_creates_hole(&connection.ooo, sequence, payload.len());
                let ooo_bytes: usize = connection.ooo.iter().map(|(_, data)| data.len()).sum();
                let duplicate = connection
                    .ooo
                    .iter()
                    .any(|(start, data)| *start == sequence && data.len() >= payload.len());
                if duplicate {
                    self.counters.in_dup_segments = self.counters.in_dup_segments.wrapping_add(1);
                } else if (new_hole && ooo_hole_count(&connection.ooo) >= connection.ooo_limit)
                    || ooo_bytes.saturating_add(payload.len()) > record.rx_cap
                {
                    self.counters.reassembly_overflows =
                        self.counters.reassembly_overflows.wrapping_add(1);
                    self.drops = self.drops.saturating_add(1);
                } else {
                    connection.ooo.push((sequence, payload.to_vec()));
                }
                immediate_ack = true;
            } else {
                let room = record.rx_cap.saturating_sub(record.rx.len());
                if room == 0 {
                    self.counters.zero_window_drops =
                        self.counters.zero_window_drops.wrapping_add(1);
                    let _ = self.send_ack_for(socket);
                    return;
                }
                let take = (payload.len() - data_start).min(room);
                record.rx.extend(&payload[data_start..data_start + take]);
                self.tcp_data_delivered = self.tcp_data_delivered.saturating_add(take);
                self.counters.in_bytes = self
                    .counters
                    .in_bytes
                    .wrapping_add(u64::try_from(take).unwrap_or(0));
                connection.last_activity = now;
                connection.rcv_nxt = connection
                    .rcv_nxt
                    .wrapping_add(u32::try_from(take).unwrap_or(0));
                if take < payload.len() - data_start {
                    self.drops = self.drops.saturating_add(1);
                }
                connection.ack_segments = connection.ack_segments.saturating_add(1);
                let delack_segments = u8::try_from(self.tunables.delack_segments).unwrap_or(2);
                if take >= usize::from(connection.mss) || connection.ack_segments >= delack_segments
                {
                    immediate_ack = true;
                } else {
                    connection.ack_deadline =
                        Some(Instant::from_millis(now.as_millis().saturating_add(
                            i64::try_from(self.tunables.delack_delay).unwrap_or(i64::MAX),
                        )));
                }
                // Drain any now-contiguous out-of-order pieces.
                loop {
                    let next = connection
                        .ooo
                        .iter()
                        .position(|(segment_seq, _)| seq_ge(connection.rcv_nxt, *segment_seq));
                    let Some(index) = next else { break };
                    let (segment_seq, data) = connection.ooo.remove(index);
                    let skip = usize::try_from(connection.rcv_nxt.wrapping_sub(segment_seq))
                        .unwrap_or(usize::MAX)
                        .min(data.len());
                    if skip >= data.len() {
                        continue;
                    }
                    let room = record.rx_cap.saturating_sub(record.rx.len());
                    let take = (data.len() - skip).min(room);
                    record.rx.extend(&data[skip..skip + take]);
                    self.counters.in_bytes = self
                        .counters
                        .in_bytes
                        .wrapping_add(u64::try_from(take).unwrap_or(0));
                    connection.rcv_nxt = connection
                        .rcv_nxt
                        .wrapping_add(u32::try_from(take).unwrap_or(0));
                    if take < data.len() - skip {
                        break;
                    }
                }
            }
        }
        if immediate_ack {
            let _ = self.send_ack_for(socket);
        }
    }

    /// Send a delayed ACK if the timer fired.
    pub(crate) fn tcp_delayed_ack(&mut self, socket: SocketId, now: Instant) {
        let due = {
            let Ok(record) = self.sockets.record(socket) else {
                return;
            };
            let Kind::Tcp(connection) = &record.kind else {
                return;
            };
            connection
                .ack_deadline
                .is_some_and(|deadline| deadline <= now)
        };
        if due {
            if let Ok(record) = self.sockets.record_mut(socket)
                && let Kind::Tcp(connection) = &mut record.kind
            {
                connection.ack_deadline = None;
                connection.ack_segments = 0;
            }
            self.counters.delack_fires = self.counters.delack_fires.wrapping_add(1);
            self.counters.delayed_acks_sent = self.counters.delayed_acks_sent.wrapping_add(1);
            let _ = self.send_ack_for(socket);
        }
    }

    /// Retry queued sends after the input batch; window updates and partial ACKs can
    /// leave data ready while no timer is armed.
    pub(crate) fn retry_tcp_sends(&mut self) {
        let sockets: Vec<SocketId> = self
            .sockets
            .records
            .iter()
            .filter_map(|(raw_id, record)| match record.kind {
                Kind::Tcp(_) if !record.closed => Some(SocketId(*raw_id)),
                _ => None,
            })
            .collect();
        for socket in sockets {
            let _ = self.tcp_try_send(socket);
        }
    }

    /// The earliest TCP timer deadline across all connections.
    pub(crate) fn next_tcp_deadline(&self) -> Option<Instant> {
        let mut earliest: Option<Instant> = None;
        for record in self.sockets.records.values() {
            let Kind::Tcp(connection) = &record.kind else {
                continue;
            };
            let deadlines = [
                connection.rto_deadline,
                connection.ack_deadline,
                connection.zero_window_deadline,
                connection.time_wait_deadline,
                connection.fin_wait2_deadline,
                connection.keepalive_deadline,
                connection.connect_deadline,
                connection
                    .tlp
                    .deadline
                    .map(|deadline| Instant::from_millis(i64::from(deadline))),
                crate::plpmtud_live::next_deadline(
                    connection,
                    i64::try_from(self.tunables.plpmtud_interval).unwrap_or(i64::MAX),
                    self.tunables.plpmtud_enable != 0,
                ),
            ];
            for deadline in deadlines.into_iter().flatten() {
                earliest = Some(match earliest {
                    Some(current) if current <= deadline => current,
                    _ => deadline,
                });
            }
        }
        earliest
    }

    /// Zero-window probe timer.
    pub(crate) fn tcp_zero_window_timer(&mut self, socket: SocketId, now: Instant) -> bool {
        let probe = {
            let Ok(record) = self.sockets.record(socket) else {
                return false;
            };
            let Kind::Tcp(connection) = &record.kind else {
                return false;
            };
            connection
                .zero_window_deadline
                .is_some_and(|deadline| deadline <= now)
                && record.tx.len()
                    > usize::try_from(connection.snd_nxt.wrapping_sub(connection.snd_una))
                        .unwrap_or(0)
        };
        if !probe {
            return false;
        }
        self.counters.zwprobe_sent = self.counters.zwprobe_sent.wrapping_add(1);
        self.counters.zwprobe_fires = self.counters.zwprobe_fires.wrapping_add(1);
        let _ = self.send_ack_for(socket);
        if let Ok(record) = self.sockets.record_mut(socket)
            && let Kind::Tcp(connection) = &mut record.kind
        {
            connection.zero_window_probes = connection.zero_window_probes.saturating_add(1);
            connection.zero_window_backoff_ms = connection
                .zero_window_backoff_ms
                .saturating_mul(2)
                .min(self.tunables.zwprobe_max_interval);
            connection.zero_window_deadline =
                Some(Instant::from_millis(now.as_millis().saturating_add(
                    i64::try_from(connection.zero_window_backoff_ms).unwrap_or(i64::MAX),
                )));
        }
        true
    }
}

/// Whether a segment at `seq` of `len` bytes touches no queued out-of-order range.
fn ooo_creates_hole(ooo: &[(u32, Vec<u8>)], seq: u32, len: usize) -> bool {
    let end = seq.wrapping_add(u32::try_from(len).unwrap_or(u32::MAX));
    !ooo.iter().any(|(start, data)| {
        let other_end = start.wrapping_add(u32::try_from(data.len()).unwrap_or(u32::MAX));
        seq_ge(end, *start) && seq_ge(other_end, seq)
    })
}

/// Copy `len` bytes of `queue` starting at `start`, clamped to what the queue holds.
pub(crate) fn queue_range(queue: &VecDeque<u8>, start: usize, len: usize) -> Vec<u8> {
    let (front, back) = queue.as_slices();
    let mut bytes = Vec::with_capacity(len.min(queue.len().saturating_sub(start)));
    if start < front.len() {
        let end = front.len().min(start.saturating_add(len));
        bytes.extend_from_slice(&front[start..end]);
    }
    let wanted = len.saturating_sub(bytes.len());
    let back_start = start.saturating_sub(front.len());
    if wanted > 0 && back_start < back.len() {
        let end = back.len().min(back_start + wanted);
        bytes.extend_from_slice(&back[back_start..end]);
    }
    bytes
}

/// Number of separate contiguous ranges held out of order.
fn ooo_hole_count(ooo: &[(u32, Vec<u8>)]) -> usize {
    ooo.iter()
        .filter(|(start, _)| {
            !ooo.iter().any(|(other, data)| {
                let other_end = other.wrapping_add(u32::try_from(data.len()).unwrap_or(u32::MAX));
                seq_lt(*other, *start) && seq_ge(other_end, *start)
            })
        })
        .count()
}

pub(crate) fn advertised_window(record: &crate::socket::SocketRecord) -> u16 {
    let Kind::Tcp(connection) = &record.kind else {
        return 0;
    };
    let free = record.rx_cap.saturating_sub(record.rx.len());
    let floor = usize::from(connection.mss).max(record.rx_cap / 2).max(1);
    let value = if free < floor && free < record.rx_cap {
        0
    } else {
        free >> connection.rcv_wscale
    };
    u16::try_from(value.min(usize::from(u16::MAX))).unwrap_or(u16::MAX)
}
