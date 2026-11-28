// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use std::path::Path;

use super::*;

fn head(short: &str, dirty: bool) -> HeadStamp {
    HeadStamp {
        short: short.to_string(),
        dirty,
    }
}

#[test]
fn a_clean_matching_release_kernel_passes() {
    let kernel = Path::new("/build/x86_64/kernel.elf");
    let data = b"RavenBSD 0.1 (release) #1: abc1234 2026-11-27\0";
    assert!(verify_stamp(kernel, data, Some(b"release\n"), &head("abc1234", false)).is_ok());
}

#[test]
fn a_kernel_from_another_commit_is_refused_as_stale() {
    let kernel = Path::new("/build/x86_64/kernel.elf");
    let data = b") #1: def5678 2026-11-27\0";
    let error = verify_stamp(kernel, data, Some(b"release\n"), &head("abc1234", false))
        .expect_err("stale kernel must be refused");
    assert!(
        format!("{error:#}").contains("stale kernel"),
        "unexpected error: {error:#}"
    );
}

#[test]
fn a_dirty_kernel_is_refused() {
    let kernel = Path::new("/build/x86_64/kernel.elf");
    let data = b") #1: abc1234-dirty 2026-11-27\0";
    let error = verify_stamp(kernel, data, Some(b"release\n"), &head("abc1234", false))
        .expect_err("dirty kernel must be refused");
    assert!(format!("{error:#}").contains("dirty tree"));
}

#[test]
fn a_dirty_worktree_is_refused_even_with_a_clean_kernel() {
    let kernel = Path::new("/build/x86_64/kernel.elf");
    let data = b") #1: abc1234 2026-11-27\0";
    let error = verify_stamp(kernel, data, Some(b"release\n"), &head("abc1234", true))
        .expect_err("dirty worktree must be refused");
    assert!(format!("{error:#}").contains("clean worktree"));
}

#[test]
fn a_debug_kernel_is_refused() {
    let kernel = Path::new("/build/x86_64/kernel.elf");
    let data = b") #1: abc1234 2026-11-27\0";
    let error = verify_stamp(kernel, data, Some(b"debug\n"), &head("abc1234", false))
        .expect_err("debug kernel must be refused");
    assert!(format!("{error:#}").contains("debug profile"));
}

#[test]
fn a_missing_profile_is_refused() {
    let kernel = Path::new("/build/x86_64/kernel.elf");
    let data = b") #1: abc1234 2026-11-27\0";
    let error = verify_stamp(kernel, data, None, &head("abc1234", false))
        .expect_err("missing profile must be refused");
    assert!(format!("{error:#}").contains("no valid release profile stamp"));
}

#[test]
fn an_unknown_profile_value_is_refused() {
    let kernel = Path::new("/build/x86_64/kernel.elf");
    let data = b") #1: abc1234 2026-11-27\0";
    let error = verify_stamp(kernel, data, Some(b"fast\n"), &head("abc1234", false))
        .expect_err("unknown profile must be refused");
    assert!(format!("{error:#}").contains("no valid release profile stamp"));
}

#[test]
fn an_empty_head_is_invalid() {
    let kernel = Path::new("/build/x86_64/kernel.elf");
    let error = verify_stamp(kernel, b"abc1234", Some(b"release\n"), &head("", false))
        .expect_err("empty HEAD must be refused");
    assert!(format!("{error:#}").contains("invalid HEAD short id"));
}

#[test]
fn a_non_hex_head_is_invalid() {
    let kernel = Path::new("/build/x86_64/kernel.elf");
    let error = verify_stamp(
        kernel,
        b"abc1234",
        Some(b"release\n"),
        &head("not-hex!", false),
    )
    .expect_err("non-hex HEAD must be refused");
    assert!(format!("{error:#}").contains("invalid HEAD short id"));
}

#[test]
fn a_longer_hash_that_contains_the_short_id_is_not_a_match() {
    assert!(!contains_clean_build_id(b") abc12345\0", "abc1234"));
    assert!(!contains_clean_build_id(b") 0abc1234\0", "abc1234"));
    assert!(!contains_clean_build_id(b") abc1234-dirty\0", "abc1234"));
    assert!(!contains_clean_build_id(b"", "abc1234"));
    assert!(!contains_clean_build_id(b"abc1234", ""));
    assert!(contains_clean_build_id(b") abc1234\0", "abc1234"));
    assert!(contains_clean_build_id(b"abc1234", "abc1234"));
}

#[test]
fn the_profile_sidecar_sits_next_to_the_kernel() {
    assert_eq!(
        profile_path(Path::new("/build/x86_64/kernel.elf")),
        Path::new("/build/x86_64/kernel.elf.profile")
    );
}
