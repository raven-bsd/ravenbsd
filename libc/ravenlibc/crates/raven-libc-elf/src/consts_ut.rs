// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::assertions_on_constants,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]
// The header parser walks a text file; the arithmetic is over small line offsets.

//! Constant checks: an independent table of the numbers, plus a re-read of `glibc/elf/elf.h`.

use super::*;

/// `(name, our value, kind)` — the kind selects the C integer syntax to compare.
enum Kind {
    /// A small unsigned value.
    U(u64),
    /// A signed dynamic tag.
    I(i64),
}

fn defined(name: &str) -> Kind {
    let value: i64 = match name {
        "DT_NULL" => DT_NULL,
        "DT_NEEDED" => DT_NEEDED,
        "DT_PLTRELSZ" => DT_PLTRELSZ,
        "DT_PLTGOT" => DT_PLTGOT,
        "DT_HASH" => DT_HASH,
        "DT_STRTAB" => DT_STRTAB,
        "DT_SYMTAB" => DT_SYMTAB,
        "DT_RELA" => DT_RELA,
        "DT_RELASZ" => DT_RELASZ,
        "DT_RELAENT" => DT_RELAENT,
        "DT_STRSZ" => DT_STRSZ,
        "DT_SYMENT" => DT_SYMENT,
        "DT_INIT" => DT_INIT,
        "DT_FINI" => DT_FINI,
        "DT_SONAME" => DT_SONAME,
        "DT_RPATH" => DT_RPATH,
        "DT_SYMBOLIC" => DT_SYMBOLIC,
        "DT_REL" => DT_REL,
        "DT_RELSZ" => DT_RELSZ,
        "DT_RELENT" => DT_RELENT,
        "DT_PLTREL" => DT_PLTREL,
        "DT_DEBUG" => DT_DEBUG,
        "DT_TEXTREL" => DT_TEXTREL,
        "DT_JMPREL" => DT_JMPREL,
        "DT_BIND_NOW" => DT_BIND_NOW,
        "DT_INIT_ARRAY" => DT_INIT_ARRAY,
        "DT_FINI_ARRAY" => DT_FINI_ARRAY,
        "DT_INIT_ARRAYSZ" => DT_INIT_ARRAYSZ,
        "DT_FINI_ARRAYSZ" => DT_FINI_ARRAYSZ,
        "DT_RUNPATH" => DT_RUNPATH,
        "DT_FLAGS" => DT_FLAGS,
        "DT_PREINIT_ARRAY" => DT_PREINIT_ARRAY,
        "DT_PREINIT_ARRAYSZ" => DT_PREINIT_ARRAYSZ,
        "DT_RELRSZ" => DT_RELRSZ,
        "DT_RELR" => DT_RELR,
        "DT_RELRENT" => DT_RELRENT,
        "DT_GNU_HASH" => DT_GNU_HASH,
        "DT_VERSYM" => DT_VERSYM,
        "DT_RELACOUNT" => DT_RELACOUNT,
        "DT_FLAGS_1" => DT_FLAGS_1,
        "DT_VERDEF" => DT_VERDEF,
        "DT_VERDEFNUM" => DT_VERDEFNUM,
        "DT_VERNEED" => DT_VERNEED,
        "DT_VERNEEDNUM" => DT_VERNEEDNUM,
        "PT_LOAD" => i64::from(PT_LOAD),
        "PT_DYNAMIC" => i64::from(PT_DYNAMIC),
        "PT_INTERP" => i64::from(PT_INTERP),
        "PT_NOTE" => i64::from(PT_NOTE),
        "PT_PHDR" => i64::from(PT_PHDR),
        "PT_TLS" => i64::from(PT_TLS),
        "PT_GNU_EH_FRAME" => i64::from(PT_GNU_EH_FRAME),
        "PT_GNU_STACK" => i64::from(PT_GNU_STACK),
        "PT_GNU_RELRO" => i64::from(PT_GNU_RELRO),
        "PT_GNU_PROPERTY" => i64::from(PT_GNU_PROPERTY),
        "PF_X" => i64::from(PF_X),
        "PF_W" => i64::from(PF_W),
        "PF_R" => i64::from(PF_R),
        "ET_EXEC" => i64::from(ET_EXEC),
        "ET_DYN" => i64::from(ET_DYN),
        "EM_X86_64" => i64::from(EM_X86_64),
        "EM_AARCH64" => i64::from(EM_AARCH64),
        "EM_RISCV" => i64::from(EM_RISCV),
        "STT_NOTYPE" => i64::from(STT_NOTYPE),
        "STT_OBJECT" => i64::from(STT_OBJECT),
        "STT_FUNC" => i64::from(STT_FUNC),
        "STT_COMMON" => i64::from(STT_COMMON),
        "STT_TLS" => i64::from(STT_TLS),
        "STT_GNU_IFUNC" => i64::from(STT_GNU_IFUNC),
        "STB_LOCAL" => i64::from(STB_LOCAL),
        "STB_GLOBAL" => i64::from(STB_GLOBAL),
        "STB_WEAK" => i64::from(STB_WEAK),
        "STV_DEFAULT" => i64::from(STV_DEFAULT),
        "STV_INTERNAL" => i64::from(STV_INTERNAL),
        "STV_HIDDEN" => i64::from(STV_HIDDEN),
        "STV_PROTECTED" => i64::from(STV_PROTECTED),
        "SHN_UNDEF" => i64::from(SHN_UNDEF),
        "SHN_ABS" => i64::from(SHN_ABS),
        "SHN_COMMON" => i64::from(SHN_COMMON),
        "VER_FLG_BASE" => i64::from(VER_FLG_BASE),
        "VER_FLG_WEAK" => i64::from(VER_FLG_WEAK),
        "VER_NDX_LOCAL" => i64::from(VER_NDX_LOCAL),
        "VER_NDX_GLOBAL" => i64::from(VER_NDX_GLOBAL),
        "VER_DEF_CURRENT" => i64::from(VER_DEF_CURRENT),
        "VER_NEED_CURRENT" => i64::from(VER_NEED_CURRENT),
        "R_X86_64_NONE" => i64::from(R_X86_64_NONE),
        "R_X86_64_64" => i64::from(R_X86_64_64),
        "R_X86_64_PC32" => i64::from(R_X86_64_PC32),
        "R_X86_64_COPY" => i64::from(R_X86_64_COPY),
        "R_X86_64_GLOB_DAT" => i64::from(R_X86_64_GLOB_DAT),
        "R_X86_64_JUMP_SLOT" => i64::from(R_X86_64_JUMP_SLOT),
        "R_X86_64_RELATIVE" => i64::from(R_X86_64_RELATIVE),
        "R_X86_64_DTPMOD64" => i64::from(R_X86_64_DTPMOD64),
        "R_X86_64_DTPOFF64" => i64::from(R_X86_64_DTPOFF64),
        "R_X86_64_TPOFF64" => i64::from(R_X86_64_TPOFF64),
        "R_X86_64_DTPOFF32" => i64::from(R_X86_64_DTPOFF32),
        "R_X86_64_TPOFF32" => i64::from(R_X86_64_TPOFF32),
        "R_X86_64_TLSDESC" => i64::from(R_X86_64_TLSDESC),
        "R_X86_64_IRELATIVE" => i64::from(R_X86_64_IRELATIVE),
        "R_AARCH64_NONE" => i64::from(R_AARCH64_NONE),
        "R_AARCH64_ABS64" => i64::from(R_AARCH64_ABS64),
        "R_AARCH64_COPY" => i64::from(R_AARCH64_COPY),
        "R_AARCH64_GLOB_DAT" => i64::from(R_AARCH64_GLOB_DAT),
        "R_AARCH64_JUMP_SLOT" => i64::from(R_AARCH64_JUMP_SLOT),
        "R_AARCH64_RELATIVE" => i64::from(R_AARCH64_RELATIVE),
        "R_AARCH64_TLS_DTPMOD64" => i64::from(R_AARCH64_TLS_DTPMOD64),
        "R_AARCH64_TLS_DTPREL64" => i64::from(R_AARCH64_TLS_DTPREL64),
        "R_AARCH64_TLS_TPREL64" => i64::from(R_AARCH64_TLS_TPREL64),
        "R_AARCH64_TLSDESC" => i64::from(R_AARCH64_TLSDESC),
        "R_AARCH64_IRELATIVE" => i64::from(R_AARCH64_IRELATIVE),
        "R_RISCV_NONE" => i64::from(R_RISCV_NONE),
        "R_RISCV_64" => i64::from(R_RISCV_64),
        "R_RISCV_RELATIVE" => i64::from(R_RISCV_RELATIVE),
        "R_RISCV_COPY" => i64::from(R_RISCV_COPY),
        "R_RISCV_JUMP_SLOT" => i64::from(R_RISCV_JUMP_SLOT),
        "R_RISCV_TLS_DTPMOD64" => i64::from(R_RISCV_TLS_DTPMOD64),
        "R_RISCV_TLS_DTPREL64" => i64::from(R_RISCV_TLS_DTPREL64),
        "R_RISCV_TLS_TPREL64" => i64::from(R_RISCV_TLS_TPREL64),
        "R_RISCV_TLSDESC" => i64::from(R_RISCV_TLSDESC),
        "R_RISCV_IRELATIVE" => i64::from(R_RISCV_IRELATIVE),
        "DF_ORIGIN" => i64::try_from(DF_ORIGIN).unwrap_or(-1),
        "DF_SYMBOLIC" => i64::try_from(DF_SYMBOLIC).unwrap_or(-1),
        "DF_TEXTREL" => i64::try_from(DF_TEXTREL).unwrap_or(-1),
        "DF_BIND_NOW" => i64::try_from(DF_BIND_NOW).unwrap_or(-1),
        "DF_STATIC_TLS" => i64::try_from(DF_STATIC_TLS).unwrap_or(-1),
        _ => return Kind::U(u64::MAX),
    };
    if name.starts_with("DT_") {
        Kind::I(value)
    } else {
        Kind::U(u64::try_from(value).unwrap_or(u64::MAX))
    }
}

