// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used
)]
// Program headers are built from small hand-written constants.

//! Segment plan tests.

use super::*;
use crate::consts::{PF_R, PF_W, PF_X, PT_GNU_EH_FRAME, PT_GNU_RELRO, PT_LOAD, PT_TLS};
use crate::errors::ElfError;

fn phdr(
    p_type: u32,
    flags: u32,
    offset: u64,
    vaddr: u64,
    filesz: u64,
    memsz: u64,
    align: u64,
) -> Phdr {
    Phdr {
        p_type,
        p_flags: flags,
        p_offset: offset,
        p_vaddr: vaddr,
        p_paddr: vaddr,
        p_filesz: filesz,
        p_memsz: memsz,
        p_align: align,
    }
}

fn rx(vaddr: u64, filesz: u64, memsz: u64) -> Phdr {
    phdr(PT_LOAD, PF_R | PF_X, vaddr, vaddr, filesz, memsz, 4096)
}

fn rw(vaddr: u64, filesz: u64, memsz: u64) -> Phdr {
    phdr(PT_LOAD, PF_R | PF_W, vaddr, vaddr, filesz, memsz, 4096)
}

#[test]
fn a_two_segment_image_has_a_file_step_per_segment() {
    let plan = plan_segments(&[rx(0, 0x100, 0x100), rw(0x1000, 0x200, 0x200)], 4096).unwrap();
    assert_eq!(plan.base_vaddr, 0);
    assert_eq!(plan.reserve_len, 0x2000);
    assert_eq!(plan.steps.len(), 2);
    assert_eq!(plan.steps[0].kind, StepKind::File);
    assert_eq!(plan.steps[0].vaddr, 0);
    assert_eq!(plan.steps[0].len, 0x1000);
    assert_eq!(plan.steps[1].vaddr, 0x1000);
    assert_eq!(plan.steps[1].file_off, 0x1000);
}

#[test]
fn a_bss_tail_is_split_into_tail_and_zero_steps() {
    let plan = plan_segments(&[rw(0x1000, 0x180, 0x300)], 4096).unwrap();
    assert_eq!(plan.steps.len(), 2);
    assert_eq!(plan.steps[0].kind, StepKind::File);
    assert_eq!(plan.steps[0].len, 0x1000);
    assert_eq!(plan.steps[1].kind, StepKind::Tail);
    assert_eq!(plan.steps[1].vaddr, 0x1180);
    assert_eq!(plan.steps[1].len, 0xe80);

    // A segment whose memory extends beyond the last file page adds a Zero step.
    let plan = plan_segments(&[rw(0x1000, 0x180, 0x1400)], 4096).unwrap();
    assert_eq!(plan.steps.len(), 3);
    assert_eq!(plan.steps[1].kind, StepKind::Tail);
    assert_eq!(plan.steps[2].kind, StepKind::Zero);
    assert_eq!(plan.steps[2].vaddr, 0x2000);
    assert_eq!(plan.steps[2].len, 0x1000);
}

#[test]
fn a_pure_bss_segment_has_one_zero_step() {
    let plan = plan_segments(&[rw(0x1000, 0, 0x400)], 4096).unwrap();
    assert_eq!(plan.steps.len(), 1);
    assert_eq!(plan.steps[0].kind, StepKind::Zero);
    assert_eq!(plan.steps[0].vaddr, 0x1000);
    assert_eq!(plan.steps[0].len, 0x1000);
}

#[test]
fn filesz_greater_than_memsz_is_rejected() {
    assert_eq!(
        plan_segments(&[rw(0x1000, 0x300, 0x200)], 4096),
        Err(ElfError::BadSegment)
    );
}

#[test]
fn overlapping_segments_are_rejected() {
    assert_eq!(
        plan_segments(&[rx(0, 0x1000, 0x1000), rw(0x800, 0x100, 0x100)], 4096),
        Err(ElfError::BadSegment)
    );
}

#[test]
fn a_writable_and_executable_segment_is_rejected() {
    let segment = phdr(PT_LOAD, PF_R | PF_W | PF_X, 0, 0, 0x100, 0x100, 4096);
    assert_eq!(plan_segments(&[segment], 4096), Err(ElfError::BadSegment));
}

