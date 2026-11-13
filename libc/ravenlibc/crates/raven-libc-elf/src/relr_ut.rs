// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used
)]
// The vectors are written as hand-computed constants; a mistake fails the test.

//! `DT_RELR` vectors.

use crate::testbuild as elfbuild;

use super::*;
use crate::errors::RelocError;
use crate::mem::SliceMem;
use elfbuild::Builder;

fn offsets(entries: &[u64]) -> Vec<u64> {
    let mut builder = Builder::new().symbol(elfbuild::global_func(b"f", 0x100));
    for entry in entries {
        builder = builder.relr(*entry);
    }
    let image = builder.build();
    let view = ObjView::digest(&image.memory(0), image.phdr_vaddr, image.phnum, 0).unwrap();
    let memory = image.memory(0);
    relr_offsets(&view, &memory)
        .unwrap()
        .collect::<Result<Vec<u64>, RelocError>>()
        .unwrap()
}

#[test]
fn one_address_entry_yields_itself() {
    assert_eq!(offsets(&[0x1500]), vec![0x1500]);
}

#[test]
fn a_bitmap_yields_the_following_words() {
    // Address 0x1400, then bits 1, 2 and 63: words 0, 1 and 62 after it.
    let bitmap = 1u64 | (1 << 1) | (1 << 2) | (1 << 63);
    let result = offsets(&[0x1400, bitmap]);
    assert_eq!(result, vec![0x1400, 0x1408, 0x1410, 0x15f8]);
}

#[test]
fn an_all_bits_bitmap_yields_sixty_three_offsets() {
    let bitmap = u64::MAX;
    let result = offsets(&[0x1400, bitmap]);
    assert_eq!(result.len(), 64);
    assert_eq!(result[0], 0x1400);
    for (index, offset) in result.iter().enumerate().skip(1) {
        assert_eq!(*offset, 0x1400 + u64::try_from(index).unwrap() * 8);
    }
}

#[test]
fn a_bitmap_may_cross_a_page_boundary() {
    // 0x1f00 + 62 words = 0x20f0, still inside the data segment's mapped span.
    let bitmap = u64::MAX;
    let result = offsets(&[0x1f00, bitmap]);
    assert_eq!(result.last(), Some(&(0x1f00 + 63 * 8)));
}

#[test]
fn two_address_entries_are_both_yielded() {
    assert_eq!(offsets(&[0x1400, 0x1600]), vec![0x1400, 0x1600]);
}

#[test]
fn an_empty_section_yields_nothing() {
    assert_eq!(offsets(&[]), Vec::<u64>::new());
}

#[test]
fn a_size_that_is_not_a_multiple_of_eight_is_bad_relr() {
    let image = Builder::new()
        .symbol(elfbuild::global_func(b"f", 0x100))
        .relr(0x1400)
        .build();
    let mut view = ObjView::digest(&image.memory(0), image.phdr_vaddr, image.phnum, 0).unwrap();
    view.relr.1 = 9;
    assert_eq!(
        relr_offsets(&view, &image.memory(0)).err(),
        Some(RelocError::BadRelr)
    );
}

#[test]
fn an_address_outside_every_load_segment_is_bad_relr() {
    let image = Builder::new()
        .symbol(elfbuild::global_func(b"f", 0x100))
        .relr(0x0090_0000)
        .build();
    let view = ObjView::digest(&image.memory(0), image.phdr_vaddr, image.phnum, 0).unwrap();
    let memory = image.memory(0);
    let mut iterator = relr_offsets(&view, &memory).unwrap();
    assert_eq!(iterator.next(), Some(Err(RelocError::BadRelr)));
}

#[test]
fn a_short_stream_is_bad_relr() {
    let image = Builder::new()
        .symbol(elfbuild::global_func(b"f", 0x100))
        .relr(0x1400)
        .build();
    let mut view = ObjView::digest(&image.memory(0), image.phdr_vaddr, image.phnum, 0).unwrap();
    // Point the range at the last four bytes of the image so the read fails.
    view.relr.0 = view.strtab + view.strsz - 4;
    let memory = image.memory(0);
    let mut iterator = relr_offsets(&view, &memory).unwrap();
    assert_eq!(iterator.next(), Some(Err(RelocError::BadRelr)));
}

#[test]
fn a_bitmap_after_an_address_that_is_outside_is_bad_relr() {
    let image = Builder::new()
        .symbol(elfbuild::global_func(b"f", 0x100))
        .relr(0x0090_0000)
        .relr(3)
        .build();
    let view = ObjView::digest(&image.memory(0), image.phdr_vaddr, image.phnum, 0).unwrap();
    let memory = image.memory(0);
    let mut iterator = relr_offsets(&view, &memory).unwrap();
    assert_eq!(iterator.next(), Some(Err(RelocError::BadRelr)));
    assert_eq!(iterator.next(), None);
}

#[test]
fn an_iterator_over_a_slice_view_works() {
    let mut builder = Builder::new().symbol(elfbuild::global_func(b"f", 0x100));
    builder = builder.relr(0x1400);
    let image = builder.build();
    let view = ObjView::digest(&image.memory(0), image.phdr_vaddr, image.phnum, 0).unwrap();
    let mut set = crate::mem::MemSet::new();
    let (rx, rest) = image.bytes.split_at(elfbuild::RX_SIZE);
    set.push(SliceMem::new(0, rx));
    set.push(SliceMem::new(elfbuild::DATA_BASE, rest));
    let mut iterator = relr_offsets(&view, &set).unwrap();
    assert_eq!(iterator.next(), Some(Ok(0x1400)));
    assert_eq!(iterator.next(), None);
}
