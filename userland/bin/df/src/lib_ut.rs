// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for `df`.

use super::{format_df, parse};
use raven_proto_fs::StatFs;

fn sample() -> StatFs {
    StatFs {
        block_size: 4096,
        blocks: 100,
        free_blocks: 50,
        files: 10,
        free_files: 5,
    }
}

#[test]
fn no_operands_is_empty() {
    assert!(parse([]).unwrap().operands.is_empty());
}

#[test]
fn an_option_is_rejected() {
    assert!(parse(["-a"]).is_err());
}

#[test]
fn header_names_the_columns() {
    assert!(
        format_df(&sample())
            .starts_with("Filesystem 1K-blocks Used Available Capacity Mounted on\n")
    );
}

#[test]
fn root_line_has_1k_blocks() {
    let text = format_df(&sample());
    assert!(text.contains("/ 400 200 200 50% /"));
}

#[test]
fn empty_filesystem_has_zero_capacity() {
    let statfs = StatFs {
        block_size: 4096,
        blocks: 0,
        free_blocks: 0,
        files: 0,
        free_files: 0,
    };
    assert!(format_df(&statfs).contains("/ 0 0 0 0% /"));
}

#[test]
fn full_filesystem_has_full_capacity() {
    let statfs = StatFs {
        block_size: 4096,
        blocks: 10,
        free_blocks: 0,
        files: 1,
        free_files: 0,
    };
    assert!(format_df(&statfs).contains("100%"));
}

#[test]
fn free_above_total_does_not_underflow() {
    let statfs = StatFs {
        block_size: 4096,
        blocks: 10,
        free_blocks: 20,
        files: 1,
        free_files: 0,
    };
    assert!(format_df(&statfs).contains("/ 40 0 80 0% /"));
}

#[test]
fn multiple_operands_are_still_parsed() {
    assert_eq!(parse(["/"]).unwrap().operands, vec!["/"]);
}
