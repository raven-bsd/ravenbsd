// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! GUID Partition Table structures and CRC-validated writers.
//!
//! The layout follows the UEFI specification: a protective MBR, a primary GPT header at LBA 1
//! with 128-byte entries at LBA 2, and a backup header/entry array at the end of the disk. All
//! fields are little-endian except the GUID field encoding, which is the usual mixed-endian form.

use anyhow::{Result, bail};

use raven_initfs::crc32;

/// A sector is always 512 bytes in the images this builder produces.
pub(crate) const SECTOR_SIZE: u64 = 512;
const ENTRY_COUNT: usize = 128;
const ENTRY_SIZE: usize = 128;
const ENTRY_SECTORS: u64 = (ENTRY_COUNT as u64 * ENTRY_SIZE as u64) / SECTOR_SIZE;
const HEADER_SIZE: usize = 92;
const GPT_REVISION: u32 = 0x0001_0000;
const FIRST_USABLE_LBA: u64 = 2 + ENTRY_SECTORS;
const GPT_SIGNATURE: &[u8; 8] = b"EFI PART";

/// ESP type GUID `C12A7328-F81F-11D2-BA4B-00A0C93EC93B`.
pub(crate) const TYPE_ESP: [u8; 16] = [
    0x28, 0x73, 0x2a, 0xc1, 0x1f, 0xf8, 0xd2, 0x11, 0xba, 0x4b, 0x00, 0xa0, 0xc9, 0x3e, 0xc9, 0x3b,
];

/// Linux data type GUID `0FC63DAF-8483-4772-8E79-3D69D8477DE4`.
pub(crate) const TYPE_LINUX_DATA: [u8; 16] = [
    0xaf, 0x3d, 0xc6, 0x0f, 0x83, 0x84, 0x72, 0x47, 0x8e, 0x79, 0x3d, 0x69, 0xd8, 0x47, 0x7d, 0xe4,
];

/// One GPT partition entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Partition {
    pub(crate) type_guid: [u8; 16],
    pub(crate) unique_guid: [u8; 16],
    pub(crate) first_lba: u64,
    pub(crate) last_lba: u64,
    pub(crate) attributes: u64,
    pub(crate) name: String,
}

impl Partition {
    /// Create a partition with a deterministic unique GUID derived from `index`.
    pub(crate) fn new(
        type_guid: [u8; 16],
        index: u32,
        first_lba: u64,
        last_lba: u64,
        name: &str,
    ) -> Self {
        Self {
            type_guid,
            unique_guid: derived_guid(b"partition", index),
            first_lba,
            last_lba,
            attributes: 0,
            name: name.to_string(),
        }
    }
}

/// A validated GPT layout for `total_sectors` 512-byte sectors.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GptLayout {
    total_sectors: u64,
    disk_guid: [u8; 16],
    partitions: Vec<Partition>,
}

impl GptLayout {
    /// Build and validate a layout. Partition LBAs are inclusive and must not overlap.
    pub(crate) fn new(total_sectors: u64, partitions: Vec<Partition>) -> Result<Self> {
        if total_sectors < FIRST_USABLE_LBA + ENTRY_SECTORS + 2 {
            bail!("disk is too small for a GPT ({total_sectors} sectors)");
        }
        if partitions.len() > ENTRY_COUNT {
            bail!("GPT supports at most {ENTRY_COUNT} partitions");
        }
        let last_usable = total_sectors - 1 - ENTRY_SECTORS - 1;
        let mut previous_end = FIRST_USABLE_LBA.saturating_sub(1);
        for partition in &partitions {
            if partition.first_lba > partition.last_lba {
                bail!("partition `{}` has an inverted LBA range", partition.name);
            }
            if partition.first_lba < FIRST_USABLE_LBA || partition.last_lba > last_usable {
                bail!(
                    "partition `{}` is outside the usable LBA range {FIRST_USABLE_LBA}..={last_usable}",
                    partition.name
                );
            }
            if partition.first_lba <= previous_end {
                bail!(
                    "partition `{}` overlaps the previous partition",
                    partition.name
                );
            }
            previous_end = partition.last_lba;
        }
        Ok(Self {
            total_sectors,
            disk_guid: derived_guid(b"disk", 0),
            partitions,
        })
    }

