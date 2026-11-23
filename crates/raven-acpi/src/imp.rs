// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! ACPI table validation, root-table walking, MCFG decoding and the Raven boot bundle.

use core::fmt;

/// RSDP signature (`RSD PTR `).
pub const RSDP_SIGNATURE: &[u8; 8] = b"RSD PTR ";
/// Minimum ACPI 1.0 RSDP length.
pub const RSDP_LENGTH_V1: usize = 20;
/// Minimum ACPI 2.0+ RSDP length.
pub const RSDP_LENGTH_V2: usize = 36;
/// Standard ACPI table header length (the MCFG allocation entries start after its reserved 8).
pub const TABLE_HEADER: usize = 36;
/// First MCFG allocation entry, after the 36-byte header and 8 reserved bytes.
pub const MCFG_FIRST_ENTRY: usize = 44;
/// One MCFG allocation entry.
pub const MCFG_ENTRY: usize = 16;
/// Largest ACPI table the boot bundle accepts.
pub const MAX_TABLE_BYTES: u32 = 64 * 1024;
/// Largest number of tables the boot bundle accepts.
pub const MAX_TABLES: usize = 64;
/// Largest bundle the x86 and aarch64 boot paths produce.
pub const MAX_BLOB_BYTES: usize = 256 * 1024;

/// Raven boot-bundle magic (`"RAC1"` little endian), shared with `pcid` and the x86 path.
pub const BLOB_MAGIC: u32 = 0x3143_4152;
/// Raven boot-bundle version.
pub const BLOB_VERSION: u16 = 1;
/// Bundle header length.
pub const BLOB_HEADER: usize = 24;
/// One bundle directory entry.
pub const BLOB_ENTRY: usize = 20;

/// Why an ACPI table or bundle was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// The input is shorter than the structure it claims to be.
    Short,
    /// A signature is not the expected ACPI signature.
    Signature,
    /// The ACPI checksum is not zero.
    Checksum,
    /// A length field is inconsistent with the supplied bytes.
    Length,
    /// Too many tables for the bundle.
    TooMany,
    /// The bundle does not fit the output buffer.
    BlobTooLarge,
    /// The caller could not copy one table.
    Copy,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Short => "short ACPI structure",
            Self::Signature => "bad ACPI signature",
            Self::Checksum => "bad ACPI checksum",
            Self::Length => "bad ACPI length",
            Self::TooMany => "too many ACPI tables",
            Self::BlobTooLarge => "ACPI bundle too large",
            Self::Copy => "ACPI table copy failed",
        })
    }
}

/// A validated RSDP.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rsdp {
    /// ACPI revision (0/1 = RSDT only, 2+ = XSDT preferred).
    pub revision: u8,
    /// RSDT physical address, zero when absent.
    pub rsdt: u32,
    /// XSDT physical address, zero when absent.
    pub xsdt: u64,
    /// Bytes of the RSDP that passed both checksums (20 or 36).
    pub length: u16,
}

/// One MCFG allocation window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct McfgWindow {
    /// ECAM base physical address.
    pub base: u64,
    /// PCI segment group.
    pub segment: u16,
    /// First bus in the window.
    pub start_bus: u8,
    /// Last bus in the window.
    pub end_bus: u8,
}

/// One table copied into the boot bundle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TableEntry {
    /// ACPI signature padded to eight bytes (`RSD PTR `, `XSDT    `, `MCFG    `, ...).
    pub signature: [u8; 8],
    /// Physical address of the table bytes.
    pub physical: u64,
    /// Table length from its standard header.
    pub length: u32,
}

impl TableEntry {
    /// An empty entry, used as a fixed-array initialiser.
    pub const EMPTY: Self = Self {
        signature: [0; 8],
        physical: 0,
        length: 0,
    };
}

/// Build a `TableEntry` from the eight-byte padded signature and a physical address/length.
#[must_use]
pub fn table_entry(signature: &[u8; 8], physical: u64, length: u32) -> TableEntry {
    TableEntry {
        signature: *signature,
        physical,
        length,
    }
}

