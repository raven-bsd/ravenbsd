// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used
)]
// The table fixtures are built from fixed symbols and addresses.

//! Symbol lookup: SysV, GNU, the bloom filter, undefined/hidden/TLS/IFUNC rules.

use crate::testbuild as elfbuild;

use super::*;
use crate::consts::{STB_LOCAL, STT_COMMON, STT_GNU_IFUNC, STT_SECTION, STT_TLS, STV_HIDDEN};
use crate::elf::Arch;
use crate::errors::LookupError;
use crate::mem::SliceMem;
use crate::object::ObjView;
use crate::scope::ObjId;
use elfbuild::{Builder, Image, SymbolSpec};

const BIAS: u64 = 0x4000;

fn symbol(name: &[u8], info: u8, value: u64, shndx: u16, other: u8) -> SymbolSpec {
    SymbolSpec {
        name: name.to_vec(),
        info,
        other,
        shndx,
        value,
        size: 16,
        version: 1,
        hidden: false,
    }
}

fn object(symbols: &[SymbolSpec], gnu: bool) -> (Image, ObjView) {
    let mut builder = Builder::new().with_gnu_hash(gnu).with_sysv_hash(!gnu);
    for entry in symbols {
        builder = builder.symbol(entry.clone());
    }
    let image = builder.build();
    let view = ObjView::digest(&image.memory(BIAS), image.phdr_vaddr, image.phnum, BIAS).unwrap();
    (image, view)
}

fn find(view: &ObjView, image: &Image, name: &[u8]) -> Option<SymHit> {
    lookup_obj(ObjId(7), view, &image.memory(BIAS), &SymLook::new(name)).unwrap()
}

#[test]
fn a_sysv_hash_finds_a_function() {
    let (image, view) = object(&[symbol(b"alpha", (1 << 4) | 2, 0x100, 1, 0)], false);
    let hit = find(&view, &image, b"alpha").unwrap();
    assert_eq!(hit.obj, ObjId(7));
    assert_eq!(hit.sym.value, 0x100);
    assert_eq!(hit.value, BIAS + 0x100);
    assert!(!hit.is_ifunc);
    assert!(!hit.is_tls);
}

#[test]
fn a_sysv_miss_returns_none() {
    let (image, view) = object(&[symbol(b"alpha", (1 << 4) | 2, 0x100, 1, 0)], false);
    assert!(find(&view, &image, b"absent").is_none());
}

#[test]
fn a_gnu_hash_finds_a_function() {
    let (image, view) = object(&[symbol(b"alpha", (1 << 4) | 2, 0x100, 1, 0)], true);
    let hit = find(&view, &image, b"alpha").unwrap();
    assert_eq!(hit.value, BIAS + 0x100);
}

#[test]
fn a_gnu_hash_miss_returns_none() {
    let (image, view) = object(&[symbol(b"alpha", (1 << 4) | 2, 0x100, 1, 0)], true);
    assert!(find(&view, &image, b"absent").is_none());
}

#[test]
fn both_tables_present_use_the_gnu_one() {
    let mut builder = Builder::new().with_gnu_hash(true).with_sysv_hash(true);
    builder = builder.symbol(symbol(b"alpha", (1 << 4) | 2, 0x100, 1, 0));
    let image = builder.build();
    let mut view =
        ObjView::digest(&image.memory(BIAS), image.phdr_vaddr, image.phnum, BIAS).unwrap();
    assert!(view.hash_gnu.is_some() && view.hash_sysv.is_some());
    // Make the SysV table unusable: a lookup that still succeeds proves GNU was chosen.
    view.hash_sysv = Some(SysvHash {
        nbucket: 1,
        nchain: 1,
        addr: 0x0090_0000,
    });
    assert!(find(&view, &image, b"alpha").is_some());
    view.hash_gnu = None;
    assert!(matches!(
        lookup_obj(
            ObjId(0),
            &view,
            &image.memory(BIAS),
            &SymLook::new(b"alpha")
        ),
        Err(LookupError::BadTable)
    ));
}

