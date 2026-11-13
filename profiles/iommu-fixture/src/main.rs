// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S18 VT-d fixture.
//!
//! The fixture is `/init` and holds the root device capability. It maps the
//! q35 ECAM and the `edu` BAR, allocates a DMA buffer (which binds the
//! allocation to the just-opened device), performs a real RAM <-> device
//! round trip through the resulting IOVA, then programs the device to an
//! unmapped IOVA when the domain is protected. A missing fault fails the run.

#![no_std]
#![no_main]

use core::arch::asm;

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::{Handle, prot};
use raven_rt::entry;
use raven_rt::println;

entry!(main);

const PAGE: usize = 4096;
const MIB: u64 = 1 << 20;
const ECAM_BUS_BYTES: u64 = 1 << 20;
const ECAM_DEVICE_STRIDE: usize = 0x8000;
const BLOB_ACPI: u32 = 1;
const BLOB_MAGIC: u32 = 0x3143_4152;
const BLOB_ENTRY: usize = 20;

const EDU_VENDOR: u16 = 0x1234;
const EDU_DEVICE: u16 = 0x11e8;
const EDU_DMA_SRC: usize = 0x80;
const EDU_DMA_DST: usize = 0x88;
const EDU_DMA_CNT: usize = 0x90;
const EDU_DMA_CMD: usize = 0x98;
const EDU_BUFFER: u32 = 0x40000;
const EDU_DMA_COUNT: u32 = 2048;
const EDU_CMD_RUN: u32 = 1;
const EDU_CMD_FROM: u32 = 2;

const DMA_LEN: u64 = MIB;
const DMA_MASK_BITS: u32 = 28;
const DMA_INFO_UNPROTECTED: u32 = 1;
const UNMAPPED_IOVA: u32 = 0x0800_0000;

#[repr(C)]
struct DmaInfo {
    bus_addr: u64,
    len: u64,
    flags: u32,
    pad: u32,
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

fn result(raw: isize) -> Result<usize, Errno> {
    if raw < 0 {
        Err(Errno(raw.unsigned_abs() as u16))
    } else {
        Ok(raw as usize)
    }
}

fn handle(raw: isize) -> Result<Handle, Errno> {
    result(raw).map(|value| Handle(u32::try_from(value).unwrap_or(0)))
}

fn boot_blob(root: Handle, kind: u32) -> Result<Handle, Errno> {
    handle(unsafe {
        syscall6(
            Syscall::BOOT_BLOB as usize,
            root.0 as usize,
            kind as usize,
            0,
            0,
            0,
            0,
        )
    })
}

fn mmio_create(root: Handle, physical: u64, len: usize) -> Result<Handle, Errno> {
    handle(unsafe {
        syscall6(
            Syscall::MMIO_CREATE as usize,
            root.0 as usize,
            physical as usize,
            len,
            0,
            0,
            0,
        )
    })
}

fn map_memory(process: Handle, object: Handle, len: usize, flags: usize) -> Result<usize, Errno> {
    result(unsafe {
        syscall6(
            Syscall::MAP as usize,
            process.0 as usize,
            object.0 as usize,
            0,
            0,
            len,
            flags,
        )
    })
}

fn dma_alloc(root: Handle, len: u64, align: u64, mask_bits: u32) -> Result<Handle, Errno> {
    handle(unsafe {
        syscall6(
            Syscall::DMA_ALLOC as usize,
            root.0 as usize,
            len as usize,
            align as usize,
            mask_bits as usize,
            0,
            0,
        )
    })
}

fn dma_info(buffer: Handle, out: &mut DmaInfo) -> Result<(), Errno> {
    result(unsafe {
        syscall6(
            Syscall::DMA_INFO as usize,
            buffer.0 as usize,
            out as *mut DmaInfo as usize,
            0,
            0,
            0,
            0,
        )
    })
    .map(|_| ())
}

fn nanosleep(ns: u64) {
    let _ = result(unsafe { syscall6(Syscall::NANOSLEEP as usize, ns as usize, 0, 0, 0, 0, 0) });
}

fn main(_args: raven_rt::Args) -> i32 {
    match run() {
        Ok(()) => {
            println!("s18: done");
            0
        }
        Err(message) => {
            println!("s18: {message}");
            1
        }
    }
}

fn run() -> Result<(), &'static str> {
    let root = Handle::ROOT_DEVICE;
    let acpi = boot_blob(root, BLOB_ACPI).map_err(|_| "acpi blob")?;
    let blob =
        map_memory(Handle::SELF_PROC, acpi, PAGE, prot::READ as usize).map_err(|_| "acpi map")?;
    if read_u32(blob) != BLOB_MAGIC {
        return Err("acpi magic");
    }
    let table_count = read_u32(blob + 16) as usize;
    let directory = read_u32(blob + 20) as usize;
    let mut mcfg_offset = 0usize;
    let mut index = 0usize;
    while index < table_count {
        let entry = directory
            .checked_add(index.saturating_mul(BLOB_ENTRY))
            .ok_or("blob entry overflow")?;
        if read_u32(blob + entry) == u32::from_le_bytes(*b"MCFG") {
            mcfg_offset = read_u32(blob + entry + 8) as usize;
            break;
        }
        index = index.saturating_add(1);
    }
    if mcfg_offset == 0 {
        return Err("no MCFG");
    }
    let ecam_base = read_u64(blob + mcfg_offset + 44);
    if ecam_base == 0 {
        return Err("no ECAM base");
    }

