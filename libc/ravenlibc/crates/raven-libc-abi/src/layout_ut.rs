// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the runtime layout table (sibling of `layout.rs`).

#![allow(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic
)]

use super::*;

#[test]
fn table_has_every_record() {
    assert_eq!(STRUCTS.len(), 26);
}

#[test]
fn find_returns_none_for_unknown() {
    assert!(find("not-a-record").is_none());
}

#[test]
fn stat_has_the_freebsd_shape() {
    let stat = find("stat").expect("stat");
    assert_eq!(stat.c_name, "struct stat");
    assert_eq!(stat.size, 224);
    assert_eq!(stat.align, 8);
    assert_eq!(stat.fields.len(), 20);
}

#[test]
fn stat_field_offsets() {
    let stat = find("stat").expect("stat");
    let offset = |name: &str| {
        stat.fields
            .iter()
            .find(|field| field.name == name)
            .map(|field| field.offset)
    };
    assert_eq!(offset("st_dev"), Some(0));
    assert_eq!(offset("st_ino"), Some(8));
    assert_eq!(offset("st_nlink"), Some(16));
    assert_eq!(offset("st_mode"), Some(24));
    assert_eq!(offset("st_bsdflags"), Some(26));
    assert_eq!(offset("st_uid"), Some(28));
    assert_eq!(offset("st_gid"), Some(32));
    assert_eq!(offset("st_padding1"), Some(36));
    assert_eq!(offset("st_rdev"), Some(40));
    assert_eq!(offset("st_atim"), Some(48));
    assert_eq!(offset("st_mtim"), Some(64));
    assert_eq!(offset("st_ctim"), Some(80));
    assert_eq!(offset("st_birthtim"), Some(96));
    assert_eq!(offset("st_size"), Some(112));
    assert_eq!(offset("st_blocks"), Some(120));
    assert_eq!(offset("st_blksize"), Some(128));
    assert_eq!(offset("st_flags"), Some(132));
    assert_eq!(offset("st_gen"), Some(136));
    assert_eq!(offset("st_filerev"), Some(144));
    assert_eq!(offset("st_spare"), Some(152));
}

#[test]
fn dirent_is_280_bytes() {
    let dirent = find("dirent").expect("dirent");
    assert_eq!(dirent.size, 280);
    assert_eq!(dirent.align, 8);
    assert_eq!(dirent.fields.len(), 8);
    assert_eq!(dirent.fields[0].name, "d_fileno");
    assert_eq!(dirent.fields[5].name, "d_namlen");
    assert_eq!(dirent.fields[7].offset, 24);
    assert_eq!(dirent.fields[7].size, 256);
}

#[test]
fn every_record_is_well_formed() {
    for layout in STRUCTS {
        assert!(layout.size > 0, "{}", layout.name);
        assert!(layout.align > 0, "{}", layout.name);
        assert_eq!(layout.size % layout.align, 0, "{}", layout.name);
        assert!(!layout.fields.is_empty(), "{}", layout.name);
        for pair in layout.fields.windows(2) {
            assert!(
                pair[0].offset + pair[0].size <= pair[1].offset,
                "{} fields overlap",
                layout.name
            );
        }
        let last = layout.fields.last().expect("fields");
        assert!(last.offset + last.size <= layout.size, "{}", layout.name);
    }
}

#[test]
fn record_names_are_unique() {
    let mut names: Vec<&str> = STRUCTS.iter().map(|layout| layout.name).collect();
    names.sort_unstable();
    let before = names.len();
    names.dedup();
    assert_eq!(before, names.len());
}

#[test]
fn sockaddr_has_no_sa_len() {
    let sockaddr = find("sockaddr").expect("sockaddr");
    assert!(
        sockaddr
            .fields
            .iter()
            .any(|field| field.name == "sa_family")
    );
    assert!(!sockaddr.fields.iter().any(|field| field.name == "sa_len"));
    assert_eq!(sockaddr.size, 16);
    assert_eq!(sockaddr.align, 2);
}

#[test]
fn rusage_reserves_128_bytes() {
    let rusage = find("rusage").expect("rusage");
    assert_eq!(rusage.size, 272);
    let reserved = rusage
        .fields
        .iter()
        .find(|field| field.name == "__reserved")
        .expect("__reserved");
    assert_eq!(reserved.offset, 144);
    assert_eq!(reserved.size, 128);
}

#[test]
fn sigaction_embeds_a_128_byte_mask() {
    let sigaction = find("sigaction").expect("sigaction");
    let mask = sigaction
        .fields
        .iter()
        .find(|field| field.name == "sa_mask")
        .expect("sa_mask");
    assert_eq!(mask.offset, 8);
    assert_eq!(mask.size, 128);
    assert_eq!(sigaction.fields[2].offset, 136);
    assert_eq!(sigaction.fields[3].offset, 144);
}
