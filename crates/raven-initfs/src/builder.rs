// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Deterministic initfs builder (feature `std`).

use std::string::String;
use std::vec::Vec;

use crate::crc::crc32;
use crate::format::{
    ENTRY_CRC_OFFSET, ENTRY_DATA_LEN_OFFSET, ENTRY_DATA_OFF_OFFSET, ENTRY_MODE_OFFSET,
    ENTRY_NAME_LEN_OFFSET, ENTRY_NAME_OFF_OFFSET, ENTRY_SIZE, HEADER_ENTRY_COUNT_OFFSET,
    HEADER_ENTRY_SIZE_OFFSET, HEADER_NAMES_LEN_OFFSET, HEADER_NAMES_OFFSET, HEADER_SIZE,
    HEADER_SIZE_OFFSET, HEADER_TOTAL_SIZE_OFFSET, HEADER_VERSION_OFFSET, MAGIC, PAGE_SIZE,
    PathError, VERSION, align_up, validate_path,
};

/// Why a file cannot be added to an [`InitfsBuilder`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BuildError {
    /// The path does not start with `/`.
    PathNotAbsolute,
    /// The path has an empty component (including `/` itself, `//` and a trailing `/`).
    EmptyPathComponent,
    /// The path has a `.` or `..` component.
    InvalidPathComponent,
    /// The path contains a NUL byte.
    ContainsNul,
    /// The path is longer than 255 bytes.
    PathTooLong,
    /// A file with the same path was already added.
    DuplicatePath,
    /// The resulting image or name table does not fit the format's counters or `usize`.
    TooLarge,
}

impl From<PathError> for BuildError {
    fn from(error: PathError) -> Self {
        match error {
            PathError::NotAbsolute => BuildError::PathNotAbsolute,
            PathError::EmptyComponent => BuildError::EmptyPathComponent,
            PathError::InvalidComponent => BuildError::InvalidPathComponent,
            PathError::Nul => BuildError::ContainsNul,
            PathError::TooLong => BuildError::PathTooLong,
        }
    }
}

impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuildError::PathNotAbsolute => f.write_str("path is not absolute"),
            BuildError::EmptyPathComponent => f.write_str("path has an empty component"),
            BuildError::InvalidPathComponent => f.write_str("path has a '.' or '..' component"),
            BuildError::ContainsNul => f.write_str("path contains a NUL byte"),
            BuildError::PathTooLong => f.write_str("path is longer than 255 bytes"),
            BuildError::DuplicatePath => f.write_str("duplicate path"),
            BuildError::TooLarge => f.write_str("image or name table is too large"),
        }
    }
}

impl std::error::Error for BuildError {}

#[derive(Clone, Copy, Debug)]
struct Layout {
    names_offset: usize,
    names_len: usize,
    data_start: usize,
    total: usize,
}

fn layout(entry_count: usize, names_len: usize, data_bytes: usize) -> Result<Layout, BuildError> {
    u32::try_from(entry_count).map_err(|_| BuildError::TooLarge)?;
    u32::try_from(names_len).map_err(|_| BuildError::TooLarge)?;

    let table_len = entry_count
        .checked_mul(ENTRY_SIZE)
        .ok_or(BuildError::TooLarge)?;
    let names_offset = HEADER_SIZE
        .checked_add(table_len)
        .ok_or(BuildError::TooLarge)?;
    let names_end = names_offset
        .checked_add(names_len)
        .ok_or(BuildError::TooLarge)?;
    let data_start = align_up(names_end, PAGE_SIZE).ok_or(BuildError::TooLarge)?;
    let total = data_start
        .checked_add(data_bytes)
        .ok_or(BuildError::TooLarge)?;
    if total > isize::MAX as usize || u64::try_from(total).is_err() {
        return Err(BuildError::TooLarge);
    }
    Ok(Layout {
        names_offset,
        names_len,
        data_start,
        total,
    })
}

#[derive(Clone, Debug)]
struct BuildFile {
    path: String,
    data: Vec<u8>,
    mode: u32,
}

/// Deterministic builder for initfs images.
///
/// Files are stored in insertion order and sorted by path when [`InitfsBuilder::build`] runs, so
/// the output depends only on the set of `(path, data, mode)` tuples.
#[derive(Clone, Debug)]
pub struct InitfsBuilder {
    files: Vec<BuildFile>,
    layout: Layout,
}

