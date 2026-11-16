// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Image-plan tests with hand-built ELF64 files.

use raven_elf::StepKind;

use super::*;

const INTERP: &str = "/lib/ld-musl-x86_64.so.1";
const STACK_TOP: u64 = 0x0000_7FFF_FFFF_F000;
const ET_EXEC: u16 = 2;

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

/// An ELF with one R|X and one R|W load segment plus an optional `PT_INTERP`.
///
/// `e_type` selects `ET_EXEC`/`ET_DYN`; `load_vaddr` is zero for a real PIE. `entry` is relative
/// to `load_vaddr`.
fn image(e_type: u16, load_vaddr: u64, entry: u64, interp: Option<&str>) -> Vec<u8> {
    let phnum = 2 + usize::from(interp.is_some());
    let mut image = vec![0u8; 0x2000];
    image[0..4].copy_from_slice(b"\x7fELF");
    image[4] = 2;
    image[5] = 1;
    image[6] = 1;
    put_u16(&mut image, 16, e_type);
    put_u16(&mut image, 18, 62);
    put_u32(&mut image, 20, 1);
    put_u64(&mut image, 24, load_vaddr + entry);
    put_u64(&mut image, 32, 64);
    put_u16(&mut image, 52, 64);
    put_u16(&mut image, 54, 56);
    put_u16(&mut image, 56, phnum as u16);
    put_phdr(
        &mut image,
        0,
        Phdr {
            kind: 1,
            flags: 5,
            offset: 0,
            vaddr: load_vaddr,
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
            vaddr: load_vaddr + 0x1000,
            filesz: 0x100,
            memsz: 0x300,
        },
    );
    if let Some(interp) = interp {
        put_phdr(
            &mut image,
            2,
            Phdr {
                kind: 3,
                flags: 4,
                offset: 0x200,
                vaddr: 0,
                filesz: interp.len() as u64 + 1,
                memsz: interp.len() as u64 + 1,
            },
        );
        image[0x200..0x200 + interp.len()].copy_from_slice(interp.as_bytes());
        image[0x200 + interp.len()] = 0;
    }
    image
}

/// The non-PIE image used by the original B20 tests.
fn static_image() -> Vec<u8> {
    image(ET_EXEC, 0x40_0000, 0x80, None)
}

/// A zero-based PIE main, optionally naming an interpreter.
fn pie_image(interp: Option<&str>) -> Vec<u8> {
    image(ET_DYN, 0, 0x80, interp)
}

fn config() -> Config {
    Config {
        main_pie_base: MAIN_PIE_BASE,
        interp_base: INTERP_BASE,
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

fn plan_static() -> ImageSpec {
    plan_image(
        &static_image(),
        None,
        &config(),
        &[b"hello".as_slice()],
        &[b"PATH=/".as_slice()],
        b"/hello",
        STACK_TOP,
    )
    .expect("the synthetic static image is valid")
}

#[test]
fn the_entry_and_program_headers_come_from_the_elf() {
    let plan = plan_static();
    assert_eq!(plan.entry, 0x40_0080);
    assert_eq!(plan.phdr, 0x40_0040);
    assert_eq!(plan.phent, 56);
    assert_eq!(plan.phnum, 2);
    assert_eq!(plan.bias, 0);
    assert_eq!(plan.interp_bias, 0);
    assert_eq!(plan.brk_start, 0x40_2000);
    assert!(plan.interp_steps.is_empty());
}

#[test]
fn the_steps_cover_text_and_the_data_tail() {
    let plan = plan_static();
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
    let plan = plan_static();
    let stack = &plan.stack;
    assert_eq!(stack.base, stack.sp);
    assert_eq!(stack.sp % 16, 0);
    assert!(stack.sp < STACK_TOP);
    assert!(stack.sp + stack.bytes.len() as u64 <= STACK_TOP);
    let argc = u64::from_le_bytes(stack.bytes[0..8].try_into().expect("argc"));
    assert_eq!(argc, 1);
}

#[test]
fn a_truncated_image_is_rejected() {
    let mut image = static_image();
    image.truncate(32);
    let error = plan_image(&image, None, &config(), &[], &[], b"/hello", STACK_TOP);
    assert!(matches!(error, Err(ImageError::Elf(_))));
}

#[test]
fn a_zero_based_pie_is_parsed_and_biased() {
    let plan = plan_image(
        &pie_image(None),
        None,
        &config(),
        &[b"pie".as_slice()],
        &[],
        b"/pie",
        STACK_TOP,
    )
    .expect("a zero-based PIE is valid");
    assert_eq!(plan.bias, MAIN_PIE_BASE);
    assert_eq!(plan.entry, MAIN_PIE_BASE + 0x80);
    assert_eq!(plan.phdr, MAIN_PIE_BASE + 0x40);
    assert_eq!(plan.brk_start, MAIN_PIE_BASE + 0x2000);
    assert_eq!(plan.steps[0].vaddr, 0);
    assert_eq!(plan.steps[1].vaddr, 0x1000);
}

#[test]
fn an_interpreter_is_planned_at_its_own_bias() {
    let main = pie_image(Some(INTERP));
    let interp = pie_image(None);
    let plan = plan_image(
        &main,
        Some(&interp),
        &config(),
        &[b"busybox".as_slice(), b"ls".as_slice()],
        &[],
        b"/compat/linux/bin/busybox",
        STACK_TOP,
    )
    .expect("the dynamic image and its interpreter are valid");
    assert_eq!(plan.bias, MAIN_PIE_BASE);
    assert_eq!(plan.interp_bias, INTERP_BASE);
    assert_eq!(plan.entry, INTERP_BASE + 0x80);
    assert_eq!(plan.phdr, MAIN_PIE_BASE + 0x40);
    assert_eq!(plan.brk_start, MAIN_PIE_BASE + 0x2000);
    assert_eq!(plan.interp_steps.len(), 2);
    assert_eq!(plan.interp_steps[0].vaddr, 0);
    assert_eq!(plan.interp_steps[1].vaddr, 0x1000);
}

#[test]
fn a_missing_interpreter_is_reported() {
    let error = plan_image(
        &pie_image(Some(INTERP)),
        None,
        &config(),
        &[],
        &[],
        b"/pie",
        STACK_TOP,
    );
    assert!(matches!(
        error,
        Err(ImageError::Linux(LinuxElfError::MissingInterpreter))
    ));
}

#[test]
fn interp_path_is_read_from_the_main_image() {
    let main = pie_image(Some(INTERP));
    assert_eq!(interp_path(&main), Ok(Some(INTERP)));
    assert_eq!(interp_path(&pie_image(None)), Ok(None));
    assert_eq!(interp_path(&static_image()), Ok(None));
}

#[test]
fn a_biased_step_outside_the_user_window_is_rejected() {
    let mut config = config();
    config.main_pie_base = USER_END;
    let error = plan_image(
        &pie_image(None),
        None,
        &config,
        &[],
        &[],
        b"/pie",
        STACK_TOP,
    );
    assert!(matches!(error, Err(ImageError::Range)));
}