#[test]
fn a_bloom_miss_returns_before_the_chain() {
    let (image, view) = object(&[symbol(b"alpha", (1 << 4) | 2, 0x100, 1, 0)], true);
    let mut bytes = image.bytes.clone();
    let bloom = view.hash_gnu.unwrap().bloom;
    let at = (bloom - BIAS) as usize;
    bytes[at..at + 8].copy_from_slice(&0u64.to_le_bytes());
    let memory = SliceMem::new(BIAS, &bytes);
    assert!(
        lookup_obj(ObjId(0), &view, &memory, &SymLook::new(b"alpha"))
            .unwrap()
            .is_none()
    );
}

#[test]
fn a_sysv_chain_loop_is_reported() {
    let (image, view) = object(&[symbol(b"alpha", (1 << 4) | 2, 0x100, 1, 0)], false);
    let table = view.hash_sysv.unwrap();
    let mut bytes = image.bytes.clone();
    // Point the bucket for "zzz" at symbol 1 and make chain[1] point back to itself.
    let bucket_index = elf_hash(b"zzz") % table.nbucket;
    let bucket_at = ((table.addr - BIAS) as usize) + 8 + (bucket_index as usize) * 4;
    bytes[bucket_at..bucket_at + 4].copy_from_slice(&1u32.to_le_bytes());
    let chains_at = ((table.addr - BIAS) as usize) + 8 + (table.nbucket as usize) * 4;
    bytes[chains_at + 4..chains_at + 8].copy_from_slice(&1u32.to_le_bytes());
    let memory = SliceMem::new(BIAS, &bytes);
    assert!(matches!(
        lookup_obj(ObjId(0), &view, &memory, &SymLook::new(b"zzz")),
        Err(LookupError::ChainLoop)
    ));
}

#[test]
fn a_sysv_bucket_value_beyond_the_chains_is_bad_table() {
    let (image, view) = object(&[symbol(b"alpha", (1 << 4) | 2, 0x100, 1, 0)], false);
    let table = view.hash_sysv.unwrap();
    let mut bytes = image.bytes.clone();
    let bucket_index = elf_hash(b"alpha") % table.nbucket;
    let at = ((table.addr - BIAS) as usize) + 8 + (bucket_index as usize) * 4;
    bytes[at..at + 4].copy_from_slice(&99u32.to_le_bytes());
    let memory = SliceMem::new(BIAS, &bytes);
    assert!(matches!(
        lookup_obj(ObjId(0), &view, &memory, &SymLook::new(b"alpha")),
        Err(LookupError::BadTable)
    ));
}

#[test]
fn a_zero_bloom_size_is_bad_table() {
    let (image, view) = object(&[symbol(b"alpha", (1 << 4) | 2, 0x100, 1, 0)], true);
    let mut view = view;
    if let Some(table) = view.hash_gnu.as_mut() {
        table.bloom_size = 0;
    }
    assert!(matches!(
        lookup_obj(
            ObjId(0),
            &view,
            &image.memory(BIAS),
            &SymLook::new(b"alpha")
        ),
        Err(LookupError::BadTable)
    ));
}

#[test]
fn a_non_power_of_two_bloom_size_is_bad_table() {
    let (image, view) = object(&[symbol(b"alpha", (1 << 4) | 2, 0x100, 1, 0)], true);
    let mut view = view;
    if let Some(table) = view.hash_gnu.as_mut() {
        table.bloom_size = 3;
    }
    assert!(matches!(
        lookup_obj(
            ObjId(0),
            &view,
            &image.memory(BIAS),
            &SymLook::new(b"alpha")
        ),
        Err(LookupError::BadTable)
    ));
}

#[test]
fn a_local_symbol_is_not_a_definition() {
    let (image, view) = object(
        &[symbol(b"local", (STB_LOCAL << 4) | 2, 0x100, 1, 0)],
        false,
    );
    assert!(find(&view, &image, b"local").is_none());
}

#[test]
fn an_undefined_function_with_a_value_needs_the_plt_flag() {
    let (image, view) = object(&[symbol(b"undef", (1 << 4) | 2, 0x200, 0, 0)], false);
    assert!(find(&view, &image, b"undef").is_none());
    let hit = lookup_obj(
        ObjId(0),
        &view,
        &image.memory(BIAS),
        &SymLook::new(b"undef").with_flags(SymFlags::IN_PLT),
    )
    .unwrap();
    assert!(hit.is_some());
}

