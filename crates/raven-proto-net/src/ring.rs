// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Shared-memory rings between `netd` and a NIC driver.
//!
//! A region contains one [`RingHeader`] (56 bytes), a receive descriptor ring, a transmit
//! descriptor ring, and a pool of 2 KiB buffers:
//!
//! ```text
//! offset 0                      RingHeader (56 bytes)
//! offset 56                     rx descriptors (rx_count * 16 bytes)
//! offset 56 + rx_bytes          tx descriptors (tx_count * 16 bytes)
//! aligned to 2048               buffer pool (pool_len bytes, BUFFER_SIZE each)
//! ```
//!
//! Descriptor `addr` values are byte offsets into the same region, never process addresses.
//! [`RingLayout`] computes and validates the offsets; [`RingProducer`] and [`RingConsumer`] move
//! descriptors and publish the indices. See `README.md` for the memory-ordering contract.
//!
//! The helpers are single-producer/single-consumer and forbid `unsafe`, so they read and write
//! indices as plain bytes. The `fence(Release)`/`fence(Acquire)` pairs are the ordering contract
//! the two processes must honour; host tests use the same code under a mutex.

use core::fmt;
use core::mem::{offset_of, size_of};
use core::sync::atomic::{Ordering, fence};

use crate::wire::{Reader, WireError, Writer};

/// Ring magic, ASCII `RAVENNET`.
pub const RING_MAGIC: [u8; 8] = *b"RAVENNET";
/// Ring layout version.
pub const RING_VERSION: u16 = 1;
/// Size of one pooled buffer in bytes.
pub const BUFFER_SIZE: usize = 2048;
/// Largest descriptor count in either ring.
pub const MAX_RING_ENTRIES: u16 = 4096;
/// Largest accepted shared region in bytes.
pub const MAX_REGION: usize = 16 * 1024 * 1024;

/// Descriptor carries the first bytes of a frame.
pub const DESC_FIRST: u32 = 1 << 0;
/// Descriptor carries the last bytes of a frame.
pub const DESC_LAST: u32 = 1 << 1;
/// The sender computed a valid checksum.
pub const DESC_CHECKSUM_OK: u32 = 1 << 2;
/// The frame is marked as failed and must be dropped.
pub const DESC_ERROR: u32 = 1 << 3;
/// Every defined descriptor flag.
pub const DESC_FLAG_MASK: u32 = DESC_FIRST | DESC_LAST | DESC_CHECKSUM_OK | DESC_ERROR;

/// Ring header at the start of the shared region.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RingHeader {
    /// [`RING_MAGIC`].
    pub magic: [u8; 8],
    /// [`RING_VERSION`].
    pub version: u16,
    /// Size of this header in bytes.
    pub header_len: u16,
    /// Number of receive descriptors.
    pub rx_count: u16,
    /// Number of transmit descriptors.
    pub tx_count: u16,
    /// Receive producer index; only the driver writes it.
    pub rx_prod: u32,
    /// Receive consumer index; only `netd` writes it.
    pub rx_cons: u32,
    /// Transmit producer index; only `netd` writes it.
    pub tx_prod: u32,
    /// Transmit consumer index; only the driver writes it.
    pub tx_cons: u32,
    /// Doorbell event id for receive work.
    pub rx_doorbell: u64,
    /// Doorbell event id for transmit work.
    pub tx_doorbell: u64,
    /// Byte offset of the buffer pool.
    pub pool_offset: u32,
    /// Length of the buffer pool in bytes.
    pub pool_len: u32,
}

const _: () = assert!(size_of::<RingHeader>() == 56);
const _: () = assert!(offset_of!(RingHeader, rx_prod) == 16);
const _: () = assert!(offset_of!(RingHeader, tx_prod) == 24);
const _: () = assert!(offset_of!(RingHeader, rx_doorbell) == 32);
const _: () = assert!(offset_of!(RingHeader, pool_offset) == 48);

/// One ring descriptor.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Desc {
    /// Byte offset of the buffer inside the shared region.
    pub addr: u64,
    /// Number of valid bytes.
    pub len: u32,
    /// [`DESC_FIRST`], [`DESC_LAST`], [`DESC_CHECKSUM_OK`] and [`DESC_ERROR`] bits.
    pub flags: u32,
}

const _: () = assert!(size_of::<Desc>() == 16);
const _: () = assert!(offset_of!(Desc, len) == 8);
const _: () = assert!(offset_of!(Desc, flags) == 12);

impl Desc {
    /// Encoded size in bytes.
    pub const LEN: usize = 16;

    /// Build a descriptor.
    pub const fn new(addr: u64, len: u32, flags: u32) -> Self {
        Self { addr, len, flags }
    }

