// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! GENET descriptor status words, ring indices and the B39 `netd` frame-ring bridge.
//!
//! The hardware descriptor is three 32-bit MMIO words: status, address low, address high.
//! Ownership is explicit: the driver hands an RX descriptor to the device by clearing `OWN`,
//! and the device returns it by setting `OWN` and the frame bounds. The ring index arithmetic
//! follows the 16-bit hardware counters; logical indices stay `u32` so a long-lived ring wraps
//! without losing the producer/consumer relationship.

use raven_proto_net::ring::{DESC_ERROR, DESC_FIRST, DESC_LAST, Desc};

/// Size of one pooled frame buffer in bytes.
pub const BUFFER_LEN: usize = 2048;
/// Two-byte receive alignment applied by `RBUF_ALIGN_2B`.
pub const ETHER_ALIGN: usize = 2;
/// Largest descriptor count accepted by the ring model.
pub const MAX_RING_ENTRIES: u16 = 4096;

/// Default TX interrupt threshold: one interrupt per ten packets.
pub const TX_DONE_THRESHOLD: u32 = 10;
/// Default RX interrupt threshold: one interrupt per ten packets.
pub const RX_DONE_THRESHOLD: u32 = 10;
/// Default RX mitigation timeout in microseconds.
pub const RX_TIMEOUT_US: u32 = 57;
/// Default RX xon threshold (in descriptors).
pub const RX_XON_THRESHOLD: u16 = 5;
/// Default RX xoff threshold (in descriptors).
pub const RX_XOFF_THRESHOLD: u16 = 16;

/// RX status: buffer length mask (bits 27:16).
pub const RX_STATUS_BUFLEN_MASK: u32 = 0x0fff_0000;
/// RX status: buffer length shift.
pub const RX_STATUS_BUFLEN_SHIFT: u32 = 16;
/// RX status: device owns the descriptor.
pub const RX_STATUS_OWN: u32 = 1 << 15;
/// RX status: end of packet.
pub const RX_STATUS_EOP: u32 = 1 << 14;
/// RX status: start of packet.
pub const RX_STATUS_SOP: u32 = 1 << 13;
/// RX status: length error.
pub const RX_STATUS_LEN_ERR: u32 = 1 << 4;
/// RX status: frame error.
pub const RX_STATUS_FRAME_ERR: u32 = 1 << 3;
/// RX status: receive error.
pub const RX_STATUS_RX_ERR: u32 = 1 << 2;
/// RX status: CRC error.
pub const RX_STATUS_CRC_ERR: u32 = 1 << 1;
/// RX status: overrun error.
pub const RX_STATUS_OVRUN_ERR: u32 = 1 << 0;
/// RX status: all error bits.
pub const RX_STATUS_ALL_ERRS: u32 = 0x1f;

/// TX status: buffer length mask (bits 27:16).
pub const TX_STATUS_BUFLEN_MASK: u32 = 0x0fff_0000;
/// TX status: buffer length shift.
pub const TX_STATUS_BUFLEN_SHIFT: u32 = 16;
/// TX status: device owns the descriptor.
pub const TX_STATUS_OWN: u32 = 1 << 15;
/// TX status: end of packet.
pub const TX_STATUS_EOP: u32 = 1 << 14;
/// TX status: start of packet.
pub const TX_STATUS_SOP: u32 = 1 << 13;
/// TX status: all queue-tag bits.
pub const TX_STATUS_QTAG: u32 = 0x0000_1f80;
/// TX status: append CRC.
pub const TX_STATUS_CRC: u32 = 1 << 6;
/// TX status: queue-tag shift.
pub const TX_STATUS_QTAG_SHIFT: u32 = 7;
/// TX status: queue-tag mask.
pub const TX_STATUS_QTAG_MASK: u32 = 0x3f;

/// The error bits of one RX descriptor.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RxErrors(u8);