#[test]
fn a_hidden_definition_is_visible_only_to_its_own_object() {
    let (image, view) = object(
        &[symbol(b"hidden", (1 << 4) | 2, 0x100, 1, STV_HIDDEN)],
        false,
    );
    assert!(find(&view, &image, b"hidden").is_none());
    let own = lookup_obj(
        ObjId(0),
        &view,
        &image.memory(BIAS),
        &SymLook::new(b"hidden").with_flags(SymFlags::SELF),
    )
    .unwrap();
    assert!(own.is_some());
}

#[test]
fn a_tls_symbol_is_skipped_with_no_tls() {
    let (image, view) = object(&[symbol(b"tls", (1 << 4) | STT_TLS, 0x18, 1, 0)], false);
    let hit = find(&view, &image, b"tls").unwrap();
    assert!(hit.is_tls);
    assert_eq!(hit.value, 0x18, "TLS values are not biased");
    assert!(
        lookup_obj(
            ObjId(0),
            &view,
            &image.memory(BIAS),
            &SymLook::new(b"tls").with_flags(SymFlags::NO_TLS),
        )
        .unwrap()
        .is_none()
    );
}

#[test]
fn an_ifunc_definition_is_flagged() {
    let (image, view) = object(
        &[symbol(b"ifunc", (1 << 4) | STT_GNU_IFUNC, 0x100, 1, 0)],
        false,
    );
    let hit = find(&view, &image, b"ifunc").unwrap();
    assert!(hit.is_ifunc);
    assert_eq!(hit.value, BIAS + 0x100);
}

#[test]
fn a_common_symbol_is_a_definition() {
    let (image, view) = object(
        &[symbol(b"common", (1 << 4) | STT_COMMON, 0x100, 0xfff2, 0)],
        false,
    );
    assert!(find(&view, &image, b"common").is_some());
}

#[test]
fn a_zero_valued_symbol_is_not_a_definition() {
    let (image, view) = object(&[symbol(b"zero", (1 << 4) | 2, 0, 1, 0)], false);
    assert!(find(&view, &image, b"zero").is_none());
}

#[test]
fn a_section_symbol_is_skipped() {
    let (image, view) = object(
        &[symbol(b"section", (1 << 4) | STT_SECTION, 0x100, 1, 0)],
        false,
    );
    assert!(find(&view, &image, b"section").is_none());
}

#[test]
fn a_symbol_index_outside_the_table_is_bad_symbol() {
    let (image, view) = object(&[symbol(b"alpha", (1 << 4) | 2, 0x100, 1, 0)], false);
    let mut view = view;
    view.dynsymcount = 1;
    let table = view.hash_sysv.unwrap();
    let mut bytes = image.bytes.clone();
    let bucket_index = elf_hash(b"alpha") % table.nbucket;
    let at = ((table.addr - BIAS) as usize) + 8 + (bucket_index as usize) * 4;
    bytes[at..at + 4].copy_from_slice(&2u32.to_le_bytes());
    let memory = SliceMem::new(BIAS, &bytes);
    assert!(matches!(
        lookup_obj(ObjId(0), &view, &memory, &SymLook::new(b"alpha")),
        Err(LookupError::BadTable) | Err(LookupError::BadSymbol)
    ));
}

#[test]
fn an_object_without_a_hash_table_is_bad_table() {
    let (image, view) = object(&[symbol(b"alpha", (1 << 4) | 2, 0x100, 1, 0)], false);
    let mut view = view;
    view.hash_sysv = None;
    assert!(matches!(
        lookup_obj(
            ObjId(0),
            &view,
            &image.memory(BIAS),
            &SymLook::new(b"alpha")
        ),
        Err(LookupError::BadTable)
    ));
}

