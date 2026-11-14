// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the measurement parsers, using output captured from the pinned LLVM 22
//! tools plus hand-built fixtures for the dynamic forms.

use super::*;

const BERKELEY_ONE: &str = "\
   text\t   data\t    bss\t    dec\t    hex\tfilename
  25248\t   2000\t   6384\t  33632\t   8360\ttiny-p0
";

const BERKELEY_TWO: &str = "\
   text\t   data\t    bss\t    dec\t    hex\tfilename
    176\t      0\t      0\t    176\t     b0\tlibc.a(absvdi2.o)
    164\t      0\t      0\t    164\t     a4\tlibc.a(divdi3.o)
";

#[test]
fn berkeley_one_file() {
    let entries = parse_berkeley(BERKELEY_ONE).expect("parse");
    assert_eq!(entries.len(), 1);
    let entry = entries.first().expect("one");
    assert_eq!(entry.name, "tiny-p0");
    assert_eq!(entry.size.text, 25248);
    assert_eq!(entry.size.data, 2000);
    assert_eq!(entry.size.bss, 6384);
    assert_eq!(entry.size.s(), 27248);
}

#[test]
fn berkeley_several_files() {
    let entries = parse_berkeley(BERKELEY_TWO).expect("parse");
    assert_eq!(entries.len(), 2);
    assert_eq!(entries.first().expect("first").size.text, 176);
    assert_eq!(entries.get(1).expect("second").name, "libc.a(divdi3.o)");
}

#[test]
fn berkeley_without_header_is_an_error() {
    assert!(parse_berkeley("  1 2 3\n").is_err());
}

#[test]
fn berkeley_bad_number_is_an_error() {
    let text = "text data bss dec hex filename\nnope 1 2 3 4 x\n";
    assert!(parse_berkeley(text).is_err());
}

const SECTIONS: &str = "\
tiny-p0  :
section               size      addr
.note.raven.abi         28   2097720
.rodata               1544   2097752
.init_array              0   2099300
.preinit_array           0   2099300
.ctors                   0   2099300
.text                25248   2099360
";

#[test]
fn section_sizes_from_real_output() {
    let sections = parse_sections(SECTIONS).expect("parse");
    assert_eq!(section_size(&sections, ".rodata"), 1544);
    assert_eq!(section_size(&sections, ".init_array"), 0);
    assert_eq!(section_size(&sections, ".missing"), 0);
}

#[test]
fn sections_ignore_a_file_label_that_starts_with_a_dot() {
    let text = "./build/x/p0.elf  :\nsection size addr\n.text 42 0\n";
    let sections = parse_sections(text).expect("parse");
    assert_eq!(sections.len(), 1);
    assert_eq!(sections.first().expect("text").size, 42);
}

#[test]
fn sections_ignore_the_header_and_the_file_label() {
    let sections = parse_sections(SECTIONS).expect("parse");
    assert_eq!(sections.len(), 6);
}

#[test]
fn sections_reject_a_bad_size() {
    assert!(parse_sections(".text nope 0\n").is_err());
}

const NM_SIZES: &str = "\
0000000000209cc8 0000000000000920 b raven_libc::ffi::platform::PLATFORM (.llvm.13736572854360460066)
0000000000202654 000000000000093e t <raven_libc_platform::platform::Platform<raven_libc_platform::backend::RealSys, raven_libc::ffi::platform::ShellLock>>::openat
000000000020a630 0000000000000c08 b raven_libc_sys::pages::REGISTRY
0000000000201a85 0000000000000216 T __raven_libc_start
";

#[test]
fn nm_demangled_and_generic_names() {
    let symbols = parse_nm_sizes(NM_SIZES).expect("parse");
    assert_eq!(symbols.len(), 4);
    let first = symbols.first().expect("first");
    assert_eq!(first.size, 0x920);
    assert_eq!(first.kind, "b");
    assert!(first.name.contains("raven_libc::ffi::platform::PLATFORM"));
    assert!(
        symbols
            .get(1)
            .expect("generic")
            .name
            .contains("Platform<raven_libc_platform::backend::RealSys")
    );
}

#[test]
fn nm_start_symbol_size() {
    let symbols = parse_nm_sizes(NM_SIZES).expect("parse");
    let start = symbols
        .iter()
        .find(|symbol| symbol.name == "__raven_libc_start")
        .expect("start");
    assert_eq!(start.size, 0x216);
    assert_eq!(start.kind, "T");
}

#[test]
fn nm_rejects_a_bad_size() {
    assert!(parse_nm_sizes("0000 nope T foo\n").is_err());
}

