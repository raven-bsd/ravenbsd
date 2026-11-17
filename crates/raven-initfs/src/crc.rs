// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! CRC-32 (IEEE 802.3, reflected) used to validate every file in the archive.

/// The reflected IEEE 802.3 polynomial.
const POLYNOMIAL: u32 = 0xEDB8_8320;

/// Build the slicing-by-8 tables: slice `n` advances the CRC by `n` bytes.
const fn make_tables() -> [[u32; 256]; 8] {
    let mut tables = [[0u32; 256]; 8];
    let mut index = 0usize;
    while index < 256 {
        let mut crc = index as u32;
        let mut bit = 0u32;
        while bit < 8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (POLYNOMIAL & mask);
            bit += 1;
        }
        tables[0][index] = crc;
        index += 1;
    }
    let mut slice = 1usize;
    while slice < 8 {
        let mut index = 0usize;
        while index < 256 {
            let previous = tables[slice - 1][index];
            tables[slice][index] = (previous >> 8) ^ tables[0][(previous & 0xff) as usize];
            index += 1;
        }
        slice += 1;
    }
    tables
}

/// The slicing-by-8 tables, built at compile time.
static TABLES: [[u32; 256]; 8] = make_tables();

/// Test-only accounting of how many bytes the CRC has been asked to scan.
///
/// The regression guard uses this to prove that `parse_unchecked` does not walk file data and
/// that `get` validates only the entry it returns. It is compiled out of production builds.
#[cfg(test)]
pub(crate) mod scan {
    use std::cell::Cell;

    std::thread_local! {
        static BYTES: Cell<usize> = const { Cell::new(0) };
    }

    pub(crate) fn reset() {
        BYTES.with(|bytes| bytes.set(0));
    }

    pub(crate) fn bytes() -> usize {
        BYTES.with(|bytes| bytes.get())
    }

    pub(crate) fn add(length: usize) {
        BYTES.with(|bytes| bytes.set(bytes.get().saturating_add(length)));
    }
}

/// Compute the IEEE 802.3 CRC-32 of `data`.
///
/// Reflected polynomial `0xEDB8_8320`, initial value `0xFFFF_FFFF`, final XOR with
/// `0xFFFF_FFFF`. Slicing-by-8 consumes eight bytes per iteration with one table lookup per
/// byte; the tail falls back to the byte loop.
pub fn crc32(data: &[u8]) -> u32 {
    #[cfg(test)]
    scan::add(data.len());
    let mut crc = 0xFFFF_FFFFu32;
    let mut chunks = data.chunks_exact(8);
    for chunk in &mut chunks {
        let first = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]) ^ crc;
        let second = u32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]);
        crc = TABLES[7][(first & 0xff) as usize]
            ^ TABLES[6][((first >> 8) & 0xff) as usize]
            ^ TABLES[5][((first >> 16) & 0xff) as usize]
            ^ TABLES[4][((first >> 24) & 0xff) as usize]
            ^ TABLES[3][(second & 0xff) as usize]
            ^ TABLES[2][((second >> 8) & 0xff) as usize]
            ^ TABLES[1][((second >> 16) & 0xff) as usize]
            ^ TABLES[0][((second >> 24) & 0xff) as usize];
    }
    for &byte in chunks.remainder() {
        crc = TABLES[0][((crc ^ u32::from(byte)) & 0xff) as usize] ^ (crc >> 8);
    }
    !crc
}

#[cfg(test)]
#[path = "crc_ut.rs"]
mod tests;
