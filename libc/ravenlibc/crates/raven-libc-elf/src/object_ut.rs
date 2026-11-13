// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used
)]
// Image construction and patching are over small layout literals.

//! Digest behaviour: every field, every hostile range, every documented error.

use crate::testbuild as elfbuild;

use super::*;
use crate::consts::{DF_TEXTREL, DT_REL, DT_TEXTREL, PT_NOTE};
use crate::mem::SliceMem;
use elfbuild::{Builder, DATA_BASE, Image, SymbolSpec};

fn minimal() -> Image {
    Builder::new()
        .symbol(SymbolSpec {
            name: b"answer".to_vec(),
            info: (1 << 4) | 1,
            other: 0,
            shndx: 1,
            value: 0x1234,
            size: 8,
            version: 1,
            hidden: false,
        })
        .soname(b"libanswer.so.1")
        .needed(b"libc.so.1")
        .build()
}

fn digest(image: &Image, bias: u64) -> Result<ObjView, DigestError> {
    ObjView::digest(&image.memory(bias), image.phdr_vaddr, image.phnum, bias)
}

fn dynamic_entry_offset(image: &Image, tag: i64) -> Option<usize> {
    let base = DATA_BASE as usize + image.layout.dynamic as usize;
    for index in 0..256usize {
        let at = base + index * 16;
        let tag_bytes: [u8; 8] = image.bytes.get(at..at + 8)?.try_into().ok()?;
        let found = i64::from_le_bytes(tag_bytes);
        if found == tag {
            return Some(at + 8);
        }
        if found == 0 {
            return None;
        }
    }
    None
}

fn patch_dyn(image: &mut Image, tag: i64, value: u64) -> Option<()> {
    let at = dynamic_entry_offset(image, tag)?;
    image.bytes[at..at + 8].copy_from_slice(&value.to_le_bytes());
    Some(())
}

fn patch_dyn_tag(image: &mut Image, tag: i64, new_tag: i64) -> Option<()> {
    let value_at = dynamic_entry_offset(image, tag)?;
    let tag_at = value_at - 8;
    image.bytes[tag_at..value_at].copy_from_slice(&new_tag.to_le_bytes());
    Some(())
}

#[test]
fn a_builder_image_digests_to_its_input_fields() {
    let image = minimal();
    let view = digest(&image, 0).unwrap();
    assert_eq!(view.bias, 0);
    assert_eq!(view.phdr, (64, image.phnum));
    assert_eq!(view.dynamic, DATA_BASE + image.layout.dynamic);
    assert_eq!(view.strtab, DATA_BASE + image.layout.dynstr);
    assert_eq!(view.symtab, DATA_BASE + image.layout.dynsym);
    assert_eq!(view.needed.len(), 1);
    let memory = image.memory(0);
    let text = view.name_at(&memory, view.needed[0]).unwrap();
    assert_eq!(text, b"libc.so.1");
    assert_eq!(
        view.name_at(&memory, view.soname.unwrap()).unwrap(),
        b"libanswer.so.1"
    );
    assert_eq!(view.names.len(), 1);
    assert_eq!(view.dynsymcount, 2);
    assert_eq!(view.segments.len(), 2);
    assert_eq!(view.segments[0].vaddr, 0);
    assert_eq!(view.segments[1].vaddr, DATA_BASE);
    assert!(view.hash_sysv.is_some());
    assert!(view.hash_gnu.is_none());
    assert!(!view.gnu_stack_exec);
}

#[test]
fn the_bias_is_added_exactly_once() {
    let image = minimal();
    let low = digest(&image, 0).unwrap();
    let bias = 0x7000_0000u64;
    let high = digest(&image, bias).unwrap();
    assert_eq!(high.bias, bias);
    assert_eq!(high.strtab - low.strtab, bias);
    assert_eq!(high.symtab - low.symtab, bias);
    assert_eq!(high.dynamic - low.dynamic, bias);
    assert_eq!(high.phdr.0 - low.phdr.0, bias);
    assert_eq!(high.segments[0].vaddr - low.segments[0].vaddr, bias);
    let memory = image.memory(bias);
    let name = high.name_at(&memory, high.needed[0]).unwrap();
    assert_eq!(name, b"libc.so.1");
}

