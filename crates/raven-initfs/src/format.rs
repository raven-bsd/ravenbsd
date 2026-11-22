// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! On-disk constants, field offsets and the shared path rules.

/// Archive magic (`"RAVENFS1"`).
pub(crate) const MAGIC: [u8; 8] = *b"RAVENFS1";
/// Format version written into the header.
///
/// Version 2 adds the image epoch and per-entry modification time in the former reserved bytes.
/// The reader still accepts version 1, where both are absent and the documented default epoch is
/// used.
pub(crate) const VERSION: u32 = 2;
/// The previous format version, accepted for reading.
pub(crate) const LEGACY_VERSION: u32 = 1;
/// Fallback modification time when an image records none: 2026-01-01T00:00:00Z.
///
/// Deterministic and host-clock-free. New images record the reproducible build epoch in the
/// header, so this only applies to legacy version 1 images and builders that did not set one.
pub const DEFAULT_EPOCH: u64 = 1_767_225_600;
/// Size of the fixed header in bytes.
pub(crate) const HEADER_SIZE: usize = 64;
/// Size of one fixed entry in bytes.
pub(crate) const ENTRY_SIZE: usize = 48;
/// Alignment of every data region and of the image itself.
pub(crate) const PAGE_SIZE: usize = 4096;
/// Maximum path length in bytes.
pub(crate) const NAME_MAX: usize = 255;

pub(crate) const HEADER_VERSION_OFFSET: usize = 8;
pub(crate) const HEADER_SIZE_OFFSET: usize = 12;
pub(crate) const HEADER_ENTRY_COUNT_OFFSET: usize = 16;
pub(crate) const HEADER_ENTRY_SIZE_OFFSET: usize = 20;
pub(crate) const HEADER_NAMES_OFFSET: usize = 24;
pub(crate) const HEADER_NAMES_LEN_OFFSET: usize = 32;
pub(crate) const HEADER_TOTAL_SIZE_OFFSET: usize = 40;
/// Image build epoch (seconds since the Unix epoch) in version 2.
pub(crate) const HEADER_EPOCH_OFFSET: usize = 48;
pub(crate) const HEADER_RESERVED_OFFSET: usize = 56;
pub(crate) const HEADER_RESERVED_LEN: usize = 8;

pub(crate) const ENTRY_NAME_OFF_OFFSET: usize = 0;
pub(crate) const ENTRY_NAME_LEN_OFFSET: usize = 4;
pub(crate) const ENTRY_DATA_OFF_OFFSET: usize = 8;
pub(crate) const ENTRY_DATA_LEN_OFFSET: usize = 16;
pub(crate) const ENTRY_MODE_OFFSET: usize = 24;
pub(crate) const ENTRY_CRC_OFFSET: usize = 28;
/// Per-entry modification time in version 2; zero inherits [`HEADER_EPOCH_OFFSET`].
pub(crate) const ENTRY_MTIME_OFFSET: usize = 32;
pub(crate) const ENTRY_RESERVED_OFFSET: usize = 40;
pub(crate) const ENTRY_RESERVED_LEN: usize = 8;

const _: () = {
    assert!(HEADER_SIZE == 64);
    assert!(ENTRY_SIZE == 48);
    assert!(PAGE_SIZE == 4096);
    assert!(PAGE_SIZE.is_power_of_two());
    assert!(HEADER_VERSION_OFFSET + 4 == HEADER_SIZE_OFFSET);
    assert!(HEADER_SIZE_OFFSET + 4 == HEADER_ENTRY_COUNT_OFFSET);
    assert!(HEADER_ENTRY_COUNT_OFFSET + 4 == HEADER_ENTRY_SIZE_OFFSET);
    assert!(HEADER_ENTRY_SIZE_OFFSET + 4 == HEADER_NAMES_OFFSET);
    assert!(HEADER_NAMES_OFFSET + 8 == HEADER_NAMES_LEN_OFFSET);
    assert!(HEADER_NAMES_LEN_OFFSET + 8 == HEADER_TOTAL_SIZE_OFFSET);
    assert!(HEADER_TOTAL_SIZE_OFFSET + 8 == HEADER_EPOCH_OFFSET);
    assert!(HEADER_EPOCH_OFFSET + 8 == HEADER_RESERVED_OFFSET);
    assert!(HEADER_RESERVED_OFFSET + HEADER_RESERVED_LEN == HEADER_SIZE);
    assert!(ENTRY_NAME_OFF_OFFSET + 4 == ENTRY_NAME_LEN_OFFSET);
    assert!(ENTRY_NAME_LEN_OFFSET + 4 == ENTRY_DATA_OFF_OFFSET);
    assert!(ENTRY_DATA_OFF_OFFSET + 8 == ENTRY_DATA_LEN_OFFSET);
    assert!(ENTRY_DATA_LEN_OFFSET + 8 == ENTRY_MODE_OFFSET);
    assert!(ENTRY_MODE_OFFSET + 4 == ENTRY_CRC_OFFSET);
    assert!(ENTRY_CRC_OFFSET + 4 == ENTRY_MTIME_OFFSET);
    assert!(ENTRY_MTIME_OFFSET + 8 == ENTRY_RESERVED_OFFSET);
    assert!(ENTRY_RESERVED_OFFSET + ENTRY_RESERVED_LEN == ENTRY_SIZE);
};

/// Why a path is not a valid initfs path.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum PathError {
    /// The path does not start with `/`.
    NotAbsolute,
    /// A component is empty (including `/` itself, `//` and a trailing `/`).
    EmptyComponent,
    /// A component is `.` or `..`.
    InvalidComponent,
    /// The path contains a NUL byte.
    Nul,
    /// The path is longer than [`NAME_MAX`] bytes.
    TooLong,
}

/// Check the path rules shared by the reader and the builder.
pub(crate) fn validate_path(path: &str) -> Result<(), PathError> {
    if !path.starts_with('/') {
        return Err(PathError::NotAbsolute);
    }
    if path.len() > NAME_MAX {
        return Err(PathError::TooLong);
    }
    if path.as_bytes().contains(&0) {
        return Err(PathError::Nul);
    }

    let mut components = path.split('/');
    if components.next() != Some("") {
        return Err(PathError::NotAbsolute);
    }
    let mut saw_component = false;
    for component in components {
        if component.is_empty() {
            return Err(PathError::EmptyComponent);
        }
        if component == "." || component == ".." {
            return Err(PathError::InvalidComponent);
        }
        saw_component = true;
    }
    if !saw_component {
        return Err(PathError::EmptyComponent);
    }
    Ok(())
}

/// Round `value` up to the next multiple of `align`, or `None` on overflow.
pub(crate) fn align_up(value: usize, align: usize) -> Option<usize> {
    value
        .checked_add(align.checked_sub(1)?)
        .map(|rounded| rounded & !(align - 1))
}

/// Read a little-endian `u32` at `offset`, or `None` if it is out of bounds.
pub(crate) fn read_u32(image: &[u8], offset: usize) -> Option<u32> {
    let end = offset.checked_add(4)?;
    let bytes: [u8; 4] = image.get(offset..end)?.try_into().ok()?;
    Some(u32::from_le_bytes(bytes))
}

/// Read a little-endian `u64` at `offset`, or `None` if it is out of bounds.
pub(crate) fn read_u64(image: &[u8], offset: usize) -> Option<u64> {
    let end = offset.checked_add(8)?;
    let bytes: [u8; 8] = image.get(offset..end)?.try_into().ok()?;
    Some(u64::from_le_bytes(bytes))
}

#[cfg(test)]
#[path = "format_ut.rs"]
mod tests;