    /// Whether this descriptor starts a frame.
    pub const fn is_first(self) -> bool {
        self.flags & DESC_FIRST != 0
    }

    /// Whether this descriptor ends a frame.
    pub const fn is_last(self) -> bool {
        self.flags & DESC_LAST != 0
    }

    /// Encode into `out`.
    pub fn write_to(self, out: &mut [u8]) -> Result<usize, WireError> {
        let mut writer = Writer::new(out);
        writer.put_u64(self.addr)?;
        writer.put_u32(self.len)?;
        writer.put_u32(self.flags)?;
        Ok(writer.position())
    }

    /// Decode exactly one descriptor.
    pub fn read_from(buffer: &[u8]) -> Option<Self> {
        let mut reader = Reader::new(buffer);
        let desc = Self {
            addr: reader.get_u64().ok()?,
            len: reader.get_u32().ok()?,
            flags: reader.get_u32().ok()?,
        };
        if !reader.is_empty() {
            return None;
        }
        Some(desc)
    }
}

/// Which direction a ring carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ring {
    /// Driver to `netd`.
    Rx,
    /// `netd` to the driver.
    Tx,
}

/// Why a ring layout or region was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutError {
    /// A descriptor count is zero.
    ZeroEntries,
    /// A descriptor count is above [`MAX_RING_ENTRIES`].
    TooManyEntries,
    /// The region is too small for the header, descriptors and one buffer.
    RegionTooSmall,
    /// The region is larger than [`MAX_REGION`].
    TooLarge,
    /// The region is not a whole number of buffers.
    NotAligned,
    /// The header magic, version or length does not match this protocol.
    BadHeader,
}

impl fmt::Display for LayoutError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::ZeroEntries => "ring needs at least one descriptor",
            Self::TooManyEntries => "ring has too many descriptors",
            Self::RegionTooSmall => "shared region is too small",
            Self::TooLarge => "shared region is too large",
            Self::NotAligned => "shared region is not a whole number of buffers",
            Self::BadHeader => "bad ring header",
        })
    }
}

/// Why a ring operation failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RingError {
    /// The producer ring has no free descriptor.
    Full,
    /// The region no longer matches the validated layout.
    Layout,
}

impl fmt::Display for RingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Full => "ring is full",
            Self::Layout => "ring layout mismatch",
        })
    }
}

/// Validated offsets for one shared region.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RingLayout {
    region_len: u32,
    rx_count: u16,
    tx_count: u16,
    tx_offset: u32,
    pool_offset: u32,
    pool_len: u32,
}

impl RingLayout {
    /// Compute a layout for a region of `region_len` bytes.
    ///
    /// The buffer pool starts at the next [`BUFFER_SIZE`] boundary after both descriptor rings,
    /// and the region must end on a buffer boundary.
    pub fn new(region_len: usize, rx_count: u16, tx_count: u16) -> Result<Self, LayoutError> {
        if rx_count == 0 || tx_count == 0 {
            return Err(LayoutError::ZeroEntries);
        }
        if rx_count > MAX_RING_ENTRIES || tx_count > MAX_RING_ENTRIES {
            return Err(LayoutError::TooManyEntries);
        }
        if region_len == 0 {
            return Err(LayoutError::RegionTooSmall);
        }
        if region_len > MAX_REGION {
            return Err(LayoutError::TooLarge);
        }
        let rx_bytes = usize::from(rx_count)
            .checked_mul(Desc::LEN)
            .ok_or(LayoutError::RegionTooSmall)?;
        let tx_bytes = usize::from(tx_count)
            .checked_mul(Desc::LEN)
            .ok_or(LayoutError::RegionTooSmall)?;
        let tx_offset = size_of::<RingHeader>()
            .checked_add(rx_bytes)
            .ok_or(LayoutError::RegionTooSmall)?;
        let desc_end = tx_offset
            .checked_add(tx_bytes)
            .ok_or(LayoutError::RegionTooSmall)?;
        let pool_offset = desc_end
            .checked_add(BUFFER_SIZE - 1)
            .ok_or(LayoutError::RegionTooSmall)?
            / BUFFER_SIZE
            * BUFFER_SIZE;
        if region_len <= pool_offset {
            return Err(LayoutError::RegionTooSmall);
        }
        let pool_len = region_len - pool_offset;
        if pool_len < BUFFER_SIZE {
            return Err(LayoutError::RegionTooSmall);
        }
        if !pool_len.is_multiple_of(BUFFER_SIZE) {
            return Err(LayoutError::NotAligned);
        }
        Ok(Self {
            region_len: region_len as u32,
            rx_count,
            tx_count,
            tx_offset: tx_offset as u32,
            pool_offset: pool_offset as u32,
            pool_len: pool_len as u32,
        })
    }

