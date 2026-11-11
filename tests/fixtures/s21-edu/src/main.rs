// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S21a `/init` fixture: driver objects, the ECAM window, the edu device and the negative matrix.
//!
//! The controller holds `Handle::ROOT_DEVICE` (handle 3). It reads the ECAM base from the ACPI
//! blob, maps it through `MMIO_CREATE`, finds the q35 host bridge (`8086:29c0`) and the `edu`
//! device (`1234:11e8`), reads its BAR0 and checks the identification register, then runs the
//! negative cases.

#![no_std]
#![no_main]

use core::arch::asm;

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::{Handle, Rights, map, prot};
use raven_rt::entry;
use raven_rt::println;

entry!(main);

const PAGE: usize = 4096;
const ECAM_BUS_BYTES: u64 = 1 << 20;
const ECAM_DEVICE_STRIDE: usize = 0x8000;

const BLOB_MAGIC: u32 = 0x3143_4152;
const BLOB_HEADER: usize = 24;
const BLOB_ENTRY: usize = 20;
const BLOB_ACPI: u32 = 1;
const BLOB_DTB: u32 = 2;
const BLOB_MEMORY_MAP: u32 = 3;

const EDU_ID: u32 = 0x0100_00ed;
const EDU_VENDOR: u16 = 0x1234;
const EDU_DEVICE: u16 = 0x11e8;
const Q35_VENDOR: u16 = 0x8086;
const Q35_DEVICE: u16 = 0x29c0;

#[inline(always)]
unsafe fn syscall6(
    number: usize,
    a0: usize,
    a1: usize,
    a2: usize,
    a3: usize,
    a4: usize,
    a5: usize,
) -> isize {
    let result: isize;
    // SAFETY: the caller passes valid registers for the RavenBSD ABI; the kernel validates every
    // pointer and handle.
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") number => result,
            in("rdi") a0,
            in("rsi") a1,
            in("rdx") a2,
            in("r10") a3,
            in("r8") a4,
            in("r9") a5,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack),
        );
    }
    result
}

fn syscall1(number: usize, a0: usize) -> isize {
    // SAFETY: scalar arguments follow the RavenBSD syscall ABI.
    unsafe { syscall6(number, a0, 0, 0, 0, 0, 0) }
}

fn syscall2(number: usize, a0: usize, a1: usize) -> isize {
    // SAFETY: scalar arguments follow the RavenBSD syscall ABI.
    unsafe { syscall6(number, a0, a1, 0, 0, 0, 0) }
}

fn syscall3(number: usize, a0: usize, a1: usize, a2: usize) -> isize {
    // SAFETY: scalar arguments follow the RavenBSD syscall ABI.
    unsafe { syscall6(number, a0, a1, a2, 0, 0, 0) }
}

fn result(raw: isize) -> Result<usize, Errno> {
    Errno::from_ret(raw)
}

fn handle(raw: isize) -> Result<Handle, Errno> {
    let value = result(raw)?;
    u32::try_from(value).map(Handle).map_err(|_| Errno::EINVAL)
}

fn fail(message: &str) -> ! {
    println!("s21: failed {message}");
    exit(1)
}

fn exit(code: i32) -> ! {
    let _ = syscall1(Syscall::EXIT as usize, code as usize);
    loop {
        core::hint::spin_loop();
    }
}

fn mmio_create(root: Handle, phys: u64, len: usize, attrs: u32) -> Result<Handle, Errno> {
    handle(unsafe {
        syscall6(
            Syscall::MMIO_CREATE as usize,
            root.0 as usize,
            phys as usize,
            len,
            attrs as usize,
            0,
            0,
        )
    })
}

fn boot_blob(root: Handle, kind: u32) -> Result<Handle, Errno> {
    handle(syscall2(
        Syscall::BOOT_BLOB as usize,
        root.0 as usize,
        kind as usize,
    ))
}

fn map_memory(
    process: Handle,
    object: Handle,
    offset: usize,
    address: usize,
    len: usize,
    flags: usize,
) -> Result<usize, Errno> {
    result(unsafe {
        syscall6(
            Syscall::MAP as usize,
            process.0 as usize,
            object.0 as usize,
            offset,
            address,
            len,
            flags,
        )
    })
}

fn handle_dup(handle: Handle, mask: u32) -> Result<Handle, Errno> {
    let raw = syscall2(
        Syscall::HANDLE_DUP as usize,
        handle.0 as usize,
        mask as usize,
    );
    let value = result(raw)?;
    u32::try_from(value).map(Handle).map_err(|_| Errno::EINVAL)
}

fn handle_close(handle: Handle) {
    let _ = syscall1(Syscall::HANDLE_CLOSE as usize, handle.0 as usize);
}

fn handle_transfer(process: Handle, object: Handle, mask: u32) -> Result<Handle, Errno> {
    let raw = syscall3(
        Syscall::HANDLE_TRANSFER as usize,
        process.0 as usize,
        object.0 as usize,
        mask as usize,
    );
    let value = result(raw)?;
    u32::try_from(value).map(Handle).map_err(|_| Errno::EINVAL)
}

