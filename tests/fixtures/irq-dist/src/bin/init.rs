// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S75a IRQ-distribution fixture.
//!
//! The controller finds the QEMU `edu` PCI device through the ECAM window, creates two MSI
//! interrupt objects and drives 1000 counted firings through each. With `smp_user=on` the kernel's
//! per-IRQ destination policy routes the messages to application processors; the kernel prints
//! `irq-dist: vector N -> cpu M` at creation and `irq-dist: vector N handled on cpu M` on the first
//! firing. With `smp_user=off` both stay on the BSP.

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

const EDU_ID: u32 = 0x0100_00ed;
const EDU_VENDOR: u16 = 0x1234;
const EDU_DEVICE: u16 = 0x11e8;

const IRQ_MSI: u32 = 2;
const FIRINGS: u64 = 1000;

#[repr(C)]
struct IrqInfo {
    vector: u32,
    address_lo: u32,
    address_hi: u32,
    data: u32,
}

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

fn result(raw: isize) -> Result<usize, Errno> {
    Errno::from_ret(raw)
}

fn handle(raw: isize) -> Result<Handle, Errno> {
    let value = result(raw)?;
    u32::try_from(value).map(Handle).map_err(|_| Errno::EINVAL)
}

fn fail(message: &str) -> ! {
    println!("irq-dist: failed {message}");
    exit(1)
}

fn exit(code: i32) -> ! {
    let _ = syscall1(Syscall::EXIT as usize, code as usize);
    loop {
        core::hint::spin_loop();
    }
}