    /// Rebuild a layout from a header read out of a region of `region_len` bytes.
    pub fn from_header(header: &RingHeader, region_len: usize) -> Result<Self, LayoutError> {
        header.validate(region_len)?;
        let rx_count = header.rx_count;
        let tx_count = header.tx_count;
        let tx_offset = size_of::<RingHeader>() + usize::from(rx_count) * Desc::LEN;
        Ok(Self {
            region_len: region_len as u32,
            rx_count,
            tx_count,
            tx_offset: tx_offset as u32,
            pool_offset: header.pool_offset,
            pool_len: header.pool_len,
        })
    }

    /// Region length the layout was computed for.
    pub const fn region_len(&self) -> usize {
        self.region_len as usize
    }

    /// Receive descriptor count.
    pub const fn rx_count(&self) -> u16 {
        self.rx_count
    }

    /// Transmit descriptor count.
    pub const fn tx_count(&self) -> u16 {
        self.tx_count
    }

    /// Byte offset of the buffer pool.
    pub const fn pool_offset(&self) -> usize {
        self.pool_offset as usize
    }

    /// Length of the buffer pool in bytes.
    pub const fn pool_len(&self) -> usize {
        self.pool_len as usize
    }

    /// Number of 2 KiB buffers in the pool.
    pub const fn buffer_count(&self) -> usize {
        self.pool_len as usize / BUFFER_SIZE
    }

    /// Byte offset of buffer `index`, if it is inside the pool.
    pub const fn buffer_offset(&self, index: usize) -> Option<usize> {
        if index >= self.buffer_count() {
            return None;
        }
        Some(self.pool_offset as usize + index * BUFFER_SIZE)
    }

    /// Byte offset of descriptor `slot` in `ring`, if it is inside the ring.
    pub const fn desc_offset(&self, ring: Ring, slot: usize) -> Option<usize> {
        if slot >= self.count(ring) as usize {
            return None;
        }
        Some(self.slot_offset(ring, slot))
    }

    /// Descriptor count of `ring`.
    pub const fn count(&self, ring: Ring) -> u16 {
        match ring {
            Ring::Rx => self.rx_count,
            Ring::Tx => self.tx_count,
        }
    }

    const fn slot_offset(&self, ring: Ring, slot: usize) -> usize {
        let base = match ring {
            Ring::Rx => size_of::<RingHeader>(),
            Ring::Tx => self.tx_offset as usize,
        };
        base + slot * Desc::LEN
    }

    const fn prod_offset(&self, ring: Ring) -> usize {
        match ring {
            Ring::Rx => offset_of!(RingHeader, rx_prod),
            Ring::Tx => offset_of!(RingHeader, tx_prod),
        }
    }

    const fn cons_offset(&self, ring: Ring) -> usize {
        match ring {
            Ring::Rx => offset_of!(RingHeader, rx_cons),
            Ring::Tx => offset_of!(RingHeader, tx_cons),
        }
    }

    const fn doorbell_offset(&self, ring: Ring) -> usize {
        match ring {
            Ring::Rx => offset_of!(RingHeader, rx_doorbell),
            Ring::Tx => offset_of!(RingHeader, tx_doorbell),
        }
    }

    /// Build the header for this layout without writing it.
    pub const fn header(&self, rx_doorbell: u64, tx_doorbell: u64) -> RingHeader {
        RingHeader {
            magic: RING_MAGIC,
            version: RING_VERSION,
            header_len: size_of::<RingHeader>() as u16,
            rx_count: self.rx_count,
            tx_count: self.tx_count,
            rx_prod: 0,
            rx_cons: 0,
            tx_prod: 0,
            tx_cons: 0,
            rx_doorbell,
            tx_doorbell,
            pool_offset: self.pool_offset,
            pool_len: self.pool_len,
        }
    }

    /// Write the header into the start of `region`.
    pub fn write_header(
        &self,
        region: &mut [u8],
        rx_doorbell: u64,
        tx_doorbell: u64,
    ) -> Result<(), LayoutError> {
        if region.len() < self.region_len as usize {
            return Err(LayoutError::RegionTooSmall);
        }
        self.header(rx_doorbell, tx_doorbell)
            .write_to(region)
            .map(|_| ())
            .map_err(|_| LayoutError::RegionTooSmall)
    }
}

