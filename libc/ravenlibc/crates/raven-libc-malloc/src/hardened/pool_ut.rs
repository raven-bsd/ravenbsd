// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn pool_fits_a_page() {
    assert!(core::mem::size_of::<Pool>() <= crate::consts::PAGE_SIZE);
}

#[test]
fn chunk_info_fits_its_record() {
    assert!(core::mem::size_of::<ChunkInfo>() <= crate::hardened::consts::CHUNK_INFO_SIZE);
}

#[test]
fn region_and_cache_layouts_are_packed() {
    assert_eq!(core::mem::size_of::<RegionInfo>(), 16);
    assert_eq!(core::mem::size_of::<CacheEntry>(), 16);
}

#[test]
fn field_offsets_are_ordered() {
    assert!(
        core::mem::offset_of!(crate::hardened::pool::Pool, chunk_dir)
            < core::mem::offset_of!(crate::hardened::pool::Pool, delayed)
    );
    assert!(
        core::mem::offset_of!(crate::hardened::pool::Pool, delayed)
            < core::mem::offset_of!(crate::hardened::pool::Pool, cache)
    );
    assert!(
        core::mem::offset_of!(crate::hardened::pool::Pool, cache)
            < core::mem::offset_of!(crate::hardened::pool::Pool, mapped)
    );
}

#[test]
fn bucket_helpers_keep_the_minimum() {
    assert_eq!(crate::hardened::consts::bucket_bytes(0), 16);
    assert_eq!(crate::hardened::consts::bucket_bytes(1), 16);
    assert_eq!(crate::hardened::consts::bucket_bytes(128), 2048);
    assert_eq!(crate::hardened::consts::find_bucket(0), 0);
    assert_eq!(crate::hardened::consts::find_bucket(1), 1);
    assert_eq!(crate::hardened::consts::find_bucket(17), 2);
}

#[test]
fn bucket_count_includes_both_ends() {
    assert_eq!(crate::hardened::consts::BUCKETS, 129);
    assert_eq!(crate::hardened::consts::find_bucket(2048), 128);
}

#[test]
fn cache_entry_is_page_and_length() {
    let entry = CacheEntry {
        page: 0x1000,
        pages: 3,
    };
    assert_eq!(entry.page, 0x1000);
    assert_eq!(entry.pages, 3);
}

#[test]
fn region_info_is_two_words() {
    let region = RegionInfo {
        p: 0x2000,
        size: 0x3000,
    };
    assert_eq!(region.p, 0x2000);
    assert_eq!(region.size, 0x3000);
}

#[test]
fn chunk_info_records_are_smaller_than_their_slot() {
    assert!(core::mem::size_of::<ChunkInfo>() <= crate::hardened::consts::CHUNK_INFO_SIZE);
}

#[test]
fn delayed_slots_are_sixteen() {
    assert_eq!(crate::hardened::consts::DELAYED, 16);
    assert_eq!(crate::hardened::consts::DELAYED_MASK, 15);
}