/// Build a `TableEntry` from a four-byte ACPI signature and a physical address/length.
#[must_use]
pub fn table_entry4(signature: &[u8; 4], physical: u64, length: u32) -> TableEntry {
    let mut padded = [0u8; 8];
    if let Some(head) = padded.get_mut(..4) {
        head.copy_from_slice(signature);
    }
    TableEntry {
        signature: padded,
        physical,
        length,
    }
}

/// Validate and parse an RSDP.
///
/// # Errors
///
/// Returns [`Error::Short`] for a truncated RSDP, [`Error::Signature`] for a wrong signature,
/// [`Error::Checksum`] when either checksum fails, and [`Error::Length`] for an inconsistent
/// ACPI 2.0 length.
pub fn parse_rsdp(bytes: &[u8]) -> Result<Rsdp, Error> {
    let head = bytes.get(..RSDP_LENGTH_V1).ok_or(Error::Short)?;
    if head.get(..8) != Some(RSDP_SIGNATURE.as_slice()) {
        return Err(Error::Signature);
    }
    if !checksum(head) {
        return Err(Error::Checksum);
    }
    let revision = *bytes.get(15).ok_or(Error::Short)?;
    let rsdt = read_u32(bytes, 16).ok_or(Error::Short)?;
    if revision < 2 {
        return Ok(Rsdp {
            revision,
            rsdt,
            xsdt: 0,
            length: RSDP_LENGTH_V1 as u16,
        });
    }
    let length = read_u32(bytes, 20).ok_or(Error::Short)?;
    let length = usize::try_from(length).map_err(|_| Error::Length)?;
    if length < RSDP_LENGTH_V2 {
        return Err(Error::Length);
    }
    let full = bytes.get(..length).ok_or(Error::Length)?;
    if !checksum(full) {
        return Err(Error::Checksum);
    }
    let xsdt = read_u64(bytes, 24).ok_or(Error::Short)?;
    Ok(Rsdp {
        revision,
        rsdt,
        xsdt,
        length: u16::try_from(length).map_err(|_| Error::Length)?,
    })
}

/// Validate one complete ACPI table (header signature, length and checksum).
///
/// `bytes` must be exactly the table the header's length field names.
///
/// # Errors
///
/// Returns [`Error::Short`], [`Error::Signature`], [`Error::Length`] or [`Error::Checksum`].
pub fn validate_table(bytes: &[u8], signature: &[u8; 4]) -> Result<u32, Error> {
    let header = bytes.get(..TABLE_HEADER).ok_or(Error::Short)?;
    if header.get(..4) != Some(signature.as_slice()) {
        return Err(Error::Signature);
    }
    let length = read_u32(bytes, 4).ok_or(Error::Short)?;
    let length_usize = usize::try_from(length).map_err(|_| Error::Length)?;
    if length_usize < TABLE_HEADER || length_usize != bytes.len() {
        return Err(Error::Length);
    }
    if !checksum(bytes) {
        return Err(Error::Checksum);
    }
    Ok(length)
}

/// The table length from a standard ACPI header, without validating the checksum.
#[must_use]
pub fn table_length(bytes: &[u8]) -> Option<u32> {
    read_u32(bytes, 4)
}

/// The four-byte signature from a standard ACPI header.
#[must_use]
pub fn table_signature(bytes: &[u8]) -> Option<[u8; 4]> {
    let signature = bytes.get(..4)?;
    let mut out = [0u8; 4];
    out.copy_from_slice(signature);
    Some(out)
}

/// Iterate the physical addresses in a validated RSDT or XSDT.
///
/// # Errors
///
/// Returns [`Error::Short`], [`Error::Signature`], [`Error::Length`] or [`Error::Checksum`].
pub fn root_entries(root: &[u8], extended: bool) -> Result<RootEntries<'_>, Error> {
    let signature: &[u8; 4] = if extended { b"XSDT" } else { b"RSDT" };
    validate_table(root, signature)?;
    let entry = if extended { 8usize } else { 4usize };
    let payload = root.len().checked_sub(TABLE_HEADER).ok_or(Error::Length)?;
    if !payload.is_multiple_of(entry) {
        return Err(Error::Length);
    }
    Ok(RootEntries {
        bytes: root,
        extended,
        offset: TABLE_HEADER,
    })
}