impl RingHeader {
    /// Encode into `out`.
    pub fn write_to(&self, out: &mut [u8]) -> Result<usize, WireError> {
        let mut writer = Writer::new(out);
        writer.put_bytes(&self.magic)?;
        writer.put_u16(self.version)?;
        writer.put_u16(self.header_len)?;
        writer.put_u16(self.rx_count)?;
        writer.put_u16(self.tx_count)?;
        writer.put_u32(self.rx_prod)?;
        writer.put_u32(self.rx_cons)?;
        writer.put_u32(self.tx_prod)?;
        writer.put_u32(self.tx_cons)?;
        writer.put_u64(self.rx_doorbell)?;
        writer.put_u64(self.tx_doorbell)?;
        writer.put_u32(self.pool_offset)?;
        writer.put_u32(self.pool_len)?;
        Ok(writer.position())
    }

    /// Read and validate the header at the start of `region`.
    pub fn read_from(region: &[u8]) -> Result<Self, LayoutError> {
        let mut reader = Reader::new(region);
        let magic = reader
            .get_bytes(8)
            .map_err(|_| LayoutError::RegionTooSmall)?;
        if magic != RING_MAGIC {
            return Err(LayoutError::BadHeader);
        }
        let version = reader.get_u16().map_err(|_| LayoutError::BadHeader)?;
        let header_len = reader.get_u16().map_err(|_| LayoutError::BadHeader)?;
        let rx_count = reader.get_u16().map_err(|_| LayoutError::BadHeader)?;
        let tx_count = reader.get_u16().map_err(|_| LayoutError::BadHeader)?;
        let header = Self {
            magic: RING_MAGIC,
            version,
            header_len,
            rx_count,
            tx_count,
            rx_prod: reader.get_u32().map_err(|_| LayoutError::BadHeader)?,
            rx_cons: reader.get_u32().map_err(|_| LayoutError::BadHeader)?,
            tx_prod: reader.get_u32().map_err(|_| LayoutError::BadHeader)?,
            tx_cons: reader.get_u32().map_err(|_| LayoutError::BadHeader)?,
            rx_doorbell: reader.get_u64().map_err(|_| LayoutError::BadHeader)?,
            tx_doorbell: reader.get_u64().map_err(|_| LayoutError::BadHeader)?,
            pool_offset: reader.get_u32().map_err(|_| LayoutError::BadHeader)?,
            pool_len: reader.get_u32().map_err(|_| LayoutError::BadHeader)?,
        };
        header.validate(region.len())?;
        Ok(header)
    }

    /// Validate the header against a region of `region_len` bytes.
    pub fn validate(&self, region_len: usize) -> Result<(), LayoutError> {
        if self.magic != RING_MAGIC
            || self.version != RING_VERSION
            || usize::from(self.header_len) != size_of::<Self>()
        {
            return Err(LayoutError::BadHeader);
        }
        if self.rx_count == 0 || self.tx_count == 0 {
            return Err(LayoutError::ZeroEntries);
        }
        if self.rx_count > MAX_RING_ENTRIES || self.tx_count > MAX_RING_ENTRIES {
            return Err(LayoutError::TooManyEntries);
        }
        if region_len > MAX_REGION {
            return Err(LayoutError::TooLarge);
        }
        let desc_end = size_of::<Self>()
            + usize::from(self.rx_count) * Desc::LEN
            + usize::from(self.tx_count) * Desc::LEN;
        let pool_offset = self.pool_offset as usize;
        let pool_len = self.pool_len as usize;
        if pool_offset < desc_end
            || !pool_offset.is_multiple_of(BUFFER_SIZE)
            || pool_len < BUFFER_SIZE
            || !pool_len.is_multiple_of(BUFFER_SIZE)
            || pool_offset.saturating_add(pool_len) > region_len
        {
            return Err(LayoutError::BadHeader);
        }
        Ok(())
    }
}

/// Producer half of one ring.
pub struct RingProducer<'a> {
    region: &'a mut [u8],
    layout: RingLayout,
    ring: Ring,
}

impl<'a> RingProducer<'a> {
    /// Attach a producer to `region`.
    pub fn new(region: &'a mut [u8], layout: RingLayout, ring: Ring) -> Result<Self, LayoutError> {
        if region.len() < layout.region_len() {
            return Err(LayoutError::RegionTooSmall);
        }
        Ok(Self {
            region,
            layout,
            ring,
        })
    }

    /// Descriptor count of this ring.
    pub const fn capacity(&self) -> u32 {
        self.layout.count(self.ring) as u32
    }

    /// Number of descriptors waiting for the consumer.
    pub fn len(&self) -> u32 {
        let producer = load_u32(self.region, self.layout.prod_offset(self.ring));
        let consumer = load_u32(self.region, self.layout.cons_offset(self.ring));
        producer.wrapping_sub(consumer).min(self.capacity())
    }

