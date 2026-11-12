// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! In-memory devices and inspection hooks for host tests and the simulator (C04).

use alloc::collections::VecDeque;
use alloc::vec;
use alloc::vec::Vec;

use raven_abi::errno::Errno;

use crate::SocketId;
use crate::device::{ChecksumOffload, Device, DeviceCaps, Medium, TxError};
use crate::socket::{Datagram, Kind};
use crate::stack::{Ipv4Addr, Stack};
use crate::time::Instant;

/// A device whose transmitted frames are queued for the same stack to receive.
///
/// This is a loopback at the device boundary, not a network. A frame queued with
/// [`Device::transmit`] is delivered to the next [`Device::receive`] call.
pub struct LoopbackDevice {
    caps: DeviceCaps,
    queue: VecDeque<Vec<u8>>,
    queued_bytes: usize,
    limit_bytes: usize,
    dropped: usize,
}

impl LoopbackDevice {
    /// Create a loopback with the default Ethernet capabilities and a 256 KiB queue cap.
    #[must_use]
    pub fn new() -> Self {
        Self::with_mtu(1500)
    }

    /// Create a loopback with a specific MTU.
    #[must_use]
    pub fn with_mtu(mtu: usize) -> Self {
        Self {
            caps: DeviceCaps {
                medium: Medium::Ethernet,
                mtu,
                hardware_addr: Some([0x02, 0, 0, 0, 0, 0x01]),
                checksum_offload: ChecksumOffload::None,
            },
            queue: VecDeque::new(),
            queued_bytes: 0,
            limit_bytes: 256 * 1024,
            dropped: 0,
        }
    }

    /// Set the queue byte cap.
    pub fn set_queue_limit(&mut self, bytes: usize) {
        self.limit_bytes = bytes;
    }

    /// Number of queued frames.
    #[must_use]
    pub fn queued_frames(&self) -> usize {
        self.queue.len()
    }

    /// Number of queued bytes.
    #[must_use]
    pub fn queued_bytes(&self) -> usize {
        self.queued_bytes
    }

    /// Frames dropped because the queue was full or the frame was too long.
    #[must_use]
    pub fn dropped(&self) -> usize {
        self.dropped
    }
}

impl Default for LoopbackDevice {
    fn default() -> Self {
        Self::new()
    }
}

impl Device for LoopbackDevice {
    fn capabilities(&self) -> DeviceCaps {
        self.caps
    }

    fn receive(&mut self, _now: Instant, consume: &mut dyn FnMut(&[u8])) -> bool {
        match self.queue.pop_front() {
            Some(frame) => {
                self.queued_bytes = self.queued_bytes.saturating_sub(frame.len());
                consume(&frame);
                true
            }
            None => false,
        }
    }

    fn transmit(
        &mut self,
        _now: Instant,
        len: usize,
        fill: &mut dyn FnMut(&mut [u8]),
    ) -> Result<(), TxError> {
        if len > self.caps.mtu {
            self.dropped = self.dropped.saturating_add(1);
            return Err(TxError::TooLong);
        }
        if self.queued_bytes.saturating_add(len) > self.limit_bytes {
            self.dropped = self.dropped.saturating_add(1);
            return Err(TxError::Busy);
        }
        let mut frame = vec![0u8; len];
        fill(&mut frame);
        self.queued_bytes = self.queued_bytes.saturating_add(len);
        self.queue.push_back(frame);
        Ok(())
    }
}

/// A device that never receives and silently discards transmitted frames.
pub struct NullDevice {
    caps: DeviceCaps,
}

impl NullDevice {
    /// Create a null Ethernet device with a 1500-byte MTU.
    #[must_use]
    pub fn new() -> Self {
        Self {
            caps: DeviceCaps {
                medium: Medium::Ethernet,
                mtu: 1500,
                hardware_addr: Some([0x02, 0, 0, 0, 0, 0x02]),
                checksum_offload: ChecksumOffload::None,
            },
        }
    }
}

impl Default for NullDevice {
    fn default() -> Self {
        Self::new()
    }
}

impl Device for NullDevice {
    fn capabilities(&self) -> DeviceCaps {
        self.caps
    }

