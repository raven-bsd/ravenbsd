// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Deterministic builder and epoch tests.

use super::{BuildError, DEFAULT_EPOCH, InitfsBuilder, layout};
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

#[test]
fn the_image_epoch_is_inherited_by_entries() {
    let mut builder = InitfsBuilder::new().with_image_epoch(1_700_000_000);
    builder.add_file("/init", b"x", 0o755).unwrap();
    let image = builder.build();
    let initfs = Initfs::parse(&image).unwrap();
    assert_eq!(initfs.image_epoch(), 1_700_000_000);
    assert_eq!(initfs.get("/init").unwrap().mtime, 1_700_000_000);
}

#[test]
fn an_explicit_entry_mtime_overrides_the_image_epoch() {
    let mut builder = InitfsBuilder::new().with_image_epoch(1_700_000_000);
    builder
        .add_file_with_mtime("/a", b"a", 0o644, 1_600_000_000)
        .unwrap();
    builder.add_file("/b", b"b", 0o644).unwrap();
    let image = builder.build();
    let initfs = Initfs::parse(&image).unwrap();
    assert_eq!(initfs.get("/a").unwrap().mtime, 1_600_000_000);
    assert_eq!(initfs.get("/b").unwrap().mtime, 1_700_000_000);
}

#[test]
fn the_default_epoch_is_nonzero_and_reproducible() {
    let mut builder = InitfsBuilder::new();
    builder.add_file("/a", b"a", 0o644).unwrap();
    let image = builder.build();
    let initfs = Initfs::parse(&image).unwrap();
    assert_eq!(initfs.image_epoch(), DEFAULT_EPOCH);
    assert!(initfs.image_epoch() >= 1_600_000_000);
    assert_eq!(initfs.get("/a").unwrap().mtime, DEFAULT_EPOCH);
}

#[test]
fn the_same_epoch_is_byte_identical_and_another_epoch_changes_only_time() {
    let mut first = InitfsBuilder::new().with_image_epoch(1_700_000_000);
    first.add_file("/a", b"data", 0o644).unwrap();
    let one = first.build();
    let mut second = InitfsBuilder::new().with_image_epoch(1_700_000_000);
    second.add_file("/a", b"data", 0o644).unwrap();
    let again = second.build();
    let mut third = InitfsBuilder::new().with_image_epoch(1_800_000_000);
    third.add_file("/a", b"data", 0o644).unwrap();
    let later = third.build();
    assert_eq!(one, again);
    assert_ne!(one, later);
    let a = Initfs::parse(&one).unwrap().get("/a").unwrap();
    let b = Initfs::parse(&later).unwrap().get("/a").unwrap();
    assert_eq!(a.data, b.data);
    assert_eq!(a.mode, b.mode);
    assert_eq!(a.data_offset, b.data_offset);
    assert_eq!(b.mtime, 1_800_000_000);
}
