// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Borrowing, allocation-free reader for the initfs format.

use core::cmp::Ordering;

use crate::crc::crc32;
use crate::format::{
    ENTRY_CRC_OFFSET, ENTRY_DATA_LEN_OFFSET, ENTRY_DATA_OFF_OFFSET, ENTRY_MODE_OFFSET,
    ENTRY_NAME_LEN_OFFSET, ENTRY_NAME_OFF_OFFSET, ENTRY_RESERVED_LEN, ENTRY_RESERVED_OFFSET,
    ENTRY_SIZE, HEADER_ENTRY_COUNT_OFFSET, HEADER_ENTRY_SIZE_OFFSET, HEADER_NAMES_LEN_OFFSET,
    HEADER_NAMES_OFFSET, HEADER_RESERVED_LEN, HEADER_RESERVED_OFFSET, HEADER_SIZE,
    HEADER_SIZE_OFFSET, HEADER_TOTAL_SIZE_OFFSET, HEADER_VERSION_OFFSET, MAGIC, PAGE_SIZE, VERSION,
    align_up, read_u32, read_u64, validate_path,
};

/// One file inside an [`Initfs`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Entry<'a> {
    /// Absolute path inside the archive, starting with `/`.
    pub path: &'a str,
    /// The file contents, borrowed from the image.
    pub data: &'a [u8],
    /// Unix permission bits. Informational; the kernel does not enforce them.
    pub mode: u32,
    /// Offset of [`Entry::data`] from the start of the image.
    pub data_offset: u64,
}

/// Why an initfs image was rejected.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum InitfsError {
    /// The image is shorter than the 64-byte header.
    TooShort,
    /// The magic does not spell `RAVENFS1`.
    BadMagic,
    /// The format version is not supported.
    BadVersion,
    /// A header field is inconsistent, or the entry table or names blob is out of bounds.
    BadHeader,
    /// Entry `u32` is malformed: bad name range, invalid path or UTF-8, bad aligned or reserved
    /// field.
    BadEntry(u32),
    /// The data of entry `u32` does not match its CRC-32.
    BadChecksum(u32),
    /// Names or data regions are not strictly increasing in entry order.
    Unsorted,
    /// The data region of entry `u32` lies outside the image.
    OutOfBounds(u32),
}

/// A parsed initfs image.
///
/// Construction validates the complete image up front; every accessor afterwards is total.
#[derive(Clone, Copy, Debug)]
pub struct Initfs<'a> {
    image: &'a [u8],
    entry_count: usize,
    names_offset: usize,
}

