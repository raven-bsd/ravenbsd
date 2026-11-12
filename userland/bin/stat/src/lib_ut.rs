// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for `stat`.

use super::{format_stat, parse};
use raven_proto_fs::Stat;

fn sample() -> Stat {
    Stat {
        ino: 42,
        size: 7,
        blocks: 8,
        mtime: 10,
        ctime: 11,
        atime: 12,
        mode: 0o100644,
        nlink: 1,
        uid: 1000,
        gid: 1000,
    }
}

#[test]
fn no_operands_is_empty() {
    assert!(parse([]).unwrap().operands.is_empty());
}

#[test]
fn operands_are_kept() {
    assert_eq!(parse(["a", "b"]).unwrap().operands, vec!["a", "b"]);
}

#[test]
fn an_option_is_rejected() {
    assert!(parse(["-l"]).is_err());
}

#[test]
fn file_name_is_printed() {
    assert!(format_stat("/tmp/a", &sample()).starts_with("File: /tmp/a\n"));
}

#[test]
fn size_and_blocks_are_printed() {
    let text = format_stat("/tmp/a", &sample());
    assert!(text.contains("Size: 7\tBlocks: 8"));
}

#[test]
fn inode_and_links_are_printed() {
    let text = format_stat("/tmp/a", &sample());
    assert!(text.contains("Inode: 42\tLinks: 1"));
}

#[test]
fn mode_is_rendered() {
    let text = format_stat("/tmp/a", &sample());
    assert!(text.contains("Mode: -rw-r--r-- (100644)"));
}

#[test]
fn uid_and_gid_are_printed() {
    let text = format_stat("/tmp/a", &sample());
    assert!(text.contains("Uid: 1000\tGid: 1000"));
}

#[test]
fn all_three_times_are_printed() {
    let text = format_stat("/tmp/a", &sample());
    assert!(text.contains("Access: 12\nModify: 10\nChange: 11"));
}

#[test]
fn directory_mode_is_rendered() {
    let mut stat = sample();
    stat.mode = 0o040755;
    assert!(format_stat("/tmp", &stat).contains("Mode: drwxr-xr-x (40755)"));
}