#[test]
fn matched_symbol_accepts_a_matching_definition_and_rejects_another_name() {
    let (image, view) = object(&[symbol(b"alpha", (1 << 4) | 2, 0x100, 1, 0)], false);
    let memory = image.memory(BIAS);
    assert!(matched_symbol(&SymLook::new(b"alpha"), &view, &memory, 1).unwrap());
    assert!(!matched_symbol(&SymLook::new(b"beta"), &view, &memory, 1).unwrap());
    assert!(!matched_symbol(&SymLook::new(b"alpha"), &view, &memory, 0).unwrap());
}

#[test]
fn the_request_computes_both_hashes() {
    let request = SymLook::new(b"libc.so.1");
    assert_eq!(request.hash_sysv, elf_hash(b"libc.so.1"));
    assert_eq!(request.hash_gnu, gnu_hash(b"libc.so.1"));
    assert_eq!(request.ver, None);
}

#[test]
fn the_flags_union_and_contains() {
    let flags = SymFlags::IN_PLT.union(SymFlags::NO_TLS);
    assert!(flags.contains(SymFlags::IN_PLT));
    assert!(flags.contains(SymFlags::NO_TLS));
    assert!(!flags.contains(SymFlags::IFUNC));
    assert!(SymFlags::default().0 == 0);
    assert!(SymFlags::IN_PLT.contains(SymFlags::IN_PLT));
    assert!(flags.0 == SymFlags::IN_PLT.0 | SymFlags::NO_TLS.0);
}

#[test]
fn the_sysv_table_size_is_header_plus_buckets_and_chains() {
    assert_eq!(sysv_hash_bytes(2, 3), Some(8 + 5 * 4));
    assert_eq!(sysv_hash_bytes(0, 0), Some(8));
    assert_eq!(sysv_hash_bytes(u32::MAX, u32::MAX), Some(34_359_738_368));
}

#[test]
fn a_gnu_bucket_points_at_the_first_chain_word() {
    let (image, view) = object(&[symbol(b"alpha", (1 << 4) | 2, 0x100, 1, 0)], true);
    let table = view.hash_gnu.unwrap();
    let bucket_index = gnu_hash(b"alpha") % table.nbuckets;
    let memory = image.memory(BIAS);
    // The bucket value is the first symbol index of the chain.
    let raw = memory
        .read(table.buckets + u64::from(bucket_index) * 4, 4)
        .unwrap();
    assert_eq!(u32::from_le_bytes(raw.try_into().unwrap()), 1);
    assert!(find(&view, &image, b"alpha").is_some());
}

#[test]
fn a_lookup_result_carries_the_defining_object() {
    let (image, view) = object(&[symbol(b"alpha", (1 << 4) | 2, 0x100, 1, 0)], false);
    let hit = find(&view, &image, b"alpha").unwrap();
    assert_eq!(hit.obj, ObjId(7));
    assert_eq!(hit.sym.typ(), 2);
}

#[test]
fn an_arch_other_than_the_object_does_not_change_lookup() {
    // `lookup_obj` has no architecture input; this pins the absence of a hidden parameter.
    let (image, view) = object(&[symbol(b"alpha", (1 << 4) | 2, 0x100, 1, 0)], true);
    for arch in [Arch::X86_64, Arch::Aarch64, Arch::Riscv64] {
        let _ = arch.name();
        assert!(find(&view, &image, b"alpha").is_some());
    }
}

#[test]
fn a_definition_in_the_second_bucket_is_found() {
    // Two symbols with different GNU buckets.
    let (image, view) = object(
        &[
            symbol(b"first", (1 << 4) | 2, 0x100, 1, 0),
            symbol(b"second", (1 << 4) | 2, 0x200, 1, 0),
        ],
        true,
    );
    assert_eq!(find(&view, &image, b"first").unwrap().sym.value, 0x100);
    assert_eq!(find(&view, &image, b"second").unwrap().sym.value, 0x200);
}

#[test]
fn two_names_with_the_same_bucket_are_both_found() {
    let (image, view) = object(
        &[
            symbol(b"aa", (1 << 4) | 2, 0x100, 1, 0),
            symbol(b"ba", (1 << 4) | 2, 0x200, 1, 0),
        ],
        true,
    );
    assert!(find(&view, &image, b"aa").is_some());
    assert!(find(&view, &image, b"ba").is_some());
    assert!(find(&view, &image, b"ca").is_none());
}
