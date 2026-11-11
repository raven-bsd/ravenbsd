// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S21a/S21b `/init` fixture: driver objects, the ECAM window, the edu device, DMA buffers and
//! the negative matrix.
//!
//! The controller holds `Handle::ROOT_DEVICE` (handle 3). It reads the ECAM base from the ACPI
//! blob, finds the q35 host bridge and the `edu` device, runs a DMA round trip through the edu
//! DMA engine against a 1 MiB buffer with a 28-bit mask, then the negative cases and the leak
//! test (children spawned with a reduced root capability).

#![no_std]
#![no_main]

use core::arch::asm;

use raven_abi::errno::Errno;
use raven_abi::layout::x86_64::{USER_END, USER_MIN};
use raven_abi::syscall::Syscall;
use raven_abi::types::{Handle, Rights, map, prot};
use raven_elf::{Elf, LoadMode, Machine, StepKind, plan_load};
use raven_initfs::Initfs;
use raven_rt::entry;
use raven_rt::println;

entry!(main);

const PAGE: usize = 4096;
const INITFS_BASE: usize = 0x1000_0000;
const STACK_SCRATCH: usize = 0x2000_0000;
const STACK_SIZE: usize = 64 * 1024;
const STACK_TOP: usize = (USER_END as usize) - PAGE;
const STACK_BASE: usize = STACK_TOP - STACK_SIZE;
const ECAM_BUS_BYTES: u64 = 1 << 20;
const ECAM_DEVICE_STRIDE: usize = 0x8000;
const MIB: u64 = 1 << 20;

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

/// edu DMA registers and device buffer.
const EDU_DMA_SRC: usize = 0x80;
const EDU_DMA_DST: usize = 0x88;
const EDU_DMA_CNT: usize = 0x90;
const EDU_DMA_CMD: usize = 0x98;
const EDU_BUFFER: u32 = 0x40000;
/// Bytes transferred per direction. QEMU 6.2 rejects a range that ends exactly at the device
/// buffer limit, so stay one page below the 4096-byte buffer.
const EDU_DMA_COUNT: u32 = 2048;
const EDU_CMD_RUN: u32 = 1;
const EDU_CMD_FROM: u32 = 2;

const DMA_FLAG_NON_CACHEABLE: u32 = 1;
const DMA_INFO_UNPROTECTED: u32 = 1;

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

