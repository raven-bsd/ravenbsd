// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Load-plan, bias, TLS and error tests.

mod common;

use common::{ET_DYN, ET_EXEC, ElfOptions, build_elf, config, parse_elf};
use linux_elf::{LinuxElfError, TlsInfo, plan_image};

const INTERP: &str = "/lib/ld-musl-x86_64.so.1";

#[test]
fn static_non_pie_plan_has_zero_bias() {
    let data = build_elf(&ElfOptions::default());
    let main = parse_elf(&data);
    let plan = plan_image(&main, None, &config()).unwrap();

    assert_eq!(plan.main_bias, 0);
    assert_eq!(plan.interp_bias, 0);
    assert_eq!(plan.main.start, 0x40_0000);
    assert_eq!(plan.main.end, 0x40_3000);
    assert_eq!(plan.main.entry, 0x40_0000);
    assert_eq!(plan.entry, 0x40_0000);
    assert_eq!(plan.phdr_addr, 0x40_0040);
    assert_eq!(plan.phent, 56);
    assert_eq!(plan.phnum, 3);
    assert_eq!(plan.brk_start, 0x40_3000);
    assert_eq!(plan.interp, None);
    assert!(!plan.exec_stack);
    assert_eq!(plan.tls, None);
}

#[test]
fn static_pie_plan_applies_the_pie_bias() {
    let data = build_elf(&ElfOptions {
        e_type: ET_DYN,
        ..ElfOptions::default()
    });
    let main = parse_elf(&data);
    let plan = plan_image(&main, None, &config()).unwrap();

    assert_eq!(plan.main_bias, 0x1000_0000);
    assert_eq!(plan.main.start, 0x1040_0000);
    assert_eq!(plan.main.end, 0x1040_3000);
    assert_eq!(plan.main.entry, 0x1040_0000);
    assert_eq!(plan.entry, 0x1040_0000);
    assert_eq!(plan.phdr_addr, 0x1040_0040);
    assert_eq!(plan.brk_start, 0x1040_3000);
}

#[test]
fn dynamic_plan_uses_the_interpreter_entry_and_base() {
    let main_data = build_elf(&ElfOptions {
        e_type: ET_DYN,
        interp: Some(INTERP),
        ..ElfOptions::default()
    });
    let interp_data = build_elf(&ElfOptions {
        e_type: ET_DYN,
        ..ElfOptions::default()
    });
    let main = parse_elf(&main_data);
    let interp = parse_elf(&interp_data);
    let plan = plan_image(&main, Some(&interp), &config()).unwrap();

    let (path, summary) = plan.interp.unwrap();
    assert_eq!(path, INTERP);
    assert_eq!(plan.main_bias, 0x1000_0000);
    assert_eq!(plan.interp_bias, 0x2000_0000);
    assert_eq!(summary.bias, 0x2000_0000);
    assert_eq!(summary.entry, 0x2040_0000);
    assert_eq!(summary.start, 0x2040_0000);
    assert_eq!(summary.end, 0x2040_3000);
    assert_eq!(plan.entry, 0x2040_0000);
    assert_eq!(plan.main.entry, 0x1040_0000);
    assert_eq!(plan.phdr_addr, 0x1040_0040);
}

#[test]
fn missing_interpreter_is_reported() {
    let data = build_elf(&ElfOptions {
        e_type: ET_DYN,
        interp: Some(INTERP),
        ..ElfOptions::default()
    });
    let main = parse_elf(&data);
    assert_eq!(
        plan_image(&main, None, &config()),
        Err(LinuxElfError::MissingInterpreter)
    );
}

#[test]
fn non_dynamic_interpreter_is_rejected() {
    let main_data = build_elf(&ElfOptions {
        e_type: ET_DYN,
        interp: Some(INTERP),
        ..ElfOptions::default()
    });
    let interp_data = build_elf(&ElfOptions {
        e_type: ET_EXEC,
        ..ElfOptions::default()
    });
    let main = parse_elf(&main_data);
    let interp = parse_elf(&interp_data);
    assert_eq!(
        plan_image(&main, Some(&interp), &config()),
        Err(LinuxElfError::InterpreterNotDynamic)
    );
}