impl RxErrors {
    /// No error.
    pub const NONE: Self = Self(0);
    /// Overrun.
    pub const OVERRUN: Self = Self(1 << 0);
    /// CRC.
    pub const CRC: Self = Self(1 << 1);
    /// Receive error.
    pub const RX: Self = Self(1 << 2);
    /// Frame error.
    pub const FRAME: Self = Self(1 << 3);
    /// Length error.
    pub const LENGTH: Self = Self(1 << 4);

    /// Build from the low five status bits.
    #[must_use]
    pub const fn from_bits(bits: u8) -> Self {
        Self(bits & RX_STATUS_ALL_ERRS as u8)
    }

    /// The encoded low five bits.
    #[must_use]
    pub const fn bits(self) -> u8 {
        self.0
    }

    /// True when any error bit is set.
    #[must_use]
    pub const fn any(self) -> bool {
        self.0 != 0
    }

    /// True for an overrun.
    #[must_use]
    pub const fn overrun(self) -> bool {
        self.0 & Self::OVERRUN.0 != 0
    }

    /// True for a CRC error.
    #[must_use]
    pub const fn crc(self) -> bool {
        self.0 & Self::CRC.0 != 0
    }

    /// True for a generic receive error.
    #[must_use]
    pub const fn receive(self) -> bool {
        self.0 & Self::RX.0 != 0
    }

    /// True for a frame error.
    #[must_use]
    pub const fn frame(self) -> bool {
        self.0 & Self::FRAME.0 != 0
    }

    /// True for a length error.
    #[must_use]
    pub const fn length(self) -> bool {
        self.0 & Self::LENGTH.0 != 0
    }
}

/// Why an RX status word does not describe a deliverable frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RxError {
    /// The device still owns the descriptor.
    Busy,
    /// A device error bits are set.
    Device(RxErrors),
    /// The start-of-packet bit is missing.
    MissingStart,
    /// The end-of-packet bit is missing.
    MissingEnd,
    /// The reported length is shorter than the two-byte alignment.
    TooShort(usize),
    /// The reported length exceeds the packet buffer.
    Oversize(usize),
}

/// A validated received frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RxFrame {
    /// Payload length after the two-byte alignment is removed.
    pub length: usize,
    /// Raw length reported by the device.
    pub raw_length: usize,
}

/// One RX descriptor status word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RxStatus(u32);

impl RxStatus {
    /// Wrap a raw status word.
    #[must_use]
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    /// The raw status word.
    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }

    /// Raw buffer length in the status word.
    #[must_use]
    pub const fn length(self) -> usize {
        ((self.0 & RX_STATUS_BUFLEN_MASK) >> RX_STATUS_BUFLEN_SHIFT) as usize
    }

    /// True when the device owns the descriptor.
    #[must_use]
    pub const fn own(self) -> bool {
        self.0 & RX_STATUS_OWN != 0
    }

    /// True when this descriptor starts a frame.
    #[must_use]
    pub const fn sop(self) -> bool {
        self.0 & RX_STATUS_SOP != 0
    }

    /// True when this descriptor ends a frame.
    #[must_use]
    pub const fn eop(self) -> bool {
        self.0 & RX_STATUS_EOP != 0
    }

    /// The error bits.
    #[must_use]
    pub const fn errors(self) -> RxErrors {
        RxErrors::from_bits(self.0 as u8)
    }

    /// Validate the descriptor and return the payload length.
    ///
    /// The device's length includes the two-byte receive alignment; a frame shorter than that
    /// cannot be delivered.
    pub fn frame(self) -> Result<RxFrame, RxError> {
        if self.own() {
            return Err(RxError::Busy);
        }
        let errors = self.errors();
        if errors.any() {
            return Err(RxError::Device(errors));
        }
        if !self.sop() {
            return Err(RxError::MissingStart);
        }
        if !self.eop() {
            return Err(RxError::MissingEnd);
        }
        let raw_length = self.length();
        if raw_length < ETHER_ALIGN {
            return Err(RxError::TooShort(raw_length));
        }
        if raw_length > BUFFER_LEN {
            return Err(RxError::Oversize(raw_length));
        }
        Ok(RxFrame {
            length: raw_length - ETHER_ALIGN,
            raw_length,
        })
    }
}

