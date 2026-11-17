// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the ar archive model.

use super::{
    ArError, Member, Operation, UsageError, elf_symbols, parse_archive, parse_args, write_archive,
};

fn member(name: &str, data: &[u8], mode: u32) -> Member {
    Member {
        name: name.into(),
        mtime: 123,
        uid: 1,
        gid: 2,
        mode,
        data: data.to_vec(),
    }
}

#[test]
fn parses_replace_list_extract_and_symbols() {
    let options = parse_args(["r", "lib.a", "a.o", "b.o"]).unwrap();
    assert_eq!(options.operation, Operation::Replace);
    assert_eq!(options.archive, "lib.a");
    assert_eq!(options.operands, vec!["a.o", "b.o"]);
    assert_eq!(
        parse_args(["-t", "lib.a"]).unwrap().operation,
        Operation::List
    );
    assert_eq!(
        parse_args(["-x", "lib.a", "a.o"]).unwrap().operation,
        Operation::Extract
    );
    assert_eq!(
        parse_args(["-sv", "lib.a"]).unwrap().operation,
        Operation::Symbols
    );
    assert_eq!(parse_args(["d", "lib.a"]), Err(UsageError::UnknownOption));
    assert_eq!(parse_args(["r", "lib.a"]), Err(UsageError::MissingMembers));
}

#[test]
fn short_and_long_members_round_trip() {
    let members = vec![
        member("a.o", b"hello", 0o644),
        member("a-very-long-member-name.o", b"world", 0o600),
    ];
    let archive = write_archive(&members, false);
    assert!(archive.starts_with(b"!<arch>\n"));
    let parsed = parse_archive(&archive).unwrap();
    assert_eq!(parsed, members);
}

#[test]
fn a_missing_magic_is_rejected() {
    assert_eq!(parse_archive(b"not an archive"), Err(ArError::BadMagic));
    assert_eq!(parse_archive(b"!<arch>\n"), Ok(Vec::new()));
}

#[test]
fn symbol_table_is_generated_for_an_elf_member() {
    let object = elf_object_with_symbol("worker");
    assert_eq!(elf_symbols(&object), vec!["worker"]);
    let members = vec![member("worker.o", &object, 0o644)];
    let archive = write_archive(&members, false);
    assert_eq!(archive.get(8), Some(&b'/'));
    let parsed = parse_archive(&archive).unwrap();
    assert_eq!(parsed, members);
}

#[test]
fn symbol_offsets_account_for_long_names() {
    let object = elf_object_with_symbol("worker");
    let archive = write_archive(&[member("worker-long-name.o", &object, 0o644)], false);
    assert_eq!(&archive[68..72], &1u32.to_be_bytes());
    assert_eq!(&archive[72..76], &84u32.to_be_bytes());
}

#[test]
fn empty_symbol_table_can_be_forced() {
    let archive = write_archive(&[member("a.o", b"x", 0o644)], true);
    assert_eq!(archive.get(8), Some(&b'/'));
    assert_eq!(parse_archive(&archive).unwrap().len(), 1);
}

fn elf_object_with_symbol(name: &str) -> Vec<u8> {
    let strtab_offset = 280usize;
    let mut data = vec![0u8; strtab_offset + name.len() + 1];
    data[..4].copy_from_slice(b"\x7fELF");
    data[4] = 2;
    data[5] = 1;
    data[6] = 1;
    data[16] = 1;
    data[18] = 0x3e;
    data[20] = 1;
    data[40] = 64;
    data[52] = 64;
    data[58] = 64;
    data[60] = 3;
    data[62] = 0;
    let symtab = 64 + 64;
    data[symtab + 4..symtab + 8].copy_from_slice(&2u32.to_le_bytes());
    data[symtab + 24..symtab + 32].copy_from_slice(&256u64.to_le_bytes());
    data[symtab + 32..symtab + 40].copy_from_slice(&24u64.to_le_bytes());
    data[symtab + 40..symtab + 44].copy_from_slice(&2u32.to_le_bytes());
    data[symtab + 56..symtab + 64].copy_from_slice(&24u64.to_le_bytes());
    let strtab = 64 + 2 * 64;
    data[strtab + 4..strtab + 8].copy_from_slice(&3u32.to_le_bytes());
    data[strtab + 24..strtab + 32].copy_from_slice(&(strtab_offset as u64).to_le_bytes());
    data[strtab + 32..strtab + 40].copy_from_slice(&((name.len() + 1) as u64).to_le_bytes());
    let symbol = 256;
    data[symbol..symbol + 4].copy_from_slice(&0u32.to_le_bytes());
    data[symbol + 4] = 0x12;
    data[symbol + 6..symbol + 8].copy_from_slice(&1u16.to_le_bytes());
    data[strtab_offset..strtab_offset + name.len()].copy_from_slice(name.as_bytes());
    data
}
