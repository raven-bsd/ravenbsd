// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host checks for `strip` against a real executable and malformed input.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use raven_elf::inspect::{Object, SHT_SYMTAB};
use strip::{Mode, strip};

fn temp(label: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("raven-strip-{label}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    path
}

fn real_binary() -> &'static str {
    env!("CARGO_BIN_EXE_strip")
}

fn read16(data: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(data[offset..offset + 2].try_into().unwrap())
}

fn read32(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap())
}

fn read64(data: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(data[offset..offset + 8].try_into().unwrap())
}

fn entry(data: &[u8]) -> u64 {
    if data[4] == 1 {
        u64::from(read32(data, 24))
    } else {
        read64(data, 24)
    }
}

fn program_headers(data: &[u8]) -> Vec<u8> {
    let (offset, size, count) = if data[4] == 1 {
        (
            read32(data, 28) as usize,
            usize::from(read16(data, 42)),
            usize::from(read16(data, 44)),
        )
    } else {
        (
            read64(data, 32) as usize,
            usize::from(read16(data, 54)),
            usize::from(read16(data, 56)),
        )
    };
    data[offset..offset + size * count].to_vec()
}

fn has_symtab(data: &[u8]) -> bool {
    Object::parse(data)
        .unwrap()
        .sections()
        .any(|section| section.section_type() == SHT_SYMTAB)
}

fn debug_sections(data: &[u8]) -> usize {
    Object::parse(data)
        .unwrap()
        .sections()
        .filter(|section| {
            section
                .name()
                .is_some_and(|name| name.starts_with(".debug"))
        })
        .count()
}

#[test]
fn strips_a_real_binary_and_it_still_runs() {
    let root = temp("real");
    let fixture = root.join("fixture");
    fs::copy(real_binary(), &fixture).unwrap();
    let before = fs::read(&fixture).unwrap();
    assert!(has_symtab(&before));

    let status = Command::new(real_binary()).arg(&fixture).status().unwrap();
    assert!(status.success(), "{status:?}");

    let after = fs::read(&fixture).unwrap();
    assert!(!has_symtab(&after));
    assert_eq!(entry(&before), entry(&after));
    assert_eq!(program_headers(&before), program_headers(&after));

    let run = Command::new(&fixture).output().unwrap();
    assert_eq!(run.status.code(), Some(2), "{run:?}");
    assert!(String::from_utf8_lossy(&run.stderr).contains("usage: strip"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn debug_mode_keeps_the_symbol_table() {
    let root = temp("debug");
    let fixture = root.join("fixture");
    fs::copy(real_binary(), &fixture).unwrap();
    let before = fs::read(&fixture).unwrap();
    assert!(has_symtab(&before));

    let status = Command::new(real_binary())
        .args(["-g", fixture.to_str().unwrap()])
        .status()
        .unwrap();
    assert!(status.success(), "{status:?}");

    let after = fs::read(&fixture).unwrap();
    assert!(has_symtab(&after));
    assert_eq!(debug_sections(&after), 0);
    let symbols = Object::parse(&after).unwrap().symbols().count();
    assert!(symbols > 0);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn output_option_leaves_the_input_untouched() {
    use std::os::unix::fs::PermissionsExt;

    let root = temp("output");
    let input = root.join("input");
    let output = root.join("output");
    fs::copy(real_binary(), &input).unwrap();
    let before = fs::read(&input).unwrap();

    let run = Command::new(real_binary())
        .arg("-o")
        .arg(&output)
        .arg(&input)
        .output()
        .unwrap();
    assert!(run.status.success(), "{run:?}");
    assert_eq!(fs::read(&input).unwrap(), before);

    let stripped = fs::read(&output).unwrap();
    assert!(!has_symtab(&stripped));
    assert_ne!(
        fs::metadata(&output).unwrap().permissions().mode() & 0o111,
        0
    );
    let run = Command::new(&output).output().unwrap();
    assert_eq!(run.status.code(), Some(2));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn malformed_input_exits_one() {
    let root = temp("malformed");
    let path = root.join("not-elf");
    fs::write(&path, b"not an ELF file\n").unwrap();
    let output = Command::new(real_binary()).arg(&path).output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("not a supported ELF32/ELF64 file"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn stripping_a_real_binary_twice_is_idempotent() {
    let mut data = fs::read(real_binary()).unwrap();
    assert!(strip(&mut data, Mode::All).unwrap().blanked > 0);
    assert_eq!(strip(&mut data, Mode::All).unwrap().blanked, 0);
    assert!(Object::parse(&data).is_ok());
}