/// Iterator over the physical table addresses in an RSDT/XSDT.
pub struct RootEntries<'a> {
    bytes: &'a [u8],
    extended: bool,
    offset: usize,
}

impl Iterator for RootEntries<'_> {
    type Item = u64;

    fn next(&mut self) -> Option<u64> {
        let address = if self.extended {
            let value = read_u64(self.bytes, self.offset)?;
            self.offset = self.offset.checked_add(8)?;
            value
        } else {
            let value = read_u32(self.bytes, self.offset)?;
            self.offset = self.offset.checked_add(4)?;
            u64::from(value)
        };
        Some(address)
    }
}

/// Extract the first MCFG allocation window.
///
/// `mcfg` must be exactly one checksum-valid MCFG table.
#[must_use]
pub fn mcfg_window(mcfg: &[u8]) -> Option<McfgWindow> {
    validate_table(mcfg, b"MCFG").ok()?;
    if mcfg.len() < MCFG_FIRST_ENTRY {
        return None;
    }
    let allocations = mcfg.len().checked_sub(MCFG_FIRST_ENTRY)? / MCFG_ENTRY;
    if allocations == 0 {
        return None;
    }
    let base = read_u64(mcfg, MCFG_FIRST_ENTRY)?;
    let segment = read_u16(mcfg, MCFG_FIRST_ENTRY + 8)?;
    let start_bus = *mcfg.get(MCFG_FIRST_ENTRY + 10)?;
    let end_bus = *mcfg.get(MCFG_FIRST_ENTRY + 11)?;
    if base == 0 || start_bus > end_bus {
        return None;
    }
    Some(McfgWindow {
        base,
        segment,
        start_bus,
        end_bus,
    })
}

/// Write the Raven `0x3143_4152` boot bundle.
///
/// `copy` receives each table and the destination slice for its bytes; returning `false` aborts
/// the bundle. The first table's physical address is the RSDP address recorded in the header.
///
/// # Errors
///
/// Returns [`Error::TooMany`], [`Error::BlobTooLarge`] or [`Error::Copy`].
pub fn write_blob<F>(out: &mut [u8], tables: &[TableEntry], mut copy: F) -> Result<usize, Error>
where
    F: FnMut(TableEntry, &mut [u8]) -> bool,
{
    if tables.is_empty() || tables.len() > MAX_TABLES {
        return Err(Error::TooMany);
    }
    let directory = tables
        .len()
        .checked_mul(BLOB_ENTRY)
        .and_then(|size| size.checked_add(BLOB_HEADER))
        .ok_or(Error::BlobTooLarge)?;
    if directory > out.len() {
        return Err(Error::BlobTooLarge);
    }
    let mut offset = directory;
    for (index, table) in tables.iter().enumerate() {
        let slot_start = BLOB_HEADER
            .checked_add(index.checked_mul(BLOB_ENTRY).ok_or(Error::BlobTooLarge)?)
            .ok_or(Error::BlobTooLarge)?;
        let slot_end = slot_start
            .checked_add(BLOB_ENTRY)
            .ok_or(Error::BlobTooLarge)?;
        let slot = out
            .get_mut(slot_start..slot_end)
            .ok_or(Error::BlobTooLarge)?;
        slot.get_mut(..8)
            .ok_or(Error::BlobTooLarge)?
            .copy_from_slice(&table.signature);
        let offset_u32 = u32::try_from(offset).map_err(|_| Error::BlobTooLarge)?;
        put_u32(slot, 8, offset_u32).ok_or(Error::BlobTooLarge)?;
        put_u32(slot, 12, table.length).ok_or(Error::BlobTooLarge)?;
        slot.get_mut(16..20).ok_or(Error::BlobTooLarge)?.fill(0);
        let length = usize::try_from(table.length).map_err(|_| Error::BlobTooLarge)?;
        let end = offset.checked_add(length).ok_or(Error::BlobTooLarge)?;
        let destination = out.get_mut(offset..end).ok_or(Error::BlobTooLarge)?;
        if !copy(*table, destination) {
            return Err(Error::Copy);
        }
        offset = end;
    }
    // Header: magic, version, header size, RSDP physical, table count, directory offset.
    put_u32(out, 0, BLOB_MAGIC).ok_or(Error::BlobTooLarge)?;
    put_u16(out, 4, BLOB_VERSION).ok_or(Error::BlobTooLarge)?;
    put_u16(out, 6, BLOB_HEADER as u16).ok_or(Error::BlobTooLarge)?;
    let rsdp = tables.first().map_or(0, |table| table.physical);
    put_u64(out, 8, rsdp).ok_or(Error::BlobTooLarge)?;
    put_u32(
        out,
        16,
        u32::try_from(tables.len()).map_err(|_| Error::BlobTooLarge)?,
    )
    .ok_or(Error::BlobTooLarge)?;
    put_u32(out, 20, BLOB_HEADER as u32).ok_or(Error::BlobTooLarge)?;
    Ok(offset)
}