impl InitfsBuilder {
    /// Create an empty builder.
    pub fn new() -> Self {
        Self {
            files: Vec::new(),
            layout: Layout {
                names_offset: HEADER_SIZE,
                names_len: 0,
                data_start: PAGE_SIZE,
                total: PAGE_SIZE,
            },
        }
    }

    /// Add a file. Duplicate paths and invalid paths are rejected.
    pub fn add_file(
        &mut self,
        path: &str,
        data: &[u8],
        mode: u32,
    ) -> Result<&mut Self, BuildError> {
        validate_path(path)?;
        if self.files.iter().any(|file| file.path == path) {
            return Err(BuildError::DuplicatePath);
        }

        let entry_count = self
            .files
            .len()
            .checked_add(1)
            .ok_or(BuildError::TooLarge)?;
        let names_len = self
            .layout
            .names_len
            .checked_add(path.len())
            .ok_or(BuildError::TooLarge)?;
        let aligned_len = align_up(data.len(), PAGE_SIZE).ok_or(BuildError::TooLarge)?;
        let data_bytes = self
            .layout
            .total
            .checked_sub(self.layout.data_start)
            .and_then(|bytes| bytes.checked_add(aligned_len))
            .ok_or(BuildError::TooLarge)?;
        let layout = layout(entry_count, names_len, data_bytes)?;

        self.files.push(BuildFile {
            path: path.to_owned(),
            data: data.to_vec(),
            mode,
        });
        self.layout = layout;
        Ok(self)
    }

    /// Build the image. The output is deterministic for a given set of files.
    pub fn build(mut self) -> Vec<u8> {
        self.files
            .sort_by(|left, right| left.path.as_bytes().cmp(right.path.as_bytes()));

        let layout = self.layout;
        let mut image = vec![0u8; layout.total];
        // `add_file` validated this layout, so every counter and offset below fits the field it is
        // written to; the fallbacks are unreachable.
        put_bytes(&mut image, 0, &MAGIC);
        put_u32(&mut image, HEADER_VERSION_OFFSET, VERSION);
        put_u32(&mut image, HEADER_SIZE_OFFSET, HEADER_SIZE as u32);
        put_u32(
            &mut image,
            HEADER_ENTRY_COUNT_OFFSET,
            u32::try_from(self.files.len()).unwrap_or(0),
        );
        put_u32(&mut image, HEADER_ENTRY_SIZE_OFFSET, ENTRY_SIZE as u32);
        put_u64(
            &mut image,
            HEADER_NAMES_OFFSET,
            u64::try_from(layout.names_offset).unwrap_or(u64::MAX),
        );
        put_u64(
            &mut image,
            HEADER_NAMES_LEN_OFFSET,
            u64::try_from(layout.names_len).unwrap_or(u64::MAX),
        );
        put_u64(
            &mut image,
            HEADER_TOTAL_SIZE_OFFSET,
            u64::try_from(layout.total).unwrap_or(u64::MAX),
        );

        let mut name_cursor = layout.names_offset;
        let mut data_cursor = layout.data_start;

        for (index, file) in self.files.iter().enumerate() {
            let Some(base) = index
                .checked_mul(ENTRY_SIZE)
                .and_then(|offset| HEADER_SIZE.checked_add(offset))
            else {
                continue;
            };

            let name_off = name_cursor.saturating_sub(layout.names_offset);
            put_u32(
                &mut image,
                base + ENTRY_NAME_OFF_OFFSET,
                u32::try_from(name_off).unwrap_or(0),
            );
            put_u32(
                &mut image,
                base + ENTRY_NAME_LEN_OFFSET,
                u32::try_from(file.path.len()).unwrap_or(0),
            );
            put_u64(
                &mut image,
                base + ENTRY_DATA_OFF_OFFSET,
                u64::try_from(data_cursor).unwrap_or(u64::MAX),
            );
            put_u64(
                &mut image,
                base + ENTRY_DATA_LEN_OFFSET,
                u64::try_from(file.data.len()).unwrap_or(u64::MAX),
            );
            put_u32(&mut image, base + ENTRY_MODE_OFFSET, file.mode);
            put_u32(&mut image, base + ENTRY_CRC_OFFSET, crc32(&file.data));

            put_bytes(&mut image, name_cursor, file.path.as_bytes());
            put_bytes(&mut image, data_cursor, &file.data);

            name_cursor = name_cursor.saturating_add(file.path.len());
            data_cursor = data_cursor
                .saturating_add(align_up(file.data.len(), PAGE_SIZE).unwrap_or(PAGE_SIZE));
        }

        image
    }
}

