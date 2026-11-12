// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-testable core of the virtio-blk driver (K15).
//!
//! The device layer (`main.rs`) owns the MMIO and DMA windows and the scheme loop; this crate owns
//! the byte-level rules so they run under `cargo test`: the modern virtio-pci register offsets, the
//! split-virtqueue layout for one request queue, the `virtio_blk_req` header/status encoding,
//! request range validation, and the deterministic pattern/marker used by the qtest.
//!
//! Nothing here touches hardware, allocates or panics: every function takes slices/arrays and
//! returns `Option`/`Result` for a bad input.

#![no_std]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// Logical sector size of the virtio-blk protocol used here.
pub const SECTOR_SIZE: u32 = 512;

/// Largest sector count one `block:` request may carry.
pub const MAX_SECTORS: u32 = 128;

/// Sectors per device request.
///
/// The host grants one 64 KiB DMA buffer. The rings, the 16-byte request header and the status byte
/// live in the low 512 bytes, so a request chunk can use the remaining 65,024 bytes: 127 sectors.
/// A 128-sector protocol request is therefore two chunks (127 + 1), and the self-test moves almost
/// 64 KiB per interrupt.
pub const CHUNK_SECTORS: u32 = 127;

/// Encoded `virtio_blk_req` header size.
pub const REQUEST_HEADER_LEN: usize = 16;

/// Encoded completion status size.
pub const STATUS_LEN: usize = 1;

/// Request type: read from the device into the buffer.
pub const BLK_T_IN: u32 = 0;
/// Request type: write from the buffer into the device.
pub const BLK_T_OUT: u32 = 1;
/// Request type: flush the device cache.
pub const BLK_T_FLUSH: u32 = 4;

/// Completion status: success.
pub const BLK_S_OK: u8 = 0;
/// Completion status: I/O error.
pub const BLK_S_IOERR: u8 = 1;
/// Completion status: unsupported request.
pub const BLK_S_UNSUPP: u8 = 2;

/// Device feature: the device is read-only (word 0, bit 5).
pub const BLK_F_RO: u32 = 1 << 5;
/// Device feature: flush is supported (word 0, bit 9).
pub const BLK_F_FLUSH: u32 = 1 << 9;
/// `VIRTIO_F_VERSION_1` lives in the second feature word (bit 32).
pub const F_VERSION_1: u32 = 1 << 0;

/// Descriptor flag: the next descriptor continues the chain.
pub const DESC_F_NEXT: u16 = 1;
/// Descriptor flag: the device writes into this buffer.
pub const DESC_F_WRITE: u16 = 2;

/// A 64-bit feature set split into the two device-feature words.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Features {
    /// Word 0 (feature bits 0..31).
    pub low: u32,
    /// Word 1 (feature bits 32..63).
    pub high: u32,
}

impl Features {
    /// True when feature `bit` is set.
    pub fn has(&self, bit: u32) -> bool {
        if bit < 32 {
            self.low & (1 << bit) != 0
        } else if bit < 64 {
            self.high & (1 << (bit - 32)) != 0
        } else {
            false
        }
    }
}

/// Byte offsets inside `virtio_pci_common_cfg`.
pub mod common {
    /// `device_feature_select`.
    pub const DEVICE_FEATURE_SELECT: usize = 0;
    /// `device_feature`.
    pub const DEVICE_FEATURE: usize = 4;
    /// `driver_feature_select`.
    pub const DRIVER_FEATURE_SELECT: usize = 8;
    /// `driver_feature`.
    pub const DRIVER_FEATURE: usize = 12;
    /// `msix_config`.
    pub const MSIX_CONFIG: usize = 16;
    /// `num_queues`.
    pub const NUM_QUEUES: usize = 18;
    /// `device_status`.
    pub const DEVICE_STATUS: usize = 20;
    /// `config_generation`.
    pub const CONFIG_GENERATION: usize = 21;
    /// `queue_select`.
    pub const QUEUE_SELECT: usize = 22;
    /// `queue_size`.
    pub const QUEUE_SIZE: usize = 24;
    /// `queue_msix_vector`.
    pub const QUEUE_MSIX_VECTOR: usize = 26;
    /// `queue_enable`.
    pub const QUEUE_ENABLE: usize = 28;
    /// `queue_notify_off`.
    pub const QUEUE_NOTIFY_OFF: usize = 30;
    /// `queue_desc` (64-bit bus address).
    pub const QUEUE_DESC: usize = 32;
    /// `queue_avail` (64-bit bus address).
    pub const QUEUE_AVAIL: usize = 40;
    /// `queue_used` (64-bit bus address).
    pub const QUEUE_USED: usize = 48;
}

/// virtio-pci capability types.
pub const CAP_COMMON_CFG: u8 = 1;
/// The notify region.
pub const CAP_NOTIFY_CFG: u8 = 2;
/// The ISR byte.
pub const CAP_ISR_CFG: u8 = 3;
/// The device-specific region (capacity and geometry).
pub const CAP_DEVICE_CFG: u8 = 4;

