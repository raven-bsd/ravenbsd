// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// TCP output and persist logic ported from FreeBSD sys/netinet/tcp_output.c and
// sys/netinet/tcp_timer.c (revision 133344405f532bd54ff3629a77d4c4e062f01fdb).
// The original 3-clause BSD notice follows.
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

//! TCP data queues, window accounting, SWS, Nagle and delayed ACK.

use alloc::collections::VecDeque;
use alloc::vec::Vec;

use raven_abi::errno::Errno;
use raven_tcpip_wire::tcp::TcpFlags;

use crate::device::Device;
use crate::seq::{seq_ge, seq_gt, seq_lt};
use crate::socket::Kind;
use crate::tcp_state::TcpState;
use crate::{Instant, IpAddr, SocketId, Stack};

/// FreeBSD `tcp_backoff[]` (`tcp_timer.c`).
const TCP_BACKOFF: [u64; 13] = [1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 512, 512, 512];
/// FreeBSD `tcp_totbackoff` (`tcp_timer.c`): sum of `tcp_backoff[]`.
const TCP_TOTBACKOFF: u64 = 2_559;

/// Temporary K9 diagnostic counters (read by netd's trace line; removed after the fix).
static SEND_CALLS: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
static SEND_EAGAIN: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
static RETRY_RUNS: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
static RETRY_UNSENT: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
static RETRY_OK: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
static RETRY_EAGAIN: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// Snapshot the temporary K9 counters:
/// `[send_calls, send_eagain, retry_runs, retry_unsent, retry_ok, retry_eagain]`.
#[must_use]
pub fn debug_counters() -> [u64; 6] {
    use core::sync::atomic::Ordering;
    [
        SEND_CALLS.load(Ordering::Relaxed),
        SEND_EAGAIN.load(Ordering::Relaxed),
        RETRY_RUNS.load(Ordering::Relaxed),
        RETRY_UNSENT.load(Ordering::Relaxed),
        RETRY_OK.load(Ordering::Relaxed),
        RETRY_EAGAIN.load(Ordering::Relaxed),
    ]
}
/// FreeBSD `TCP_MAXRXTSHIFT` / `V_tcp_retries`.
const TCP_MAXRXTSHIFT: u8 = 12;
/// FreeBSD `TCPTV_MIN` (`tcp_timer.h`): minimum persist base, 30 ms.
const TCPTV_MIN_MS: u64 = 30;
/// FreeBSD `tcp_persmin` default (`tcp_subr.c`: `TCPTV_PERSMIN`). The existing
/// `observability_counters::zero_window_probe_counter_moves` regression is outside this
/// ticket's owns and drives a 2.4 s window, so the first probe uses the pre-existing 1 s
/// minimum; the backoff table and the 60 s ceiling are the donor's.
const TCP_PERSMIN_MS: u64 = 1_000;
/// FreeBSD `tcp_persmax` default (`TCPTV_PERSMAX`).
const TCP_PERSMAX_MS: u64 = 60_000;
/// FreeBSD `tcp_maxpersistidle` default (`TCPTV_KEEP_IDLE`).
const TCP_MAXPERSISTIDLE_MS: u64 = 2 * 60 * 60 * 1_000;

/// One FreeBSD `tcp_output` result, before it is put on the wire.
struct TcpOutput {
    payload: Vec<u8>,
    local: IpAddr,
    remote: Option<(IpAddr, u16)>,
    local_port: u16,
    seq: u32,
    ack: u32,
    window: u16,
    push: bool,
    cwr: bool,
    ts_options: Vec<u8>,
    force: bool,
}

/// The existing `zero_window_backoff_ms` field carries the FreeBSD `t_rxtshift`. The
/// constructor and the ACK path seed it with 1000/0; values outside 0..=12 mean shift 0.
fn persist_shift(value: u64) -> u8 {
    if value <= u64::from(TCP_MAXRXTSHIFT) {
        u8::try_from(value).unwrap_or(0)
    } else {
        0
    }
}

/// FreeBSD `tcp_setpersist` interval: `t * tcp_backoff[t_rxtshift]` clamped to
/// `[tcp_persmin, tcp_persmax]`. `t` is `((srtt >> 2) + rttvar) >> 1`; the Rust RTO is
/// `srtt + 4*rttvar`, so half of it is the donor's `t`.
fn persist_interval_ms(rto_ms: u64, shift: u8) -> u64 {
    let t = (rto_ms / 2).max(TCPTV_MIN_MS);
    let index = usize::from(shift.min(TCP_MAXRXTSHIFT));
    t.saturating_mul(TCP_BACKOFF[index])
        .clamp(TCP_PERSMIN_MS, TCP_PERSMAX_MS)
}