/// Locate `glibc/elf/elf.h` by walking up from the crate directory.
fn glibc_elf_h() -> Option<std::path::PathBuf> {
    let mut directory = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for _ in 0..12 {
        let candidate = directory.join("ravenbsd-resources/glibc/elf/elf.h");
        if candidate.is_file() {
            return Some(candidate);
        }
        if !directory.pop() {
            break;
        }
    }
    std::env::var_os("RAVENBSD_RESOURCES")
        .map(|root| std::path::PathBuf::from(root).join("glibc/elf/elf.h"))
        .filter(|path| path.is_file())
}

/// Parse a `#define NAME ...` value as an integer, including hex, octal, `(N << S)` and decimal.
fn define_value(text: &str, name: &str) -> Option<i64> {
    let mut rest = text;
    loop {
        let line_start = rest;
        let end = line_start.find('\n').unwrap_or(line_start.len());
        let mut line = line_start.get(..end)?.trim();
        // Drop a trailing comment.
        if let Some(comment) = line.find("/*") {
            line = line.get(..comment)?.trim();
        }
        if let Some(tail) = line.strip_prefix("#define ") {
            let mut parts = tail.split_whitespace();
            if parts.next() == Some(name) {
                let expression: alloc::string::String =
                    parts.collect::<alloc::vec::Vec<_>>().join(" ");
                let expression = expression.trim();
                let expression = expression
                    .trim_start_matches('(')
                    .trim_end_matches(')')
                    .trim();
                if let Some((left, right)) = expression.split_once("<<") {
                    let left = parse_int(left.trim())?;
                    let right = parse_int(right.trim())?;
                    return left.checked_shl(u32::try_from(right).ok()?);
                }
                return parse_int(expression);
            }
        }
        if end >= rest.len() {
            return None;
        }
        rest = rest.get(end + 1..)?;
    }
}

