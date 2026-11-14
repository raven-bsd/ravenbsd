// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn a_name_preserves_its_bytes() {
    let name = Name::new(b"libc.so.1").expect("name");
    assert_eq!(name.as_bytes(), b"libc.so.1");
    assert!(!name.is_empty());
}

#[test]
fn an_empty_name_is_empty() {
    assert!(Name::empty().is_empty());
    assert!(Name::new(b"").expect("name").is_empty());
}

#[test]
fn the_w_x_and_textrel_messages_are_golden() {
    assert_eq!(
        LoadError::Elf(ElfError::BadSegment).text(),
        "segment is writable and executable"
    );
    assert_eq!(
        LoadError::Digest(DigestError::TextRel).text(),
        "text relocations not supported"
    );
    assert_eq!(
        LoadError::SecurePath.text(),
        "relative path refused in secure mode"
    );
    assert_eq!(
        LoadError::Tls(LayoutError::TooLarge).text(),
        "No space available for static Thread Local Storage"
    );
}

#[test]
fn the_golden_diagnostics_cover_every_suffix() {
    const GOLDEN: &str = include_str!("../tests/golden/dlerror.txt");
    for suffix in [
        "I/O error",
        "invalid ELF object",
        "invalid dynamic section",
        "segment is writable and executable",
        "text relocations not supported",
        "cannot map segment",
        "shared object not found",
        "too many loaded objects",
        "executable stack",
        "relocation failed",
        "No space available for static Thread Local Storage",
        "Tainted process refusing to run binary",
        "bad direct-execution arguments",
        "relative path refused in secure mode",
    ] {
        assert!(GOLDEN.lines().any(|line| line == suffix), "{suffix}");
    }
}

#[test]
fn errno_values_are_stable() {
    assert_eq!(LoadError::NotFound.errno(), 2);
    assert_eq!(LoadError::SecurePath.errno(), 1);
    assert_eq!(LoadError::Tainted.errno(), 1);
}