    let ecam_region =
        mmio_create(root, ecam_base, ECAM_BUS_BYTES as usize).map_err(|_| "ecam mmio")?;
    let ecam = map_memory(
        Handle::SELF_PROC,
        ecam_region,
        ECAM_BUS_BYTES as usize,
        prot::READ as usize,
    )
    .map_err(|_| "ecam map")?;
    let mut edu_config = None;
    let mut device = 0usize;
    while device < 32 {
        let config = ecam + device.saturating_mul(ECAM_DEVICE_STRIDE);
        let vendor = read_u16(config);
        if vendor == 0xFFFF {
            device = device.saturating_add(1);
            continue;
        }
        let ident = read_u16(config + 2);
        if vendor == EDU_VENDOR && ident == EDU_DEVICE {
            edu_config = Some(config);
            println!("s18: edu bdf=00:{:02x}.0", device);
            break;
        }
        device = device.saturating_add(1);
    }
    let Some(edu_config) = edu_config else {
        return Err("edu not found");
    };
    let bar0 = read_u32(edu_config + 0x10);
    if bar0 & 1 != 0 || bar0 == 0 {
        return Err("edu bar0 missing");
    }
    let edu_region = mmio_create(root, u64::from(bar0), PAGE).map_err(|_| "edu mmio")?;
    let edu = map_memory(
        Handle::SELF_PROC,
        edu_region,
        PAGE,
        (prot::READ | prot::WRITE) as usize,
    )
    .map_err(|_| "edu map")?;
    if read_u32(edu) != 0x0100_00ed {
        return Err("edu id");
    }

    let dma = dma_alloc(root, DMA_LEN, PAGE as u64, DMA_MASK_BITS).map_err(|_| "dma alloc")?;
    let mut info = DmaInfo {
        bus_addr: 0,
        len: 0,
        flags: 0,
        pad: 0,
    };
    dma_info(dma, &mut info).map_err(|_| "dma info")?;
    let protected = info.flags & DMA_INFO_UNPROTECTED == 0;
    if info.len != DMA_LEN {
        return Err("dma length differs");
    }
    if protected {
        if info.pad == 0 {
            return Err("protected domain has no id");
        }
        if info.bus_addr >= (1u64 << DMA_MASK_BITS) {
            return Err("IOVA exceeds device mask");
        }
        println!(
            "s18: dma bus={:#x} len={} domain={} protected=yes",
            info.bus_addr, info.len, info.pad
        );
    } else {
        println!("s18: fallback unprotected domain=0");
    }