/// Why a TX status word cannot be built or is not complete.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TxError {
    /// The length is zero or exceeds one packet buffer.
    BadLength(usize),
    /// The queue tag is wider than six bits.
    BadQueueTag(u8),
    /// A B39 descriptor carried the error flag or an unknown flag.
    BadFlags(u32),
    /// The device has not completed the descriptor.
    Incomplete,
}

/// One TX descriptor status word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TxStatus(u32);

impl TxStatus {
    /// Wrap a raw status word.
    #[must_use]
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    /// The raw status word.
    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }

    /// Raw buffer length in the status word.
    #[must_use]
    pub const fn length(self) -> usize {
        ((self.0 & TX_STATUS_BUFLEN_MASK) >> TX_STATUS_BUFLEN_SHIFT) as usize
    }

    /// True when the device owns the descriptor.
    #[must_use]
    pub const fn own(self) -> bool {
        self.0 & TX_STATUS_OWN != 0
    }

    /// True when this descriptor starts a frame.
    #[must_use]
    pub const fn sop(self) -> bool {
        self.0 & TX_STATUS_SOP != 0
    }

    /// True when this descriptor ends a frame.
    #[must_use]
    pub const fn eop(self) -> bool {
        self.0 & TX_STATUS_EOP != 0
    }

    /// True when the descriptor asks for a CRC append.
    #[must_use]
    pub const fn crc(self) -> bool {
        self.0 & TX_STATUS_CRC != 0
    }

    /// The six-bit queue tag.
    #[must_use]
    pub const fn queue_tag(self) -> u8 {
        ((self.0 & TX_STATUS_QTAG) >> TX_STATUS_QTAG_SHIFT) as u8
    }

    /// Build a status word for one segment.
    ///
    /// The driver sets `SOP` on the first segment, `EOP` on the last, and always asks the
    /// device to append the CRC. `queue_tag` is the six-bit `QTAG` field.
    pub fn segment(length: usize, queue_tag: u8, first: bool, last: bool) -> Result<Self, TxError> {
        if length == 0 || length > BUFFER_LEN {
            return Err(TxError::BadLength(length));
        }
        if queue_tag > TX_STATUS_QTAG_MASK as u8 {
            return Err(TxError::BadQueueTag(queue_tag));
        }
        let mut status = ((length as u32) << TX_STATUS_BUFLEN_SHIFT) | TX_STATUS_CRC;
        status |= (queue_tag as u32 & TX_STATUS_QTAG_MASK) << TX_STATUS_QTAG_SHIFT;
        if first {
            status |= TX_STATUS_SOP;
        }
        if last {
            status |= TX_STATUS_EOP;
        }
        Ok(Self(status))
    }

    /// Build the `SOP|CRC|QTAG` first-segment word used by the Pi 4 driver.
    pub fn first(length: usize) -> Result<Self, TxError> {
        Self::segment(length, TX_STATUS_QTAG_MASK as u8, true, false)
    }

    /// Build the `EOP|CRC|QTAG` last-segment word used by the Pi 4 driver.
    pub fn last(length: usize) -> Result<Self, TxError> {
        Self::segment(length, TX_STATUS_QTAG_MASK as u8, false, true)
    }

    /// Build the single-segment `SOP|EOP|CRC|QTAG` word.
    pub fn single(length: usize) -> Result<Self, TxError> {
        Self::segment(length, TX_STATUS_QTAG_MASK as u8, true, true)
    }
}

/// One RX descriptor as three MMIO words.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RxDescriptor {
    /// Status word.
    pub status: RxStatus,
    /// Packet buffer bus address.
    pub address: u64,
}

impl RxDescriptor {
    /// Build a descriptor.
    #[must_use]
    pub const fn new(address: u64, status: RxStatus) -> Self {
        Self { status, address }
    }

    /// Encode the descriptor in register order: status, address low, address high.
    #[must_use]
    pub const fn registers(self) -> [u32; 3] {
        [
            self.status.raw(),
            self.address as u32,
            (self.address >> 32) as u32,
        ]
    }
}