/// Parse one integer literal (`0x` hex, leading `0` octal, decimal; optional `L`/`U` suffixes).
fn parse_int(text: &str) -> Option<i64> {
    let value = text.trim_end_matches(['L', 'U', 'l', 'u']);
    if let Some(hex) = value.strip_prefix("0x") {
        return i64::from_str_radix(hex, 16).ok();
    }
    if value.len() > 1 && value.starts_with('0') {
        return i64::from_str_radix(value.trim_start_matches('0'), 8).ok();
    }
    value.parse().ok()
}

#[test]
fn every_constant_matches_the_glibc_header_or_is_skipped() {
    let Some(path) = glibc_elf_h() else {
        println!("SKIP: glibc checkout absent");
        return;
    };
    let text = std::fs::read_to_string(&path).expect("read elf.h");
    let names = [
        "DT_NULL",
        "DT_NEEDED",
        "DT_PLTRELSZ",
        "DT_PLTGOT",
        "DT_HASH",
        "DT_STRTAB",
        "DT_SYMTAB",
        "DT_RELA",
        "DT_RELASZ",
        "DT_RELAENT",
        "DT_STRSZ",
        "DT_SYMENT",
        "DT_INIT",
        "DT_FINI",
        "DT_SONAME",
        "DT_RPATH",
        "DT_SYMBOLIC",
        "DT_REL",
        "DT_RELSZ",
        "DT_RELENT",
        "DT_PLTREL",
        "DT_DEBUG",
        "DT_TEXTREL",
        "DT_JMPREL",
        "DT_BIND_NOW",
        "DT_INIT_ARRAY",
        "DT_FINI_ARRAY",
        "DT_INIT_ARRAYSZ",
        "DT_FINI_ARRAYSZ",
        "DT_RUNPATH",
        "DT_FLAGS",
        "DT_PREINIT_ARRAY",
        "DT_PREINIT_ARRAYSZ",
        "DT_RELRSZ",
        "DT_RELR",
        "DT_RELRENT",
        "DT_GNU_HASH",
        "DT_VERSYM",
        "DT_RELACOUNT",
        "DT_FLAGS_1",
        "DT_VERDEF",
        "DT_VERDEFNUM",
        "DT_VERNEED",
        "DT_VERNEEDNUM",
        "PT_LOAD",
        "PT_DYNAMIC",
        "PT_INTERP",
        "PT_NOTE",
        "PT_PHDR",
        "PT_TLS",
        "PT_GNU_EH_FRAME",
        "PT_GNU_STACK",
        "PT_GNU_RELRO",
        "PT_GNU_PROPERTY",
        "PF_X",
        "PF_W",
        "PF_R",
        "ET_EXEC",
        "ET_DYN",
        "EM_X86_64",
        "EM_AARCH64",
        "EM_RISCV",
        "STT_NOTYPE",
        "STT_OBJECT",
        "STT_FUNC",
        "STT_COMMON",
        "STT_TLS",
        "STT_GNU_IFUNC",
        "STB_LOCAL",
        "STB_GLOBAL",
        "STB_WEAK",
        "STV_DEFAULT",
        "STV_INTERNAL",
        "STV_HIDDEN",
        "STV_PROTECTED",
        "SHN_UNDEF",
        "SHN_ABS",
        "SHN_COMMON",
        "VER_FLG_BASE",
        "VER_FLG_WEAK",
        "VER_NDX_LOCAL",
        "VER_NDX_GLOBAL",
        "VER_DEF_CURRENT",
        "VER_NEED_CURRENT",
        "R_X86_64_NONE",
        "R_X86_64_64",
        "R_X86_64_PC32",
        "R_X86_64_COPY",
        "R_X86_64_GLOB_DAT",
        "R_X86_64_JUMP_SLOT",
        "R_X86_64_RELATIVE",
        "R_X86_64_DTPMOD64",
        "R_X86_64_DTPOFF64",
        "R_X86_64_TPOFF64",
        "R_X86_64_DTPOFF32",
        "R_X86_64_TPOFF32",
        "R_X86_64_TLSDESC",
        "R_X86_64_IRELATIVE",
        "R_AARCH64_NONE",
        "R_AARCH64_ABS64",
        "R_AARCH64_COPY",
        "R_AARCH64_GLOB_DAT",
        "R_AARCH64_JUMP_SLOT",
        "R_AARCH64_RELATIVE",
        "R_AARCH64_TLS_DTPMOD64",
        "R_AARCH64_TLS_DTPREL64",
        "R_AARCH64_TLS_TPREL64",
        "R_AARCH64_TLSDESC",
        "R_AARCH64_IRELATIVE",
        "R_RISCV_NONE",
        "R_RISCV_64",
        "R_RISCV_RELATIVE",
        "R_RISCV_COPY",
        "R_RISCV_JUMP_SLOT",
        "R_RISCV_TLS_DTPMOD64",
        "R_RISCV_TLS_DTPREL64",
        "R_RISCV_TLS_TPREL64",
        "R_RISCV_TLSDESC",
        "R_RISCV_IRELATIVE",
        "DF_ORIGIN",
        "DF_SYMBOLIC",
        "DF_TEXTREL",
        "DF_BIND_NOW",
        "DF_STATIC_TLS",
    ];
    let mut compared = 0usize;
    for name in names {
        let Some(expected) = define_value(&text, name) else {
            // Some psABI numbers live in the arch `<asm/elf.h>` of the host, not in elf.h.
            continue;
        };
        match defined(name) {
            Kind::U(value) => assert_eq!(
                i64::try_from(value).unwrap_or(-1),
                expected,
                "{name} differs from glibc elf.h"
            ),
            Kind::I(value) => assert_eq!(value, expected, "{name} differs from glibc elf.h"),
        }
        compared += 1;
    }
    assert!(compared >= 40, "only {compared} constants were compared");
}

