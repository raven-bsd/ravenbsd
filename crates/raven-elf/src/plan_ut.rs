// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the biased load planner.

use super::*;
use crate::Machine;
use std::vec::Vec;

const PAGE: u64 = 4096;
const USER_END: u64 = 0x0000_7FFF_FFFF_F000;
const EM_X86_64: u16 = 62;
const ET_DYN: u16 = 3;
const PT_LOAD: u32 = 1;

fn put16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes
        .get_mut(offset..offset + 2)
        .expect("header field")
        .copy_from_slice(&value.to_le_bytes());
}

fn put32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes
        .get_mut(offset..offset + 4)
        .expect("header field")
        .copy_from_slice(&value.to_le_bytes());
}

fn put64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes
        .get_mut(offset..offset + 8)
        .expect("header field")
        .copy_from_slice(&value.to_le_bytes());
}

/// A two-segment zero-based `ET_DYN` image: RX at 0 and R at 0x1000.
fn image() -> Vec<u8> {
    let mut bytes = vec![0u8; 2 * PAGE as usize];
    bytes
        .get_mut(0..4)
        .expect("ident")
        .copy_from_slice(b"\x7fELF");
    bytes[4] = 2;
    bytes[5] = 1;
    bytes[6] = 1;
    put16(&mut bytes, 16, ET_DYN);
    put16(&mut bytes, 18, EM_X86_64);
    put32(&mut bytes, 20, 1);
    put64(&mut bytes, 24, 0x100);
    put64(&mut bytes, 32, 64);
    put16(&mut bytes, 52, 64);
    put16(&mut bytes, 54, 56);
    put16(&mut bytes, 56, 2);

    put32(&mut bytes, 64, PT_LOAD);
    put32(&mut bytes, 68, 5);
    put64(&mut bytes, 72, 0);
    put64(&mut bytes, 80, 0);
    put64(&mut bytes, 88, 0);
    put64(&mut bytes, 96, PAGE);
    put64(&mut bytes, 104, PAGE);
    put64(&mut bytes, 112, PAGE);

    put32(&mut bytes, 120, PT_LOAD);
    put32(&mut bytes, 124, 4);
    put64(&mut bytes, 128, PAGE);
    put64(&mut bytes, 136, PAGE);
    put64(&mut bytes, 144, PAGE);
    put64(&mut bytes, 152, PAGE);
    put64(&mut bytes, 160, PAGE);
    put64(&mut bytes, 168, PAGE);
    bytes
}

#[test]
fn a_zero_based_image_is_moved_by_the_bias() {
    let bytes = image();
    let elf = Elf::parse(&bytes, Machine::X86_64, 0..USER_END).expect("parse");
    let steps: Vec<MapStep> = plan_load_biased(&elf, PAGE, 0x0000_1000_0000_0000, LoadMode::Copy)
        .expect("plan")
        .collect();
    assert_eq!(steps.len(), 2);
    assert_eq!(
        steps.first().map(|step| step.vaddr),
        Some(0x0000_1000_0000_0000)
    );
    assert_eq!(
        steps.get(1).map(|step| step.vaddr),
        Some(0x0000_1000_0000_1000)
    );
    assert_eq!(steps.first().map(|step| step.prot), Some(5));
    assert_eq!(steps.get(1).map(|step| step.prot), Some(1));
    assert_eq!(
        steps.first().map(|step| step.kind),
        Some(StepKind::File { offset: 0 })
    );
    assert_eq!(
        steps.get(1).map(|step| step.kind),
        Some(StepKind::File { offset: PAGE })
    );
}

#[test]
fn zero_bias_keeps_the_existing_plan() {
    let bytes = image();
    let elf = Elf::parse(&bytes, Machine::X86_64, 0..USER_END).expect("parse");
    let stepped: Vec<MapStep> = plan_load(&elf, PAGE, LoadMode::Copy)
        .expect("plan")
        .collect();
    let biased: Vec<MapStep> = plan_load_biased(&elf, PAGE, 0, LoadMode::Copy)
        .expect("plan")
        .collect();
    assert_eq!(stepped, biased);
}

#[test]
fn a_bias_must_be_page_aligned() {
    let bytes = image();
    let elf = Elf::parse(&bytes, Machine::X86_64, 0..USER_END).expect("parse");
    assert_eq!(
        plan_load_biased(&elf, PAGE, 1, LoadMode::Copy).err(),
        Some(ElfError::BadPhdr)
    );
}

#[test]
fn a_bias_that_leaves_the_user_window_is_rejected() {
    let bytes = image();
    let elf = Elf::parse(&bytes, Machine::X86_64, 0..USER_END).expect("parse");
    assert_eq!(
        plan_load_biased(&elf, PAGE, USER_END, LoadMode::Copy).err(),
        Some(ElfError::SegmentOutsideUserRange)
    );
}
