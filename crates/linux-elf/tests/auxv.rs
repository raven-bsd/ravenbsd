// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Auxiliary-vector content and order tests.

mod common;

use common::{ET_DYN, ElfOptions, build_elf, config, parse_elf};
use linux_elf::{build_auxv, plan_image};

const INTERP: &str = "/lib/ld-musl-x86_64.so.1";

#[test]
fn static_non_pie_auxv_has_the_ticket_order_and_values() {
    let data = build_elf(&ElfOptions::default());
    let main = parse_elf(&data);
    let plan = plan_image(&main, None, &config()).unwrap();
    let auxv = build_auxv(&plan, &config(), 0x1000, 0x2000, 0x3000);

    assert_eq!(
        auxv.0,
        vec![
            (3, 0x40_0040),
            (4, 56),
            (5, 3),
            (6, 4096),
            (7, 0),
            (8, 0),
            (9, 0x40_0000),
            (11, 1000),
            (12, 1000),
            (13, 1000),
            (14, 1000),
            (15, 0x2000),
            (16, 0x1234),
            (17, 100),
            (23, 0),
            (25, 0x3000),
            (31, 0x1000),
            (0, 0),
        ]
    );
}

#[test]
fn static_pie_auxv_uses_biased_addresses() {
    let data = build_elf(&ElfOptions {
        e_type: ET_DYN,
        ..ElfOptions::default()
    });
    let main = parse_elf(&data);
    let plan = plan_image(&main, None, &config()).unwrap();
    let auxv = build_auxv(&plan, &config(), 0x1000, 0x2000, 0x3000);

    assert_eq!(common::auxv_value(&auxv.0, 3), Some(0x1040_0040));
    assert_eq!(common::auxv_value(&auxv.0, 9), Some(0x1040_0000));
    assert_eq!(common::auxv_value(&auxv.0, 7), Some(0));
}

#[test]
fn dynamic_auxv_uses_main_entry_and_interpreter_base() {
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
    let auxv = build_auxv(&plan, &config(), 0x1000, 0x2000, 0x3000);

    assert_eq!(common::auxv_value(&auxv.0, 3), Some(0x1040_0040));
    assert_eq!(common::auxv_value(&auxv.0, 7), Some(0x2000_0000));
    assert_eq!(common::auxv_value(&auxv.0, 9), Some(0x1040_0000));
    assert_eq!(auxv.0.last(), Some(&(0, 0)));
}

#[test]
fn sysinfo_ehdr_is_only_present_when_configured() {
    let data = build_elf(&ElfOptions::default());
    let main = parse_elf(&data);
    let plan = plan_image(&main, None, &config()).unwrap();

    let without = build_auxv(&plan, &config(), 0x1000, 0x2000, 0x3000);
    assert_eq!(common::auxv_value(&without.0, 33), None);

    let mut config = config();
    config.sysinfo_ehdr = Some(0x1234_5000);
    let with = build_auxv(&plan, &config, 0x1000, 0x2000, 0x3000);
    assert_eq!(common::auxv_value(&with.0, 33), Some(0x1234_5000));
    assert_eq!(with.0.last(), Some(&(0, 0)));
}

#[test]
fn credentials_and_secure_flag_are_reported() {
    let data = build_elf(&ElfOptions::default());
    let main = parse_elf(&data);
    let plan = plan_image(&main, None, &config()).unwrap();
    let mut config = config();
    config.uid = 10;
    config.euid = 11;
    config.gid = 12;
    config.egid = 13;
    config.secure = true;
    let auxv = build_auxv(&plan, &config, 0x1000, 0x2000, 0x3000);

    assert_eq!(common::auxv_value(&auxv.0, 11), Some(10));
    assert_eq!(common::auxv_value(&auxv.0, 12), Some(11));
    assert_eq!(common::auxv_value(&auxv.0, 13), Some(12));
    assert_eq!(common::auxv_value(&auxv.0, 14), Some(13));
    assert_eq!(common::auxv_value(&auxv.0, 23), Some(1));
}