impl<'a> Initfs<'a> {
    /// Parse and fully validate an initfs image.
    ///
    /// All arithmetic is checked and no input can make this function panic.
    pub fn parse(image: &'a [u8]) -> Result<Initfs<'a>, InitfsError> {
        if image.len() < HEADER_SIZE {
            return Err(InitfsError::TooShort);
        }
        if image.get(..MAGIC.len()) != Some(&MAGIC[..]) {
            return Err(InitfsError::BadMagic);
        }

        let version = read_u32(image, HEADER_VERSION_OFFSET).ok_or(InitfsError::BadHeader)?;
        if version != VERSION {
            return Err(InitfsError::BadVersion);
        }

        let header_size = read_u32(image, HEADER_SIZE_OFFSET).ok_or(InitfsError::BadHeader)?;
        if header_size as usize != HEADER_SIZE {
            return Err(InitfsError::BadHeader);
        }

        let entry_count_u32 =
            read_u32(image, HEADER_ENTRY_COUNT_OFFSET).ok_or(InitfsError::BadHeader)?;
        let entry_size = read_u32(image, HEADER_ENTRY_SIZE_OFFSET).ok_or(InitfsError::BadHeader)?;
        if entry_size as usize != ENTRY_SIZE {
            return Err(InitfsError::BadHeader);
        }

        let names_offset_u64 =
            read_u64(image, HEADER_NAMES_OFFSET).ok_or(InitfsError::BadHeader)?;
        let names_len_u64 =
            read_u64(image, HEADER_NAMES_LEN_OFFSET).ok_or(InitfsError::BadHeader)?;
        let total_size_u64 =
            read_u64(image, HEADER_TOTAL_SIZE_OFFSET).ok_or(InitfsError::BadHeader)?;

        let reserved = image
            .get(HEADER_RESERVED_OFFSET..HEADER_RESERVED_OFFSET + HEADER_RESERVED_LEN)
            .ok_or(InitfsError::BadHeader)?;
        if reserved.iter().any(|&byte| byte != 0) {
            return Err(InitfsError::BadHeader);
        }

        let image_len = u64::try_from(image.len()).map_err(|_| InitfsError::BadHeader)?;
        if total_size_u64 != image_len || total_size_u64 % PAGE_SIZE as u64 != 0 {
            return Err(InitfsError::BadHeader);
        }

        let entry_count = usize::try_from(entry_count_u32).map_err(|_| InitfsError::BadHeader)?;
        let names_offset = usize::try_from(names_offset_u64).map_err(|_| InitfsError::BadHeader)?;
        let names_len = usize::try_from(names_len_u64).map_err(|_| InitfsError::BadHeader)?;

        let table_len = entry_count
            .checked_mul(ENTRY_SIZE)
            .ok_or(InitfsError::BadHeader)?;
        let table_end = HEADER_SIZE
            .checked_add(table_len)
            .ok_or(InitfsError::BadHeader)?;
        if table_end > image.len() {
            return Err(InitfsError::BadHeader);
        }
        if names_offset < table_end {
            return Err(InitfsError::BadHeader);
        }

        let names_end = names_offset
            .checked_add(names_len)
            .ok_or(InitfsError::BadHeader)?;
        if names_end > image.len() {
            return Err(InitfsError::BadHeader);
        }
        let data_start = align_up(names_end, PAGE_SIZE).ok_or(InitfsError::BadHeader)?;
        if data_start > image.len() {
            return Err(InitfsError::BadHeader);
        }

        let mut previous_name: Option<&[u8]> = None;
        let mut previous_data_end = data_start as u64;

        for index in 0..entry_count {
            let entry_index = u32::try_from(index).map_err(|_| InitfsError::BadHeader)?;
            let base = HEADER_SIZE
                .checked_add(
                    index
                        .checked_mul(ENTRY_SIZE)
                        .ok_or(InitfsError::BadHeader)?,
                )
                .ok_or(InitfsError::BadHeader)?;

            let name_off = read_u32(image, base + ENTRY_NAME_OFF_OFFSET)
                .ok_or(InitfsError::BadEntry(entry_index))?;
            let name_len = read_u32(image, base + ENTRY_NAME_LEN_OFFSET)
                .ok_or(InitfsError::BadEntry(entry_index))?;
            let data_off = read_u64(image, base + ENTRY_DATA_OFF_OFFSET)
                .ok_or(InitfsError::BadEntry(entry_index))?;
            let data_len = read_u64(image, base + ENTRY_DATA_LEN_OFFSET)
                .ok_or(InitfsError::BadEntry(entry_index))?;
            let crc = read_u32(image, base + ENTRY_CRC_OFFSET)
                .ok_or(InitfsError::BadEntry(entry_index))?;

            let entry_reserved = image
                .get(
                    base + ENTRY_RESERVED_OFFSET..base + ENTRY_RESERVED_OFFSET + ENTRY_RESERVED_LEN,
                )
                .ok_or(InitfsError::BadEntry(entry_index))?;
            if entry_reserved.iter().any(|&byte| byte != 0) {
                return Err(InitfsError::BadEntry(entry_index));
            }

            let name_off = u64::from(name_off);
            let name_len = u64::from(name_len);
            let name_end = name_off
                .checked_add(name_len)
                .ok_or(InitfsError::BadEntry(entry_index))?;
            if name_end > names_len as u64 {
                return Err(InitfsError::BadEntry(entry_index));
            }

            let name_start = names_offset
                .checked_add(
                    usize::try_from(name_off).map_err(|_| InitfsError::BadEntry(entry_index))?,
                )
                .ok_or(InitfsError::BadEntry(entry_index))?;
            let name_end = name_start
                .checked_add(
                    usize::try_from(name_len).map_err(|_| InitfsError::BadEntry(entry_index))?,
                )
                .ok_or(InitfsError::BadEntry(entry_index))?;
            let name_bytes = image
                .get(name_start..name_end)
                .ok_or(InitfsError::BadEntry(entry_index))?;
            let path =
                core::str::from_utf8(name_bytes).map_err(|_| InitfsError::BadEntry(entry_index))?;
            validate_path(path).map_err(|_| InitfsError::BadEntry(entry_index))?;

            if let Some(previous) = previous_name
                && previous >= name_bytes
            {
                return Err(InitfsError::Unsorted);
            }
            previous_name = Some(name_bytes);

            if data_off % PAGE_SIZE as u64 != 0 {
                return Err(InitfsError::BadEntry(entry_index));
            }
            let data_end = data_off
                .checked_add(data_len)
                .ok_or(InitfsError::OutOfBounds(entry_index))?;
            if data_end > total_size_u64 {
                return Err(InitfsError::OutOfBounds(entry_index));
            }
            if data_off < data_start as u64 {
                return Err(InitfsError::BadEntry(entry_index));
            }
            if data_off < previous_data_end {
                return Err(InitfsError::Unsorted);
            }
            previous_data_end = data_end;

            let data_begin =
                usize::try_from(data_off).map_err(|_| InitfsError::OutOfBounds(entry_index))?;
            let data_end =
                usize::try_from(data_end).map_err(|_| InitfsError::OutOfBounds(entry_index))?;
            let data = image
                .get(data_begin..data_end)
                .ok_or(InitfsError::OutOfBounds(entry_index))?;
            if crc32(data) != crc {
                return Err(InitfsError::BadChecksum(entry_index));
            }
        }

        Ok(Initfs {
            image,
            entry_count,
            names_offset,
        })
    }

    /// Look up an exact path with a binary search over the sorted entry table.
    pub fn get(&self, path: &str) -> Option<Entry<'a>> {
        let needle = path.as_bytes();
        let mut low = 0usize;
        let mut high = self.entry_count;
        while low < high {
            let mid = low + (high - low) / 2;
            let entry = self.entry_at(mid)?;
            match entry.path.as_bytes().cmp(needle) {
                Ordering::Less => low = mid.checked_add(1)?,
                Ordering::Greater => high = mid,
                Ordering::Equal => return Some(entry),
            }
        }
        None
    }

