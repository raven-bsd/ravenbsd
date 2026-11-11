// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Cheap C7 initfs name-table scan for the boot decision.
//!
//! The strict `raven_initfs::Initfs` parser validates every file's CRC, which dominates a debug
//! build's boot time for the multi-megabyte initfs. PID 1 only needs to know whether the image
//! carries a `/etc/services.d/*.toml` manifest before choosing between the service manager and the
//! legacy banner. This module reads the header, entry table and names blob only; it never reads
//! file data and never validates a data CRC.
//!
//! This is safe as a *decision* aid: the kernel parses and validates the same image before it
//! starts `/init`, and every path that consumes initfs data still goes through `Initfs::parse`.
//! Any structural problem returns `None`, so the caller falls back to the strict parser and gets
//! its exact error.

const MAGIC: &[u8] = b"RAVENFS1";
const VERSION: u32 = 1;
const PAGE_SIZE: usize = 4096;
const HEADER_SIZE: usize = 64;
const ENTRY_SIZE: usize = 48;

const HEADER_VERSION_OFFSET: usize = 8;
const HEADER_SIZE_OFFSET: usize = 12;
const HEADER_ENTRY_COUNT_OFFSET: usize = 16;
const HEADER_ENTRY_SIZE_OFFSET: usize = 20;
const HEADER_NAMES_OFFSET: usize = 24;
const HEADER_NAMES_LEN_OFFSET: usize = 32;
const HEADER_TOTAL_SIZE_OFFSET: usize = 40;

const ENTRY_NAME_OFF_OFFSET: usize = 0;
const ENTRY_NAME_LEN_OFFSET: usize = 4;

const MANIFEST_PREFIX: &str = "/etc/services.d/";
const MANIFEST_SUFFIX: &str = ".toml";

/// Report whether the initfs name table contains a service manifest.
///
/// Returns `Some(true)` when a `/etc/services.d/*.toml` path is present, `Some(false)` when the
/// complete name table is readable and has none, and `None` when the header, table or names are
/// not structurally readable. `None` means "use `raven_initfs::Initfs::parse`": this scan does
/// not validate file data, paths or data ranges.
pub fn initfs_has_manifest(image: &[u8]) -> Option<bool> {
    if image.len() < HEADER_SIZE || image.get(..MAGIC.len())? != MAGIC {
        return None;
    }
    let version = read_u32(image, HEADER_VERSION_OFFSET)?;
    let header_size = read_u32(image, HEADER_SIZE_OFFSET)?;
    let entry_size = read_u32(image, HEADER_ENTRY_SIZE_OFFSET)?;
    if version != VERSION
        || header_size as usize != HEADER_SIZE
        || entry_size as usize != ENTRY_SIZE
    {
        return None;
    }
    let entry_count = usize::try_from(read_u32(image, HEADER_ENTRY_COUNT_OFFSET)?).ok()?;
    let names_offset = usize::try_from(read_u64(image, HEADER_NAMES_OFFSET)?).ok()?;
    let names_len = usize::try_from(read_u64(image, HEADER_NAMES_LEN_OFFSET)?).ok()?;
    let total_size = usize::try_from(read_u64(image, HEADER_TOTAL_SIZE_OFFSET)?).ok()?;
    if total_size != image.len() || !total_size.is_multiple_of(PAGE_SIZE) {
        return None;
    }

    let table_len = entry_count.checked_mul(ENTRY_SIZE)?;
    let table_end = HEADER_SIZE.checked_add(table_len)?;
    if table_end > image.len() || names_offset < table_end {
        return None;
    }
    let names_end = names_offset.checked_add(names_len)?;
    let names = image.get(names_offset..names_end)?;

    for index in 0..entry_count {
        let base = HEADER_SIZE.checked_add(index.checked_mul(ENTRY_SIZE)?)?;
        let name_off = usize::try_from(read_u32(image, base + ENTRY_NAME_OFF_OFFSET)?).ok()?;
        let name_len = usize::try_from(read_u32(image, base + ENTRY_NAME_LEN_OFFSET)?).ok()?;
        let name_end = name_off.checked_add(name_len)?;
        let name = names.get(name_off..name_end)?;
        let path = core::str::from_utf8(name).ok()?;
        if path.starts_with(MANIFEST_PREFIX) && path.ends_with(MANIFEST_SUFFIX) {
            return Some(true);
        }
    }
    Some(false)
}

fn read_u32(image: &[u8], offset: usize) -> Option<u32> {
    let end = offset.checked_add(4)?;
    let bytes = image.get(offset..end)?;
    Some(u32::from_le_bytes(bytes.try_into().ok()?))
}

fn read_u64(image: &[u8], offset: usize) -> Option<u64> {
    let end = offset.checked_add(8)?;
    let bytes = image.get(offset..end)?;
    Some(u64::from_le_bytes(bytes.try_into().ok()?))
}

#[cfg(test)]
mod tests {
    use super::{PAGE_SIZE, initfs_has_manifest};
    use raven_initfs::{Initfs, InitfsBuilder};

    fn image(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut builder = InitfsBuilder::new();
        for (path, data) in files {
            builder.add_file(path, data, 0o644).expect("add file");
        }
        builder.build()
    }

    fn base_image() -> Vec<u8> {
        image(&[("/init", b"init"), ("/bin/uname", b"uname")])
    }

    #[test]
    fn an_image_without_manifests_reports_false() {
        assert_eq!(initfs_has_manifest(&base_image()), Some(false));
    }

    #[test]
    fn a_manifest_reports_true() {
        let image = image(&[
            ("/init", b"init"),
            ("/etc/services.d/10-log.toml", b"name = \"log\"\n"),
        ]);
        assert_eq!(initfs_has_manifest(&image), Some(true));
    }

    #[test]
    fn a_non_manifest_toml_reports_false() {
        let image = image(&[
            ("/etc/rc.conf", b"exit_when_settled = true\n"),
            ("/etc/services.d/readme.txt", b"not a manifest\n"),
        ]);
        assert_eq!(initfs_has_manifest(&image), Some(false));
    }

    #[test]
    fn the_scan_agrees_with_the_strict_reader() {
        let image = image(&[
            ("/init", b"init"),
            ("/etc/services.d/20-worker.toml", b"name = \"worker\"\n"),
            ("/etc/services.d/10-log.toml", b"name = \"log\"\n"),
        ]);
        let strict = Initfs::parse(&image)
            .expect("strict parse")
            .entries()
            .any(|entry| {
                entry.path.starts_with("/etc/services.d/") && entry.path.ends_with(".toml")
            });
        assert_eq!(initfs_has_manifest(&image), Some(strict));
        assert!(strict);
    }

    #[test]
    fn data_crc_is_not_part_of_the_scan() {
        let mut image = base_image();
        let offset = Initfs::parse(&image)
            .expect("strict parse before corruption")
            .get("/init")
            .expect("/init")
            .data_offset as usize;
        image[offset] ^= 0xff;
        assert_eq!(initfs_has_manifest(&image), Some(false));
        assert!(Initfs::parse(&image).is_err());
    }

    #[test]
    fn corrupt_magic_is_none() {
        let mut image = base_image();
        image[0] ^= 0xff;
        assert_eq!(initfs_has_manifest(&image), None);
    }

    #[test]
    fn truncated_image_is_none() {
        let mut image = base_image();
        image.truncate(image.len() - PAGE_SIZE);
        assert_eq!(initfs_has_manifest(&image), None);
    }
}