    /// Number of 512-byte sectors in the disk.
    pub(crate) fn total_sectors(&self) -> u64 {
        self.total_sectors
    }

    /// Protective MBR (`LBA 0`).
    pub(crate) fn protective_mbr(&self) -> [u8; 512] {
        let mut mbr = [0u8; 512];
        let entry = 446;
        mbr[entry] = 0x00;
        mbr[entry + 1] = 0x00;
        mbr[entry + 2] = 0x02;
        mbr[entry + 3] = 0x00;
        mbr[entry + 4] = 0xee;
        mbr[entry + 5] = 0xff;
        mbr[entry + 6] = 0xff;
        mbr[entry + 7] = 0xff;
        put_u32(&mut mbr, entry + 8, 1);
        let sectors = self
            .total_sectors
            .saturating_sub(1)
            .min(u64::from(u32::MAX));
        put_u32(&mut mbr, entry + 12, sectors as u32);
        mbr[510] = 0x55;
        mbr[511] = 0xaa;
        mbr
    }

    /// Primary GPT header (`LBA 1`), padded to a sector.
    pub(crate) fn primary_header(&self) -> [u8; 512] {
        self.header(1, self.total_sectors - 1, 2)
    }

    /// Backup GPT header (last LBA), padded to a sector.
    pub(crate) fn backup_header(&self) -> [u8; 512] {
        self.header(
            self.total_sectors - 1,
            1,
            self.total_sectors - 1 - ENTRY_SECTORS,
        )
    }

    /// Primary/backup entry array (16384 bytes; identical in both copies).
    pub(crate) fn entry_array(&self) -> [u8; ENTRY_COUNT * ENTRY_SIZE] {
        let mut entries = [0u8; ENTRY_COUNT * ENTRY_SIZE];
        for (index, partition) in self.partitions.iter().enumerate() {
            let base = index * ENTRY_SIZE;
            entries[base..base + 16].copy_from_slice(&partition.type_guid);
            entries[base + 16..base + 32].copy_from_slice(&partition.unique_guid);
            put_u64(&mut entries, base + 32, partition.first_lba);
            put_u64(&mut entries, base + 40, partition.last_lba);
            put_u64(&mut entries, base + 48, partition.attributes);
            let name = utf16_name(&partition.name);
            entries[base + 56..base + 56 + name.len()].copy_from_slice(&name);
        }
        entries
    }

    fn header(&self, my_lba: u64, alternate_lba: u64, entries_lba: u64) -> [u8; 512] {
        let mut header = [0u8; 512];
        header[0..8].copy_from_slice(GPT_SIGNATURE);
        put_u32(&mut header, 8, GPT_REVISION);
        put_u32(&mut header, 12, HEADER_SIZE as u32);
        put_u64(&mut header, 24, my_lba);
        put_u64(&mut header, 32, alternate_lba);
        put_u64(&mut header, 40, FIRST_USABLE_LBA);
        put_u64(&mut header, 48, self.total_sectors - 1 - ENTRY_SECTORS - 1);
        header[56..72].copy_from_slice(&self.disk_guid);
        put_u64(&mut header, 72, entries_lba);
        put_u32(&mut header, 80, ENTRY_COUNT as u32);
        put_u32(&mut header, 84, ENTRY_SIZE as u32);
        put_u32(&mut header, 88, crc32(&self.entry_array()));
        let crc = crc32(&header[..HEADER_SIZE]);
        put_u32(&mut header, 16, crc);
        header
    }
}

