// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use crate::testutil::TempDir;

fn check(temp: &TempDir, text: &str, donor: &str) -> Result<()> {
    let path = temp.write("donor/file.c", text);
    check_file(&path, donor, temp.path())
}

#[test]
fn each_reject_phrase_refuses() {
    for phrase in REJECT_PHRASES {
        let temp = TempDir::new("gate-reject");
        let text = format!("/* Copyright (c) 2026 Example */\n * {phrase}\n */\n");
        let error = check(&temp, &text, "openbsd").unwrap_err();
        assert!(error.to_string().contains(phrase));
    }
}

#[test]
fn a_headerless_file_refuses() {
    let temp = TempDir::new("gate-headerless");
    let error = check(&temp, "int f(void) { return 1; }\n", "openbsd").unwrap_err();
    assert!(error.to_string().contains("no copyright"));
}

#[test]
fn public_domain_text_is_accepted() {
    let temp = TempDir::new("gate-pd");
    check(
        &temp,
        "/* Public domain. No copyright claimed. */\nint f(void) { return 1; }\n",
        "netbsd",
    )
    .expect("public domain");
}

#[test]
fn copyright_after_the_first_sixty_lines_does_not_count() {
    let temp = TempDir::new("gate-late");
    let mut text = String::from("/* nothing here */\n");
    for _ in 0..80 {
        text.push_str("int filler;\n");
    }
    text.push_str("/* Copyright (c) 2026 Example */\n");
    let error = check(&temp, &text, "openbsd").unwrap_err();
    assert!(error.to_string().contains("first 60 lines"));
}

#[test]
fn musl_is_accepted_when_the_donor_copyright_exists() {
    let temp = TempDir::new("gate-musl");
    temp.directory("musl");
    temp.write("musl/COPYRIGHT", "Copyright (c) 2005-2020 Rich Felker\n");
    check(&temp, "void *memchr(void);\n", "musl").expect("musl copyright");
}

#[test]
fn musl_without_the_donor_copyright_refuses() {
    let temp = TempDir::new("gate-musl-missing");
    let error = check(&temp, "void *memchr(void);\n", "musl").unwrap_err();
    assert!(error.to_string().contains("no copyright"));
}

#[test]
fn the_musl_exception_does_not_cover_other_donors() {
    let temp = TempDir::new("gate-other");
    temp.directory("musl");
    temp.write("musl/COPYRIGHT", "Copyright (c) 2005-2020 Rich Felker\n");
    let error = check(&temp, "int f(void);\n", "openbsd").unwrap_err();
    assert!(error.to_string().contains("no copyright"));
}