/// Device status bits.
pub const STATUS_ACKNOWLEDGE: u8 = 1;
/// The driver knows how to drive the device.
pub const STATUS_DRIVER: u8 = 2;
/// The driver is ready for the device to process queues.
pub const STATUS_DRIVER_OK: u8 = 4;
/// The driver accepted the feature set.
pub const STATUS_FEATURES_OK: u8 = 8;

/// Layout of the one split request queue inside the DMA window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RingLayout {
    /// Offset of the descriptor table.
    pub desc: usize,
    /// Offset of the available ring.
    pub avail: usize,
    /// Offset of the used ring.
    pub used: usize,
    /// Number of entries in each ring.
    pub size: u16,
    /// One past the last byte the queue uses.
    pub end: usize,
}

impl RingLayout {
    /// Lay the queue out at `base`, rounding each region up to its alignment.
    pub fn new(base: usize, size: u16) -> Self {
        let count = usize::from(size);
        let desc = align_up(base, 16);
        let avail = align_up(desc + 16 * count, 2);
        let avail_len = 4 + 2 * count;
        let used = align_up(avail + avail_len, 4);
        let used_len = 4 + 8 * count;
        Self {
            desc,
            avail,
            used,
            size,
            end: used + used_len,
        }
    }

    /// Offset of available-ring slot `index`.
    pub fn avail_slot(&self, index: u16) -> Option<usize> {
        if index >= self.size {
            return None;
        }
        Some(self.avail + 4 + 2 * usize::from(index))
    }

    /// Offset of used-ring entry `index`.
    pub fn used_slot(&self, index: u16) -> Option<usize> {
        if index >= self.size {
            return None;
        }
        Some(self.used + 4 + 8 * usize::from(index))
    }

    /// Offset of the available-ring `idx` field.
    pub fn avail_idx(&self) -> usize {
        self.avail + 2
    }

    /// Offset of the used-ring `idx` field.
    pub fn used_idx(&self) -> usize {
        self.used + 2
    }
}

/// One split-virtqueue descriptor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Descriptor {
    /// Device-visible buffer address.
    pub addr: u64,
    /// Buffer length in bytes.
    pub len: u32,
    /// [`DESC_F_NEXT`], [`DESC_F_WRITE`] and friends.
    pub flags: u16,
    /// The next descriptor when `DESC_F_NEXT` is set.
    pub next: u16,
}

/// Encode a descriptor in the ring's 16-byte little-endian format.
pub fn encode_descriptor(descriptor: Descriptor) -> [u8; 16] {
    let mut bytes = [0u8; 16];
    bytes[0..8].copy_from_slice(&descriptor.addr.to_le_bytes());
    bytes[8..12].copy_from_slice(&descriptor.len.to_le_bytes());
    bytes[12..14].copy_from_slice(&descriptor.flags.to_le_bytes());
    bytes[14..16].copy_from_slice(&descriptor.next.to_le_bytes());
    bytes
}

/// Decode one 16-byte descriptor.
pub fn decode_descriptor(bytes: &[u8]) -> Option<Descriptor> {
    if bytes.len() < 16 {
        return None;
    }
    Some(Descriptor {
        addr: u64::from_le_bytes(bytes.get(0..8)?.try_into().ok()?),
        len: u32::from_le_bytes(bytes.get(8..12)?.try_into().ok()?),
        flags: u16::from_le_bytes(bytes.get(12..14)?.try_into().ok()?),
        next: u16::from_le_bytes(bytes.get(14..16)?.try_into().ok()?),
    })
}

/// One used-ring entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UsedEntry {
    /// Descriptor index the device finished.
    pub id: u32,
    /// Number of bytes written or consumed.
    pub len: u32,
}

/// Decode one 8-byte used-ring entry.
pub fn decode_used(bytes: &[u8]) -> Option<UsedEntry> {
    if bytes.len() < 8 {
        return None;
    }
    Some(UsedEntry {
        id: u32::from_le_bytes(bytes.get(0..4)?.try_into().ok()?),
        len: u32::from_le_bytes(bytes.get(4..8)?.try_into().ok()?),
    })
}

/// Choose the queue size the driver programs: the largest power of two up to 8 that the device
/// offers.
pub fn queue_size(device_max: u16) -> Option<u16> {
    let capped = device_max.min(8);
    if capped < 4 {
        return None;
    }
    Some(if capped >= 8 { 8 } else { 4 })
}

/// Encode the 16-byte `virtio_blk_req` header.
pub fn encode_request(op: u32, sector: u64) -> [u8; REQUEST_HEADER_LEN] {
    let mut bytes = [0u8; REQUEST_HEADER_LEN];
    bytes[0..4].copy_from_slice(&op.to_le_bytes());
    bytes[4..8].copy_from_slice(&0u32.to_le_bytes());
    bytes[8..16].copy_from_slice(&sector.to_le_bytes());
    bytes
}

/// Decode a `virtio_blk_req` header.
pub fn decode_request(bytes: &[u8]) -> Option<(u32, u64)> {
    if bytes.len() < REQUEST_HEADER_LEN {
        return None;
    }
    let op = u32::from_le_bytes(bytes.get(0..4)?.try_into().ok()?);
    let reserved = u32::from_le_bytes(bytes.get(4..8)?.try_into().ok()?);
    if reserved != 0 {
        return None;
    }
    let sector = u64::from_le_bytes(bytes.get(8..16)?.try_into().ok()?);
    Some((op, sector))
}