/// Deterministic GUID derivation: a fixed namespace and an index hashed with FNV-1a.
///
/// This is not a cryptographic UUID generator; it only guarantees stable bytes with no randomness.
fn derived_guid(kind: &[u8], index: u32) -> [u8; 16] {
    let mut first = 0xcbf2_9ce4_8422_2325u64;
    for byte in kind.iter().chain(index.to_le_bytes().iter()) {
        first ^= u64::from(*byte);
        first = first.wrapping_mul(0x0000_0100_0000_01b3);
    }
    let mut second = first ^ 0x9e37_79b9_7f4a_7c15;
    for byte in kind.iter().chain(index.to_le_bytes().iter()).rev() {
        second ^= u64::from(*byte);
        second = second.wrapping_mul(0x0000_0100_0000_01b3);
    }
    let mut guid = [0u8; 16];
    guid[..8].copy_from_slice(&first.to_le_bytes());
    guid[8..].copy_from_slice(&second.to_le_bytes());
    guid
}

fn utf16_name(name: &str) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(72);
    for unit in name.encode_utf16().take(35) {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    bytes
}

fn put_u32(bytes: &mut [u8], offset: usize, value: u32) {
    if let Some(slice) = bytes.get_mut(offset..offset + 4) {
        slice.copy_from_slice(&value.to_le_bytes());
    }
}