impl Default for InitfsBuilder {
    fn default() -> Self {
        Self::new()
    }
}

fn put_bytes(image: &mut [u8], offset: usize, bytes: &[u8]) {
    let Some(end) = offset.checked_add(bytes.len()) else {
        return;
    };
    if let Some(slot) = image.get_mut(offset..end) {
        slot.copy_from_slice(bytes);
    }
}

fn put_u32(image: &mut [u8], offset: usize, value: u32) {
    put_bytes(image, offset, &value.to_le_bytes());
}

fn put_u64(image: &mut [u8], offset: usize, value: u64) {
    put_bytes(image, offset, &value.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::{BuildError, InitfsBuilder, layout};
    use crate::{Initfs, crc32};

    fn build(files: &[(&str, &[u8], u32)]) -> Vec<u8> {
        let mut builder = InitfsBuilder::new();
        for (path, data, mode) in files {
            builder.add_file(path, data, *mode).unwrap();
        }
        builder.build()
    }

    #[test]
    fn zero_files_roundtrip() {
        let image = InitfsBuilder::new().build();
        assert_eq!(image.len(), 4096);
        let initfs = Initfs::parse(&image).unwrap();
        assert_eq!(initfs.len(), 0);
        assert!(initfs.is_empty());
        assert_eq!(initfs.entries().count(), 0);
    }

    #[test]
    fn one_file_roundtrip() {
        let image = build(&[("/init", b"hello", 0o755)]);
        let initfs = Initfs::parse(&image).unwrap();
        assert_eq!(initfs.len(), 1);
        let entry = initfs.get("/init").unwrap();
        assert_eq!(entry.path, "/init");
        assert_eq!(entry.data, b"hello");
        assert_eq!(entry.mode, 0o755);
        assert_eq!(entry.data_offset, 4096);
    }

    #[test]
    fn three_files_roundtrip_with_edge_sizes() {
        let big = vec![0xA5u8; 10 * 1024];
        let image = build(&[
            ("/empty", b"", 0o644),
            ("/one", b"x", 0o600),
            ("/big", big.as_slice(), 0o755),
        ]);
        let initfs = Initfs::parse(&image).unwrap();
        assert_eq!(initfs.len(), 3);
        assert_eq!(initfs.get("/empty").unwrap().data, b"");
        assert_eq!(initfs.get("/one").unwrap().data, b"x");
        assert_eq!(initfs.get("/big").unwrap().data, &big[..]);
    }

    #[test]
    fn hundred_files_roundtrip() {
        let data: Vec<Vec<u8>> = (0..100).map(|index| vec![index as u8; index * 7]).collect();
        let paths: Vec<String> = (0..100).map(|index| format!("/f{index:03}")).collect();
        let mut builder = InitfsBuilder::new();
        for (path, bytes) in paths.iter().zip(data.iter()) {
            builder.add_file(path, bytes, 0o644).unwrap();
        }
        let image = builder.build();
        let initfs = Initfs::parse(&image).unwrap();
        assert_eq!(initfs.len(), 100);
        for (path, bytes) in paths.iter().zip(data.iter()) {
            assert_eq!(initfs.get(path).unwrap().data, &bytes[..]);
        }
    }

    #[test]
    fn insertion_order_does_not_change_bytes() {
        let files = [
            ("/bin/b", b"bbb".as_slice(), 0o755),
            ("/init", b"init".as_slice(), 0o755),
            ("/etc/a", b"aaa".as_slice(), 0o644),
        ];
        let forward = build(&files);
        let mut reversed = files;
        reversed.reverse();
        let backward = build(&reversed);
        assert_eq!(forward, backward);
    }

    #[test]
    fn repeated_builds_are_identical() {
        let files = [
            ("/a", b"1".as_slice(), 0o644),
            ("/b", b"2".as_slice(), 0o600),
        ];
        assert_eq!(build(&files), build(&files));
    }

    #[test]
    fn duplicate_path_is_rejected() {
        let mut builder = InitfsBuilder::new();
        builder.add_file("/init", b"a", 0o755).unwrap();
        assert_eq!(
            builder.add_file("/init", b"b", 0o755).unwrap_err(),
            BuildError::DuplicatePath
        );
    }

    #[test]
    fn path_not_absolute_is_rejected() {
        let mut builder = InitfsBuilder::new();
        assert_eq!(
            builder.add_file("init", b"", 0o755).unwrap_err(),
            BuildError::PathNotAbsolute
        );
    }

    #[test]
    fn empty_path_component_is_rejected() {
        let mut builder = InitfsBuilder::new();
        assert_eq!(
            builder.add_file("/", b"", 0o755).unwrap_err(),
            BuildError::EmptyPathComponent
        );
        assert_eq!(
            builder.add_file("/bin/", b"", 0o755).unwrap_err(),
            BuildError::EmptyPathComponent
        );
        assert_eq!(
            builder.add_file("/bin//sh", b"", 0o755).unwrap_err(),
            BuildError::EmptyPathComponent
        );
    }

    #[test]
    fn dot_component_is_rejected() {
        let mut builder = InitfsBuilder::new();
        assert_eq!(
            builder.add_file("/a/../b", b"", 0o644).unwrap_err(),
            BuildError::InvalidPathComponent
        );
        assert_eq!(
            builder.add_file("/a/./b", b"", 0o644).unwrap_err(),
            BuildError::InvalidPathComponent
        );
    }

    #[test]
    fn nul_in_path_is_rejected() {
        let mut builder = InitfsBuilder::new();
        assert_eq!(
            builder.add_file("/a\0b", b"", 0o644).unwrap_err(),
            BuildError::ContainsNul
        );
    }

    #[test]
    fn path_length_limit_is_enforced() {
        let mut builder = InitfsBuilder::new();
        let too_long = format!("/{}", "a".repeat(255));
        assert_eq!(
            builder.add_file(&too_long, b"", 0o644).unwrap_err(),
            BuildError::PathTooLong
        );

        let max = format!("/{}", "a".repeat(254));
        assert_eq!(max.len(), 255);
        builder.add_file(&max, b"ok", 0o644).unwrap();
        let image = builder.build();
        let initfs = Initfs::parse(&image).unwrap();
        assert_eq!(initfs.get(&max).unwrap().data, b"ok");
    }

    #[test]
    fn layout_overflow_is_rejected() {
        assert_eq!(layout(usize::MAX, 0, 0).unwrap_err(), BuildError::TooLarge);
    }

    #[test]
    fn data_offsets_are_page_aligned_and_ordered() {
        let big = vec![7u8; 4097];
        let image = build(&[
            ("/c", big.as_slice(), 0o644),
            ("/a", b"", 0o644),
            ("/b", b"x", 0o644),
        ]);
        let initfs = Initfs::parse(&image).unwrap();
        let mut previous = 0u64;
        for entry in initfs.entries() {
            assert_eq!(entry.data_offset % 4096, 0, "{}", entry.path);
            assert!(entry.data_offset >= previous, "{}", entry.path);
            previous = entry.data_offset;
        }
    }

    #[test]
    fn unicode_paths_roundtrip() {
        let image = build(&[("/bin/日本語", b"unicode", 0o755)]);
        let initfs = Initfs::parse(&image).unwrap();
        assert_eq!(initfs.get("/bin/日本語").unwrap().data, b"unicode");
    }

    #[test]
    fn modes_and_crcs_are_preserved() {
        let data = b"payload";
        let image = build(&[("/init", data, 0o751)]);
        let initfs = Initfs::parse(&image).unwrap();
        let entry = initfs.get("/init").unwrap();
        assert_eq!(entry.mode, 0o751);
        assert_eq!(crc32(entry.data), crc32(data));
    }

    #[test]
    fn entries_are_sorted_by_name() {
        let image = build(&[
            ("/z", b"z", 0o644),
            ("/a", b"a", 0o644),
            ("/m", b"m", 0o644),
        ]);
        let initfs = Initfs::parse(&image).unwrap();
        let paths: Vec<&str> = initfs.entries().map(|entry| entry.path).collect();
        assert_eq!(paths, vec!["/a", "/m", "/z"]);
    }
}
