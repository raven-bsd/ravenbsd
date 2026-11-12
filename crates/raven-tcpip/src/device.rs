// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The link-layer boundary (contract C15).

use crate::Instant;

/// The medium a device carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Medium {
    /// Ethernet II framing.
    Ethernet,
    /// Raw IP packets without a link header.
    Ip,
}

/// Which checksums a device can complete in hardware.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChecksumOffload {
    /// No offload; the stack verifies and computes every checksum.
    None,
    /// Receive checksums are verified by the device.
    Rx,
    /// Transmit checksums are completed by the device.
    Tx,
    /// Both directions are offloaded.
    Both,
}

/// What a device can do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeviceCaps {
    /// Link medium.
    pub medium: Medium,
    /// Maximum frame size the device accepts, including the link header.
    pub mtu: usize,
    /// Hardware address, when the medium has one.
    pub hardware_addr: Option<[u8; 6]>,
    /// Checksum offload capabilities.
    pub checksum_offload: ChecksumOffload,
}

/// A transmit failure reported by a device.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TxError {
    /// The device has no room for another frame right now.
    Busy,
    /// The frame exceeds the device MTU.
    TooLong,
    /// The link is down.
    Down,
}

/// The one link-layer trait the stack uses.
pub trait Device {
    /// Describe the device.
    fn capabilities(&self) -> DeviceCaps;

    /// Hands the next frame to `consume` and returns true, or returns false when the queue is
    /// empty.
    fn receive(&mut self, now: Instant, consume: &mut dyn FnMut(&[u8])) -> bool;

    /// Reserves `len` bytes, lets `fill` write them, then queues the frame.
    fn transmit(
        &mut self,
        now: Instant,
        len: usize,
        fill: &mut dyn FnMut(&mut [u8]),
    ) -> Result<(), TxError>;
}
