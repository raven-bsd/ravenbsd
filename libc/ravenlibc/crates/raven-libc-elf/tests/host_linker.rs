// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used
)]

//! The host linker as oracle: every function the compiler emitted must be found through both
//! hash tables, and a name that was not emitted must not be found.
//!
//! The test compiles a shared object with 2000 functions three times (`--hash-style=both`,
//! `gnu`, `sysv`). Without a C compiler it prints `SKIP: cc not found` unless
//! `RL_ELF_REQUIRED=1` is set.

use std::path::PathBuf;
use std::process::Command;

use raven_libc_elf::elf::{Ehdr, read_phdrs};
use raven_libc_elf::lookup::{SymLook, lookup_obj};
use raven_libc_elf::mem::{MemSet, SliceMem};
use raven_libc_elf::object::ObjView;
use raven_libc_elf::scope::ObjId;

const FUNCTIONS: usize = 2000;

fn required() -> bool {
    std::env::var("RL_ELF_REQUIRED").ok().as_deref() == Some("1")
}

fn have(tool: &str) -> bool {
    Command::new(tool)
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn workdir() -> PathBuf {
    let directory = std::env::temp_dir().join(format!("raven-libc-elf-c52-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    directory
}

fn source(path: &PathBuf) {
    let mut text = String::new();
    for index in 0..FUNCTIONS {
        text.push_str(&format!("int f{index}(void) {{ return {index}; }}\n"));
    }
    std::fs::write(path, text).unwrap();
}

/// Map a shared object's file bytes the way the kernel would.
fn image(bytes: &[u8]) -> (ObjView, MemSet<'_>) {
    let header = Ehdr::parse(bytes).unwrap();
    let file_view = SliceMem::new(0, bytes);
    let phdrs = read_phdrs(&file_view, header.e_phoff, header.e_phnum).unwrap();
    let mut set = MemSet::new();
    for phdr in &phdrs {
        if phdr.p_type != 1 {
            continue;
        }
        let start = phdr.p_offset as usize;
        let end = start + phdr.p_filesz as usize;
        if let Some(content) = bytes.get(start..end) {
            set.push(SliceMem::new(phdr.p_vaddr, content));
        }
    }
    let phdr_vaddr = phdrs
        .iter()
        .find(|phdr| {
            phdr.p_type == 1
                && phdr.p_offset <= header.e_phoff
                && header.e_phoff < phdr.p_offset + phdr.p_filesz
        })
        .map(|phdr| phdr.p_vaddr + (header.e_phoff - phdr.p_offset))
        .expect("program headers are inside a PT_LOAD");
    let view = ObjView::digest(&set, phdr_vaddr, header.e_phnum, 0).unwrap();
    (view, set)
}

fn check_style(style: &str) {
    let directory = workdir();
    let source_path = directory.join(format!("libt_{style}.c"));
    let library = directory.join(format!("libt_{style}.so"));
    source(&source_path);
    let status = Command::new("cc")
        .args([
            "-shared",
            "-fPIC",
            "-O0",
            "-Wl,--hash-style",
            style,
            "-Wl,-soname,libt.so",
            "-o",
        ])
        .arg(&library)
        .arg(&source_path)
        .status()
        .unwrap();
    assert!(status.success(), "cc failed for --hash-style={style}");
    let bytes = std::fs::read(&library).unwrap();
    let (view, memory) = image(&bytes);
    assert_eq!(&bytes[0..4], b"\x7fELF");
    assert!(view.hash_sysv.is_some() || view.hash_gnu.is_some());
    let mut found = 0usize;
    for index in 0..FUNCTIONS {
        let name = format!("f{index}");
        let request = SymLook::new(name.as_bytes());
        let hit = lookup_obj(ObjId(0), &view, &memory, &request)
            .unwrap()
            .unwrap_or_else(|| panic!("{style}: {name} not found"));
        assert_eq!(hit.sym.typ(), 2, "{style}: {name} is not a function");
        found += 1;
    }
    assert_eq!(found, FUNCTIONS);
    let missing_name = format!("f{FUNCTIONS}");
    let missing = SymLook::new(missing_name.as_bytes());
    assert!(
        lookup_obj(ObjId(0), &view, &memory, &missing)
            .unwrap()
            .is_none(),
        "{style}: a name that was never emitted was found"
    );
    println!(
        "hash-style={style}: {} symbols, sysv={} gnu={}",
        FUNCTIONS,
        view.hash_sysv.is_some(),
        view.hash_gnu.is_some()
    );
}

#[test]
fn every_emitted_function_is_found_through_both_hash_styles() {
    if !have("cc") {
        if required() {
            panic!("RL_ELF_REQUIRED=1 but cc was not found");
        }
        println!("SKIP: cc not found");
        return;
    }
    let version = Command::new("cc").arg("--version").output().unwrap();
    let first = String::from_utf8_lossy(&version.stdout)
        .lines()
        .next()
        .unwrap_or("")
        .to_string();
    println!("compiler: {first}");
    for style in ["both", "gnu", "sysv"] {
        check_style(style);
    }
}