    /// Whether the ring has no outstanding descriptor.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Whether every descriptor slot is occupied.
    pub fn is_full(&self) -> bool {
        self.len() >= self.capacity()
    }

    /// Publish one descriptor, or fail when the ring is full.
    pub fn push(&mut self, desc: Desc) -> Result<(), RingError> {
        let producer_offset = self.layout.prod_offset(self.ring);
        let consumer_offset = self.layout.cons_offset(self.ring);
        let producer = load_u32(self.region, producer_offset);
        let consumer = load_u32(self.region, consumer_offset);
        let used = producer.wrapping_sub(consumer);
        if used >= self.capacity() {
            return Err(RingError::Full);
        }
        let slot = (producer % self.capacity()) as usize;
        let Some(offset) = self.layout.desc_offset(self.ring, slot) else {
            return Err(RingError::Layout);
        };
        let end = offset.saturating_add(Desc::LEN);
        let Some(bytes) = self.region.get_mut(offset..end) else {
            return Err(RingError::Layout);
        };
        desc.write_to(bytes).map_err(|_| RingError::Layout)?;
        // Publish the descriptor before the index that makes it visible.
        fence(Ordering::Release);
        store_u32(self.region, producer_offset, producer.wrapping_add(1));
        Ok(())
    }

    /// Doorbell id the producer must signal after [`Self::push`].
    pub fn doorbell(&self) -> u64 {
        load_u64(self.region, self.layout.doorbell_offset(self.ring))
    }
}

/// Consumer half of one ring.
pub struct RingConsumer<'a> {
    region: &'a mut [u8],
    layout: RingLayout,
    ring: Ring,
}

impl<'a> RingConsumer<'a> {
    /// Attach a consumer to `region`.
    pub fn new(region: &'a mut [u8], layout: RingLayout, ring: Ring) -> Result<Self, LayoutError> {
        if region.len() < layout.region_len() {
            return Err(LayoutError::RegionTooSmall);
        }
        Ok(Self {
            region,
            layout,
            ring,
        })
    }

    /// Descriptor count of this ring.
    pub const fn capacity(&self) -> u32 {
        self.layout.count(self.ring) as u32
    }

    /// Number of descriptors waiting to be consumed.
    pub fn len(&self) -> u32 {
        let producer = load_u32(self.region, self.layout.prod_offset(self.ring));
        let consumer = load_u32(self.region, self.layout.cons_offset(self.ring));
        producer.wrapping_sub(consumer).min(self.capacity())
    }

    /// Whether the ring has no outstanding descriptor.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Whether every descriptor slot is occupied.
    pub fn is_full(&self) -> bool {
        self.len() >= self.capacity()
    }

    /// Take the oldest descriptor, or `None` when the ring is empty.
    pub fn pop(&mut self) -> Option<Desc> {
        let desc = self.peek()?;
        let consumer_offset = self.layout.cons_offset(self.ring);
        let consumer = load_u32(self.region, consumer_offset);
        // Finish reading the descriptor before freeing its slot.
        fence(Ordering::Release);
        store_u32(self.region, consumer_offset, consumer.wrapping_add(1));
        Some(desc)
    }

    /// Look at the oldest descriptor without consuming it.
    pub fn peek(&self) -> Option<Desc> {
        // Read the published index before touching the descriptor.
        fence(Ordering::Acquire);
        let producer = load_u32(self.region, self.layout.prod_offset(self.ring));
        let consumer = load_u32(self.region, self.layout.cons_offset(self.ring));
        if producer == consumer {
            return None;
        }
        let slot = (consumer % self.capacity()) as usize;
        let offset = self.layout.desc_offset(self.ring, slot)?;
        let end = offset.saturating_add(Desc::LEN);
        let bytes = self.region.get(offset..end)?;
        Desc::read_from(bytes)
    }

    /// Doorbell id the consumer must signal after [`Self::pop`].
    pub fn doorbell(&self) -> u64 {
        load_u64(self.region, self.layout.doorbell_offset(self.ring))
    }
}

/// Read a little-endian `u32` at `offset`; a short region reads as zero (layouts are validated).
fn load_u32(region: &[u8], offset: usize) -> u32 {
    match region.get(offset..offset.saturating_add(4)) {
        Some(bytes) => u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
        None => 0,
    }
}

/// Read a little-endian `u64` at `offset`; a short region reads as zero.
fn load_u64(region: &[u8], offset: usize) -> u64 {
    match region.get(offset..offset.saturating_add(8)) {
        Some(bytes) => u64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]),
        None => 0,
    }
}