#[test]
fn a_static_object_has_no_dynamic_section() {
    let mut image = minimal();
    // Turn PT_DYNAMIC into a PT_NOTE: the image becomes static.
    let phdr = 64usize;
    for index in 0..usize::from(image.phnum) {
        let at = phdr + index * 56;
        let kind = u32::from_le_bytes(image.bytes[at..at + 4].try_into().unwrap());
        if kind == PT_DYNAMIC {
            image.bytes[at..at + 4].copy_from_slice(&PT_NOTE.to_le_bytes());
        }
    }
    assert_eq!(digest(&image, 0), Err(DigestError::NotDynamic));
}

#[test]
fn a_phdr_address_outside_the_view_is_a_bad_dynamic() {
    let image = minimal();
    assert_eq!(
        ObjView::digest(&image.memory(0), u64::MAX, 3, 0),
        Err(DigestError::BadDynamic)
    );
}

#[test]
fn a_missing_strtab_or_symtab_is_no_symtab() {
    let mut image = minimal();
    patch_dyn(&mut image, DT_STRTAB, 0).unwrap();
    assert_eq!(digest(&image, 0), Err(DigestError::NoSymtab));

    let mut image = minimal();
    patch_dyn(&mut image, DT_SYMTAB, 0).unwrap();
    assert_eq!(digest(&image, 0), Err(DigestError::NoSymtab));
}

#[test]
fn a_missing_hash_table_is_no_hash() {
    let mut image = minimal();
    patch_dyn_tag(&mut image, DT_HASH, 0x7fff_ffff).unwrap();
    assert_eq!(digest(&image, 0), Err(DigestError::NoHash));
}

#[test]
fn each_dynamic_range_outside_a_load_segment_names_its_tag() {
    let mut image = minimal();
    patch_dyn(&mut image, DT_STRTAB, 0x8000_0000).unwrap();
    assert_eq!(
        digest(&image, 0),
        Err(DigestError::RangeOutsideLoad("DT_STRTAB"))
    );

    let mut image = minimal();
    patch_dyn(&mut image, DT_SYMTAB, 0x8000_0000).unwrap();
    assert_eq!(
        digest(&image, 0),
        Err(DigestError::RangeOutsideLoad("DT_SYMTAB"))
    );

    let mut image = minimal();
    patch_dyn(&mut image, DT_HASH, 0x8000_0000).unwrap();
    assert_eq!(
        digest(&image, 0),
        Err(DigestError::RangeOutsideLoad("DT_HASH"))
    );

    let mut image = Builder::new()
        .symbol(elfbuild::global_func(b"f", 0x100))
        .rela(0x1000, 0, 8, 0)
        .build();
    patch_dyn(&mut image, DT_RELA, 0x8000_0000).unwrap();
    assert_eq!(
        digest(&image, 0),
        Err(DigestError::RangeOutsideLoad("DT_RELA"))
    );

    let mut image = Builder::new()
        .with_sysv_hash(false)
        .with_gnu_hash(true)
        .symbol(elfbuild::global_func(b"f", 0x100))
        .build();
    patch_dyn(&mut image, crate::consts::DT_GNU_HASH, 0x8000_0000).unwrap();
    assert_eq!(
        digest(&image, 0),
        Err(DigestError::RangeOutsideLoad("DT_GNU_HASH"))
    );
}

#[test]
fn dt_rel_is_rejected() {
    let mut image = minimal();
    patch_dyn_tag(&mut image, DT_HASH, DT_REL).unwrap();
    assert_eq!(digest(&image, 0), Err(DigestError::RelNotSupported));
}