const DYNAMIC: &str = "\
Dynamic section at offset 0x2a18 contains 20 entries:
  Tag        Type                 Name/Value
 0x0000000000000001 (NEEDED)     Shared library: [libc.so.1]
 0x0000000000000001 (NEEDED)     Shared library: [libfoo.so.2]
 0x000000000000000c (INIT)       0x1000
";

#[test]
fn needed_counts_every_entry() {
    assert_eq!(count_needed(DYNAMIC), 2);
    assert_eq!(count_needed("Dynamic section ...\n"), 0);
}

const RELOCS: &str = "\
Relocation section '.rela.dyn' at offset 0x300 contains 4 entries:
    Offset          Info           Type           Sym. Value    Sym. Name + Addend
0000000000001000  0000000000000008 R_X86_64_RELATIVE                    1234
0000000000001008  0000000000000008 R_X86_64_RELATIVE                    1240
0000000000001010  0000000000000006 R_X86_64_GLOB_DAT      0000000000000000 environ + 0
Relocation section '.rela.plt' at offset 0x360 contains 2 entries:
0000000000001020  0000000000000007 R_X86_64_JUMP_SLOT     0000000000000000 free + 0
0000000000001028  0000000000000007 R_X86_64_JUMP_SLOT     0000000000000000 malloc + 0
";

#[test]
fn relocation_counts() {
    assert_eq!(relative_count(RELOCS), 2, "IRELATIVE does not count");
    assert_eq!(glob_dat_count(RELOCS), 1);
    assert_eq!(jump_slot_count(RELOCS), 2);
}

#[test]
fn irelative_is_not_relative() {
    let text = "R_X86_64_IRELATIVE\nR_X86_64_RELATIVE\n";
    assert_eq!(relative_count(text), 1);
}

#[test]
fn dynamic_info_combines_both_outputs() {
    let info = dynamic_info(DYNAMIC, RELOCS);
    assert_eq!(info.needed, 2);
    assert_eq!(info.jump_slot, 2);
    assert_eq!(info.glob_dat, 1);
    assert_eq!(info.relative, 2);
}

const MAP: &str = "\
LOAD build/x86_64/crt1.o
Archive member included to satisfy reference by file (symbol)

build/x86_64/libc.a(raven_libc-abc.raven_libc.cgu.017.rcgu.o) (__raven_libc_start)
                              build/x86_64/crt1.o (__raven_libc_start)
build/x86_64/libc.a(raven_libc-str-1.raven_libc_str.cgu.2.rcgu.o) (strlen)
                              build/x86_64/libc.a(raven_libc-abc.raven_libc.cgu.017.rcgu.o) (strlen)
build/x86_64/libc.a(raven_libc-str-1.raven_libc_str.cgu.2.rcgu.o) (strlen)
";

#[test]
fn map_members_are_distinct_and_sorted() {
    let members = parse_map_members(MAP);
    assert_eq!(members.len(), 2);
    assert_eq!(
        members.first().map(String::as_str),
        Some("raven_libc-abc.raven_libc.cgu.017.rcgu.o")
    );
}

#[test]
fn map_without_archive_is_empty() {
    assert!(parse_map_members("LOAD /x\n").is_empty());
}

const ARCHIVE_SIZES: &str = "\
   text\t   data\t    bss\t    dec\t    hex\tfilename
    176\t      0\t      0\t    176\t     b0\t45c91108-absvdi2.o (ex build/x86_64/libc.a)
  42975\t   1024\t    512\t  44511\t    addf\traven_libc.abc-cgu.00.rcgu.o (ex build/x86_64/libc.a)
";

#[test]
fn archive_member_names_are_stripped() {
    let members = parse_archive_sizes(ARCHIVE_SIZES).expect("parse");
    assert_eq!(members.len(), 2);
    assert_eq!(members.first().expect("first").name, "45c91108-absvdi2.o");
    assert_eq!(
        members.get(1).expect("second").name,
        "raven_libc.abc-cgu.00.rcgu.o"
    );
}

#[test]
fn parse_u64_accepts_hex_and_decimal() {
    assert_eq!(parse_u64("0x10").expect("hex"), 16);
    assert_eq!(parse_u64("16").expect("dec"), 16);
    assert!(parse_u64("0xzz").is_err());
}

#[test]
fn file_size_reads_bytes() {
    let dir = crate::testutil::TempDir::new("measure-size").expect("temp");
    let path = dir.write("f", b"12345").expect("write");
    assert_eq!(file_size(&path).expect("size"), 5);
    assert!(file_size(&path.join("missing")).is_err());
}

#[test]
fn size_output_fixture_round_trips() {
    let entries =
        parse_berkeley(&crate::testutil::size_output_named("sample", 10, 20, 30)).expect("parse");
    assert_eq!(entries.first().expect("one").size.bss, 30);
}