/// Find one table's bytes inside a validated Raven boot bundle.
#[must_use]
pub fn blob_table<'a>(blob: &'a [u8], signature: &[u8; 4]) -> Option<&'a [u8]> {
    if read_u32(blob, 0)? != BLOB_MAGIC {
        return None;
    }
    let count = usize::try_from(read_u32(blob, 16)?).ok()?;
    if count > MAX_TABLES {
        return None;
    }
    for index in 0..count {
        let start = BLOB_HEADER.checked_add(index.checked_mul(BLOB_ENTRY)?)?;
        let entry = blob.get(start..start.checked_add(BLOB_ENTRY)?)?;
        if entry.get(..4) != Some(signature.as_slice()) {
            continue;
        }
        let offset = usize::try_from(read_u32(entry, 8)?).ok()?;
        let length = usize::try_from(read_u32(entry, 12)?).ok()?;
        let end = offset.checked_add(length)?;
        return blob.get(offset..end);
    }
    None
}

fn checksum(bytes: &[u8]) -> bool {
    bytes.iter().fold(0u8, |sum, byte| sum.wrapping_add(*byte)) == 0
}

fn read_u16(bytes: &[u8], at: usize) -> Option<u16> {
    let source = bytes.get(at..at.checked_add(2)?)?;
    let mut word = [0u8; 2];
    word.copy_from_slice(source);
    Some(u16::from_le_bytes(word))
}

pub(crate) fn read_u32(bytes: &[u8], at: usize) -> Option<u32> {
    let source = bytes.get(at..at.checked_add(4)?)?;
    let mut word = [0u8; 4];
    word.copy_from_slice(source);
    Some(u32::from_le_bytes(word))
}

fn read_u64(bytes: &[u8], at: usize) -> Option<u64> {
    let source = bytes.get(at..at.checked_add(8)?)?;
    let mut word = [0u8; 8];
    word.copy_from_slice(source);
    Some(u64::from_le_bytes(word))
}

fn put_u16(bytes: &mut [u8], at: usize, value: u16) -> Option<()> {
    let end = at.checked_add(2)?;
    bytes
        .get_mut(at..end)?
        .copy_from_slice(&value.to_le_bytes());
    Some(())
}

fn put_u32(bytes: &mut [u8], at: usize, value: u32) -> Option<()> {
    let end = at.checked_add(4)?;
    bytes
        .get_mut(at..end)?
        .copy_from_slice(&value.to_le_bytes());
    Some(())
}

fn put_u64(bytes: &mut [u8], at: usize, value: u64) -> Option<()> {
    let end = at.checked_add(8)?;
    bytes
        .get_mut(at..end)?
        .copy_from_slice(&value.to_le_bytes());
    Some(())
}