#[test]
fn text_relocations_are_rejected_through_both_tags() {
    let image = Builder::new()
        .symbol(elfbuild::global_func(b"f", 0x100))
        .extra_dyn(DT_TEXTREL, 0)
        .build();
    assert_eq!(digest(&image, 0), Err(DigestError::TextRel));

    let image = Builder::new()
        .symbol(elfbuild::global_func(b"f", 0x100))
        .flags(DF_TEXTREL as u32)
        .build();
    assert_eq!(digest(&image, 0), Err(DigestError::TextRel));
}

#[test]
fn a_rel_plt_relocation_type_is_rejected() {
    let image = Builder::new()
        .symbol(elfbuild::global_func(b"f", 0x100))
        .plt_rela(0x1000, 1, 7, 0)
        .build();
    // Rebuild with DT_PLTREL patched to DT_REL.
    let mut image = image;
    patch_dyn(&mut image, crate::consts::DT_PLTREL, DT_REL as u64).unwrap();
    assert_eq!(digest(&image, 0), Err(DigestError::RelNotSupported));
}

#[test]
fn a_dynamic_array_without_a_null_is_bad_dynamic() {
    let mut image = minimal();
    // Find the first DT_NULL and replace the tag of every entry from there on: the array now
    // has no terminator and the walker must stop at MAX_DYN_ENTRIES.
    let base = DATA_BASE as usize + image.layout.dynamic as usize;
    for index in 0..512usize {
        let at = base + index * 16;
        let tag = i64::from_le_bytes(image.bytes[at..at + 8].try_into().unwrap());
        if tag == crate::consts::DT_NULL {
            for fill in index..512usize {
                let at = base + fill * 16;
                image.bytes[at..at + 8].copy_from_slice(&0x7fff_ffffi64.to_le_bytes());
            }
            break;
        }
    }
    assert_eq!(digest(&image, 0), Err(DigestError::BadDynamic));
}

#[test]
fn too_many_needed_entries_are_rejected() {
    let mut builder = Builder::new().symbol(elfbuild::global_func(b"f", 0x100));
    for index in 0..257u16 {
        let name = format!("lib{index}.so.1");
        builder = builder.needed(name.as_bytes());
    }
    let image = builder.build();
    assert_eq!(digest(&image, 0), Err(DigestError::TooManyNeeded));
}

#[test]
fn a_string_offset_past_dt_strsz_is_bad_string() {
    let mut image = minimal();
    patch_dyn(&mut image, crate::consts::DT_STRSZ, 1).unwrap();
    assert_eq!(digest(&image, 0), Err(DigestError::BadString));
}

#[test]
fn an_executable_stack_is_recorded() {
    let image = Builder::new()
        .symbol(elfbuild::global_func(b"f", 0x100))
        .stack_exec()
        .build();
    let view = digest(&image, 0).unwrap();
    assert!(view.gnu_stack_exec);
}

#[test]
fn the_tls_template_and_eh_frame_are_copied() {
    let image = Builder::new()
        .symbol(elfbuild::global_func(b"f", 0x100))
        .tls(&[1, 2, 3, 4], 16, 8)
        .eh_frame(0x40, 24)
        .build();
    let view = digest(&image, 0).unwrap();
    let tls = view.tls.unwrap();
    assert_eq!(tls.filesz, 4);
    assert_eq!(tls.memsz, 16);
    assert_eq!(tls.align, 8);
    assert_eq!(tls.vaddr, DATA_BASE + image.layout.tls);
    assert_eq!(view.eh_frame, Some((DATA_BASE + image.layout.eh_frame, 24)));
}

#[test]
fn the_relro_end_is_page_rounded_down() {
    let image = Builder::new()
        .symbol(elfbuild::global_func(b"f", 0x100))
        .relro(0x100, 0x234)
        .build();
    let view = digest(&image, 0).unwrap();
    let (start, end) = view.relro.unwrap();
    assert_eq!(start, DATA_BASE + 0x100);
    // 0x1000 + 0x100 + 0x234 = 0x1334 -> 0x1000
    assert_eq!(end, DATA_BASE);
}

