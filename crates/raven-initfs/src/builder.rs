// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Deterministic initfs builder (feature `std`).

use std::string::String;
use std::vec::Vec;

use crate::crc::crc32;
use crate::format::{
    DEFAULT_EPOCH, ENTRY_CRC_OFFSET, ENTRY_DATA_LEN_OFFSET, ENTRY_DATA_OFF_OFFSET,
    ENTRY_MODE_OFFSET, ENTRY_MTIME_OFFSET, ENTRY_NAME_LEN_OFFSET, ENTRY_NAME_OFF_OFFSET,
    ENTRY_SIZE, HEADER_ENTRY_COUNT_OFFSET, HEADER_ENTRY_SIZE_OFFSET, HEADER_EPOCH_OFFSET,
    HEADER_NAMES_LEN_OFFSET, HEADER_NAMES_OFFSET, HEADER_SIZE, HEADER_SIZE_OFFSET,
    HEADER_TOTAL_SIZE_OFFSET, HEADER_VERSION_OFFSET, MAGIC, PAGE_SIZE, PathError, VERSION,
    align_up, validate_path,
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
    mtime: u64,
}

/// Deterministic builder for initfs images.
///
/// Files are stored in insertion order and sorted by path when [`InitfsBuilder::build`] runs, so
/// the output depends only on the set of `(path, data, mode, mtime)` tuples and the image epoch.
#[derive(Clone, Debug)]
pub struct InitfsBuilder {
    files: Vec<BuildFile>,
    layout: Layout,
    image_epoch: u64,
}

impl InitfsBuilder {
    /// Create an empty builder with the documented default epoch.
    pub fn new() -> Self {
        Self {
            files: Vec::new(),
            layout: Layout {
                names_offset: HEADER_SIZE,
                names_len: 0,
                data_start: PAGE_SIZE,
                total: PAGE_SIZE,
            },
            image_epoch: DEFAULT_EPOCH,
        }
    }

    /// Record the image build epoch (seconds since the Unix epoch).
    ///
    /// The value is written into the header and inherited by every entry whose own mtime is zero,
    /// so no file falls back to 1970. Callers use the reproducible build time, never the host
    /// clock.
    #[must_use]
    pub fn with_image_epoch(mut self, epoch: u64) -> Self {
        self.image_epoch = epoch;
        self
    }

    /// Add a file. Duplicate paths and invalid paths are rejected.
    pub fn add_file(
        &mut self,
        path: &str,
        data: &[u8],
        mode: u32,
    ) -> Result<&mut Self, BuildError> {
        self.add_file_with_mtime(path, data, mode, 0)
    }

    /// Add a file with an explicit modification time; zero inherits the image epoch.
    pub fn add_file_with_mtime(
        &mut self,
        path: &str,
        data: &[u8],
        mode: u32,
        mtime: u64,
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
            mtime,
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
        put_u64(&mut image, HEADER_EPOCH_OFFSET, self.image_epoch);

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
            put_u64(&mut image, base + ENTRY_MTIME_OFFSET, file.mtime);

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
#[path = "builder_ut.rs"]
mod tests;
