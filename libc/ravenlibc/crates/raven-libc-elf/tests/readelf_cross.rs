// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used
)]

//! `readelf` as oracle: the `DT_NEEDED`, `DT_SONAME`, `DT_FLAGS` and relocation counts of a
//! linker-produced library must equal what the reader reports.
//!
//! Without `readelf`/`llvm-readelf` the test prints `SKIP: readelf not found` unless
//! `RL_ELF_REQUIRED=1` is set.

use std::path::PathBuf;
use std::process::Command;

use raven_libc_elf::elf::{Ehdr, read_phdrs};
use raven_libc_elf::mem::{MemSet, SliceMem};
use raven_libc_elf::object::ObjView;

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

fn readelf() -> Option<&'static str> {
    if have("readelf") {
        Some("readelf")
    } else if have("llvm-readelf") {
        Some("llvm-readelf")
    } else {
        None
    }
}

fn workdir() -> PathBuf {
    let directory =
        std::env::temp_dir().join(format!("raven-libc-elf-c52-readelf-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    directory
}

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

fn dynamic_field(tool: &str, path: &PathBuf, tag: &str) -> Vec<String> {
    let output = Command::new(tool).arg("-d").arg(path).output().unwrap();
    let text = String::from_utf8_lossy(&output.stdout);
    let mut values = Vec::new();
    for line in text.lines() {
        if line.contains(&format!("({tag})"))
            && let Some(start) = line.find('[')
            && let Some(end) = line[start..].find(']')
        {
            values.push(line[start + 1..start + end].to_string());
        }
    }
    values
}

fn flags_line(tool: &str, path: &PathBuf) -> String {
    let output = Command::new(tool).arg("-d").arg(path).output().unwrap();
    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        if line.contains("(FLAGS)") {
            return line.to_string();
        }
    }
    String::new()
}

fn relocation_lines(tool: &str, path: &PathBuf) -> usize {
    let output = Command::new(tool).arg("-r").arg(path).output().unwrap();
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines()
        .filter(|line| {
            line.contains("R_X86_64_") || line.contains("R_AARCH64_") || line.contains("R_RISCV_")
        })
        .count()
}

#[test]
fn the_dynamic_tags_and_relocation_counts_match_readelf() {
    let Some(tool) = readelf() else {
        if required() {
            panic!("RL_ELF_REQUIRED=1 but readelf was not found");
        }
        println!("SKIP: readelf not found");
        return;
    };
    if !have("cc") {
        if required() {
            panic!("RL_ELF_REQUIRED=1 but cc was not found");
        }
        println!("SKIP: cc not found");
        return;
    }
    let directory = workdir();
    let dep_source = directory.join("dep.c");
    let user_source = directory.join("user.c");
    std::fs::write(&dep_source, "int dep(void) { return 7; }\n").unwrap();
    std::fs::write(
        &user_source,
        "extern int dep(void);\nint call(void) { return dep(); }\n",
    )
    .unwrap();
    let dep = directory.join("libdep.so");
    let user = directory.join("libuser.so");
    assert!(
        Command::new("cc")
            .args(["-shared", "-fPIC", "-o"])
            .arg(&dep)
            .arg(&dep_source)
            .args(["-Wl,-soname,libdep.so.1"])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        Command::new("cc")
            .args(["-shared", "-fPIC", "-o"])
            .arg(&user)
            .arg(&user_source)
            .arg(format!("-L{}", directory.display()))
            .args(["-ldep", "-Wl,-soname,libuser.so.1", "-Wl,--hash-style=gnu"])
            .status()
            .unwrap()
            .success()
    );

    let bytes = std::fs::read(&user).unwrap();
    let (view, _memory) = image(&bytes);
    let memory = SliceMem::new(0, &bytes);
    let needed = dynamic_field(tool, &user, "NEEDED");
    let soname = dynamic_field(tool, &user, "SONAME");
    assert_eq!(soname, vec!["libuser.so.1".to_string()]);
    assert_eq!(needed, vec!["libdep.so.1".to_string()]);
    let reported_needed: Vec<String> = view
        .needed
        .iter()
        .map(|offset| String::from_utf8_lossy(view.name_at(&memory, *offset).unwrap()).into_owned())
        .collect();
    assert_eq!(reported_needed, needed);
    assert_eq!(
        view.soname.map(
            |offset| String::from_utf8_lossy(view.name_at(&memory, offset).unwrap()).into_owned()
        ),
        soname.first().cloned()
    );

    let flags = flags_line(tool, &user);
    let bind_now = flags.contains("BIND_NOW") || flags.contains("NOW");
    assert_eq!(view.bind_now, bind_now, "flags line: {flags:?}");

    let ours = (view.rela.1 / 24 + view.plt_rela.1 / 24) as usize;
    let theirs = relocation_lines(tool, &user);
    assert_eq!(ours, theirs, "relocation count differs from {tool} -r");
    println!("{tool}: needed={needed:?} soname={soname:?} relocs={theirs} bind_now={bind_now}");
    assert_eq!(bytes.get(..4), Some(&b"\x7fELF"[..]), "not an ELF file");
}
