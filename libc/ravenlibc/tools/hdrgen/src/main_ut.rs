// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the command-line parser.

use super::*;

fn args(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_string()).collect()
}

#[test]
fn parses_every_option() {
    let options = Options::parse(&args(&[
        "--root",
        "/r",
        "--abi",
        "/a",
        "--rust",
        "/u",
        "--resources",
        "/s",
        "--target",
        "x86_64",
        "--clang",
        "clang-17",
        "--sed",
        "gsed",
        "--date",
        "2026-11-13",
    ]))
    .expect("parse");
    assert_eq!(options.root.as_deref(), Some(std::path::Path::new("/r")));
    assert_eq!(options.abi.as_deref(), Some(std::path::Path::new("/a")));
    assert_eq!(options.rust.as_deref(), Some(std::path::Path::new("/u")));
    assert_eq!(
        options.resources.as_deref(),
        Some(std::path::Path::new("/s"))
    );
    assert_eq!(options.target.as_deref(), Some("x86_64"));
    assert_eq!(options.clang.as_deref(), Some("clang-17"));
    assert_eq!(options.sed.as_deref(), Some("gsed"));
    assert_eq!(options.date.as_deref(), Some("2026-11-13"));
}

#[test]
fn rejects_an_unknown_option() {
    assert!(Options::parse(&args(&["--frobnicate"])).is_err());
}

#[test]
fn rejects_a_missing_value() {
    assert!(Options::parse(&args(&["--root"])).is_err());
}

#[test]
fn default_paths_are_relative_to_the_root() {
    let options = Options::parse(&args(&["--root", "/repo"])).expect("parse");
    assert_eq!(
        options.abi().expect("abi"),
        std::path::Path::new("/repo/libc/ravenlibc/abi")
    );
    assert_eq!(
        options.rust().expect("rust"),
        std::path::Path::new("/repo/libc/ravenlibc/crates/raven-libc-abi/src/generated")
    );
}

#[test]
fn explicit_paths_win() {
    let options =
        Options::parse(&args(&["--root", "/repo", "--abi", "/x", "--rust", "/y"])).expect("parse");
    assert_eq!(options.abi().expect("abi"), std::path::Path::new("/x"));
    assert_eq!(options.rust().expect("rust"), std::path::Path::new("/y"));
}

#[test]
fn explicit_resources_win_over_discovery() {
    let options = Options::parse(&args(&["--resources", "/res"])).expect("parse");
    assert_eq!(
        options.resources().expect("resources"),
        std::path::Path::new("/res")
    );
}
