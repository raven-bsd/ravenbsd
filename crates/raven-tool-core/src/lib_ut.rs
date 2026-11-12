// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the pure helper crate.

use super::error::{EXIT_ERROR, EXIT_OK, EXIT_USAGE, line, usage};
use super::format::{blocks_string, capacity, mode_string, size_string};
use super::opts::{UsageError, parse};
use super::path::{
    basename, join, move_into_directory, sort_names, visible_names, with_file_scheme,
};
use alloc::string::ToString;
use alloc::vec;
use raven_abi::errno::Errno;

#[test]
fn combined_flags_are_parsed() {
    let parsed = parse(["-lh"], "alh").unwrap();
    assert!(parsed.flags.l);
    assert!(parsed.flags.h);
    assert!(parsed.operands.is_empty());
    assert!(!parsed.flags.a);
}

#[test]
fn every_flag_is_recognized() {
    let parsed = parse(["-a", "-l", "-p", "-r", "-f", "-h", "-i"], "alprfhi").unwrap();
    assert!(parsed.flags.a && parsed.flags.l && parsed.flags.p);
    assert!(parsed.flags.r && parsed.flags.f && parsed.flags.h && parsed.flags.i);
}

#[test]
fn unknown_flag_is_rejected() {
    assert_eq!(parse(["-z"], "al"), Err(UsageError::UnknownOption('z')));
}

#[test]
fn double_dash_ends_options() {
    let parsed = parse(["a", "--", "-l", "b"], "al").unwrap();
    assert_eq!(parsed.operands, vec!["a", "-l", "b"]);
    assert!(!parsed.flags.l);
}

#[test]
fn a_lone_dash_is_an_operand() {
    let parsed = parse(["-"], "al").unwrap();
    assert_eq!(parsed.operands, vec!["-"]);
}

#[test]
fn operands_keep_their_order() {
    let parsed = parse(["one", "two", "three"], "al").unwrap();
    assert_eq!(parsed.operands, vec!["one", "two", "three"]);
}

#[test]
fn defaults_are_all_false() {
    let parsed = parse([], "al").unwrap();
    assert_eq!(parsed.flags, super::opts::Flags::default());
    assert!(parsed.operands.is_empty());
}

#[test]
fn repeated_flags_stay_set() {
    let parsed = parse(["-l", "-l"], "l").unwrap();
    assert!(parsed.flags.l);
}

#[test]
fn options_after_operands_are_accepted() {
    let parsed = parse(["file", "-l"], "l").unwrap();
    assert!(parsed.flags.l);
    assert_eq!(parsed.operands, vec!["file"]);
}

#[test]
fn directory_mode_renders() {
    assert_eq!(mode_string(0o040755), "drwxr-xr-x");
}

#[test]
fn regular_mode_renders() {
    assert_eq!(mode_string(0o100644), "-rw-r--r--");
}

#[test]
fn empty_mode_renders() {
    assert_eq!(mode_string(0o100000), "----------");
}

#[test]
fn symlink_mode_renders() {
    assert_eq!(mode_string(0o120777), "lrwxrwxrwx");
}

#[test]
fn char_and_block_modes_render() {
    assert_eq!(mode_string(0o020600), "crw-------");
    assert_eq!(mode_string(0o060600), "brw-------");
}

#[test]
fn setuid_mode_renders() {
    assert_eq!(mode_string(0o104755), "-rwsr-xr-x");
}

#[test]
fn setgid_mode_renders() {
    assert_eq!(mode_string(0o102755), "-rwxr-sr-x");
}

#[test]
fn sticky_mode_renders() {
    assert_eq!(mode_string(0o041777), "drwxrwxrwt");
}

#[test]
fn size_is_decimal() {
    assert_eq!(size_string(0), "0");
    assert_eq!(size_string(1), "1");
    assert_eq!(size_string(1_234_567), "1234567");
}

#[test]
fn blocks_are_one_kib_units() {
    assert_eq!(blocks_string(4, 4096), "16");
    assert_eq!(blocks_string(0, 4096), "0");
}