#[test]
fn unexpected_interpreter_is_rejected() {
    let main_data = build_elf(&ElfOptions::default());
    let interp_data = build_elf(&ElfOptions {
        e_type: ET_DYN,
        ..ElfOptions::default()
    });
    let main = parse_elf(&main_data);
    let interp = parse_elf(&interp_data);
    assert_eq!(
        plan_image(&main, Some(&interp), &config()),
        Err(LinuxElfError::UnexpectedInterpreter)
    );
}

#[test]
fn invalid_page_size_is_rejected() {
    let data = build_elf(&ElfOptions::default());
    let main = parse_elf(&data);
    let mut config = config();
    config.page = 0;
    assert_eq!(
        plan_image(&main, None, &config),
        Err(LinuxElfError::InvalidConfig)
    );
}

#[test]
fn misaligned_pie_base_is_rejected() {
    let data = build_elf(&ElfOptions {
        e_type: ET_DYN,
        ..ElfOptions::default()
    });
    let main = parse_elf(&data);
    let mut config = config();
    config.main_pie_base = 1;
    assert_eq!(
        plan_image(&main, None, &config),
        Err(LinuxElfError::InvalidConfig)
    );
}

#[test]
fn misaligned_interpreter_base_is_rejected() {
    let main_data = build_elf(&ElfOptions {
        e_type: ET_DYN,
        interp: Some(INTERP),
        ..ElfOptions::default()
    });
    let interp_data = build_elf(&ElfOptions {
        e_type: ET_DYN,
        ..ElfOptions::default()
    });
    let main = parse_elf(&main_data);
    let interp = parse_elf(&interp_data);
    let mut config = config();
    config.interp_base = 1;
    assert_eq!(
        plan_image(&main, Some(&interp), &config),
        Err(LinuxElfError::InvalidConfig)
    );
}

#[test]
fn executable_stack_flag_is_reported_both_ways() {
    let executable = build_elf(&ElfOptions {
        exec_stack: Some(true),
        ..ElfOptions::default()
    });
    let non_executable = build_elf(&ElfOptions {
        exec_stack: Some(false),
        ..ElfOptions::default()
    });
    let executable = parse_elf(&executable);
    let non_executable = parse_elf(&non_executable);
    assert!(plan_image(&executable, None, &config()).unwrap().exec_stack);
    assert!(
        !plan_image(&non_executable, None, &config())
            .unwrap()
            .exec_stack
    );
}

#[test]
fn tls_template_is_reported_with_the_bias() {
    let data = build_elf(&ElfOptions {
        e_type: ET_DYN,
        tls: true,
        ..ElfOptions::default()
    });
    let main = parse_elf(&data);
    let plan = plan_image(&main, None, &config()).unwrap();
    assert_eq!(
        plan.tls,
        Some(TlsInfo {
            vaddr: 0x1040_2000,
            offset: 0x3080,
            filesz: 16,
            memsz: 32,
            align: 16,
        })
    );
}

#[test]
fn missing_program_headers_are_rejected() {
    let data = build_elf(&ElfOptions {
        include_phdr: false,
        ..ElfOptions::default()
    });
    let main = parse_elf(&data);
    assert_eq!(
        plan_image(&main, None, &config()),
        Err(LinuxElfError::BadProgramHeaders)
    );
}

#[test]
fn address_overflow_is_reported() {
    let near_top = 0xffff_ffff_ffff_c000;
    let data = build_elf(&ElfOptions {
        e_type: ET_DYN,
        entry: near_top,
        load_vaddr: near_top,
        ..ElfOptions::default()
    });
    let main = parse_elf(&data);
    let mut config = config();
    config.main_pie_base = 0x4000;
    assert_eq!(
        plan_image(&main, None, &config),
        Err(LinuxElfError::AddressOverflow)
    );
}