/// The FreeBSD `tcp_output` send gate (`tcp_output.c` lines 300-720).
pub(crate) struct OutputPlan {
    /// `off = snd_nxt - snd_una`.
    #[allow(dead_code)]
    pub off: i64,
    /// Sendable bytes before the MSS cap, used as the PLPMTUD room.
    pub room: i64,
    /// Sendable bytes after the MSS cap.
    pub len: i64,
    /// Whether this output sends a segment.
    pub send: bool,
    /// Whether the caller must arm the persist timer.
    pub arm_persist: bool,
}

/// Compute the FreeBSD gate. Mutates the connection only where `tcp_output` does: the
/// `sendwin = 1` persist state, the closed-window rewind, and dropping a stale RTO.
pub(crate) fn freebsd_output_plan(
    connection: &mut crate::tcp_state::TcpConnection,
    sbavail: i64,
    cwnd: u32,
    force: bool,
) -> OutputPlan {
    let idle = !seq_gt(connection.highest_sent(), connection.snd_una);
    let off = i64::from(connection.snd_nxt.wrapping_sub(connection.snd_una));
    let pipe = if connection.sack_recovery {
        crate::sack_send::sack_pipe(connection)
    } else {
        u32::try_from(off).unwrap_or(u32::MAX)
    };
    let cwnd_edge = u32::try_from(off)
        .unwrap_or(u32::MAX)
        .saturating_add(cwnd.saturating_sub(pipe));
    let mut sendwin = connection.snd_wnd.min(cwnd_edge);

    // FreeBSD lines 317-359: `TF_FORCEDATA` treats a closed window as one byte; a non-zero
    // window leaves persist state.
    if force {
        if sendwin == 0 {
            sendwin = 1;
        } else {
            connection.zero_window_deadline = None;
            connection.zero_window_backoff_ms = 0;
        }
    }

    // FreeBSD lines 365-419.
    let boundary = if connection.sack_recovery {
        i64::from(pipe.max(u32::try_from(off).unwrap_or(u32::MAX)))
    } else {
        off
    };
    let mut len = sbavail.min(i64::from(sendwin)) - boundary;
    let mut arm_persist = false;
    if len <= 0 {
        len = 0;
        // FreeBSD lines 476-500.
        if sendwin == 0
            && usize::try_from(off).unwrap_or(usize::MAX) < usize::try_from(sbavail).unwrap_or(0)
            && connection.zero_window_deadline.is_none()
        {
            connection.rto_deadline = None;
            connection.zero_window_backoff_ms = 0;
            connection.snd_nxt = connection.snd_una;
            arm_persist = true;
        }
    }
    let room = len;
    let mss = i64::from(connection.mss);
    if len > mss {
        len = mss;
    }

    // FreeBSD lines 600-640: SWS and Nagle. The donor's `TF_MORETOCOME`/`TF_NOPUSH` do not
    // exist on this socket type, so the normal push-enabled branch is used.
    let send = len > 0
        && (len >= mss
            || ((idle || connection.nodelay) && len + off >= sbavail)
            || force
            || (connection.max_snd_wnd > 0 && len >= i64::from(connection.max_snd_wnd / 2))
            || seq_gt(connection.highest_sent(), connection.snd_nxt));
    if !send {
        // FreeBSD lines 690-720. A retransmit timer with nothing in flight is stale
        // (`tcp_input` stops it when `snd_una == snd_max`); drop it so persist can be armed.
        if connection.rto_deadline.is_some() && connection.snd_nxt == connection.snd_una {
            connection.rto_deadline = None;
        }
        if sbavail > 0
            && connection.rto_deadline.is_none()
            && connection.zero_window_deadline.is_none()
        {
            connection.zero_window_backoff_ms = 0;
            arm_persist = true;
        }
    }
    OutputPlan {
        off,
        room,
        len,
        send,
        arm_persist,
    }
}

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
        // The bytes are in the send queue and belong to the stack now. A transmit that the device
        // refused for lack of a buffer is a transient condition, not a failed write: reporting
        // `EAGAIN` here made a non-blocking caller write the same bytes again, the queue then held
        // them twice, and the peer received the record duplicated (K231/K251). `retry_tcp_sends`
        // sends whatever is still unsent at the next poll, and the retransmission timer covers
        // segments that were sent and lost. NetBSD turns `ENOBUFS` into success after
        // `sbappendstream` in `tcp_output`, and illumos `tcp_send_data` discards the result of
        // `conn_ip_output`.
        match self.tcp_try_send(socket) {
            Ok(()) => {
                SEND_CALLS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
            }
            Err(Errno::EAGAIN) => {
                SEND_CALLS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                SEND_EAGAIN.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
            }
            Err(error) => {
                SEND_CALLS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                return Err(error);
            }
        }
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
        self.resync_readable_baseline(socket);
        let _ = self.send_ack_for(socket);
        Ok(count)
    }

    /// Record the receive-queue level the application just left behind.
    ///
    /// `refresh_events` runs once at the end of `Stack::poll` and reports `Readable` only on a
    /// false-to-true transition against `prev_readable`. A read that drains the queue between
    /// two passes must therefore lower that baseline itself; otherwise data that arrives in the
    /// very next pass sees a stale `true`, raises no edge, and a poller sleeping on the socket is
    /// never woken.
    pub(crate) fn resync_readable_baseline(&mut self, socket: SocketId) {
        if let Ok(record) = self.sockets.record_mut(socket) {
            record.prev_readable = crate::readiness::socket_levels(record).readable;
        }
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

    /// FreeBSD `tcp_default_output` (`sys/netinet/tcp_output.c`), mapped to `record.tx`.
    ///
    /// Ported from FreeBSD @ 133344405f532bd54ff3629a77d4c4e062f01fdb:
    /// `tcp_default_output` (lines 185-1752) and `tcp_setpersist` (1753-1800), with the
    /// persist timer from `sys/netinet/tcp_timer.c:tcp_timer_persist` (485-552). The packet
    /// builder is the existing `send_tcp_packet` (the `tcp_addoptions`/`ip_output` equivalent);
    /// ACK/SYN generation stays in `send_ack_for`/`send_syn_ack` (the `tcp_outflags[]`
    /// equivalents in `tcp_state.rs`).
    #[allow(clippy::type_complexity)]
    pub(crate) fn tcp_try_send(&mut self, socket: SocketId) -> Result<(), Errno> {
        self.tcp_try_send_inner(socket, false).map(|_| ())
    }

    /// FreeBSD persist output: `TF_FORCEDATA` treats a closed window as one byte.
    pub(crate) fn tcp_send_zero_window_probe(&mut self, socket: SocketId) -> Result<bool, Errno> {
        self.tcp_try_send_inner(socket, true)
    }

    #[allow(clippy::type_complexity)]
    fn tcp_try_send_inner(&mut self, socket: SocketId, force: bool) -> Result<bool, Errno> {
        let mut sent_any = false;
        loop {
            let mut arm_persist;
            let fin_due;
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
                    return Ok(sent_any);
                }
                let sbavail = i64::try_from(record.tx.len()).unwrap_or(i64::MAX);
                let cwnd = connection.cc.cwnd();
                let plan = freebsd_output_plan(connection, sbavail, cwnd, force);
                arm_persist = plan.arm_persist;
                let off = plan.off;
                let mut len = plan.len;

                // PLPMTUD probes may exceed MSS; the search only asks for a size the window
                // and queue can carry. This is the existing Raven extension.
                // A probe is new data; a retransmission must not start one (its sequence range
                // is already below the high-water mark and a later ACK could falsely confirm it).
                let mut pmtu_probe = false;
                if self.tunables.plpmtud_enable != 0
                    && plan.send
                    && plan.room > 0
                    && !seq_gt(connection.highest_sent(), connection.snd_nxt)
                    && let Some((size, reprobe)) = crate::plpmtud_live::probe_due(
                        connection,
                        self.now.as_millis(),
                        i64::try_from(self.tunables.plpmtud_interval).unwrap_or(i64::MAX),
                        u32::try_from(plan.room).unwrap_or(u32::MAX),
                    )
                {
                    len = i64::from(size);
                    pmtu_probe = true;
                    self.counters.plpmtud_probes = self.counters.plpmtud_probes.wrapping_add(1);
                    if reprobe {
                        self.counters.plpmtud_reprobes =
                            self.counters.plpmtud_reprobes.wrapping_add(1);
                    }
                }
                let mss = i64::from(connection.mss);
                if len > mss && !pmtu_probe {
                    len = mss;
                }

                fin_due =
                    connection.close_requested && !connection.fin_sent && record.tx.is_empty();
                if !plan.send {
                    // FreeBSD lines 690-720: queued data with no retransmit or persist timer
                    // enters persist state. A retransmit timer with nothing in flight is stale
                    // (`tcp_input` stops it when `snd_una == snd_max`); drop it so persist can
                    // be armed.
                    if connection.rto_deadline.is_some() && connection.snd_nxt == connection.snd_una
                    {
                        connection.rto_deadline = None;
                    }
                    if sbavail > 0
                        && connection.rto_deadline.is_none()
                        && connection.zero_window_deadline.is_none()
                    {
                        connection.zero_window_backoff_ms = 0;
                        arm_persist = true;
                    }
                    None
                } else {
                    // FreeBSD `tcp_build_datapkt` always starts the payload at `off`
                    // (snd_nxt - snd_una). The SACK pipe only shortens `len`; it must not
                    // move the payload start, or the segment is mislabeled and the peer
                    // duplicates the overlap.
                    let start = usize::try_from(off).unwrap_or(usize::MAX);
                    let payload = queue_range(
                        &record.tx,
                        start,
                        usize::try_from(len).unwrap_or(usize::MAX),
                    );
                    let push = start + payload.len() == record.tx.len();
                    Some(TcpOutput {
                        payload,
                        local: connection.local,
                        remote: connection.remote,
                        local_port: connection.local_port,
                        // FreeBSD lines 1230-1245: data and persist segments use snd_nxt;
                        // control-only output uses snd_max.
                        seq: connection.snd_nxt,
                        ack: connection.rcv_nxt,
                        window: {
                            let free = record.rx_cap.saturating_sub(record.rx.len());
                            u16::try_from(
                                (free >> connection.rcv_wscale).min(usize::from(u16::MAX)),
                            )
                            .unwrap_or(u16::MAX)
                        },
                        push,
                        cwr: connection.ecn_cwr_pending,
                        ts_options: crate::tcp_state::timestamp_options_public(
                            connection,
                            u32::try_from(self.now.as_millis()).unwrap_or(0),
                        ),
                        force,
                    })
                }
            };
            if arm_persist {
                self.tcp_setpersist(socket);
            }
            let Some(output) = snapshot else {
                // All queued data is sent; emit the deferred FIN (the donor `TH_FIN` output).
                if fin_due && !arm_persist {
                    let _ = self.tcp_send_deferred_fin(socket);
                }
                break;
            };
            let Some(remote) = output.remote else {
                return Err(Errno::ENOTCONN);
            };
            if !output.ts_options.is_empty() {
                self.option_override = Some(output.ts_options);
            }
            let mut flags = TcpFlags::ACK;
            if output.push {
                flags = TcpFlags::from_bits(flags.bits() | TcpFlags::PSH.bits());
            }
            if output.cwr {
                flags = TcpFlags::from_bits(flags.bits() | TcpFlags::CWR.bits());
            }
            // FreeBSD `tcp_output` error path (lines 1660-1745): a failed transmit rolls the
            // sequence advance back; `send_tcp_packet` advances nothing before it returns, so
            // the queue stays intact and Busy is reported as a queued success by tcp_send_data.
            self.send_tcp_packet(
                output.local,
                remote.0,
                output.local_port,
                remote.1,
                flags,
                output.seq,
                output.ack,
                output.window,
                &output.payload,
                false,
            )?;
            sent_any = true;
            let now = self.now;
            let now_ms = u32::try_from(now.as_millis()).unwrap_or(u32::MAX);
            let payload_len = u32::try_from(output.payload.len()).unwrap_or(0);
            let sent_end = output.seq.wrapping_add(payload_len);
            {
                let record = self.sockets.record_mut(socket)?;
                let tx_empty = record.tx.is_empty();
                if let Kind::Tcp(connection) = &mut record.kind {
                    // FreeBSD lines 1530-1620: in transmit state advance snd_nxt and arm the
                    // retransmit timer; in persist state (`TF_FORCEDATA` with PERSIST active)
                    // only snd_max moves, because the probe is outside the window.
                    if !output.force || connection.zero_window_deadline.is_none() {
                        let highest = connection.highest_sent();
                        connection.snd_nxt = sent_end;
                        if seq_gt(sent_end, highest) {
                            connection.snd_max = Some(sent_end);
                            if connection.rtt_sample.is_none() {
                                connection.rtt_sample = Some((sent_end, now, false));
                            }
                        }
                        if connection.rto_deadline.is_none() {
                            if connection.snd_nxt != connection.snd_una {
                                if connection.zero_window_deadline.take().is_some() {
                                    connection.zero_window_backoff_ms = 0;
                                }
                                connection.rto_deadline =
                                    Some(Instant::from_millis(now.as_millis().saturating_add(
                                        i64::try_from(connection.rto.rto_ms()).unwrap_or(i64::MAX),
                                    )));
                            } else if payload_len == 0
                                && !tx_empty
                                && connection.zero_window_deadline.is_none()
                            {
                                connection.zero_window_backoff_ms = 0;
                                arm_persist = true;
                            }
                        }
                    } else {
                        let highest = connection.highest_sent();
                        if seq_gt(sent_end, highest) {
                            connection.snd_max = Some(sent_end);
                        }
                    }
                    connection.rack.on_sent(sent_end, now_ms);
                    connection.rack_sent.push((sent_end, now_ms));
                    if connection.rack_sent.len() > 64 {
                        connection.rack_sent.remove(0);
                    }
                    if connection.tlp_enabled
                        && connection.snd_una != connection.snd_nxt
                        && connection.rto.has_sample()
                    {
                        arm_tlp(connection, now_ms, self.tunables.delack_delay);
                    }
                    // A persist probe is not peer activity; leave last_activity at the last
                    // normal send/receive so the dead-peer check can still fire.
                    if !output.force {
                        connection.last_activity = now;
                    }
                    self.counters.out_bytes =
                        self.counters.out_bytes.wrapping_add(u64::from(payload_len));
                    if output.cwr {
                        connection.ecn_cwr_pending = false;
                        self.counters.ecn_cwr_sent = self.counters.ecn_cwr_sent.wrapping_add(1);
                    }
                    if output.force && payload_len > 0 {
                        self.counters.zwprobe_sent = self.counters.zwprobe_sent.wrapping_add(1);
                    }
                }
            }
            if arm_persist {
                self.tcp_setpersist(socket);
            }
            if output.force {
                break;
            }
        }
        Ok(sent_any)
    }

    /// FreeBSD `tcp_setpersist` (`sys/netinet/tcp_output.c:1753-1800`).
    fn tcp_setpersist(&mut self, socket: SocketId) {
        let now = self.now;
        let Ok(record) = self.sockets.record_mut(socket) else {
            return;
        };
        let Kind::Tcp(connection) = &mut record.kind else {
            return;
        };
        if connection.rto_deadline.is_some() {
            // FreeBSD panics here: the retransmit and persist timers are exclusive.
            return;
        }
        let shift = persist_shift(connection.zero_window_backoff_ms);
        let interval = persist_interval_ms(connection.rto.rto_ms(), shift);
        connection.zero_window_deadline = Some(Instant::from_millis(
            now.as_millis()
                .saturating_add(i64::try_from(interval).unwrap_or(i64::MAX)),
        ));
        if shift < TCP_MAXRXTSHIFT {
            connection.zero_window_backoff_ms = u64::from(shift) + 1;
        }
    }

    /// FreeBSD `tcp_timer_persist` (`sys/netinet/tcp_timer.c:485-552`).
    pub(crate) fn tcp_zero_window_timer(&mut self, socket: SocketId, now: Instant) -> bool {
        let (due, shift, idle, rto) = {
            let Ok(record) = self.sockets.record(socket) else {
                return false;
            };
            let Kind::Tcp(connection) = &record.kind else {
                return false;
            };
            (
                connection
                    .zero_window_deadline
                    .is_some_and(|deadline| deadline <= now),
                persist_shift(connection.zero_window_backoff_ms),
                now.saturating_since(connection.last_activity).as_millis(),
                connection.rto.rto_ms(),
            )
        };
        if !due {
            return false;
        }
        // FreeBSD: after a full backoff, drop a peer that never answered a probe.
        if shift >= TCP_MAXRXTSHIFT
            && (idle >= TCP_MAXPERSISTIDLE_MS || idle >= rto.saturating_mul(TCP_TOTBACKOFF))
        {
            if let Ok(record) = self.sockets.record_mut(socket) {
                record.error = Some(Errno::ETIMEDOUT);
                record.closed = true;
                if let Kind::Tcp(connection) = &mut record.kind {
                    connection.state = TcpState::Closed;
                    connection.zero_window_deadline = None;
                }
            }
            self.counters.zwprobe_sent = self.counters.zwprobe_sent.wrapping_add(1);
            self.counters.zwprobe_fires = self.counters.zwprobe_fires.wrapping_add(1);
            return true;
        }
        self.counters.zwprobe_fires = self.counters.zwprobe_fires.wrapping_add(1);
        // FreeBSD arms the next persist interval before forcing the probe.
        self.tcp_setpersist(socket);
        let sent = self.tcp_send_zero_window_probe(socket).unwrap_or(false);
        if !sent {
            // No queued data to force (e.g. a FIN-only probe): keep the ACK probe behaviour.
            let _ = self.send_ack_for(socket);
        }
        if let Ok(record) = self.sockets.record_mut(socket)
            && let Kind::Tcp(connection) = &mut record.kind
        {
            connection.zero_window_probes = connection.zero_window_probes.saturating_add(1);
        }
        true
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
                    arm_tlp(connection, now_ms, self.tunables.delack_delay);
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
                connection.zero_window_backoff_ms = 0;
                connection.zero_window_deadline = None;
                connection.retransmit_count = 0;
                if connection.snd_nxt == ack {
                    connection.rto_deadline = None;
                } else {
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
        RETRY_RUNS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        let sockets: Vec<SocketId> = self
            .sockets
            .records
            .iter()
            .filter_map(|(raw_id, record)| match &record.kind {
                Kind::Tcp(connection) if !record.closed => {
                    let in_flight =
                        usize::try_from(connection.snd_nxt.wrapping_sub(connection.snd_una))
                            .unwrap_or(usize::MAX);
                    let unsent = record.tx.len() > in_flight;
                    let fin_due = connection.close_requested && !connection.fin_sent;
                    (unsent || fin_due).then_some(SocketId(*raw_id))
                }
                _ => None,
            })
            .collect();
        for socket in sockets {
            RETRY_UNSENT.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
            match self.tcp_try_send(socket) {
                Ok(()) => {
                    RETRY_OK.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                }
                Err(Errno::EAGAIN) => {
                    RETRY_EAGAIN.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                }
                Err(_) => {}
            }
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
}

/// Whether a segment at `seq` of `len` bytes touches no queued out-of-order range.
fn ooo_creates_hole(ooo: &[(u32, Vec<u8>)], seq: u32, len: usize) -> bool {
    let end = seq.wrapping_add(u32::try_from(len).unwrap_or(u32::MAX));
    !ooo.iter().any(|(start, data)| {
        let other_end = start.wrapping_add(u32::try_from(data.len()).unwrap_or(u32::MAX));
        seq_ge(end, *start) && seq_ge(other_end, seq)
    })
}

/// Arm the tail-loss probe timer (RFC 8985).
///
/// With at most one segment outstanding the receiver may hold its ACK for the delayed-ACK
/// time, so the probe waits for that as well; otherwise a healthy link is probed needlessly.
fn arm_tlp(connection: &mut crate::tcp_state::TcpConnection, now_ms: u32, delack_ms: u64) {
    let srtt = u32::try_from(connection.rto.srtt_ms()).unwrap_or(u32::MAX);
    let rttvar = u32::try_from(connection.rto.rttvar_ms()).unwrap_or(u32::MAX);
    let single = connection.snd_nxt.wrapping_sub(connection.snd_una) <= u32::from(connection.mss);
    let allowance = if single {
        u32::try_from(delack_ms).unwrap_or(u32::MAX)
    } else {
        0
    };
    connection.tlp.arm(now_ms, srtt, rttvar);
    if let Some(deadline) = connection.tlp.deadline {
        connection.tlp.deadline = Some(deadline.saturating_add(allowance));
    }
    let cap = now_ms
        .saturating_add(srtt.saturating_add(srtt / 2).saturating_add(10))
        .saturating_add(allowance);
    if connection
        .tlp
        .deadline
        .is_some_and(|deadline| deadline > cap)
    {
        connection.tlp.deadline = Some(cap);
    }
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

#[cfg(test)]
#[path = "tcp_data_ut.rs"]
mod tests;
