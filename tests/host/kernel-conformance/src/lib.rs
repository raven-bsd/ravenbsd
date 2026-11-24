// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K572 conformance checks for CONTRACTS C4 (kernel entry/banner), C5 (test-exit protocol)
//! and C10 (serial). Every check reads the source that the contract names, so a drifting format
//! string, port, divisor or stack constant fails here without an image build.
//!
//! Mutation named per row: change the asserted source line (banner format, uname constant,
//! exit byte, `out` port, UART divisor, `loader: ` prefix, boot-stack skip) and the
//! corresponding test fails.

#![cfg(test)]

use std::fs;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn read(relative: &str) -> String {
    let path = root().join(relative);
    fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

fn assert_contains(source: &str, needle: &str, what: &str) {
    assert!(
        source.contains(needle),
        "{what}: expected {needle:?} in the contract source"
    );
}

fn start_asm() -> String {
    let source = read("kernel/src/arch/x86_64/start.rs");
    let start = source.find("_start:").expect("C4 `_start:` label");
    let end = source[start..]
        .find("call kmain")
        .map(|offset| start + offset)
        .expect("C4 `call kmain`");
    source[start..end].to_string()
}

#[test]
fn c4_entry_keeps_bootinfo_in_rdi() {
    let asm = start_asm();
    assert_contains(&asm, "cli", "C4 entry: interrupts off");
    assert_contains(&asm, "cld", "C4 entry: direction flag clear");
    assert_contains(
        &asm,
        "lea rsp, [rip + __boot_stack_top]",
        "C4 entry: boot stack switch",
    );
    assert!(
        !asm.contains("rdi") && !asm.contains("edi"),
        "C4 entry must not touch rdi; the loader passes the BootInfo in it"
    );
}

#[test]
fn c4_boot_stack_is_8_mib() {
    let source = read("kernel/src/arch/x86_64/start.rs");
    let skip_at = source.find(".skip ").expect("C4 boot stack .skip");
    let number: u64 = source[skip_at + ".skip ".len()..]
        .split_whitespace()
        .next()
        .expect("boot stack size")
        .parse()
        .expect("boot stack number");
    // CONTRACTS C4: `_start` switches to an 8 MiB static boot stack in .bss (amended 2026-11-24).
    assert_eq!(
        number,
        8 * 1024 * 1024,
        "CONTRACTS C4 says an 8 MiB boot stack, but kernel/src/arch/x86_64/start.rs reserves \
         {number} bytes ({})",
        if number == 64 * 1024 {
            "64 KiB"
        } else {
            "different"
        }
    );
}

#[test]
fn c4_banner_strings() {
    let source = read("kernel/src/boot.rs");
    assert_contains(
        &source,
        "\"RavenBSD kernel 0.1 ({}) {}\"",
        "C4 banner: first line",
    );
    assert_contains(
        &source,
        "\"mem: {} MiB usable in {} regions\"",
        "C4 banner: memory line",
    );
    assert_contains(&source, "env!(\"RAVEN_BUILD_ID\")", "C4 banner: build id");
    assert_contains(
        &read("kernel/src/arch/mod.rs"),
        "pub const ARCH_NAME: &str = \"x86_64\";",
        "C4 banner: x86_64 arch name",
    );
}

#[test]
fn c4_uname_strings() {
    let abi = read("crates/raven-abi/src/uname.rs");
    assert_contains(
        &abi,
        "pub const SYSNAME: &str = \"RavenBSD\";",
        "C4 uname sysname",
    );
    assert_contains(
        &abi,
        "pub const RELEASE: &str = \"0.1\";",
        "C4 uname release",
    );
    let basic = read("kernel/src/syscall/basic.rs");
    assert_contains(
        &basic,
        "\"RavenBSD 0.1 ({profile}) #1: {}\"",
        "C4 uname version format",
    );
    assert_contains(
        &basic,
        "Uname::set_field(&mut uname.nodename, \"raven\")",
        "C4 uname nodename",
    );
    assert_contains(
        &basic,
        "Uname::set_field(&mut uname.machine, crate::arch::ARCH_NAME)",
        "C4 uname machine",
    );
}

#[test]
fn c5_x86_test_exit_port_and_bytes() {
    let boot = read("kernel/src/boot.rs");
    assert_contains(&boot, "out dx, al", "C5 exit: out instruction");
    assert_contains(&boot, "in(\"dx\") 0xf4u16", "C5 exit: port 0xf4");
    assert_contains(&boot, "exit_qemu(0x10)", "C5 exit: success byte 0x10");
    assert_contains(&boot, "exit_qemu(0x11)", "C5 exit: failure byte 0x11");
}

#[test]
fn c5_init_exited_and_code_mapping() {
    let process = read("kernel/src/process/mod.rs");
    assert_contains(&process, "\"init exited ({code})\"", "C5 PID 1 exit line");
    assert_contains(
        &process,
        "crate::log::test_exit()",
        "C5 test-exit cmdline gate",
    );
    assert_contains(
        &process,
        "if code == 0 { 0x10 } else { 0x11 }",
        "C5 exit code mapping",
    );
}

#[test]
fn c10_com1_divisor_and_format() {
    let serial = read("kernel/src/arch/x86_64/serial.rs");
    assert_contains(&serial, "const COM1: u16 = 0x3f8;", "C10 COM1 base");
    assert_contains(&serial, "outb(COM1 + 3, 0x80)", "C10 divisor latch enable");
    assert_contains(&serial, "outb(COM1, 0x01)", "C10 divisor 1 = 115200 baud");
    assert_contains(&serial, "outb(COM1 + 3, 0x03)", "C10 8N1 line format");
    assert_contains(
        &serial,
        "impl fmt::Write for Serial",
        "C10 fmt::Write, no allocation",
    );
}

#[test]
fn c10_loader_prefix_and_no_allocation() {
    let loader = read("boot/loader/src/serial.rs");
    assert_contains(&loader, "\"loader: {args}\"", "C10 loader prefix");
    assert!(
        !loader.contains("alloc::") && !loader.contains("format!"),
        "C10 loader line must render into a fixed buffer, no allocation"
    );
}