fn yield_now() {
    let _ = syscall1(Syscall::YIELD as usize, 0);
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

fn dma_alloc(
    root: Handle,
    len: usize,
    align: usize,
    mask_bits: u32,
    flags: u32,
) -> Result<Handle, Errno> {
    handle(unsafe {
        syscall6(
            Syscall::DMA_ALLOC as usize,
            root.0 as usize,
            len,
            align,
            mask_bits as usize,
            flags as usize,
            0,
        )
    })
}

fn dma_info(buffer: Handle, out: &mut DmaInfo) -> Result<(), Errno> {
    result(syscall2(
        Syscall::DMA_INFO as usize,
        buffer.0 as usize,
        (out as *mut DmaInfo) as usize,
    ))
    .map(|_| ())
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

fn memobj_create(len: usize) -> Result<Handle, Errno> {
    handle(syscall2(Syscall::MEMOBJ_CREATE as usize, len, 0))
}

fn unmap(process: Handle, address: usize, len: usize) -> Result<(), Errno> {
    result(syscall3(
        Syscall::UNMAP as usize,
        process.0 as usize,
        address,
        len,
    ))
    .map(|_| ())
}

fn thread_create(process: Handle, entry: usize, stack: usize) -> Result<Handle, Errno> {
    handle(unsafe {
        syscall6(
            Syscall::THREAD_CREATE as usize,
            process.0 as usize,
            entry,
            stack,
            stack,
            0,
            0,
        )
    })
}

fn proc_wait(process: Handle) -> Result<i32, Errno> {
    let mut code = -1i32;
    result(syscall2(
        Syscall::PROC_WAIT as usize,
        process.0 as usize,
        &mut code as *mut i32 as usize,
    ))?;
    Ok(code)
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

/// The child role selected by `argv[1]`: the leak test children only run this path.
fn child_main(role: &str) -> i32 {
    match role {
        "clean" => 0,
        "holder" => {
            // The transferred root capability lives at handle 3 in the child.
            match dma_alloc(Handle::new(3, 0), MIB as usize, 0, 28, 0) {
                Ok(_) => 0,
                Err(_) => 1,
            }
        }
        _ => 2,
    }
}

fn main(args: raven_rt::Args) -> i32 {
    if let Some(role) = args.get(1) {
        return child_main(role);
    }

    let root = Handle::ROOT_DEVICE;
    let admin = match handle_dup(root, Rights::ADMIN.union(Rights::DUP).0) {
        Ok(handle) => handle,
        Err(_) => fail("root dup"),
    };

    // Part 1: ACPI blob, ECAM, edu identification.
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

    // Part 2: DMA buffers.
    if let Err(message) = dma_part(admin, edu) {
        fail(message);
    }

    // Leak test: 200 balanced cycles, then clean/holder/clean children. The kernel prints
    // `s21: frames free=` on every child exit (s21-framecheck cmdline).
    let image = match map_initfs_image() {
        Ok(image) => image,
        Err(_) => fail("initfs map"),
    };
    let initfs = match Initfs::parse(image) {
        Ok(initfs) => initfs,
        Err(_) => fail("initfs parse"),
    };
    let mut cycle = 0usize;
    while cycle < 200 {
        match dma_alloc(root, MIB as usize, 0, 28, 0) {
            Ok(buffer) => handle_close(buffer),
            Err(_) => fail("cycle alloc"),
        }
        cycle = cycle.saturating_add(1);
    }
    // Spawn, wait and close one child at a time so that each exit's frame print reflects the
    // previous child's released buffers.
    let mut ok = true;
    for (role, name) in [
        ("clean", "clean0"),
        ("holder", "holder"),
        ("clean", "clean1"),
    ] {
        let child = match spawn_role(&initfs, root, role) {
            Ok(child) => child,
            Err(_) => {
                println!("s21: failed spawn {name}");
                ok = false;
                break;
            }
        };
        let code = match proc_wait(child) {
            Ok(code) => code,
            Err(_) => {
                ok = false;
                break;
            }
        };
        if code != 0 {
            println!("s21: failed {name} exit={}", code);
            ok = false;
            break;
        }
        handle_close(child);
    }
    if !ok {
        return 62;
    }
    println!("s21: dma leak ok");
    0
}

fn dma_part(admin: Handle, edu: usize) -> Result<(), &'static str> {
    let buffer = dma_alloc(admin, MIB as usize, 0, 28, 0).map_err(|_| "dma alloc")?;
    let mut info = DmaInfo {
        bus_addr: 0,
        len: 0,
        flags: 0,
        pad: 0,
    };
    dma_info(buffer, &mut info).map_err(|_| "dma info")?;
    if info.len != MIB || info.flags & DMA_INFO_UNPROTECTED == 0 {
        return Err("dma info fields");
    }
    let pointer = map_memory(
        Handle::SELF_PROC,
        buffer,
        0,
        0,
        MIB as usize,
        map::pack(prot::READ | prot::WRITE, 0),
    )
    .map_err(|_| "dma map")?;
    // SAFETY: the kernel mapped the 1 MiB DMA buffer read/write at `pointer`.
    let bytes = unsafe { core::slice::from_raw_parts_mut(pointer as *mut u8, MIB as usize) };
    let mut i = 0usize;
    while i < bytes.len() {
        *bytes.get_mut(i).ok_or("dma pattern")? = (i as u8).wrapping_mul(31).wrapping_add(7);
        i = i.saturating_add(1);
    }
    println!("s21: dma ok bus={:#x} len={}", info.bus_addr, info.len);

    // RAM -> edu (cmd 1): the device reads the first EDU_DMA_COUNT bytes from the bus address.
    write_u32(edu + EDU_DMA_SRC, info.bus_addr as u32);
    write_u32(edu + EDU_DMA_DST, EDU_BUFFER);
    write_u32(edu + EDU_DMA_CNT, EDU_DMA_COUNT);
    write_u32(edu + EDU_DMA_CMD, EDU_CMD_RUN);
    if !dma_poll(edu) {
        return Err("dma to device timeout");
    }
    // Clear the copied region, then edu -> RAM (cmd 3) must restore the pattern.
    let mut i = 0usize;
    while i < EDU_DMA_COUNT as usize {
        *bytes.get_mut(i).ok_or("dma clear")? = 0;
        i = i.saturating_add(1);
    }
    write_u32(edu + EDU_DMA_SRC, EDU_BUFFER);
    write_u32(edu + EDU_DMA_DST, info.bus_addr as u32);
    write_u32(edu + EDU_DMA_CNT, EDU_DMA_COUNT);
    write_u32(edu + EDU_DMA_CMD, EDU_CMD_RUN | EDU_CMD_FROM);
    if !dma_poll(edu) {
        return Err("dma from device timeout");
    }
    let mut i = 0usize;
    while i < EDU_DMA_COUNT as usize {
        let expected = (i as u8).wrapping_mul(31).wrapping_add(7);
        if *bytes.get(i).ok_or("dma verify")? != expected {
            return Err("dma data mismatch");
        }
        i = i.saturating_add(1);
    }
    println!("s21: dma roundtrip ok");
    unmap(Handle::SELF_PROC, pointer, MIB as usize).map_err(|_| "dma unmap")?;
    handle_close(buffer);

    // Negatives.
    let no_admin = handle_dup(Handle::ROOT_DEVICE, Rights::DUP.0).map_err(|_| "dup rights")?;
    if dma_alloc(no_admin, PAGE, 0, 32, 0) != Err(Errno::EPERM) {
        return Err("dma without capability accepted");
    }
    if dma_alloc(admin, 0, 0, 32, 0) != Err(Errno::EINVAL) {
        return Err("zero len accepted");
    }
    if dma_alloc(admin, PAGE + 1, 0, 32, 0) != Err(Errno::EINVAL) {
        return Err("misaligned len accepted");
    }
    if dma_alloc(admin, PAGE, 0x3000, 32, 0) != Err(Errno::EINVAL) {
        return Err("non power of two align accepted");
    }
    if dma_alloc(admin, PAGE, 0x800, 32, 0) != Err(Errno::EINVAL) {
        return Err("sub-page align accepted");
    }
    if dma_alloc(admin, PAGE, 0, 19, 0) != Err(Errno::EINVAL) {
        return Err("low mask accepted");
    }
    if dma_alloc(admin, PAGE, 0, 65, 0) != Err(Errno::EINVAL) {
        return Err("high mask accepted");
    }
    if dma_alloc(admin, PAGE, 0, 32, 0x2) != Err(Errno::EINVAL) {
        return Err("unknown flags accepted");
    }
    // A 28-bit mask cannot hold 2 MiB.
    if dma_alloc(admin, 2 * MIB as usize, 0, 20, 0) != Err(Errno::ENOMEM) {
        return Err("above-mask allocation accepted");
    }
    // The per-process budget (64 MiB) is enforced.
    let first = dma_alloc(admin, 32 * MIB as usize, 0, 32, 0).map_err(|_| "limit a")?;
    let second = dma_alloc(admin, 32 * MIB as usize, 0, 32, 0).map_err(|_| "limit b")?;
    if dma_alloc(admin, PAGE, 0, 32, 0) != Err(Errno::ENOMEM) {
        return Err("per-process limit accepted");
    }
    handle_close(first);
    handle_close(second);
    let small = dma_alloc(admin, PAGE, 0, 32, 0).map_err(|_| "limit released")?;
    handle_close(small);
    // Non-cacheable buffers round trip through the CPU mapping.
    let uncached = dma_alloc(admin, PAGE, 0, 32, DMA_FLAG_NON_CACHEABLE).map_err(|_| "uncached")?;
    let uncached_ptr = map_memory(
        Handle::SELF_PROC,
        uncached,
        0,
        0,
        PAGE,
        map::pack(prot::READ | prot::WRITE, 0),
    )
    .map_err(|_| "uncached map")?;
    // SAFETY: the kernel mapped one page read/write at `uncached_ptr`.
    unsafe { (uncached_ptr as *mut u32).write_unaligned(0xDEAD_BEEF) };
    // SAFETY: the kernel mapped one page read/write at `uncached_ptr`.
    if unsafe { (uncached_ptr as *const u32).read_unaligned() } != 0xDEAD_BEEF {
        return Err("uncached roundtrip");
    }
    unmap(Handle::SELF_PROC, uncached_ptr, PAGE).map_err(|_| "uncached unmap")?;
    handle_close(uncached);
    // DMA_INFO requires a DMA buffer handle: the read-only Namespace handle is not one.
    let mut info = DmaInfo {
        bus_addr: 0,
        len: 0,
        flags: 0,
        pad: 0,
    };
    if dma_info(Handle::new(2, 0), &mut info) != Err(Errno::EBADF) {
        return Err("dma_info on non-buffer accepted");
    }
    println!("s21: dma negatives ok");
    Ok(())
}

fn dma_poll(edu: usize) -> bool {
    let mut spins = 0usize;
    while read_u32(edu + EDU_DMA_CMD) & EDU_CMD_RUN != 0 {
        spins = spins.saturating_add(1);
        if spins > 10_000_000 {
            return false;
        }
        yield_now();
    }
    true
}

fn map_initfs_image() -> Result<&'static [u8], Errno> {
    let page = map_memory(Handle::SELF_PROC, Handle::INITFS, 0, 0, PAGE, prot::READ)?;
    // SAFETY: the mapping is one readable page; the initfs header stores total_size at byte 40.
    let total = unsafe { ((page + 40) as *const u64).read_unaligned() as usize };
    let _ = unmap(Handle::SELF_PROC, page, PAGE);
    if total == 0 || !total.is_multiple_of(PAGE) {
        return Err(Errno::EINVAL);
    }
    let base = map_memory(
        Handle::SELF_PROC,
        Handle::INITFS,
        0,
        INITFS_BASE,
        total,
        map::pack(prot::READ, map::FIXED),
    )?;
    // SAFETY: the kernel mapped the validated initfs bytes at `base` for `total` bytes.
    Ok(unsafe { core::slice::from_raw_parts(base as *const u8, total) })
}

/// Spawn a child running this binary with `argv[1] = role` and a reduced root capability.
fn spawn_role(initfs: &Initfs<'_>, root: Handle, role: &str) -> Result<Handle, Errno> {
    let entry = initfs.get("/init").ok_or(Errno::ENOENT)?;
    let elf =
        Elf::parse(entry.data, Machine::X86_64, USER_MIN..USER_END).map_err(|_| Errno::EINVAL)?;
    let child = proc_create()?;
    let steps = plan_load(&elf, PAGE as u64, LoadMode::Strict).map_err(|_| Errno::EINVAL)?;
    for step in steps {
        let flags = map::pack(step.prot as usize, map::FIXED);
        match step.kind {
            StepKind::File { offset } => {
                let offset = entry.data_offset.checked_add(offset).ok_or(Errno::EINVAL)?;
                let offset = usize::try_from(offset).map_err(|_| Errno::EINVAL)?;
                map_memory(
                    child,
                    Handle::INITFS,
                    offset,
                    step.vaddr as usize,
                    step.len as usize,
                    flags,
                )?;
            }
            StepKind::Zero => {
                map_memory(
                    child,
                    Handle::NONE,
                    0,
                    step.vaddr as usize,
                    step.len as usize,
                    flags,
                )?;
            }
            StepKind::CopyTail { .. } => return Err(Errno::EINVAL),
        }
    }
    let stack = memobj_create(STACK_SIZE)?;
    let scratch = map_memory(
        Handle::SELF_PROC,
        stack,
        0,
        STACK_SCRATCH,
        STACK_SIZE,
        map::pack(prot::READ | prot::WRITE, map::FIXED | map::SHARED),
    )?;
    map_memory(
        child,
        stack,
        0,
        STACK_BASE,
        STACK_SIZE,
        map::pack(prot::READ | prot::WRITE, map::FIXED | map::SHARED),
    )?;
    let stack_pointer = build_stack(scratch, role.as_bytes());
    let _ = handle_transfer(child, root, Rights::ADMIN.0)?;
    let thread = thread_create(child, elf.entry() as usize, stack_pointer)?;
    let _ = handle_close(thread);
    unmap(Handle::SELF_PROC, STACK_SCRATCH, STACK_SIZE)?;
    let _ = handle_close(stack);
    Ok(child)
}

fn build_stack(base: usize, role: &[u8]) -> usize {
    let stack = STACK_TOP - 0x100;
    let argv0 = STACK_TOP - 0x40;
    let argv1 = argv0 + 2;
    let mut offset = stack - STACK_BASE;
    let words = [
        2u64,
        argv0 as u64,
        argv1 as u64,
        0u64,
        0u64,
        6u64,
        PAGE as u64,
        0u64,
        0u64,
    ];
    for value in words {
        // SAFETY: the shared stack mapping is writable and `offset` stays inside its fixed size.
        unsafe { ((base + offset) as *mut u64).write_unaligned(value.to_le()) };
        offset += 8;
    }
    // SAFETY: the strings are inside the writable shared stack mapping.
    unsafe {
        ((base + STACK_TOP - STACK_BASE - 0x40) as *mut u8).write(b'x');
        ((base + STACK_TOP - STACK_BASE - 0x3f) as *mut u8).write(0);
        let role_start = base + STACK_TOP - STACK_BASE - 0x40 + 2;
        let mut index = 0usize;
        while index < role.len() {
            ((role_start + index) as *mut u8).write(*role.get(index).unwrap_or(&0));
            index = index.saturating_add(1);
        }
        ((role_start + role.len()) as *mut u8).write(0);
    }
    stack
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