    /// Iterate over all entries in name order.
    pub fn entries(&self) -> impl Iterator<Item = Entry<'a>> + '_ {
        (0..self.entry_count).filter_map(|index| self.entry_at(index))
    }

    /// Number of entries in the archive.
    pub fn len(&self) -> usize {
        self.entry_count
    }

    /// True when the archive contains no files.
    pub fn is_empty(&self) -> bool {
        self.entry_count == 0
    }

    /// Read one validated entry, or `None` if the index is out of range.
    fn entry_at(&self, index: usize) -> Option<Entry<'a>> {
        if index >= self.entry_count {
            return None;
        }
        let base = HEADER_SIZE.checked_add(index.checked_mul(ENTRY_SIZE)?)?;
        let name_off = read_u32(self.image, base.checked_add(ENTRY_NAME_OFF_OFFSET)?)?;
        let name_len = read_u32(self.image, base.checked_add(ENTRY_NAME_LEN_OFFSET)?)?;
        let data_off = read_u64(self.image, base.checked_add(ENTRY_DATA_OFF_OFFSET)?)?;
        let data_len = read_u64(self.image, base.checked_add(ENTRY_DATA_LEN_OFFSET)?)?;
        let mode = read_u32(self.image, base.checked_add(ENTRY_MODE_OFFSET)?)?;

        let name_start = self
            .names_offset
            .checked_add(usize::try_from(name_off).ok()?)?;
        let name_end = name_start.checked_add(usize::try_from(name_len).ok()?)?;
        let path = core::str::from_utf8(self.image.get(name_start..name_end)?).ok()?;

        let data_start = usize::try_from(data_off).ok()?;
        let data_end = usize::try_from(data_off.checked_add(data_len)?).ok()?;
        let data = self.image.get(data_start..data_end)?;

        Some(Entry {
            path,
            data,
            mode,
            data_offset: data_off,
        })
    }
}

impl core::fmt::Display for InitfsError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            InitfsError::TooShort => f.write_str("image shorter than the 64-byte header"),
            InitfsError::BadMagic => f.write_str("bad magic (expected RAVENFS1)"),
            InitfsError::BadVersion => f.write_str("unsupported initfs version"),
            InitfsError::BadHeader => f.write_str("inconsistent initfs header or table"),
            InitfsError::BadEntry(index) => write!(f, "malformed initfs entry {index}"),
            InitfsError::BadChecksum(index) => write!(f, "CRC-32 mismatch in initfs entry {index}"),
            InitfsError::Unsorted => f.write_str("initfs entries are not sorted"),
            InitfsError::OutOfBounds(index) => {
                write!(f, "initfs entry {index} data region is out of bounds")
            }
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for InitfsError {}

#[cfg(test)]
mod tests {
    use super::{Entry, Initfs, InitfsError};
    use crate::format::{
        ENTRY_DATA_LEN_OFFSET, ENTRY_DATA_OFF_OFFSET, ENTRY_NAME_OFF_OFFSET, ENTRY_RESERVED_OFFSET,
        HEADER_ENTRY_COUNT_OFFSET, HEADER_ENTRY_SIZE_OFFSET, HEADER_NAMES_LEN_OFFSET,
        HEADER_NAMES_OFFSET, HEADER_RESERVED_OFFSET, HEADER_SIZE, HEADER_SIZE_OFFSET,
        HEADER_TOTAL_SIZE_OFFSET, HEADER_VERSION_OFFSET, PAGE_SIZE,
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
        put_u32(&mut image, HEADER_VERSION_OFFSET, 2);
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
}