#[test]
fn zero_block_size_is_zero() {
    assert_eq!(blocks_string(99, 0), "0");
}

#[test]
fn capacity_is_a_percentage() {
    assert_eq!(capacity(50, 100), "50%");
    assert_eq!(capacity(100, 100), "100%");
    assert_eq!(capacity(0, 100), "0%");
}

#[test]
fn capacity_of_empty_filesystem_is_zero() {
    assert_eq!(capacity(1, 0), "0%");
}

#[test]
fn capacity_does_not_overflow_large_values() {
    assert_eq!(capacity(u64::MAX, u64::MAX), "100%");
}

#[test]
fn error_line_has_tool_path_and_text() {
    assert_eq!(
        line("cat", "/nonexistent", Errno::ENOENT),
        "cat: /nonexistent: no such file or directory"
    );
}

#[test]
fn error_line_has_permission_text() {
    assert_eq!(
        line("rm", "/blocked", Errno::EACCES),
        "rm: /blocked: permission denied"
    );
}

#[test]
fn usage_line_has_syntax() {
    assert_eq!(usage("cat", "[file ...]"), "usage: cat [file ...]");
}

#[test]
fn exit_codes_are_the_documented_ones() {
    assert_eq!(EXIT_OK, 0);
    assert_eq!(EXIT_ERROR, 1);
    assert_eq!(EXIT_USAGE, 2);
}

#[test]
fn basename_returns_the_final_component() {
    assert_eq!(basename("/tmp/a"), "a");
    assert_eq!(basename("a"), "a");
}

#[test]
fn basename_of_root_is_empty() {
    assert_eq!(basename("/"), "");
}

#[test]
fn join_below_root() {
    assert_eq!(join("/", "a").unwrap(), "/a");
}

#[test]
fn join_below_directory() {
    assert_eq!(join("/tmp", "a").unwrap(), "/tmp/a");
}

#[test]
fn join_below_trailing_slash() {
    assert_eq!(join("/tmp/", "a").unwrap(), "/tmp/a");
}

#[test]
fn join_rejects_dot_components() {
    assert_eq!(join("/tmp", ".."), Err(Errno::EINVAL));
    assert_eq!(join("/tmp", "."), Err(Errno::EINVAL));
}

#[test]
fn join_rejects_separators_and_empty_names() {
    assert_eq!(join("/tmp", "a/b"), Err(Errno::EINVAL));
    assert_eq!(join("/tmp", ""), Err(Errno::EINVAL));
}

#[test]
fn names_sort_in_byte_order() {
    let mut names = vec!["b".to_string(), "a".to_string(), "C".to_string()];
    sort_names(&mut names);
    assert_eq!(names, vec!["C", "a", "b"]);
}

#[test]
fn dot_names_hide_by_default() {
    let names = vec![".x".to_string(), "y".to_string()];
    assert_eq!(visible_names(names.clone(), false), vec!["y"]);
    assert_eq!(visible_names(names, true), vec![".x", "y"]);
}

#[test]
fn move_into_directory_appends_basename() {
    assert_eq!(move_into_directory("/tmp/a", "/dest"), "/dest/a");
    assert_eq!(move_into_directory("/tmp/a", "/dest/"), "/dest/a");
    assert_eq!(move_into_directory("/tmp/a", "/"), "/a");
}

#[test]
fn file_scheme_is_added_to_bare_paths() {
    assert_eq!(with_file_scheme("/etc/mounts"), "file:/etc/mounts");
}

#[test]
fn existing_scheme_is_kept() {
    assert_eq!(with_file_scheme("file:/etc/mounts"), "file:/etc/mounts");
    assert_eq!(with_file_scheme("tty:console"), "tty:console");
}

#[test]
fn a_colon_after_a_slash_is_part_of_the_path() {
    assert_eq!(with_file_scheme("/tmp/a:b"), "file:/tmp/a:b");
}
