// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::{Entry, Initfs, InitfsError};
use crate::format::{
    DEFAULT_EPOCH, ENTRY_DATA_LEN_OFFSET, ENTRY_DATA_OFF_OFFSET, ENTRY_MTIME_OFFSET,
    ENTRY_NAME_OFF_OFFSET, ENTRY_RESERVED_OFFSET, HEADER_ENTRY_COUNT_OFFSET,
    HEADER_ENTRY_SIZE_OFFSET, HEADER_EPOCH_OFFSET, HEADER_NAMES_LEN_OFFSET, HEADER_NAMES_OFFSET,
    HEADER_RESERVED_OFFSET, HEADER_SIZE, HEADER_SIZE_OFFSET, HEADER_TOTAL_SIZE_OFFSET,
    HEADER_VERSION_OFFSET, PAGE_SIZE,
};
use crate::testutil::{File, image};

fn valid_image() -> Vec<u8> {
    image(&[
        File {
            path: b"/a",
            data: b"alpha",
            mode: 0o644,
        },
        File {
            path: b"/b",
            data: b"bravo!",
            mode: 0o755,
        },
    ])
}

fn one_file(data: &[u8]) -> Vec<u8> {
    image(&[File {
        path: b"/init",
        data,
        mode: 0o755,
    }])
}

