// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! In-process Ethernet loopback used by the loopback milestone.
//!
//! The native stack also delivers `127.0.0.1` locally without transmitting a frame; this device
//! keeps the stack's device boundary honest and is the seam B12b replaces with the virtio-net ring.

use alloc::collections::VecDeque;
use alloc::vec;
use alloc::vec::Vec;

use raven_netcore::{ChecksumOffload, Device, DeviceCaps, Instant, Medium, TxError};

/// Byte cap for the loopback queue.
pub const LOOPBACK_QUEUE_BYTES: usize = 256 * 1024;

/// An Ethernet device whose transmitted frames are received by the same stack.
pub struct Loopback {
    caps: DeviceCaps,
    queue: VecDeque<Vec<u8>>,
    queued_bytes: usize,
}

impl Loopback {
    /// Create the loopback device.
    #[must_use]
    pub fn new(mac: [u8; 6]) -> Self {
        Self {
            caps: DeviceCaps {
                medium: Medium::Ethernet,
                mtu: 1514,
                hardware_addr: Some(mac),
                checksum_offload: ChecksumOffload::None,
            },
            queue: VecDeque::new(),
            queued_bytes: 0,
        }
    }

    /// Number of frames waiting for the stack.
    #[must_use]
    pub fn queued_frames(&self) -> usize {
        self.queue.len()
    }
}

impl Device for Loopback {
    fn capabilities(&self) -> DeviceCaps {
        self.caps
    }

    fn receive(&mut self, _now: Instant, consume: &mut dyn FnMut(&[u8])) -> bool {
        let Some(frame) = self.queue.pop_front() else {
            return false;
        };
        self.queued_bytes = self.queued_bytes.saturating_sub(frame.len());
        consume(&frame);
        true
    }

    fn transmit(
        &mut self,
        _now: Instant,
        len: usize,
        fill: &mut dyn FnMut(&mut [u8]),
    ) -> Result<(), TxError> {
        if len > self.caps.mtu {
            return Err(TxError::TooLong);
        }
        if self.queued_bytes.saturating_add(len) > LOOPBACK_QUEUE_BYTES {
            return Err(TxError::Busy);
        }
        let mut frame = vec![0u8; len];
        fill(&mut frame);
        self.queued_bytes = self.queued_bytes.saturating_add(len);
        self.queue.push_back(frame);
        Ok(())
    }
}