    let dma_window = map_memory(
        Handle::SELF_PROC,
        dma,
        DMA_LEN as usize,
        (prot::READ | prot::WRITE) as usize,
    )
    .map_err(|_| "dma map")?;
    for page in 0..(DMA_LEN as usize / PAGE) {
        let base = dma_window + page * PAGE;
        for byte in 0..PAGE {
            write_u8(base + byte, (page.wrapping_add(byte) & 0xff) as u8);
        }
    }

    write_u32(edu + EDU_DMA_SRC, info.bus_addr as u32);
    write_u32(edu + EDU_DMA_DST, EDU_BUFFER);
    write_u32(edu + EDU_DMA_CNT, EDU_DMA_COUNT);
    write_u32(edu + EDU_DMA_CMD, EDU_CMD_RUN);
    if !dma_poll(edu) {
        return Err("edu ram->device timed out");
    }
    for byte in 0..EDU_DMA_COUNT as usize {
        write_u8(dma_window + byte, 0);
    }
    write_u32(edu + EDU_DMA_SRC, EDU_BUFFER);
    write_u32(edu + EDU_DMA_DST, info.bus_addr as u32);
    write_u32(edu + EDU_DMA_CNT, EDU_DMA_COUNT);
    write_u32(edu + EDU_DMA_CMD, EDU_CMD_RUN | EDU_CMD_FROM);
    if !dma_poll(edu) {
        return Err("edu device->ram timed out");
    }
    for byte in 0..EDU_DMA_COUNT as usize {
        let expected = (byte & 0xff) as u8;
        if read_u8(dma_window + byte) != expected {
            return Err("dma data differs");
        }
    }
    println!("s18: dma roundtrip ok");

    if protected {
        write_u32(edu + EDU_DMA_SRC, UNMAPPED_IOVA);
        write_u32(edu + EDU_DMA_DST, EDU_BUFFER);
        write_u32(edu + EDU_DMA_CNT, EDU_DMA_COUNT);
        write_u32(edu + EDU_DMA_CMD, EDU_CMD_RUN);
        let completed = dma_poll(edu);
        println!(
            "s18: fault injected iova={:#x} completed={}",
            UNMAPPED_IOVA, completed
        );
        // Let the VT-d fault worker drain the recording registers.
        nanosleep(200_000_000);
        let mut after = DmaInfo {
            bus_addr: 0,
            len: 0,
            flags: 0,
            pad: 0,
        };
        dma_info(dma, &mut after).map_err(|_| "post-fault dma info")?;
        println!("s18: after fault ok");
    } else {
        println!("s18: fallback no fault injection");
    }
    Ok(())
}

fn dma_poll(edu: usize) -> bool {
    let mut spins = 0u32;
    while read_u32_volatile(edu + EDU_DMA_CMD) & EDU_CMD_RUN != 0 {
        spins = spins.saturating_add(1);
        if spins > 10_000_000 {
            return false;
        }
        core::hint::spin_loop();
    }
    true
}

fn read_u8(address: usize) -> u8 {
    // SAFETY: the fixture reads its own mappings.
    unsafe { (address as *const u8).read_unaligned() }
}

fn read_u16(address: usize) -> u16 {
    // SAFETY: the fixture reads its own mappings.
    unsafe { (address as *const u16).read_unaligned() }
}

fn read_u32(address: usize) -> u32 {
    // SAFETY: the fixture reads its own mappings.
    unsafe { (address as *const u32).read_unaligned() }
}

fn read_u32_volatile(address: usize) -> u32 {
    // SAFETY: the fixture reads a naturally aligned device register in its own mapping.
    unsafe { (address as *const u32).read_volatile() }
}

fn read_u64(address: usize) -> u64 {
    // SAFETY: the fixture reads its own mappings.
    unsafe { (address as *const u64).read_unaligned() }
}

fn write_u8(address: usize, value: u8) {
    // SAFETY: the fixture writes its own mappings.
    unsafe {
        (address as *mut u8).write_unaligned(value);
    }
}

fn write_u32(address: usize, value: u32) {
    // SAFETY: the fixture writes its own device or DMA mapping.
    unsafe {
        (address as *mut u32).write_volatile(value);
    }
}