#[test]
fn the_declared_record_sizes_match_the_abi() {
    assert_eq!(ELF_MAGIC, [0x7f, b'E', b'L', b'F']);
    assert_eq!(EHDR_SIZE, 64);
    assert_eq!(PHDR_SIZE, 56);
    assert_eq!(DYN_SIZE, 16);
    assert_eq!(SYM_SIZE, 24);
    assert_eq!(RELA_SIZE, 24);
    assert_eq!(RELR_ENTRY_SIZE, 8);
    assert_eq!(VERDEF_SIZE, 20);
    assert_eq!(VERDAUX_SIZE, 8);
    assert_eq!(VERNEED_SIZE, 16);
    assert_eq!(VERNAUX_SIZE, 16);
    assert_eq!(NOTE_HEADER_SIZE, 12);
}

#[test]
fn the_version_index_mask_splits_index_and_hidden_bit() {
    let reference = 0x8003u16;
    assert_eq!(reference & VER_NDX_MASK, 3);
    assert_eq!(reference & VER_NDX_HIDDEN, VER_NDX_HIDDEN);
    assert_eq!(VER_NDX_MASK & VER_NDX_HIDDEN, 0);
    assert_eq!(VER_NDX_LOCAL, 0);
    assert_eq!(VER_NDX_GLOBAL, 1);
    assert!(VER_NDX_LORESERVE > 1);
}

#[test]
fn the_arch_relocation_numbers_are_the_psabi_ones() {
    assert_eq!(R_AARCH64_TLS_DTPMOD64, 1028);
    assert_eq!(R_AARCH64_TLS_DTPREL64, 1029);
    assert_eq!(R_AARCH64_TLS_TPREL64, 1030);
    assert_eq!(R_RISCV_TLS_DTPMOD64, 7);
    assert_eq!(R_RISCV_TLS_DTPREL64, 9);
    assert_eq!(R_RISCV_TLS_TPREL64, 11);
    assert_eq!(R_RISCV_IRELATIVE, 58);
    assert_eq!(R_X86_64_TPOFF64, 18);
    assert_eq!(R_X86_64_IRELATIVE, 37);
}