    fn receive(&mut self, _now: Instant, _consume: &mut dyn FnMut(&[u8])) -> bool {
        false
    }

    fn transmit(
        &mut self,
        _now: Instant,
        _len: usize,
        _fill: &mut dyn FnMut(&mut [u8]),
    ) -> Result<(), TxError> {
        Ok(())
    }
}

impl<D: Device> Stack<D> {
    /// Enable or disable the raw receive trace used by determinism tests.
    pub fn test_set_trace(&mut self, enabled: bool) {
        self.trace_enabled = enabled;
    }

    /// The raw frames received since the trace was last cleared.
    #[must_use]
    pub fn test_trace(&self) -> &[Vec<u8>] {
        &self.trace
    }

    /// Clear the raw receive trace.
    pub fn test_clear_trace(&mut self) {
        self.trace.clear();
    }

    /// Number of allocated socket records.
    #[must_use]
    pub fn test_socket_count(&self) -> usize {
        self.sockets.records.len()
    }

    /// Force the readable transition state for a socket.
    pub fn test_set_readable(&mut self, socket: SocketId, readable: bool) -> Result<(), Errno> {
        self.sockets.record_mut(socket)?.forced_readable = readable;
        Ok(())
    }

    /// Mark a socket closed without going through `close`.
    pub fn test_set_closed(&mut self, socket: SocketId, closed: bool) -> Result<(), Errno> {
        self.sockets.record_mut(socket)?.peer_closed = closed;
        Ok(())
    }

    /// Set a pending socket error.
    pub fn test_set_error(&mut self, socket: SocketId, error: Option<Errno>) -> Result<(), Errno> {
        self.sockets.record_mut(socket)?.error = error;
        Ok(())
    }

    /// Push received bytes into a TCP socket's receive queue, respecting its cap.
    pub fn test_push_rx(&mut self, socket: SocketId, data: &[u8]) -> Result<usize, Errno> {
        let mut dropped = 0usize;
        let outcome = {
            let record = self.sockets.record_mut(socket)?;
            let room = record.rx_cap.saturating_sub(record.rx.len());
            if data.len() > room {
                dropped = data.len();
                Err(Errno::EAGAIN)
            } else {
                record.rx.extend(data.iter().copied());
                Ok(data.len())
            }
        };
        self.drops = self.drops.saturating_add(dropped);
        outcome
    }

    /// Push a received UDP datagram, respecting the socket receive cap.
    pub fn test_push_udp(
        &mut self,
        socket: SocketId,
        data: &[u8],
        source: (Ipv4Addr, u16),
    ) -> Result<usize, Errno> {
        let mut dropped = 0usize;
        let outcome = {
            let record = self.sockets.record_mut(socket)?;
            let Kind::Udp { datagrams, .. } = &mut record.kind else {
                return Err(Errno::ENOTSOCK);
            };
            if data.len() > record.rx_cap {
                dropped = data.len();
                Err(Errno::EAGAIN)
            } else {
                datagrams.push_back(Datagram {
                    data: data.to_vec(),
                    src: (crate::ipv6_stack::IpAddr::V4(source.0), source.1),
                });
                Ok(data.len())
            }
        };
        self.drops = self.drops.saturating_add(dropped);
        outcome
    }

    /// Allocate a TCP record and mark it as a completed connection on `listener`.
    pub fn test_pending_connection(&mut self, listener: SocketId) -> Result<SocketId, Errno> {
        if !matches!(self.sockets.record(listener)?.kind, Kind::Listener { .. }) {
            return Err(Errno::ENOTSOCK);
        }
        let rx_cap = self.config.rx_queue_bytes;
        let tx_cap = self.config.tx_queue_bytes;
        let connection = self.sockets.alloc(
            Kind::Tcp {
                _remote: None,
                local_port: 0,
            },
            rx_cap,
            tx_cap,
        )?;
        let record = self.sockets.record_mut(listener)?;
        let Kind::Listener { pending, .. } = &mut record.kind else {
            return Err(Errno::ENOTSOCK);
        };
        pending.push_back(connection);
        Ok(connection)
    }
}