/// Why a request was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequestError {
    /// The operation is not read, write or flush.
    BadOp,
    /// The sector range is empty, overflows or lies outside the device.
    BadRange,
    /// A write was requested on a read-only device.
    ReadOnly,
}

/// Validate a protocol request against the device geometry.
///
/// Returns the data length in bytes: `count * 512` for read/write, zero for flush.
pub fn validate_request(
    op: u32,
    sector: u64,
    count: u32,
    capacity: u64,
    read_only: bool,
) -> Result<usize, RequestError> {
    match op {
        BLK_T_IN | BLK_T_OUT => {
            if count == 0 || count > MAX_SECTORS {
                return Err(RequestError::BadRange);
            }
            if op == BLK_T_OUT && read_only {
                return Err(RequestError::ReadOnly);
            }
            let end = sector
                .checked_add(u64::from(count))
                .ok_or(RequestError::BadRange)?;
            if end > capacity {
                return Err(RequestError::BadRange);
            }
            usize::try_from(count)
                .map_err(|_| RequestError::BadRange)?
                .checked_mul(SECTOR_SIZE as usize)
                .ok_or(RequestError::BadRange)
        }
        BLK_T_FLUSH => {
            if sector != 0 || count != 0 {
                return Err(RequestError::BadRange);
            }
            Ok(0)
        }
        _ => Err(RequestError::BadOp),
    }
}

/// Fill `out` (a whole number of sectors) with the deterministic pattern for `base_lba`.
///
/// Each sector starts with its little-endian LBA and has `fill` in every other byte, so a sector
/// written at the wrong offset is detectable.
pub fn fill_pattern(base_lba: u64, fill: u8, out: &mut [u8]) {
    out.fill(fill);
    let mut sector = 0usize;
    while let Some(chunk) = out.get_mut(sector * SECTOR_SIZE as usize..) {
        let Some(bytes) = chunk.get_mut(..SECTOR_SIZE as usize) else {
            break;
        };
        let lba = base_lba.saturating_add(sector as u64);
        bytes[0..8].copy_from_slice(&lba.to_le_bytes());
        sector += 1;
    }
}

/// Verify `bytes` against the pattern [`fill_pattern`] generates for `base_lba`.
pub fn verify_pattern(base_lba: u64, fill: u8, bytes: &[u8]) -> bool {
    if !bytes.len().is_multiple_of(SECTOR_SIZE as usize) {
        return false;
    }
    bytes
        .chunks_exact(SECTOR_SIZE as usize)
        .enumerate()
        .all(|(sector, chunk)| {
            let lba = base_lba.saturating_add(sector as u64);
            chunk.get(..8) == Some(&lba.to_le_bytes())
                && chunk
                    .get(8..)
                    .is_some_and(|rest| rest.iter().all(|byte| *byte == fill))
        })
}

/// A streaming 64-bit FNV-1a hash over 8-byte words (the qtest checksum).
#[derive(Clone, Copy, Debug)]
pub struct Hasher(u64);

impl Default for Hasher {
    fn default() -> Self {
        Self::new()
    }
}

impl Hasher {
    /// A fresh hash.
    pub fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }

    /// Fold `bytes` into the hash.
    pub fn update(&mut self, bytes: &[u8]) {
        let mut words = bytes.chunks_exact(8);
        for word in &mut words {
            let value = u64::from_le_bytes(word.try_into().unwrap_or([0; 8]));
            self.0 ^= value;
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
        let mut tail = [0u8; 8];
        let rest = words.remainder();
        if let Some(destination) = tail.get_mut(..rest.len()) {
            destination.copy_from_slice(rest);
        }
        self.0 ^= u64::from_le_bytes(tail);
        self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
    }

    /// The current hash.
    pub fn finish(self) -> u64 {
        self.0
    }
}

/// Marker written after the first pattern pass: magic plus the data checksum.
pub const MARKER_MAGIC: &[u8; 16] = b"RavenBSD-K15-cy\n";

/// Encode the marker sector.
pub fn marker_encode(checksum: u64) -> [u8; SECTOR_SIZE as usize] {
    let mut sector = [0u8; SECTOR_SIZE as usize];
    sector[0..16].copy_from_slice(MARKER_MAGIC);
    sector[16..24].copy_from_slice(&checksum.to_le_bytes());
    sector
}

/// Decode the marker sector, returning the stored checksum.
pub fn marker_decode(sector: &[u8]) -> Option<u64> {
    if sector.len() < SECTOR_SIZE as usize {
        return None;
    }
    if sector.get(..16)? != MARKER_MAGIC {
        return None;
    }
    Some(u64::from_le_bytes(sector.get(16..24)?.try_into().ok()?))
}

fn align_up(value: usize, alignment: usize) -> usize {
    if alignment <= 1 {
        return value;
    }
    let remainder = value % alignment;
    if remainder == 0 {
        value
    } else {
        value + (alignment - remainder)
    }
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
