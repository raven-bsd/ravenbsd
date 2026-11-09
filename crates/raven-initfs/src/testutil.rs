// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Independent image writer used only by the reader tests.
//!
//! It deliberately does not share the builder implementation: the reader must be tested against
//! bytes laid out from the contract, not only against the crate's own builder.

use crate::crc::crc32;
use crate::format::{
    ENTRY_CRC_OFFSET, ENTRY_DATA_LEN_OFFSET, ENTRY_DATA_OFF_OFFSET, ENTRY_MODE_OFFSET,
    ENTRY_NAME_LEN_OFFSET, ENTRY_NAME_OFF_OFFSET, ENTRY_SIZE, HEADER_ENTRY_COUNT_OFFSET,
    HEADER_ENTRY_SIZE_OFFSET, HEADER_NAMES_LEN_OFFSET, HEADER_NAMES_OFFSET, HEADER_SIZE,
    HEADER_SIZE_OFFSET, HEADER_TOTAL_SIZE_OFFSET, HEADER_VERSION_OFFSET, MAGIC, PAGE_SIZE, VERSION,
    align_up,
};

/// One file for [`image`]. Paths are raw bytes so tests can also build invalid UTF-8 names.
pub(crate) struct File<'a> {
    pub path: &'a [u8],
    pub data: &'a [u8],
    pub mode: u32,
}

/// Build a canonical image in the given entry order without validating it.
pub(crate) fn image(files: &[File<'_>]) -> Vec<u8> {
    let names_len: usize = files.iter().map(|file| file.path.len()).sum();
    let names_offset = HEADER_SIZE + files.len() * ENTRY_SIZE;
    let data_start = align_up(names_offset + names_len, PAGE_SIZE).expect("test image fits");
    let mut total = data_start;
    for file in files {
        total += align_up(file.data.len(), PAGE_SIZE).expect("test image fits");
    }

    let mut image = vec![0u8; total];
    image[..MAGIC.len()].copy_from_slice(&MAGIC);
    put_u32(&mut image, HEADER_VERSION_OFFSET, VERSION);
    put_u32(&mut image, HEADER_SIZE_OFFSET, HEADER_SIZE as u32);
    put_u32(&mut image, HEADER_ENTRY_COUNT_OFFSET, files.len() as u32);
    put_u32(&mut image, HEADER_ENTRY_SIZE_OFFSET, ENTRY_SIZE as u32);
    put_u64(&mut image, HEADER_NAMES_OFFSET, names_offset as u64);
    put_u64(&mut image, HEADER_NAMES_LEN_OFFSET, names_len as u64);
    put_u64(&mut image, HEADER_TOTAL_SIZE_OFFSET, total as u64);

    let mut name_cursor = names_offset;
    let mut data_cursor = data_start;
    for (index, file) in files.iter().enumerate() {
        let base = HEADER_SIZE + index * ENTRY_SIZE;
        put_u32(
            &mut image,
            base + ENTRY_NAME_OFF_OFFSET,
            (name_cursor - names_offset) as u32,
        );
        put_u32(
            &mut image,
            base + ENTRY_NAME_LEN_OFFSET,
            file.path.len() as u32,
        );
        put_u64(&mut image, base + ENTRY_DATA_OFF_OFFSET, data_cursor as u64);
        put_u64(
            &mut image,
            base + ENTRY_DATA_LEN_OFFSET,
            file.data.len() as u64,
        );
        put_u32(&mut image, base + ENTRY_MODE_OFFSET, file.mode);
        put_u32(&mut image, base + ENTRY_CRC_OFFSET, crc32(file.data));

        image[name_cursor..name_cursor + file.path.len()].copy_from_slice(file.path);
        image[data_cursor..data_cursor + file.data.len()].copy_from_slice(file.data);

        name_cursor += file.path.len();
        data_cursor += align_up(file.data.len(), PAGE_SIZE).expect("test image fits");
    }
    image
}

fn put_u32(image: &mut [u8], offset: usize, value: u32) {
    image[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn put_u64(image: &mut [u8], offset: usize, value: u64) {
    image[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}