fn proc_create() -> Result<Handle, Errno> {
    handle(syscall1(Syscall::PROC_CREATE as usize, 0))
}

fn read_u16(address: usize) -> u16 {
    // SAFETY: the fixture reads its own device mappings, which the kernel mapped read/write.
    u16::from_le(unsafe { (address as *const u16).read_unaligned() })
}

fn read_u32(address: usize) -> u32 {
    // SAFETY: the fixture reads its own device mappings, which the kernel mapped read/write.
    u32::from_le(unsafe { (address as *const u32).read_unaligned() })
}

fn read_u64(address: usize) -> u64 {
    // SAFETY: the fixture reads its own blob mapping, which the kernel mapped read-only.
    u64::from_le(unsafe { (address as *const u64).read_unaligned() })
}

fn main(_args: raven_rt::Args) -> i32 {
    let root = Handle::ROOT_DEVICE;
    let admin = match handle_dup(root, Rights::ADMIN.union(Rights::DUP).0) {
        Ok(handle) => handle,
        Err(_) => fail("root dup"),
    };

    let acpi_blob = match boot_blob(admin, BLOB_ACPI) {
        Ok(blob) => blob,
        Err(_) => fail("acpi blob"),
    };
    let blob_ptr = match map_memory(
        Handle::SELF_PROC,
        acpi_blob,
        0,
        0,
        PAGE,
        prot::READ as usize,
    ) {
        Ok(address) => address,
        Err(_) => fail("blob map"),
    };
    let magic = read_u32(blob_ptr);
    if magic != BLOB_MAGIC {
        fail("blob magic");
    }
    let count = read_u32(blob_ptr + 16) as usize;
    if count == 0 || count > 128 {
        fail("blob count");
    }
    let mut mcfg_offset = 0usize;
    let mut mcfg_length = 0usize;
    let mut index = 0usize;
    while index < count {
        let entry = blob_ptr + BLOB_HEADER + index * BLOB_ENTRY;
        if read_u32(entry) == u32::from_le_bytes(*b"MCFG") {
            mcfg_offset = read_u32(entry + 8) as usize;
            mcfg_length = read_u32(entry + 12) as usize;
        }
        index = index.saturating_add(1);
    }
    if mcfg_offset == 0 || mcfg_length < 56 || mcfg_offset.saturating_add(mcfg_length) > PAGE {
        fail("mcfg entry");
    }
    let mcfg = blob_ptr + mcfg_offset;
    let ecam_base = read_u64(mcfg + 44);
    let segment = read_u16(mcfg + 52);
    // SAFETY: the MCFG copy lives inside the mapped read-only blob page range.
    let start_bus = u16::from(unsafe { ((mcfg + 54) as *const u8).read() });
    // SAFETY: the MCFG copy lives inside the mapped read-only blob page range.
    let end_bus = u16::from(unsafe { ((mcfg + 55) as *const u8).read() });
    if segment != 0 || start_bus > 0 || end_bus < 1 || ecam_base == 0 {
        fail("mcfg range");
    }

    let ecam_region = match mmio_create(admin, ecam_base, ECAM_BUS_BYTES as usize, 0) {
        Ok(region) => region,
        Err(error) => {
            println!("s21: failed ecam mmio errno={}", error.0);
            return 60;
        }
    };
    let ecam = match map_memory(
        Handle::SELF_PROC,
        ecam_region,
        0,
        0,
        ECAM_BUS_BYTES as usize,
        map::pack(prot::READ | prot::WRITE, 0),
    ) {
        Ok(address) => address,
        Err(_) => fail("ecam map"),
    };

    let mut host_bridge = false;
    let mut edu_device = None;
    let mut device = 0usize;
    while device < 32 {
        let config = ecam + device * ECAM_DEVICE_STRIDE;
        let vendor = read_u16(config);
        let ident = read_u16(config + 2);
        if vendor == Q35_VENDOR && ident == Q35_DEVICE {
            host_bridge = true;
            println!("s21: ecam ok vendor={:#x}", vendor);
        }
        if vendor == EDU_VENDOR && ident == EDU_DEVICE {
            edu_device = Some(device);
        }
        if vendor == 0xFFFF {
            break;
        }
        device = device.saturating_add(1);
    }
    if !host_bridge {
        fail("host bridge");
    }
    let Some(edu_device) = edu_device else {
        fail("edu not found");
    };
    let edu_config = ecam + edu_device * ECAM_DEVICE_STRIDE;
    let bar0 = read_u32(edu_config + 0x10);
    if bar0 & 1 != 0 || bar0 & 0x6 != 0 {
        fail("edu bar0 io");
    }
    let bar0_phys = u64::from(bar0 & 0xFFFF_FFF0);
    let edu_region = match mmio_create(admin, bar0_phys, PAGE, 0) {
        Ok(region) => region,
        Err(_) => fail("edu mmio"),
    };
    let edu = match map_memory(
        Handle::SELF_PROC,
        edu_region,
        0,
        0,
        PAGE,
        map::pack(prot::READ | prot::WRITE, 0),
    ) {
        Ok(address) => address,
        Err(_) => fail("edu map"),
    };
    let id = read_u32(edu);
    if id != EDU_ID {
        println!("s21: failed edu id={:#010x}", id);
        return 61;
    }
    println!("s21: edu id={:#010x}", id);

    // The root capability travels to a child with a reduced mask before its first thread.
    let child = match proc_create() {
        Ok(child) => child,
        Err(_) => fail("proc create"),
    };
    let transferred = match handle_transfer(child, root, Rights::ADMIN.union(Rights::DUP).0) {
        Ok(transferred) => transferred,
        Err(_) => fail("root transfer"),
    };
    if transferred != Handle::new(3, 0) {
        fail("root transfer index");
    }
    handle_close(child);
    println!("s21: transfer ok");

    if let Err(message) = negatives(admin, ecam_base, bar0_phys, acpi_blob) {
        fail(message);
    }
    println!("s21: negatives ok");
    0
}