fn put_u32(image: &mut [u8], offset: usize, value: u32) {
    image[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn put_u64(image: &mut [u8], offset: usize, value: u64) {
    image[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

#[test]
fn parse_rejects_too_short() {
    for len in 0..HEADER_SIZE {
        let image = vec![0u8; len];
        assert_eq!(
            Initfs::parse(&image).unwrap_err(),
            InitfsError::TooShort,
            "len={len}"
        );
    }
}

#[test]
fn parse_rejects_bad_magic() {
    let mut image = valid_image();
    image[0] ^= 0xFF;
    assert_eq!(Initfs::parse(&image).unwrap_err(), InitfsError::BadMagic);
}

#[test]
fn parse_rejects_bad_version() {
    let mut image = valid_image();
    put_u32(&mut image, HEADER_VERSION_OFFSET, 99);
    assert_eq!(Initfs::parse(&image).unwrap_err(), InitfsError::BadVersion);
}

#[test]
fn parse_rejects_bad_header_fields() {
    type Mutation = Box<dyn Fn(&mut Vec<u8>)>;
    let mutations: [(&str, Mutation); 8] = [
        (
            "header_size",
            Box::new(|i: &mut Vec<u8>| put_u32(i, HEADER_SIZE_OFFSET, 63)),
        ),
        (
            "entry_size",
            Box::new(|i: &mut Vec<u8>| put_u32(i, HEADER_ENTRY_SIZE_OFFSET, 47)),
        ),
        (
            "header reserved",
            Box::new(|i: &mut Vec<u8>| i[HEADER_RESERVED_OFFSET] = 1),
        ),
        (
            "total too large",
            Box::new(|i: &mut Vec<u8>| {
                let total = (i.len() as u64) + PAGE_SIZE as u64;
                put_u64(i, HEADER_TOTAL_SIZE_OFFSET, total);
            }),
        ),
        (
            "total not page multiple",
            Box::new(|i: &mut Vec<u8>| {
                let total = (i.len() as u64) - 1;
                put_u64(i, HEADER_TOTAL_SIZE_OFFSET, total);
            }),
        ),
        (
            "names before table",
            Box::new(|i: &mut Vec<u8>| put_u64(i, HEADER_NAMES_OFFSET, 0)),
        ),
        (
            "names length overflow",
            Box::new(|i: &mut Vec<u8>| {
                put_u64(i, HEADER_NAMES_LEN_OFFSET, u64::MAX);
            }),
        ),
        (
            "entry count too large",
            Box::new(|i: &mut Vec<u8>| {
                put_u32(i, HEADER_ENTRY_COUNT_OFFSET, 3);
            }),
        ),
    ];

    for (name, mutate) in mutations {
        let mut image = valid_image();
        mutate(&mut image);
        assert_eq!(
            Initfs::parse(&image).unwrap_err(),
            InitfsError::BadHeader,
            "{name}"
        );
    }
}

#[test]
fn parse_rejects_bad_entry_reserved() {
    let mut image = valid_image();
    image[HEADER_SIZE + ENTRY_RESERVED_OFFSET] = 1;
    assert_eq!(Initfs::parse(&image).unwrap_err(), InitfsError::BadEntry(0));
}

#[test]
fn parse_rejects_bad_name_offset() {
    let mut image = one_file(b"x");
    put_u32(&mut image, HEADER_SIZE + ENTRY_NAME_OFF_OFFSET, 5);
    assert_eq!(Initfs::parse(&image).unwrap_err(), InitfsError::BadEntry(0));
}

#[test]
fn parse_rejects_invalid_utf8_name() {
    let image = image(&[File {
        path: b"/\xFF",
        data: b"",
        mode: 0,
    }]);
    assert_eq!(Initfs::parse(&image).unwrap_err(), InitfsError::BadEntry(0));
}

#[test]
fn parse_rejects_invalid_paths() {
    for path in [
        b"relative".as_slice(),
        b"/",
        b"/bin/",
        b"/a//b",
        b"/./a",
        b"/../a",
        b"/a\0b",
    ] {
        let image = image(&[File {
            path,
            data: b"",
            mode: 0,
        }]);
        assert_eq!(
            Initfs::parse(&image).unwrap_err(),
            InitfsError::BadEntry(0),
            "{path:?}"
        );
    }
}

#[test]
fn parse_rejects_unsorted_names() {
    let image = image(&[
        File {
            path: b"/b",
            data: b"",
            mode: 0,
        },
        File {
            path: b"/a",
            data: b"",
            mode: 0,
        },
    ]);
    assert_eq!(Initfs::parse(&image).unwrap_err(), InitfsError::Unsorted);
}

#[test]
fn parse_rejects_unaligned_data() {
    let mut image = one_file(b"x");
    put_u64(&mut image, HEADER_SIZE + ENTRY_DATA_OFF_OFFSET, 4097);
    assert_eq!(Initfs::parse(&image).unwrap_err(), InitfsError::BadEntry(0));
}

#[test]
fn parse_rejects_data_out_of_bounds() {
    let mut image = one_file(b"x");
    let total = image.len() as u64;
    put_u64(&mut image, HEADER_SIZE + ENTRY_DATA_OFF_OFFSET, total);
    put_u64(&mut image, HEADER_SIZE + ENTRY_DATA_LEN_OFFSET, 1);
    assert_eq!(
        Initfs::parse(&image).unwrap_err(),
        InitfsError::OutOfBounds(0)
    );
}

#[test]
fn parse_rejects_overlapping_data() {
    let mut image = image(&[
        File {
            path: b"/a",
            data: b"aaaa",
            mode: 0,
        },
        File {
            path: b"/b",
            data: b"bbbb",
            mode: 0,
        },
    ]);
    let first = Initfs::parse(&image).unwrap();
    let first_offset = first.get("/a").unwrap().data_offset;
    put_u64(
        &mut image,
        HEADER_SIZE + 48 + ENTRY_DATA_OFF_OFFSET,
        first_offset,
    );
    assert_eq!(Initfs::parse(&image).unwrap_err(), InitfsError::Unsorted);
}

#[test]
fn parse_rejects_bad_checksum() {
    let mut image = one_file(b"checksum");
    let data_offset = Initfs::parse(&image)
        .unwrap()
        .get("/init")
        .unwrap()
        .data_offset as usize;
    image[data_offset] ^= 1;
    assert_eq!(
        Initfs::parse(&image).unwrap_err(),
        InitfsError::BadChecksum(0)
    );
}

#[test]
fn unchecked_parse_accepts_a_bad_checksum_and_get_rejects_it() {
    let mut image = one_file(b"checksum");
    let data_offset = Initfs::parse(&image)
        .unwrap()
        .get("/init")
        .unwrap()
        .data_offset as usize;
    image[data_offset] ^= 1;

    let unchecked = Initfs::parse_unchecked(&image).unwrap();
    assert!(unchecked.get("/init").is_none());
    assert_eq!(unchecked.entries().count(), 1);
}

#[test]
fn unchecked_parse_still_rejects_bad_structure() {
    let mut bad_magic = valid_image();
    bad_magic[0] ^= 0xFF;
    assert_eq!(
        Initfs::parse_unchecked(&bad_magic).unwrap_err(),
        InitfsError::BadMagic
    );

    let mut bad_name = one_file(b"x");
    put_u32(&mut bad_name, HEADER_SIZE + ENTRY_NAME_OFF_OFFSET, 5);
    assert_eq!(
        Initfs::parse_unchecked(&bad_name).unwrap_err(),
        InitfsError::BadEntry(0)
    );

    let image = image(&[
        File {
            path: b"/b",
            data: b"",
            mode: 0,
        },
        File {
            path: b"/a",
            data: b"",
            mode: 0,
        },
    ]);
    assert_eq!(
        Initfs::parse_unchecked(&image).unwrap_err(),
        InitfsError::Unsorted
    );
}

#[test]
fn get_verifies_each_entry_independently() {
    let mut image = valid_image();
    let initfs = Initfs::parse_unchecked(&image).unwrap();
    assert!(initfs.get("/a").is_some());
    assert!(initfs.get("/b").is_some());

    let a_offset = initfs.get("/a").unwrap().data_offset as usize;
    image[a_offset] ^= 1;
    let initfs = Initfs::parse_unchecked(&image).unwrap();
    assert!(initfs.get("/a").is_none());
    assert!(initfs.get("/b").is_some());
}

#[test]
fn get_finds_entries_and_misses() {
    let image = valid_image();
    let initfs = Initfs::parse(&image).unwrap();
    assert_eq!(initfs.get("/a").unwrap().data, b"alpha");
    assert_eq!(initfs.get("/b").unwrap().data, b"bravo!");
    assert!(initfs.get("/c").is_none());
    assert!(initfs.get("a").is_none());
    assert!(initfs.get("/").is_none());
}

#[test]
fn entries_expose_all_fields_in_order() {
    let image = valid_image();
    let initfs = Initfs::parse(&image).unwrap();
    let entries: Vec<Entry<'_>> = initfs.entries().collect();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].path, "/a");
    assert_eq!(entries[0].data, b"alpha");
    assert_eq!(entries[0].mode, 0o644);
    assert_eq!(entries[1].path, "/b");
    assert_eq!(entries[1].data, b"bravo!");
    assert_eq!(entries[1].mode, 0o755);
}

#[test]
fn len_and_is_empty() {
    let empty_image = crate::testutil::image(&[]);
    let empty = Initfs::parse(&empty_image).unwrap();
    assert_eq!(empty.len(), 0);
    assert!(empty.is_empty());

    let image = valid_image();
    let initfs = Initfs::parse(&image).unwrap();
    assert_eq!(initfs.len(), 2);
    assert!(!initfs.is_empty());
}

#[test]
fn parse_rejects_every_truncation() {
    let image = valid_image();
    for len in 0..image.len() {
        assert!(
            Initfs::parse(&image[..len]).is_err(),
            "truncation at {len} bytes was accepted"
        );
    }
}

#[test]
fn every_metadata_bit_flip_does_not_panic() {
    let image = valid_image();
    let initfs = Initfs::parse(&image).unwrap();
    let metadata_end = initfs
        .entries()
        .map(|entry| entry.data_offset as usize)
        .min()
        .unwrap_or(image.len());

    for bit in 0..metadata_end * 8 {
        let mut mutated = image.clone();
        mutated[bit / 8] ^= 1 << (bit % 8);
        if let Ok(parsed) = Initfs::parse(&mutated) {
            assert_eq!(parsed.entries().count(), parsed.len());
            for entry in parsed.entries() {
                assert!(parsed.get(entry.path).is_some());
            }
        }
    }
}

#[test]
fn every_data_bit_flip_is_a_checksum_error() {
    let image = one_file(b"0123456789");
    let initfs = Initfs::parse(&image).unwrap();
    let entry = initfs.get("/init").unwrap();
    let data_offset = entry.data_offset as usize;
    for bit in 0..entry.data.len() * 8 {
        let mut mutated = image.clone();
        mutated[data_offset + bit / 8] ^= 1 << (bit % 8);
        assert_eq!(
            Initfs::parse(&mutated).unwrap_err(),
            InitfsError::BadChecksum(0),
            "bit {bit}"
        );
        let unchecked = Initfs::parse_unchecked(&mutated).unwrap();
        assert!(unchecked.get("/init").is_none(), "bit {bit}");
    }
}

fn xorshift(state: &mut u64) -> u64 {
    let mut value = *state;
    value ^= value << 13;
    value ^= value >> 7;
    value ^= value << 17;
    *state = value;
    value
}

#[test]
fn random_mutations_never_panic() {
    let image = valid_image();
    let mut state = 0x5EED_1234_ABCD_9876u64;
    for case in 0..10_000 {
        let mut mutated = image.clone();
        let changes = (xorshift(&mut state) % 3) as usize + 1;
        for _ in 0..changes {
            let index = (xorshift(&mut state) as usize) % mutated.len();
            let value = xorshift(&mut state) as u8;
            mutated[index] ^= value;
        }
        if let Ok(parsed) = Initfs::parse(&mutated) {
            assert_eq!(parsed.entries().count(), parsed.len(), "case {case}");
            for entry in parsed.entries() {
                assert!(parsed.get(entry.path).is_some(), "case {case}");
            }
        }
        if let Ok(unchecked) = Initfs::parse_unchecked(&mutated) {
            assert_eq!(unchecked.entries().count(), unchecked.len(), "case {case}");
        }
    }
}

#[test]
fn zero_length_file_roundtrip() {
    let image = one_file(b"");
    let initfs = Initfs::parse(&image).unwrap();
    let entry = initfs.get("/init").unwrap();
    assert!(entry.data.is_empty());
    assert_eq!(entry.data_offset % PAGE_SIZE as u64, 0);
}

/// K26k guard: `parse_unchecked` must not walk file data, and `get` must check only its entry.
///
/// A 16 MiB file makes a full-image scan unmistakable in the byte counter; the test does not
/// depend on wall-clock time, so a loaded host cannot make it flaky.
#[test]
fn parse_unchecked_and_get_scan_only_what_they_must() {
    let big = vec![0xA5u8; 16 * 1024 * 1024];
    let small = b"small".to_vec();
    let image = image(&[
        File {
            path: b"/big",
            data: &big,
            mode: 0o644,
        },
        File {
            path: b"/small",
            data: &small,
            mode: 0o644,
        },
    ]);
    let total = big.len() + small.len();

    crate::crc::scan::reset();
    let initfs = Initfs::parse_unchecked(&image).unwrap();
    assert_eq!(
        crate::crc::scan::bytes(),
        0,
        "parse_unchecked scanned file data"
    );

    crate::crc::scan::reset();
    assert!(initfs.get("/big").is_some());
    assert_eq!(
        crate::crc::scan::bytes(),
        big.len(),
        "get scanned beyond its entry"
    );

    crate::crc::scan::reset();
    assert!(initfs.get("/missing").is_none());
    assert_eq!(crate::crc::scan::bytes(), 0, "a miss scanned file data");

    crate::crc::scan::reset();
    assert!(Initfs::parse(&image).is_ok());
    assert_eq!(
        crate::crc::scan::bytes(),
        total,
        "parse did not validate exactly the data"
    );
}

/// K26k guard: a corrupt entry costs one entry scan, not a full-image rescan.
#[test]
fn get_checks_only_the_requested_entry() {
    let big = vec![0x5Au8; 8 * 1024 * 1024];
    let mut image = image(&[
        File {
            path: b"/big",
            data: &big,
            mode: 0o644,
        },
        File {
            path: b"/small",
            data: b"ok",
            mode: 0o644,
        },
    ]);
    let offset = Initfs::parse_unchecked(&image)
        .unwrap()
        .get("/big")
        .unwrap()
        .data_offset as usize;
    image[offset] ^= 1;
    let initfs = Initfs::parse_unchecked(&image).unwrap();

    crate::crc::scan::reset();
    assert!(initfs.get("/big").is_none());
    assert_eq!(
        crate::crc::scan::bytes(),
        big.len(),
        "a failed get scanned the whole image"
    );

    crate::crc::scan::reset();
    assert!(initfs.get("/small").is_some());
    assert_eq!(
        crate::crc::scan::bytes(),
        2,
        "a good get scanned more than its entry"
    );
}

#[test]
fn entries_inherit_the_image_epoch() {
    let image = valid_image();
    let initfs = Initfs::parse(&image).unwrap();
    assert_eq!(initfs.image_epoch(), DEFAULT_EPOCH);
    assert_eq!(initfs.get("/a").unwrap().mtime, DEFAULT_EPOCH);
    assert_eq!(initfs.get("/b").unwrap().mtime, DEFAULT_EPOCH);
}

#[test]
fn an_explicit_entry_mtime_is_read() {
    let mut image = one_file(b"x");
    put_u64(&mut image, HEADER_EPOCH_OFFSET, 1_700_000_000);
    put_u64(&mut image, HEADER_SIZE + ENTRY_MTIME_OFFSET, 1_650_000_000);
    let initfs = Initfs::parse(&image).unwrap();
    assert_eq!(initfs.get("/init").unwrap().mtime, 1_650_000_000);
}

#[test]
fn legacy_version_one_uses_the_default_epoch() {
    let mut image = valid_image();
    put_u32(&mut image, HEADER_VERSION_OFFSET, 1);
    for byte in image
        .get_mut(HEADER_EPOCH_OFFSET..HEADER_EPOCH_OFFSET + 16)
        .unwrap()
    {
        *byte = 0;
    }
    let initfs = Initfs::parse(&image).unwrap();
    assert_eq!(initfs.image_epoch(), DEFAULT_EPOCH);
    assert_eq!(initfs.get("/a").unwrap().mtime, DEFAULT_EPOCH);
}
