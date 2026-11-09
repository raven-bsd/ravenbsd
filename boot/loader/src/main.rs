// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `raven-loader` UEFI application entry point.
//!
//! S02 reads the kernel, initfs and optional `boot.toml` from the ESP, validates and loads the
//! kernel ELF, and exits QEMU. Building page tables and jumping to the kernel is S03.

#![cfg_attr(target_os = "uefi", no_std)]
#![cfg_attr(target_os = "uefi", no_main)]

#[cfg(target_os = "uefi")]
extern crate alloc;

#[cfg(target_os = "uefi")]
use alloc::format;
#[cfg(target_os = "uefi")]
use alloc::string::String;
#[cfg(target_os = "uefi")]
use alloc::vec::Vec;

#[cfg(target_os = "uefi")]
use uefi::fs::FileSystem;

#[cfg(target_os = "uefi")]
use raven_loader::{config, fs, image, serial};

#[cfg(target_os = "uefi")]
#[global_allocator]
static ALLOCATOR: uefi::allocator::Allocator = uefi::allocator::Allocator;

#[cfg(target_os = "uefi")]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo<'_>) -> ! {
    serial::init();
    serial::log(format_args!("error: panic: {info}"));
    exit_qemu(0x11)
}

#[cfg(target_os = "uefi")]
#[uefi::entry]
fn main() -> uefi::Status {
    serial::init();
    serial::log(format_args!("RavenBSD loader 0.1 (x86_64)"));

    match run() {
        Ok(()) => exit_qemu(0x10),
        Err(error) => {
            serial::log(format_args!("error: {error}"));
            exit_qemu(0x11)
        }
    }
}

#[cfg(target_os = "uefi")]
fn run() -> Result<(), String> {
    let mut filesystem = FileSystem::new(
        uefi::boot::get_image_file_system(uefi::boot::image_handle())
            .map_err(|error| format!("cannot open the boot volume: {error}"))?,
    );

    let boot_toml = if fs::exists(&mut filesystem, fs::BOOT_TOML_PATH)? {
        fs::read_file(&mut filesystem, fs::BOOT_TOML_PATH)?
    } else {
        Vec::new()
    };
    let (config, warnings) = config::parse_with_warnings(&boot_toml)
        .map_err(|error| format!("bad boot.toml: {error}"))?;
    for warning in warnings {
        serial::log(format_args!("warning: {warning}"));
    }
    // S03 applies the level; parsing it here already validates the value.
    let _ = config.log_level;

    let kernel = fs::read_file(&mut filesystem, fs::KERNEL_PATH)?;
    let initfs = fs::read_file(&mut filesystem, fs::INITFS_PATH)?;
    let loaded = image::load(&kernel).map_err(|error| format!("cannot load kernel: {error}"))?;

    serial::log(format_args!(
        "kernel loaded phys={:#x} virt={:#x} size={:#x} entry={:#x}",
        loaded.phys_base, loaded.virt_base, loaded.size, loaded.entry
    ));
    serial::log(format_args!(
        "S02 done (segments={})",
        loaded.segments.len()
    ));
    let _ = initfs.len();
    Ok(())
}

#[cfg(target_os = "uefi")]
fn exit_qemu(code: u8) -> ! {
    // SAFETY: QEMU's isa-debug-exit device watches port 0xf4. On real hardware the port write is
    // harmless; the CPU then halts in a loop.
    unsafe {
        core::arch::asm!(
            "out dx, al",
            in("dx") 0xf4u16,
            in("al") code,
            options(nomem, nostack, preserves_flags),
        );
    }
    loop {
        // SAFETY: `cli; hlt` is valid in ring 0, touches no memory and preserves flags.
        unsafe {
            core::arch::asm!("cli", "hlt", options(nomem, nostack, preserves_flags));
        }
    }
}

#[cfg(not(target_os = "uefi"))]
fn main() {}