fn mmio_create(root: Handle, phys: u64, len: usize) -> Result<Handle, Errno> {
    // SAFETY: MMIO_CREATE takes scalar arguments; the kernel validates the range and root right.
    handle(unsafe {
        syscall6(
            Syscall::MMIO_CREATE as usize,
            root.0 as usize,
            phys as usize,
            len,
            0,
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
    // SAFETY: MAP takes scalar arguments; the kernel validates the handles and range.
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

fn irq_create(
    root: Handle,
    kind: u32,
    number: u32,
    info: Option<&mut IrqInfo>,
) -> Result<Handle, Errno> {
    let flags = match info {
        Some(info) => (info as *mut IrqInfo) as usize,
        None => 0,
    };
    // SAFETY: IRQ_CREATE takes scalar arguments plus one optional pointer; the kernel validates
    // the root right and fills `info` through usercopy.
    handle(unsafe {
        syscall6(
            Syscall::IRQ_CREATE as usize,
            root.0 as usize,
            kind as usize,
            number as usize,
            flags,
            0,
            0,
        )
    })
}

fn irq_wait(irq: Handle, timeout_ns: u64) -> Result<usize, Errno> {
    result(syscall2(
        Syscall::IRQ_WAIT as usize,
        irq.0 as usize,
        timeout_ns as usize,
    ))
}

fn read_u8(address: usize) -> u8 {
    // SAFETY: the fixture reads its own device mappings, which the kernel mapped read/write.
    unsafe { (address as *const u8).read() }
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

fn write_u32(address: usize, value: u32) {
    // SAFETY: the fixture writes its own device mappings, which the kernel mapped read/write.
    unsafe { (address as *mut u32).write_unaligned(value.to_le()) }
}

fn write_u16(address: usize, value: u16) {
    // SAFETY: the fixture writes its own device mappings, which the kernel mapped read/write.
    unsafe { (address as *mut u16).write_unaligned(value.to_le()) }
}

/// Locate the MSI capability of the edu device and program it with the kernel's message.
fn program_msi(config: usize, info: &IrqInfo) -> Result<(), &'static str> {
    let mut cap = usize::from(read_u8(config + 0x34) & 0xFC);
    let mut msi = 0usize;
    while cap != 0 {
        if read_u8(config + cap) == 0x05 {
            msi = cap;
            break;
        }
        cap = usize::from(read_u8(config + cap + 1) & 0xFC);
    }
    if msi == 0 {
        return Err("no msi capability");
    }
    let control = read_u16(config + msi + 2);
    let wide = control & (1 << 7) != 0;
    write_u32(config + msi + 4, info.address_lo);
    if wide {
        write_u32(config + msi + 8, info.address_hi);
        write_u32(config + msi + 12, info.data);
    } else {
        write_u32(config + msi + 8, info.data);
    }
    write_u16(config + msi + 2, control | 1);
    Ok(())
}

fn device_ack(edu: usize) {
    write_u32(edu + 0x64, 0x1);
}

/// Create an MSI object, program the edu device and count `FIRINGS` firings.
fn run_msi(admin: Handle, edu: usize, edu_config: usize) -> Result<(), &'static str> {
    let mut info = IrqInfo {
        vector: 0,
        address_lo: 0,
        address_hi: 0,
        data: 0,
    };
    let msi = irq_create(admin, IRQ_MSI, 0, Some(&mut info)).map_err(|_| "msi create")?;
    program_msi(edu_config, &info)?;
    let mut count = 0u64;
    while count < FIRINGS {
        write_u32(edu + 0x60, 0x1);
        let got = irq_wait(msi, 5_000_000_000).map_err(|_| "msi wait")?;
        if got == 0 {
            return Err("msi timeout");
        }
        device_ack(edu);
        count = count.saturating_add(got as u64);
    }
    if count != FIRINGS {
        return Err("msi count drift");
    }
    println!("irq-dist: msi vector={} count={}", info.vector, count);
    handle_close(msi);
    Ok(())
}

fn main(_args: raven_rt::Args) -> i32 {
    let root = Handle::ROOT_DEVICE;
    let admin = match handle_dup(root, Rights::ADMIN.union(Rights::DUP).0) {
        Ok(handle) => handle,
        Err(_) => fail("root dup"),
    };

    // ACPI blob -> MCFG -> ECAM base.
    let acpi_blob = match boot_blob(admin, BLOB_ACPI) {
        Ok(blob) => blob,
        Err(_) => fail("acpi blob"),
    };
    let blob_ptr = match map_memory(Handle::SELF_PROC, acpi_blob, 0, 0, PAGE, prot::READ) {
        Ok(address) => address,
        Err(_) => fail("blob map"),
    };
    if read_u32(blob_ptr) != BLOB_MAGIC {
        fail("blob magic");
    }
    let count = read_u32(blob_ptr + 16) as usize;
    let mut mcfg_offset = 0usize;
    let mut index = 0usize;
    while index < count {
        let entry = blob_ptr + BLOB_HEADER + index * BLOB_ENTRY;
        if read_u32(entry) == u32::from_le_bytes(*b"MCFG") {
            mcfg_offset = read_u32(entry + 8) as usize;
        }
        index = index.saturating_add(1);
    }
    if mcfg_offset == 0 {
        fail("mcfg entry");
    }
    let mcfg = blob_ptr + mcfg_offset;
    let ecam_base = read_u64(mcfg + 44);
    if ecam_base == 0 {
        fail("ecam base");
    }

    let ecam_region = match mmio_create(admin, ecam_base, ECAM_BUS_BYTES as usize) {
        Ok(region) => region,
        Err(_) => fail("ecam mmio"),
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

    let mut edu_device = None;
    let mut device = 0usize;
    while device < 32 {
        let config = ecam + device * ECAM_DEVICE_STRIDE;
        let vendor = read_u16(config);
        let ident = read_u16(config + 2);
        if vendor == EDU_VENDOR && ident == EDU_DEVICE {
            edu_device = Some(device);
        }
        if vendor == 0xFFFF {
            break;
        }
        device = device.saturating_add(1);
    }
    let Some(edu_device) = edu_device else {
        fail("edu not found");
    };
    let edu_config = ecam + edu_device * ECAM_DEVICE_STRIDE;
    let bar0 = read_u32(edu_config + 0x10);
    if bar0 & 1 != 0 || bar0 & 0x6 != 0 {
        fail("edu bar0");
    }
    let bar0_phys = u64::from(bar0 & 0xFFFF_FFF0);
    let edu_region = match mmio_create(admin, bar0_phys, PAGE) {
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
    if read_u32(edu) != EDU_ID {
        fail("edu id");
    }

    // Two MSI objects: the round-robin policy moves the destination across APs when smp_user=on.
    if let Err(message) = run_msi(admin, edu, edu_config) {
        fail(message);
    }
    if let Err(message) = run_msi(admin, edu, edu_config) {
        fail(message);
    }

    println!("irq-dist: done");
    0
}
