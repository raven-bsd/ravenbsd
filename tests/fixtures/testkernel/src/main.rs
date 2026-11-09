// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tiny test kernel for the loader (CONTRACTS C12).
//!
//! The S03 loader jumps here with a `BootInfo` in `rdi`. The fixture validates the structure,
//! walks the memory map through the HHDM, prints one exact line and exits QEMU via the C5 port.

#![no_std]
#![no_main]

use core::arch::asm;
use core::mem::{align_of, size_of};

use raven_bootinfo::{BootInfo, MemoryRegion, arch, total_usable, validate_memory_map};

mod serial;

core::arch::global_asm!(
    r#"
    .section .text._start, "ax"
    .global _start
    .type _start, @function
_start:
    cli
    cld
    lea rsp, [rip + __boot_stack_top]
    xor ebp, ebp
    call tk_main
    ud2
    .size _start, . - _start

    .section .bss.__boot_stack, "aw", @nobits
    .balign 16
    .global __boot_stack_bottom
__boot_stack_bottom:
    .skip 65536
    .global __boot_stack_top
__boot_stack_top:
    .size __boot_stack_bottom, __boot_stack_top - __boot_stack_bottom
"#
);

/// Test-kernel entry point, called by `_start` with the `BootInfo` pointer in `rdi`.
#[unsafe(no_mangle)]
pub extern "C" fn tk_main(boot: *const BootInfo) -> ! {
    serial::init();
    match run(boot) {
        Ok(()) => exit_qemu(0x10),
        Err(reason) => {
            serial::print(format_args!("TESTKERNEL FAIL {reason}\n"));
            exit_qemu(0x11)
        }
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    serial::print(format_args!("TESTKERNEL FAIL panic\n"));
    exit_qemu(0x11)
}

fn run(boot: *const BootInfo) -> Result<(), &'static str> {
    if boot.is_null() {
        return Err("null BootInfo");
    }
    // SAFETY: the loader passes a valid BootInfo pointer in `rdi` (CONTRACTS C4).
    let boot = unsafe { &*boot };
    boot.validate().map_err(|_| "invalid BootInfo")?;
    if boot.arch != arch::X86_64 {
        return Err("wrong architecture");
    }

    let entry_size =
        usize::try_from(boot.memory_map_entry_size).map_err(|_| "memory map entry size")?;
    if entry_size != size_of::<MemoryRegion>() {
        return Err("unsupported memory map entry size");
    }
    let map_address = boot
        .hhdm_offset
        .checked_add(boot.memory_map_base)
        .ok_or("memory map address overflow")?;
    let map_len = usize::try_from(boot.memory_map_len).map_err(|_| "memory map length")?;
    let map_bytes = map_len
        .checked_mul(entry_size)
        .ok_or("memory map length overflow")?;
    if map_bytes > isize::MAX as usize {
        return Err("memory map too large");
    }
    if map_address as usize % align_of::<MemoryRegion>() != 0 {
        return Err("memory map is misaligned");
    }
    // SAFETY: the loader maps all physical RAM in the HHDM; `map_address` was checked for
    // overflow, the entry size matches `MemoryRegion`, and the total byte length fits `isize`.
    let map: &[MemoryRegion] =
        unsafe { core::slice::from_raw_parts(map_address as *const MemoryRegion, map_len) };
    validate_memory_map(map).map_err(|_| "invalid memory map")?;
    let usable_mib = total_usable(map) / (1024 * 1024);

    let cmdline = if boot.cmdline.len == 0 {
        ""
    } else {
        let address = boot
            .hhdm_offset
            .checked_add(boot.cmdline.base)
            .ok_or("cmdline address overflow")?;
        let len = usize::try_from(boot.cmdline.len).map_err(|_| "cmdline length")?;
        // SAFETY: the loader placed the command line in HHDM-mapped RAM and recorded its length.
        let bytes = unsafe { core::slice::from_raw_parts(address as *const u8, len) };
        core::str::from_utf8(bytes).map_err(|_| "cmdline is not UTF-8")?
    };

    serial::print(format_args!(
        "TESTKERNEL OK arch={} mem_regions={} usable_mib={} initfs_len={} cmdline=\"{}\"\n",
        boot.arch, map_len, usable_mib, boot.initfs.len, cmdline
    ));
    Ok(())
}

fn exit_qemu(code: u8) -> ! {
    // SAFETY: QEMU's isa-debug-exit device watches port 0xf4; the CPU halts afterwards.
    unsafe {
        asm!(
            "out dx, al",
            in("dx") 0xf4u16,
            in("al") code,
            options(nomem, nostack, preserves_flags),
        );
    }
    loop {
        // SAFETY: `cli; hlt` is valid in ring 0 and touches no memory.
        unsafe {
            asm!("cli", "hlt", options(nomem, nostack, preserves_flags));
        }
    }
}