fn negatives(
    admin: Handle,
    ecam_base: u64,
    bar0_phys: u64,
    acpi_blob: Handle,
) -> Result<(), &'static str> {
    // No capability: a duplicate without ADMIN, and a non-root handle, must be refused.
    let no_admin = handle_dup(Handle::ROOT_DEVICE, Rights::DUP.0).map_err(|_| "dup rights")?;
    if mmio_create(no_admin, ecam_base, PAGE, 0) != Err(Errno::EPERM) {
        return Err("no capability accepted");
    }
    if mmio_create(Handle::new(2, 0), ecam_base, PAGE, 0) != Err(Errno::EPERM) {
        return Err("namespace capability accepted");
    }
    // RAM must never become a device window.
    if mmio_create(admin, 0x10_0000, PAGE, 0) != Err(Errno::EPERM) {
        return Err("ram accepted");
    }
    // The kernel image is refused via the memory-map summary.
    let map_blob = boot_blob(admin, BLOB_MEMORY_MAP).map_err(|_| "map blob")?;
    let map_ptr = map_memory(Handle::SELF_PROC, map_blob, 0, 0, PAGE, prot::READ as usize)
        .map_err(|_| "map blob map")?;
    let mut kernel_base = 0u64;
    let mut record = 0usize;
    while record + 24 <= PAGE {
        let at = map_ptr + record;
        let base = read_u64(at);
        let len = read_u64(at + 8);
        let kind = read_u32(at + 16);
        if base == 0 && len == 0 {
            break;
        }
        if kind == 2 {
            kernel_base = base;
        }
        record = record.saturating_add(24);
    }
    if kernel_base != 0 && mmio_create(admin, kernel_base, PAGE, 0) != Err(Errno::EPERM) {
        return Err("kernel image accepted");
    }
    // Alignment, length and attribute validation.
    if mmio_create(admin, ecam_base + 1, PAGE, 0) != Err(Errno::EINVAL) {
        return Err("misaligned phys accepted");
    }
    if mmio_create(admin, ecam_base, PAGE + 1, 0) != Err(Errno::EINVAL) {
        return Err("misaligned len accepted");
    }
    if mmio_create(admin, ecam_base, 0, 0) != Err(Errno::EINVAL) {
        return Err("zero len accepted");
    }
    if mmio_create(admin, ecam_base, PAGE, 1) != Err(Errno::EINVAL) {
        return Err("unknown attrs accepted");
    }
    if mmio_create(admin, 0xFFFF_FFFF_FFFF_F000, 2 * PAGE, 0) != Err(Errno::EINVAL) {
        return Err("overflow accepted");
    }
    // Mappings must stay inside the region and never be executable.
    let region = mmio_create(admin, bar0_phys, PAGE, 0).map_err(|_| "region")?;
    let writable = map::pack(prot::READ | prot::WRITE, 0);
    if map_memory(Handle::SELF_PROC, region, PAGE, 0, PAGE, writable) != Err(Errno::EINVAL) {
        return Err("offset past region accepted");
    }
    if map_memory(Handle::SELF_PROC, region, 0, 0, 2 * PAGE, writable) != Err(Errno::EINVAL) {
        return Err("length past region accepted");
    }
    if map_memory(Handle::SELF_PROC, region, 0x100, 0, PAGE, writable) != Err(Errno::EINVAL) {
        return Err("unaligned offset accepted");
    }
    if map_memory(
        Handle::SELF_PROC,
        region,
        0,
        0,
        PAGE,
        map::pack(prot::READ | prot::EXEC, 0),
    ) != Err(Errno::EINVAL)
    {
        return Err("executable mmio accepted");
    }
    // Boot blobs are read-only.
    if map_memory(
        Handle::SELF_PROC,
        acpi_blob,
        0,
        0,
        PAGE,
        map::pack(prot::READ | prot::WRITE, 0),
    ) != Err(Errno::EPERM)
    {
        return Err("writable blob accepted");
    }
    if boot_blob(admin, BLOB_DTB) != Err(Errno::ENOENT) {
        return Err("dtb blob expected ENOENT");
    }
    Ok(())
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    exit(2)
}