/// Store a little-endian `u32` at `offset`; out-of-range offsets are ignored.
fn store_u32(region: &mut [u8], offset: usize, value: u32) {
    if let Some(bytes) = region.get_mut(offset..offset.saturating_add(4)) {
        bytes.copy_from_slice(&value.to_le_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RX: u16 = 4;
    const TX: u16 = 4;
    // Header and both descriptor rings end at 184 bytes, so the pool starts at 2048.
    const REGION: usize = 2048 + 8 * BUFFER_SIZE;

    fn layout() -> RingLayout {
        RingLayout::new(REGION, RX, TX).unwrap()
    }

    fn region() -> Vec<u8> {
        let mut region = vec![0u8; REGION];
        layout().write_header(&mut region, 11, 12).unwrap();
        region
    }

    #[test]
    fn header_and_descriptor_sizes_are_stable() {
        assert_eq!(size_of::<RingHeader>(), 56);
        assert_eq!(size_of::<Desc>(), 16);
        assert_eq!(Desc::LEN, 16);
    }

    #[test]
    fn layout_aligns_the_pool_and_counts_buffers() {
        let layout = layout();
        assert_eq!(layout.pool_offset(), 2048);
        assert_eq!(layout.buffer_count(), 8);
        assert_eq!(layout.buffer_offset(0), Some(2048));
        assert_eq!(layout.buffer_offset(7), Some(2048 + 7 * BUFFER_SIZE));
        assert_eq!(layout.buffer_offset(8), None);
    }

    #[test]
    fn layout_rejects_bad_counts_and_regions() {
        assert_eq!(
            RingLayout::new(REGION, 0, TX),
            Err(LayoutError::ZeroEntries)
        );
        assert_eq!(
            RingLayout::new(REGION, RX, MAX_RING_ENTRIES + 1),
            Err(LayoutError::TooManyEntries)
        );
        assert!(matches!(
            RingLayout::new(100, RX, TX),
            Err(LayoutError::RegionTooSmall)
        ));
        assert_eq!(
            RingLayout::new(REGION + 1, RX, TX),
            Err(LayoutError::NotAligned)
        );
        assert_eq!(
            RingLayout::new(MAX_REGION + BUFFER_SIZE, RX, TX),
            Err(LayoutError::TooLarge)
        );
    }

    #[test]
    fn descriptor_offsets_are_inside_each_ring() {
        let layout = layout();
        assert_eq!(layout.desc_offset(Ring::Rx, 0), Some(56));
        assert_eq!(layout.desc_offset(Ring::Rx, 3), Some(56 + 3 * 16));
        assert_eq!(layout.desc_offset(Ring::Rx, 4), None);
        assert_eq!(layout.desc_offset(Ring::Tx, 0), Some(56 + 4 * 16));
        assert_eq!(layout.desc_offset(Ring::Tx, 3), Some(56 + 7 * 16));
        assert_eq!(layout.desc_offset(Ring::Tx, 4), None);
    }

    #[test]
    fn header_roundtrips_and_validates() {
        let mut region = region();
        let header = RingHeader::read_from(&region).unwrap();
        assert_eq!(header.magic, RING_MAGIC);
        assert_eq!(header.version, RING_VERSION);
        assert_eq!(header.rx_count, RX);
        assert_eq!(header.tx_count, TX);
        assert_eq!(header.rx_doorbell, 11);
        assert_eq!(header.tx_doorbell, 12);
        assert_eq!(RingLayout::from_header(&header, REGION), Ok(layout()));

        region[0] = b'X';
        assert_eq!(RingHeader::read_from(&region), Err(LayoutError::BadHeader));
    }

    #[test]
    fn header_golden_bytes() {
        let mut bytes = [0u8; 56];
        layout()
            .header(0x0102_0304_0506_0708, 0)
            .write_to(&mut bytes)
            .unwrap();
        assert_eq!(&bytes[0..8], b"RAVENNET");
        assert_eq!(
            &bytes[8..16],
            &[0x01, 0x00, 0x38, 0x00, 0x04, 0x00, 0x04, 0x00]
        );
        assert_eq!(&bytes[16..32], &[0u8; 16]);
        assert_eq!(&bytes[32..40], &[8, 7, 6, 5, 4, 3, 2, 1]);
        assert_eq!(
            &bytes[48..56],
            &[0x00, 0x08, 0x00, 0x00, 0x00, 0x40, 0x00, 0x00]
        );
    }

    #[test]
    fn descriptor_golden_bytes_and_flags() {
        let desc = Desc::new(0x0102_0304_0506_0708, 0x1122_3344, DESC_FIRST | DESC_LAST);
        let mut bytes = [0u8; 16];
        desc.write_to(&mut bytes).unwrap();
        assert_eq!(
            bytes,
            [
                8, 7, 6, 5, 4, 3, 2, 1, 0x44, 0x33, 0x22, 0x11, 0x03, 0x00, 0x00, 0x00
            ]
        );
        assert_eq!(Desc::read_from(&bytes), Some(desc));
        assert!(desc.is_first() && desc.is_last());
        assert!(Desc::read_from(&bytes[..15]).is_none());
    }

    #[test]
    fn producer_and_consumer_move_descriptors() {
        let mut region = region();
        {
            let mut producer = RingProducer::new(&mut region, layout(), Ring::Rx).unwrap();
            assert!(producer.is_empty());
            assert_eq!(producer.capacity(), 4);
            assert_eq!(producer.doorbell(), 11);
            producer
                .push(Desc::new(2048, 10, DESC_FIRST | DESC_LAST))
                .unwrap();
            producer
                .push(Desc::new(4096, 20, DESC_FIRST | DESC_LAST))
                .unwrap();
            assert_eq!(producer.len(), 2);
            assert!(!producer.is_empty());
        }
        let mut consumer = RingConsumer::new(&mut region, layout(), Ring::Rx).unwrap();
        assert_eq!(consumer.len(), 2);
        assert_eq!(consumer.doorbell(), 11);
        assert_eq!(consumer.peek(), Some(Desc::new(2048, 10, 3)));
        assert_eq!(consumer.pop(), Some(Desc::new(2048, 10, 3)));
        assert_eq!(consumer.pop(), Some(Desc::new(4096, 20, 3)));
        assert_eq!(consumer.pop(), None);
        assert!(consumer.is_empty());
    }

    #[test]
    fn ring_wraps_around() {
        let mut region = region();
        // Three full rings of traffic on a four-slot ring: the indices wrap twice.
        for round in 0..3u32 {
            {
                let mut producer = RingProducer::new(&mut region, layout(), Ring::Tx).unwrap();
                for slot in 0..4u32 {
                    let index = round * 4 + slot;
                    producer
                        .push(Desc::new(u64::from(index), index, DESC_FIRST))
                        .unwrap();
                }
                assert!(producer.is_full());
            }
            {
                let mut consumer = RingConsumer::new(&mut region, layout(), Ring::Tx).unwrap();
                for slot in 0..4u32 {
                    let index = round * 4 + slot;
                    assert_eq!(
                        consumer.pop(),
                        Some(Desc::new(u64::from(index), index, DESC_FIRST))
                    );
                }
                assert_eq!(consumer.pop(), None);
            }
        }
    }

    #[test]
    fn full_ring_rejects_more_descriptors() {
        let mut region = region();
        let mut producer = RingProducer::new(&mut region, layout(), Ring::Rx).unwrap();
        for index in 0..4 {
            producer.push(Desc::new(0, index, 0)).unwrap();
        }
        assert!(producer.is_full());
        assert_eq!(producer.len(), 4);
        assert_eq!(producer.push(Desc::new(0, 99, 0)), Err(RingError::Full));
    }

    #[test]
    fn empty_ring_has_nothing_to_peek() {
        let mut region = region();
        let consumer = RingConsumer::new(&mut region, layout(), Ring::Rx).unwrap();
        assert_eq!(consumer.peek(), None);
        assert!(consumer.is_empty());
    }

    #[test]
    fn short_region_is_rejected() {
        let mut short = vec![0u8; 64];
        assert_eq!(
            RingProducer::new(&mut short, layout(), Ring::Rx).map(|_| ()),
            Err(LayoutError::RegionTooSmall)
        );
    }

    #[test]
    fn rings_are_independent() {
        let mut region = region();
        {
            let mut rx = RingProducer::new(&mut region, layout(), Ring::Rx).unwrap();
            rx.push(Desc::new(1, 1, 0)).unwrap();
        }
        {
            let mut tx = RingProducer::new(&mut region, layout(), Ring::Tx).unwrap();
            tx.push(Desc::new(2, 2, 0)).unwrap();
            tx.push(Desc::new(3, 3, 0)).unwrap();
        }
        let mut rx = RingConsumer::new(&mut region, layout(), Ring::Rx).unwrap();
        assert_eq!(rx.pop(), Some(Desc::new(1, 1, 0)));
        assert_eq!(rx.pop(), None);
        let mut tx = RingConsumer::new(&mut region, layout(), Ring::Tx).unwrap();
        assert_eq!(tx.pop(), Some(Desc::new(2, 2, 0)));
        assert_eq!(tx.pop(), Some(Desc::new(3, 3, 0)));
    }

    #[test]
    fn header_pool_validation_rejects_misaligned_offset() {
        let mut region = region();
        // pool_offset lives at byte 48; 0x0801 is not a multiple of the buffer size.
        region[48] = 0x01;
        assert_eq!(RingHeader::read_from(&region), Err(LayoutError::BadHeader));
    }

    #[test]
    fn header_validation_rejects_zero_counts() {
        let mut region = region();
        let mut header = RingHeader::read_from(&region).unwrap();
        header.rx_count = 0;
        header.write_to(&mut region).unwrap();
        assert_eq!(
            RingHeader::read_from(&region),
            Err(LayoutError::ZeroEntries)
        );
    }

    #[test]
    fn indices_survive_a_header_roundtrip() {
        let mut region = region();
        let mut header = RingHeader::read_from(&region).unwrap();
        header.rx_prod = 3;
        header.rx_cons = 1;
        header.tx_prod = u32::MAX;
        header.tx_cons = u32::MAX - 1;
        header.write_to(&mut region).unwrap();
        assert_eq!(RingHeader::read_from(&region), Ok(header));
    }

    #[test]
    fn buffer_offsets_are_buffer_aligned() {
        let layout = layout();
        assert_eq!(layout.buffer_count(), 8);
        for index in 0..layout.buffer_count() {
            let offset = layout.buffer_offset(index).unwrap();
            assert_eq!(offset % BUFFER_SIZE, 0, "buffer {index}");
            assert!(offset >= layout.pool_offset());
            assert!(offset + BUFFER_SIZE <= layout.region_len());
        }
    }

    #[test]
    fn descriptor_slots_are_distinct() {
        let layout = layout();
        let mut offsets = Vec::new();
        for ring in [Ring::Rx, Ring::Tx] {
            for slot in 0..layout.count(ring) as usize {
                offsets.push(layout.desc_offset(ring, slot).unwrap());
            }
        }
        offsets.sort_unstable();
        offsets.dedup();
        assert_eq!(offsets.len(), usize::from(RX) + usize::from(TX));
    }

    #[test]
    fn producer_and_consumer_track_occupancy() {
        let mut region = region();
        {
            let mut producer = RingProducer::new(&mut region, layout(), Ring::Rx).unwrap();
            producer.push(Desc::new(1, 1, 0)).unwrap();
            producer.push(Desc::new(2, 2, 0)).unwrap();
        }
        {
            let mut consumer = RingConsumer::new(&mut region, layout(), Ring::Rx).unwrap();
            assert_eq!(consumer.len(), 2);
            assert_eq!(consumer.pop(), Some(Desc::new(1, 1, 0)));
            assert_eq!(consumer.len(), 1);
        }
        {
            let mut producer = RingProducer::new(&mut region, layout(), Ring::Rx).unwrap();
            assert_eq!(producer.len(), 1);
            assert!(!producer.is_full());
            producer.push(Desc::new(3, 3, 0)).unwrap();
            assert_eq!(producer.len(), 2);
        }
    }

    #[test]
    fn largest_layout_is_accepted_and_short_one_is_not() {
        // 4096 + 4096 descriptors need 131 128 bytes; the pool starts at 133 120.
        let fit = 133_120 + BUFFER_SIZE;
        let layout = RingLayout::new(fit, MAX_RING_ENTRIES, MAX_RING_ENTRIES).unwrap();
        assert_eq!(layout.pool_offset(), 133_120);
        assert_eq!(layout.buffer_count(), 1);
        assert!(RingLayout::new(133_120, MAX_RING_ENTRIES, MAX_RING_ENTRIES).is_err());
    }

    #[test]
    fn doorbells_are_per_ring() {
        let mut region = vec![0u8; REGION];
        layout().write_header(&mut region, 5, 6).unwrap();
        {
            let producer = RingProducer::new(&mut region, layout(), Ring::Rx).unwrap();
            assert_eq!(producer.doorbell(), 5);
        }
        let consumer = RingConsumer::new(&mut region, layout(), Ring::Tx).unwrap();
        assert_eq!(consumer.doorbell(), 6);
    }

    #[test]
    fn random_descriptors_roundtrip() {
        let mut state = 0x1234_5678_9abc_def0u64;
        let mut bytes = [0u8; 16];
        for _ in 0..4000 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let desc = Desc::new(state, state as u32, (state >> 32) as u32);
            let length = desc.write_to(&mut bytes).unwrap();
            assert_eq!(length, 16);
            assert_eq!(Desc::read_from(&bytes), Some(desc));
        }
    }
}