/// One TX descriptor as three MMIO words.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TxDescriptor {
    /// Status word.
    pub status: TxStatus,
    /// Packet buffer bus address.
    pub address: u64,
}

impl TxDescriptor {
    /// Build a descriptor.
    #[must_use]
    pub const fn new(address: u64, status: TxStatus) -> Self {
        Self { status, address }
    }

    /// Encode the descriptor in register order: status, address low, address high.
    #[must_use]
    pub const fn registers(self) -> [u32; 3] {
        [
            self.status.raw(),
            self.address as u32,
            (self.address >> 32) as u32,
        ]
    }
}

/// Why an index operation was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RingError {
    /// A ring must have at least one entry.
    ZeroCount,
    /// The count exceeds [`MAX_RING_ENTRIES`].
    TooManyEntries,
    /// The producer/consumer distance is larger than the ring.
    Inconsistent,
    /// There is no room for the requested descriptors.
    Full,
    /// There are fewer complete descriptors than requested.
    Empty,
}

/// Producer and consumer indices of one hardware ring.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RingIndices {
    producer: u32,
    consumer: u32,
    count: u16,
}

impl RingIndices {
    /// An empty ring with `count` entries.
    pub const fn new(count: u16) -> Result<Self, RingError> {
        if count == 0 {
            return Err(RingError::ZeroCount);
        }
        if count > MAX_RING_ENTRIES {
            return Err(RingError::TooManyEntries);
        }
        Ok(Self {
            producer: 0,
            consumer: 0,
            count,
        })
    }

    /// Rebuild indices from hardware counters, rejecting an impossible distance.
    pub const fn from_counters(
        count: u16,
        producer: u32,
        consumer: u32,
    ) -> Result<Self, RingError> {
        if count == 0 {
            return Err(RingError::ZeroCount);
        }
        if count > MAX_RING_ENTRIES {
            return Err(RingError::TooManyEntries);
        }
        if producer.wrapping_sub(consumer) > count as u32 {
            return Err(RingError::Inconsistent);
        }
        Ok(Self {
            producer,
            consumer,
            count,
        })
    }

    /// Number of entries in the ring.
    #[must_use]
    pub const fn count(self) -> u16 {
        self.count
    }

    /// Logical producer index.
    #[must_use]
    pub const fn producer(self) -> u32 {
        self.producer
    }

    /// Logical consumer index.
    #[must_use]
    pub const fn consumer(self) -> u32 {
        self.consumer
    }

    /// Number of descriptors between the consumer and the producer.
    pub fn used(self) -> Result<usize, RingError> {
        let used = self.producer.wrapping_sub(self.consumer);
        if used > self.count as u32 {
            return Err(RingError::Inconsistent);
        }
        Ok(used as usize)
    }

    /// Number of free descriptor slots.
    pub fn free(self) -> Result<usize, RingError> {
        Ok(self.count as usize - self.used()?)
    }

    /// True when no descriptor is outstanding.
    pub fn is_empty(self) -> bool {
        self.used() == Ok(0)
    }

    /// True when every descriptor is outstanding.
    pub fn is_full(self) -> bool {
        self.used() == Ok(self.count as usize)
    }

    /// Slot for a logical index.
    #[must_use]
    pub fn slot(self, index: u32) -> usize {
        index as usize % self.count as usize
    }

    /// Slot for the next producer write.
    #[must_use]
    pub fn producer_slot(self) -> usize {
        self.slot(self.producer)
    }

    /// Slot for the next consumer read.
    #[must_use]
    pub fn consumer_slot(self) -> usize {
        self.slot(self.consumer)
    }

    /// Advance the producer after publishing `count` descriptors.
    pub fn advance_producer(&mut self, count: usize) -> Result<(), RingError> {
        if count > self.free()? {
            return Err(RingError::Full);
        }
        self.producer = self.producer.wrapping_add(count as u32);
        Ok(())
    }