#[test]
fn rpath_is_recorded_only_when_runpath_is_absent() {
    let image = Builder::new()
        .symbol(elfbuild::global_func(b"f", 0x100))
        .rpath(b"/old")
        .build();
    let view = digest(&image, 0).unwrap();
    assert!(view.rpath.is_some());
    assert!(view.runpath.is_none());

    let image = Builder::new()
        .symbol(elfbuild::global_func(b"f", 0x100))
        .rpath(b"/old")
        .runpath(b"/new")
        .build();
    let view = digest(&image, 0).unwrap();
    assert!(view.rpath.is_none());
    let runpath = view.runpath.unwrap();
    assert_eq!(view.name_at(&image.memory(0), runpath).unwrap(), b"/new");
}

#[test]
fn symbolic_and_bind_now_are_recorded_from_tags_and_flags() {
    let image = Builder::new()
        .symbol(elfbuild::global_func(b"f", 0x100))
        .symbolic()
        .bind_now()
        .build();
    let view = digest(&image, 0).unwrap();
    assert!(view.symbolic);
    assert!(view.bind_now);

    let image = Builder::new()
        .symbol(elfbuild::global_func(b"f", 0x100))
        .flags(crate::consts::DF_SYMBOLIC as u32 | crate::consts::DF_BIND_NOW as u32)
        .build();
    let view = digest(&image, 0).unwrap();
    assert!(view.symbolic);
    assert!(view.bind_now);
}

#[test]
fn the_debug_slot_points_at_its_own_dynamic_entry() {
    let image = Builder::new()
        .symbol(elfbuild::global_func(b"f", 0x100))
        .with_debug_slot()
        .build();
    let view = digest(&image, 0).unwrap();
    let slot = view.debug_slot.unwrap();
    assert!(slot >= view.dynamic && slot < view.dynamic + 64 * 16);
    let entry_address = slot - 8;
    let memory = image.memory(0);
    let bytes = memory.read(entry_address, 8).unwrap();
    let tag = i64::from_le_bytes(bytes.try_into().unwrap());
    assert_eq!(tag, crate::consts::DT_DEBUG);
}

#[test]
fn a_gnu_only_object_digests_and_counts_its_symbols() {
    let image = Builder::new()
        .with_sysv_hash(false)
        .with_gnu_hash(true)
        .symbol(elfbuild::global_func(b"alpha", 0x100))
        .symbol(elfbuild::global_func(b"beta", 0x200))
        .symbol(elfbuild::global_func(b"gamma", 0x300))
        .build();
    let view = digest(&image, 0).unwrap();
    assert!(view.hash_sysv.is_none());
    assert!(view.hash_gnu.is_some());
    assert_eq!(view.dynsymcount, 4);
    let request = crate::lookup::SymLook::new(b"beta");
    let hit = crate::lookup::lookup_obj(crate::scope::ObjId(0), &view, &image.memory(0), &request)
        .unwrap()
        .unwrap();
    assert_eq!(hit.sym.value, 0x200);
}

#[test]
fn the_init_and_fini_arrays_are_recorded() {
    let image = Builder::new()
        .symbol(elfbuild::global_func(b"f", 0x100))
        .init(0x100)
        .fini(0x200)
        .init_array(&[0x300, 0x400])
        .fini_array(&[0x500])
        .preinit_array(&[0x600])
        .build();
    let view = digest(&image, 0).unwrap();
    assert_eq!(view.init, 0x100);
    assert_eq!(view.fini, 0x200);
    assert_eq!(view.init_array, (DATA_BASE + image.layout.init_array, 16));
    assert_eq!(view.fini_array, (DATA_BASE + image.layout.fini_array, 8));
    assert_eq!(
        view.preinit_array,
        (DATA_BASE + image.layout.preinit_array, 8)
    );
}

#[test]
fn the_rela_plt_and_relr_ranges_are_recorded() {
    let image = Builder::new()
        .symbol(elfbuild::global_func(b"f", 0x100))
        .rela(0x1000, 0, 8, 0)
        .plt_rela(0x1008, 1, 7, 0)
        .relr(0x1000)
        .build();
    let view = digest(&image, 0).unwrap();
    assert_eq!(view.rela, (DATA_BASE + image.layout.rela, 24));
    assert_eq!(view.plt_rela, (DATA_BASE + image.layout.plt_rela, 24));
    assert_eq!(view.relr, (DATA_BASE + image.layout.relr, 8));
}

