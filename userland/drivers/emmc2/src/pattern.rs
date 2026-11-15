// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Deterministic block pattern, checksum and marker for the `pi4-emmc2` test mode.
//!
//! The qtest writes a sector-stamped pattern through the driver and records its FNV-1a checksum
//! in a reserved marker sector. The probe reads the pattern and the marker through the `disk:`
//! scheme and compares them, so a wrong sector, a truncated reply or a lost write is visible.

/// Logical sector size of the test disk.
pub const SECTOR_SIZE: usize = 512;

/// Marker magic in the reserved test sector.
pub const MARKER_MAGIC: &[u8; 16] = b"RavenBSD-J4-cy\n\0";

/// FNV-1a hash over 8-byte little-endian words.
#[derive(Clone, Copy, Debug)]
pub struct Hasher(u64);

impl Default for Hasher {
    fn default() -> Self {
        Self::new()
    }
}

impl Hasher {
    /// A fresh hash.
    pub const fn new() -> Self {
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
        if !rest.is_empty() {
            if let Some(destination) = tail.get_mut(..rest.len()) {
                destination.copy_from_slice(rest);
            }
            self.0 ^= u64::from_le_bytes(tail);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }

    /// The current hash.
    pub const fn finish(self) -> u64 {
        self.0
    }
}

/// Fill `out` with a sector-stamped pattern.
///
/// Each sector starts with its little-endian LBA; every other byte is `fill`. A sector written at
/// the wrong address therefore fails [`verify_pattern`].
pub fn fill_pattern(base_lba: u64, fill: u8, out: &mut [u8]) {
    out.fill(fill);
    for (sector, chunk) in out.chunks_exact_mut(SECTOR_SIZE).enumerate() {
        let lba = base_lba.saturating_add(sector as u64);
        if let Some(header) = chunk.get_mut(..8) {
            header.copy_from_slice(&lba.to_le_bytes());
        }
    }
}

/// Verify `bytes` against the pattern [`fill_pattern`] generates for `base_lba`.
pub fn verify_pattern(base_lba: u64, fill: u8, bytes: &[u8]) -> bool {
    if !bytes.len().is_multiple_of(SECTOR_SIZE) {
        return false;
    }
    bytes
        .chunks_exact(SECTOR_SIZE)
        .enumerate()
        .all(|(sector, chunk)| {
            let lba = base_lba.saturating_add(sector as u64);
            chunk.get(..8) == Some(&lba.to_le_bytes())
                && chunk
                    .get(8..)
                    .is_some_and(|rest| rest.iter().all(|byte| *byte == fill))
        })
}

/// Encode the marker sector carrying `checksum`.
pub fn marker_encode(checksum: u64) -> [u8; SECTOR_SIZE] {
    let mut sector = [0u8; SECTOR_SIZE];
    sector[..16].copy_from_slice(MARKER_MAGIC);
    sector[16..24].copy_from_slice(&checksum.to_le_bytes());
    sector
}

/// Decode the checksum from a marker sector.
pub fn marker_decode(sector: &[u8]) -> Option<u64> {
    if sector.len() < SECTOR_SIZE {
        return None;
    }
    if sector.get(..16)? != MARKER_MAGIC {
        return None;
    }
    Some(u64::from_le_bytes(sector.get(16..24)?.try_into().ok()?))
}

#[cfg(test)]
#[path = "pattern_ut.rs"]
mod tests;