fn put_u64(bytes: &mut [u8], offset: usize, value: u64) {
    if let Some(slice) = bytes.get_mut(offset..offset + 8) {
        slice.copy_from_slice(&value.to_le_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout() -> GptLayout {
        let total = 256 * 1024 * 1024 / SECTOR_SIZE;
        GptLayout::new(
            total,
            vec![
                Partition::new(TYPE_ESP, 0, 2048, 2048 + 131072 - 1, "ESP"),
                Partition::new(TYPE_LINUX_DATA, 1, 133120, 133120 + 131072 - 1, "ROOT"),
            ],
        )
        .unwrap()
    }

    fn read_u32(bytes: &[u8], offset: usize) -> u32 {
        u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
    }

    fn read_u64(bytes: &[u8], offset: usize) -> u64 {
        u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
    }

    #[test]
    fn esp_and_linux_type_guids_match_the_uefi_values() {
        let guid = |bytes: [u8; 16]| {
            format!(
                "{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}",
                u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
                u16::from_le_bytes(bytes[4..6].try_into().unwrap()),
                u16::from_le_bytes(bytes[6..8].try_into().unwrap()),
                bytes[8],
                bytes[9],
                bytes[10],
                bytes[11],
                bytes[12],
                bytes[13],
                bytes[14],
                bytes[15]
            )
        };
        assert_eq!(guid(TYPE_ESP), "C12A7328-F81F-11D2-BA4B-00A0C93EC93B");
        assert_eq!(
            guid(TYPE_LINUX_DATA),
            "0FC63DAF-8483-4772-8E79-3D69D8477DE4"
        );
    }

    #[test]
    fn protective_mbr_describes_the_whole_disk() {
        let mbr = layout().protective_mbr();
        assert_eq!(mbr[510], 0x55);
        assert_eq!(mbr[511], 0xaa);
        assert_eq!(mbr[446 + 4], 0xee);
        assert_eq!(read_u32(&mbr, 446 + 8), 1);
        assert_eq!(
            read_u32(&mbr, 446 + 12),
            u32::try_from(256 * 1024 * 1024 / SECTOR_SIZE - 1).unwrap()
        );
    }

    #[test]
    fn primary_header_crc_and_geometry_are_valid() {
        let layout = layout();
        let header = layout.primary_header();
        assert_eq!(&header[..8], b"EFI PART");
        assert_eq!(read_u32(&header, 8), GPT_REVISION);
        assert_eq!(read_u32(&header, 12), HEADER_SIZE as u32);
        assert_eq!(read_u64(&header, 24), 1);
        assert_eq!(read_u64(&header, 32), layout.total_sectors - 1);
        assert_eq!(read_u64(&header, 40), 34);
        assert_eq!(read_u64(&header, 48), layout.total_sectors - 34);
        let mut copy = header;
        put_u32(&mut copy, 16, 0);
        assert_eq!(read_u32(&header, 16), crc32(&copy[..HEADER_SIZE]));
    }

    #[test]
    fn backup_header_points_back_to_the_primary() {
        let layout = layout();
        let header = layout.backup_header();
        assert_eq!(read_u64(&header, 24), layout.total_sectors - 1);
        assert_eq!(read_u64(&header, 32), 1);
        assert_eq!(read_u64(&header, 72), layout.total_sectors - 33);
        let mut copy = header;
        put_u32(&mut copy, 16, 0);
        assert_eq!(read_u32(&header, 16), crc32(&copy[..HEADER_SIZE]));
    }

    #[test]
    fn entry_array_has_type_guid_name_and_crc() {
        let layout = layout();
        let entries = layout.entry_array();
        assert_eq!(&entries[..16], &TYPE_ESP);
        assert_eq!(&entries[16..32], &layout.partitions[0].unique_guid);
        assert_eq!(read_u64(&entries, 32), 2048);
        assert_eq!(read_u64(&entries, 40), 2048 + 131072 - 1);
        let name: Vec<u16> = entries[56..56 + 6]
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        assert_eq!(String::from_utf16(&name).unwrap(), "ESP");
        let second = ENTRY_SIZE;
        assert_eq!(&entries[second..second + 16], &TYPE_LINUX_DATA);
        assert_eq!(read_u64(&entries, second + 32), 133120);
    }

    #[test]
    fn round_trip_parser_checks_crcs_and_backup() {
        let layout = layout();
        let primary = layout.primary_header();
        let backup = layout.backup_header();
        let entries = layout.entry_array();
        let parse = |header: &[u8; 512]| {
            assert_eq!(&header[..8], b"EFI PART");
            let mut copy = *header;
            let stored = read_u32(&copy, 16);
            put_u32(&mut copy, 16, 0);
            assert_eq!(stored, crc32(&copy[..HEADER_SIZE]));
            (
                read_u64(header, 24),
                read_u64(header, 32),
                read_u64(header, 72),
                read_u64(header, 40),
                read_u64(header, 48),
            )
        };
        assert_eq!(parse(&primary).0, 1);
        assert_eq!(parse(&backup).1, 1);
        assert_eq!(parse(&primary).2, 2);
        assert_eq!(parse(&backup).2, parse(&backup).0 - 32);
        assert_eq!(parse(&primary).3, 34);
        assert_eq!(parse(&primary).4, layout.total_sectors - 34);
        assert_eq!(crc32(&entries), read_u32(&primary, 88));
        assert_eq!(crc32(&entries), read_u32(&backup, 88));
    }

    #[test]
    fn overlapping_or_out_of_range_partitions_are_rejected() {
        let total = 256 * 1024 * 1024 / SECTOR_SIZE;
        let bad = |first: u64, last: u64| {
            GptLayout::new(total, vec![Partition::new(TYPE_ESP, 0, first, last, "bad")])
        };
        assert!(bad(0, 10).is_err());
        assert!(bad(2048, total).is_err());
        let overlap = vec![
            Partition::new(TYPE_ESP, 0, 2048, 4096, "one"),
            Partition::new(TYPE_LINUX_DATA, 1, 4096, 8192, "two"),
        ];
        assert!(GptLayout::new(total, overlap).is_err());
    }

    #[test]
    fn derived_guids_are_stable_and_distinct() {
        assert_eq!(derived_guid(b"partition", 1), derived_guid(b"partition", 1));
        assert_ne!(derived_guid(b"partition", 1), derived_guid(b"partition", 2));
        assert_ne!(derived_guid(b"disk", 0), derived_guid(b"partition", 0));
    }
}