#[test]
fn digesting_twice_gives_the_same_view() {
    let image = minimal();
    let first = digest(&image, 0x1234_0000).unwrap();
    let second = digest(&image, 0x1234_0000).unwrap();
    assert_eq!(first, second);
}

#[test]
fn a_name_offset_outside_the_table_is_rejected_by_name_at() {
    let image = minimal();
    let view = digest(&image, 0).unwrap();
    assert!(view.name_at(&image.memory(0), u32::MAX).is_err());
}

#[test]
fn a_symbol_index_outside_the_table_is_none() {
    let image = minimal();
    let view = digest(&image, 0).unwrap();
    assert!(view.sym(&image.memory(0), view.dynsymcount).is_none());
    assert!(view.sym(&image.memory(0), 1).is_some());
}

#[test]
fn filesz_greater_than_memsz_is_rejected() {
    let mut image = minimal();
    // Patch the second PT_LOAD's p_filesz to exceed p_memsz.
    let phdr = 64usize;
    for index in 0..usize::from(image.phnum) {
        let at = phdr + index * 56;
        let kind = u32::from_le_bytes(image.bytes[at..at + 4].try_into().unwrap());
        let offset = u64::from_le_bytes(image.bytes[at + 8..at + 16].try_into().unwrap());
        if kind == 1 && offset != 0 {
            image.bytes[at + 32..at + 40].copy_from_slice(&1_000_000u64.to_le_bytes());
            image.bytes[at + 40..at + 48].copy_from_slice(&8u64.to_le_bytes());
        }
    }
    assert_eq!(digest(&image, 0), Err(DigestError::BadSize));
}

#[test]
fn a_dynamic_table_with_an_empty_sysv_hash_has_no_hash() {
    let mut image = minimal();
    let hash = DATA_BASE + image.layout.hash;
    let bytes = image.bytes.as_mut_slice();
    let at = hash as usize;
    bytes[at..at + 4].copy_from_slice(&0u32.to_le_bytes());
    bytes[at + 4..at + 8].copy_from_slice(&0u32.to_le_bytes());
    assert_eq!(digest(&image, 0), Err(DigestError::NoHash));
}

#[test]
fn a_view_is_a_memset_of_the_two_segments() {
    let image = minimal();
    let mut set = crate::mem::MemSet::new();
    let (rx, rest) = image.bytes.split_at(elfbuild::RX_SIZE);
    set.push(SliceMem::new(0, rx));
    set.push(SliceMem::new(DATA_BASE, rest));
    let view = ObjView::digest(&set, image.phdr_vaddr, image.phnum, 0).unwrap();
    assert_eq!(view.dynsymcount, 2);
}

#[test]
fn a_bad_rela_entry_size_is_rejected() {
    let mut image = Builder::new()
        .symbol(elfbuild::global_func(b"f", 0x100))
        .rela(0x1000, 0, 8, 0)
        .build();
    patch_dyn(&mut image, crate::consts::DT_RELAENT, 32).unwrap();
    assert_eq!(digest(&image, 0), Err(DigestError::BadSize));
}

#[test]
fn a_bad_symtab_entry_size_is_rejected() {
    let mut image = minimal();
    patch_dyn(&mut image, crate::consts::DT_SYMENT, 16).unwrap();
    assert_eq!(digest(&image, 0), Err(DigestError::BadSize));
}

#[test]
fn flags_one_records_nodeflib_and_origin() {
    let image = Builder::new()
        .symbol(elfbuild::global_func(b"f", 0x100))
        .flags1((crate::consts::DF_1_NODEFLIB as u32) | (crate::consts::DF_1_ORIGIN as u32))
        .build();
    let view = digest(&image, 0).unwrap();
    assert!(view.nodeflib());
    assert!(view.has_origin());
    assert!(!view.bind_now);
}
