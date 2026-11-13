// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Image-plan tests with a hand-built ELF64 file.

use raven_elf::StepKind;

use super::*;

fn put_u16(image: &mut [u8], offset: usize, value: u16) {
    image[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn put_u32(image: &mut [u8], offset: usize, value: u32) {
    image[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn put_u64(image: &mut [u8], offset: usize, value: u64) {
    image[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

#[derive(Clone, Copy)]
struct Phdr {
    kind: u32,
    flags: u32,
    offset: u64,
    vaddr: u64,
    filesz: u64,
    memsz: u64,
}

fn put_phdr(image: &mut [u8], index: usize, phdr: Phdr) {
    let base = 64 + index * 56;
    put_u32(image, base, phdr.kind);
    put_u32(image, base + 4, phdr.flags);
    put_u64(image, base + 8, phdr.offset);
    put_u64(image, base + 16, phdr.vaddr);
    put_u64(image, base + 24, phdr.vaddr);
    put_u64(image, base + 32, phdr.filesz);
    put_u64(image, base + 40, phdr.memsz);
    put_u64(image, base + 48, 0x1000);
}

/// A two-segment `ET_EXEC` image: R|X text covering the headers, R|W data with a zero tail.
fn image() -> Vec<u8> {
    let mut image = vec![0u8; 0x2000];
    image[0..4].copy_from_slice(b"\x7fELF");
    image[4] = 2;
    image[5] = 1;
    image[6] = 1;
    put_u16(&mut image, 16, 2);
    put_u16(&mut image, 18, 62);
    put_u32(&mut image, 20, 1);
    put_u64(&mut image, 24, 0x400080);
    put_u64(&mut image, 32, 64);
    put_u16(&mut image, 52, 64);
    put_u16(&mut image, 54, 56);
    put_u16(&mut image, 56, 2);
    put_phdr(
        &mut image,
        0,
        Phdr {
            kind: 1,
            flags: 5,
            offset: 0,
            vaddr: 0x40_0000,
            filesz: 0x1000,
            memsz: 0x1000,
        },
    );
    put_phdr(
        &mut image,
        1,
        Phdr {
            kind: 1,
            flags: 6,
            offset: 0x1000,
            vaddr: 0x40_1000,
            filesz: 0x100,
            memsz: 0x300,
        },
    );
    image
}

fn config() -> Config {
    Config {
        main_pie_base: 0,
        interp_base: 0,
        page: 4096,
        uid: 0,
        euid: 0,
        gid: 0,
        egid: 0,
        secure: false,
        hwcap: 0,
        platform: "x86_64",
        clktck: 100,
        sysinfo_ehdr: None,
        random: [7u8; 16],
    }
}

fn plan() -> ImageSpec {
    plan_image(
        &image(),
        &config(),
        &[b"hello".as_slice()],
        &[b"PATH=/".as_slice()],
        b"/hello",
        0x0000_7FFF_FFFF_F000,
    )
    .expect("the synthetic image is valid")
}

#[test]
fn the_entry_and_program_headers_come_from_the_elf() {
    let plan = plan();
    assert_eq!(plan.entry, 0x40_0080);
    assert_eq!(plan.phdr, 0x40_0040);
    assert_eq!(plan.phent, 56);
    assert_eq!(plan.phnum, 2);
    assert_eq!(plan.bias, 0);
    assert_eq!(plan.brk_start, 0x40_2000);
}

#[test]
fn the_steps_cover_text_and_the_data_tail() {
    let plan = plan();
    assert_eq!(plan.steps.len(), 2);
    assert_eq!(plan.steps[0].vaddr, 0x40_0000);
    assert_eq!(plan.steps[0].len, 0x1000);
    assert_eq!(plan.steps[0].prot, 5);
    assert!(matches!(plan.steps[0].kind, StepKind::File { offset: 0 }));
    assert_eq!(plan.steps[1].vaddr, 0x40_1000);
    assert_eq!(plan.steps[1].prot, 3);
    assert!(matches!(
        plan.steps[1].kind,
        StepKind::CopyTail {
            offset: 0x1000,
            len: 0x100
        }
    ));
}

#[test]
fn the_initial_stack_is_well_formed() {
    let plan = plan();
    let stack = &plan.stack;
    assert_eq!(stack.base, stack.sp);
    assert_eq!(stack.sp % 16, 0);
    assert!(stack.sp < 0x0000_7FFF_FFFF_F000);
    assert!(stack.sp + stack.bytes.len() as u64 <= 0x0000_7FFF_FFFF_F000);
    let argc = u64::from_le_bytes(stack.bytes[0..8].try_into().expect("argc"));
    assert_eq!(argc, 1);
}

#[test]
fn a_truncated_image_is_rejected() {
    let mut image = image();
    image.truncate(32);
    let error = plan_image(
        &image,
        &config(),
        &[],
        &[],
        b"/hello",
        0x0000_7FFF_FFFF_F000,
    );
    assert!(matches!(error, Err(ImageError::Elf(_))));
}