#[test]
fn seventeen_load_segments_are_rejected() {
    let mut segments = Vec::new();
    for index in 0..17u64 {
        segments.push(rw(index * 0x1000, 0x10, 0x10));
    }
    assert_eq!(
        plan_segments(&segments, 4096),
        Err(ElfError::TooManySegments)
    );
}

#[test]
fn a_page_offset_mismatch_is_rejected() {
    let segment = phdr(PT_LOAD, PF_R, 0x100, 0x200, 0x100, 0x100, 4096);
    assert_eq!(plan_segments(&[segment], 4096), Err(ElfError::BadSegment));
}

#[test]
fn a_large_but_supported_alignment_is_accepted_and_ignored() {
    let segment = phdr(PT_LOAD, PF_R, 0, 0, 0x100, 0x100, 0x1_0000);
    let plan = plan_segments(&[segment], 4096).unwrap();
    assert_eq!(plan.steps[0].vaddr, 0);
}

#[test]
fn an_alignment_above_the_cap_is_rejected() {
    let segment = phdr(PT_LOAD, PF_R, 0, 0, 0x100, 0x100, 0x2_0000);
    assert_eq!(plan_segments(&[segment], 4096), Err(ElfError::BadSegment));
}

#[test]
fn an_address_overflow_is_rejected() {
    let segment = phdr(PT_LOAD, PF_R, 0, u64::MAX - 0x10, 0x100, 0x100, 4096);
    assert_eq!(plan_segments(&[segment], 4096), Err(ElfError::BadSegment));
}

#[test]
fn empty_input_has_no_load_segment() {
    assert_eq!(plan_segments(&[], 4096), Err(ElfError::NoLoad));
}

#[test]
fn a_zero_page_size_is_rejected() {
    assert_eq!(
        plan_segments(&[rx(0, 0x10, 0x10)], 0),
        Err(ElfError::BadEntry)
    );
    assert_eq!(
        plan_segments(&[rx(0, 0x10, 0x10)], 3000),
        Err(ElfError::BadEntry)
    );
}

#[test]
fn the_relro_range_is_rounded_down_at_its_end() {
    let mut segment = phdr(PT_LOAD, PF_R | PF_W, 0, 0x1000, 0x400, 0x400, 4096);
    segment.p_type = PT_GNU_RELRO;
    let plan = plan_segments(&[rw(0x1000, 0x400, 0x400), segment], 4096).unwrap();
    assert_eq!(plan.relro, Some((0x1000, 0x1000)));
}

#[test]
fn the_tls_template_and_eh_frame_are_recorded() {
    let mut tls = phdr(PT_LOAD, PF_R, 0, 0x1000, 0x20, 0x40, 8);
    tls.p_type = PT_TLS;
    let mut eh = phdr(PT_LOAD, PF_R, 0, 0x1040, 0x18, 0x18, 8);
    eh.p_type = PT_GNU_EH_FRAME;
    let plan = plan_segments(&[rw(0x1000, 0x60, 0x60), tls, eh], 4096).unwrap();
    let template = plan.tls.unwrap();
    assert_eq!(template.filesz, 0x20);
    assert_eq!(template.memsz, 0x40);
    assert_eq!(plan.eh_frame, Some((0x1040, 0x1058)));
}

#[test]
fn segments_are_planned_in_address_order() {
    let plan = plan_segments(&[rw(0x2000, 0x10, 0x10), rx(0, 0x10, 0x10)], 4096).unwrap();
    assert_eq!(plan.steps[0].vaddr, 0);
    assert_eq!(plan.steps[1].vaddr, 0x2000);
    assert_eq!(plan.reserve_len, 0x3000);
}

#[test]
fn a_non_page_aligned_first_segment_is_rounded_down() {
    let segment = phdr(PT_LOAD, PF_R, 0x100, 0x4100, 0x100, 0x100, 4096);
    let plan = plan_segments(&[segment], 4096).unwrap();
    assert_eq!(plan.base_vaddr, 0x4000);
    assert_eq!(plan.steps[0].vaddr, 0x4000);
    assert_eq!(plan.steps[0].file_off, 0);
}