    /// Advance the consumer after retiring `count` descriptors.
    pub fn advance_consumer(&mut self, count: usize) -> Result<(), RingError> {
        if count > self.used()? {
            return Err(RingError::Empty);
        }
        self.consumer = self.consumer.wrapping_add(count as u32);
        Ok(())
    }

    /// The 16-bit hardware producer counter.
    #[must_use]
    pub const fn hardware_producer(self) -> u16 {
        self.producer as u16
    }

    /// The 16-bit hardware consumer counter.
    #[must_use]
    pub const fn hardware_consumer(self) -> u16 {
        self.consumer as u16
    }
}

/// The 16-bit distance between two hardware counters.
#[must_use]
pub const fn hardware_delta(producer: u16, consumer: u16) -> u16 {
    producer.wrapping_sub(consumer)
}

/// Ring values the driver programs during initialisation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RingSetup {
    /// Number of descriptors.
    pub desc_count: u16,
    /// Buffer length in bytes.
    pub buffer_len: u16,
    /// Encoded `RING_BUF_SIZE` register value.
    pub buf_size: u32,
    /// Encoded `END_ADDR` register value.
    pub end_address: u32,
    /// Encoded `XON_XOFF_THRES` register value.
    pub xon_xoff: u32,
    /// TX interrupt threshold.
    pub tx_threshold: u32,
    /// RX interrupt threshold.
    pub rx_threshold: u32,
    /// RX mitigation timeout in 125 MHz/1024 ticks.
    pub rx_timeout_ticks: u32,
}

impl RingSetup {
    /// The Pi 4 defaults: 256 descriptors, 2 KiB buffers, ten-packet thresholds and 57 us.
    #[must_use]
    pub const fn pi4_default() -> Self {
        Self {
            desc_count: crate::regs::DESC_COUNT,
            buffer_len: BUFFER_LEN as u16,
            buf_size: crate::regs::ring_buf_size(crate::regs::DESC_COUNT, BUFFER_LEN as u16),
            end_address: crate::regs::ring_end_address(crate::regs::DESC_COUNT),
            xon_xoff: crate::regs::rx_xon_xoff(RX_XON_THRESHOLD, RX_XOFF_THRESHOLD),
            tx_threshold: TX_DONE_THRESHOLD,
            rx_threshold: RX_DONE_THRESHOLD,
            rx_timeout_ticks: crate::regs::microseconds_to_ticks(RX_TIMEOUT_US),
        }
    }
}

/// Map one RX status into the B39 [`Desc`] flags and payload length.
///
/// A busy, truncated or errored descriptor is still returned as a descriptor with [`DESC_ERROR`]
/// set and length zero, so the consumer can account for it rather than losing ring state.
#[must_use]
pub fn netd_rx_descriptor(status: RxStatus, offset: u64) -> Desc {
    let mut flags = 0u32;
    if status.sop() {
        flags |= DESC_FIRST;
    }
    if status.eop() {
        flags |= DESC_LAST;
    }
    let malformed = status.own()
        || status.errors().any()
        || status.length() < ETHER_ALIGN
        || status.length() > BUFFER_LEN;
    if malformed {
        flags |= DESC_ERROR;
        Desc::new(offset, 0, flags)
    } else {
        Desc::new(offset, (status.length() - ETHER_ALIGN) as u32, flags)
    }
}

/// Convert one B39 TX descriptor into a GENET TX status word.
pub fn tx_status_from_netd(descriptor: Desc) -> Result<TxStatus, TxError> {
    let known = DESC_FIRST | DESC_LAST | raven_proto_net::ring::DESC_CHECKSUM_OK;
    if descriptor.flags & !known != 0 {
        return Err(TxError::BadFlags(descriptor.flags));
    }
    if descriptor.flags & DESC_ERROR != 0 {
        return Err(TxError::BadFlags(descriptor.flags));
    }
    let length = descriptor.len as usize;
    TxStatus::segment(
        length,
        TX_STATUS_QTAG_MASK as u8,
        descriptor.is_first(),
        descriptor.is_last(),
    )
}

#[cfg(test)]
#[path = "ring_ut.rs"]
mod tests;
