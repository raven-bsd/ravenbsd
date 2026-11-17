// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host checks for `nm` against real and malformed files.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use nm::{Class, inspect, parse_args, render};

fn temp(label: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("raven-nm-{label}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    path
}

fn real_binary() -> &'static str {
    env!("CARGO_BIN_EXE_nm")
}

#[test]
fn library_lists_symbols_from_a_real_elf_binary() {
    let path = real_binary();
    let data = fs::read(path).unwrap();
    let options = parse_args([path]).unwrap();
    let (class, entries) = inspect(&data, &options).unwrap();
    assert!(matches!(class, Class::Elf32 | Class::Elf64));
    assert!(!entries.is_empty(), "the host binary has symbols");
    assert!(
        entries
            .iter()
            .any(|entry| matches!(entry.kind, 'T' | 't' | 'W' | 'w')),
        "at least one text or weak symbol"
    );
    assert_eq!(render(&entries, class).len(), entries.len());
}

#[test]
fn command_lists_itself_and_undefined_symbols() {
    let path = real_binary();
    let output = Command::new(path).arg(path).output().unwrap();
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.lines().any(|line| line.contains(" T ")));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("no symbols"));

    let undefined = Command::new(path).arg("-u").arg(path).output().unwrap();
    assert!(undefined.status.success(), "{undefined:?}");
    let stdout = String::from_utf8_lossy(&undefined.stdout);
    assert!(stdout.lines().any(|line| line.contains(" U ")));
}

#[test]
fn command_headers_each_file() {
    let path = real_binary();
    let output = Command::new(path).arg(path).arg(path).output().unwrap();
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let header = format!("{path}:");
    assert!(stdout.starts_with(&format!("{header}\n")));
    assert_eq!(stdout.lines().filter(|line| *line == header).count(), 2);
}

#[test]
fn malformed_input_exits_one() {
    let root = temp("malformed");
    let path = root.join("not-elf");
    fs::write(&path, b"not an ELF file\n").unwrap();
    let output = Command::new(real_binary()).arg(&path).output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("not a supported ELF32/ELF64 object"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn missing_operand_is_a_usage_error() {
    let output = Command::new(real_binary()).output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("usage: nm"));
}
