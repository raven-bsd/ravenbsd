// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S21d `/init` fixture: driver objects on aarch64.
//!
//! The fixture holds `Handle::ROOT_DEVICE` (handle 3). It scans the QEMU `virt` PCIe ECAM with an
//! `MmioRegion`, finds the `edu` device and reads its identification register, allocates and maps
//! a Normal-NC DMA buffer (checking the bus address and the mask rejection), routes the PL031 RTC
//! SPI through the GIC-400 and waits for real interrupts, then exercises teardown and the
//! negative matrix.

#![no_std]
#![no_main]

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::{Handle, map, prot};
use raven_rt::entry;
use raven_rt::println;
use raven_rt::sys::{exit, handle_close, syscall1, syscall2, syscall6};

entry!(main);

const PAGE: usize = 4096;
const ROOT: Handle = Handle::ROOT_DEVICE;
/// QEMU `virt` PCIe ECAM (high memory, `pci-host-ecam-generic`).
const ECAM_BASE: u64 = 0x40_1000_0000;
/// Bus 0 covers 32 devices x 8 functions x 4 KiB.
const ECAM_BUS_BYTES: usize = 1 << 20;
const EDU_VENDOR: u16 = 0x1234;
const EDU_DEVICE: u16 = 0x11e8;
const EDU_ID: u32 = 0x0100_00ed;
/// QEMU `virt` PL031 RTC.
const PL031_BASE: u64 = 0x0901_0000;
/// PL031 GIC interrupt id: the tree's SPI 2 plus the 32-line SPI base.
const PL031_IRQ: u32 = 34;
const PL031_RTCMR: usize = 0x04;
const PL031_RTCCR: usize = 0x0c;
const PL031_RTCIMSC: usize = 0x10;
const PL031_RTCICR: usize = 0x14;
const IRQ_LEGACY: u32 = 1;
const IRQ_MSI: u32 = 2;
const DMA_LEN: usize = 1 << 20;
const DMA_MASK_BITS: u32 = 32;

#[repr(C)]
struct DmaInfo {
    bus_addr: u64,
    len: u64,
    flags: u32,
    pad: u32,
}

fn result(raw: isize) -> Result<usize, Errno> {
    Errno::from_ret(raw)
}

fn handle(raw: isize) -> Result<Handle, Errno> {
    let value = result(raw)?;
    u32::try_from(value).map(Handle).map_err(|_| Errno::EINVAL)
}

fn mmio_create(root: Handle, physical: u64, len: usize, attrs: u32) -> Result<Handle, Errno> {
    // SAFETY: scalar arguments follow the RavenBSD syscall ABI; the kernel validates the range.
    handle(unsafe {
        syscall6(
            Syscall::MMIO_CREATE as usize,
            root.0 as usize,
            physical as usize,
            len,
            attrs as usize,
            0,
            0,
        )
    })
}

fn dma_alloc(
    root: Handle,
    len: usize,
    align: usize,
    mask_bits: u32,
    flags: u32,
) -> Result<Handle, Errno> {
    // SAFETY: scalar arguments follow the RavenBSD syscall ABI.
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
    // SAFETY: `out` points at a live `DmaInfo` in this process and the kernel validates it.
    result(unsafe {
        syscall2(
            Syscall::DMA_INFO as usize,
            buffer.0 as usize,
            out as *mut DmaInfo as usize,
        )
    })
    .map(|_| ())
}

fn irq_create(root: Handle, kind: u32, number: u32, flags: usize) -> Result<Handle, Errno> {
    // SAFETY: scalar arguments follow the RavenBSD syscall ABI; aarch64 legacy has flags = 0.
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
    // SAFETY: scalar arguments follow the RavenBSD syscall ABI.
    result(unsafe {
        syscall2(
            Syscall::IRQ_WAIT as usize,
            irq.0 as usize,
            timeout_ns as usize,
        )
    })
}

fn irq_ack(irq: Handle) -> Result<(), Errno> {
    // SAFETY: scalar arguments follow the RavenBSD syscall ABI.
    result(unsafe { syscall1(Syscall::IRQ_ACK as usize, irq.0 as usize) }).map(|_| ())
}

fn map_region(object: Handle, len: usize) -> Result<usize, Errno> {
    result(unsafe {
        // SAFETY: scalar arguments follow the RavenBSD syscall ABI; the mapping is private and
        // read/write, and the kernel chooses a free virtual range.
        syscall6(
            Syscall::MAP as usize,
            Handle::SELF_PROC.0 as usize,
            object.0 as usize,
            0,
            0,
            len,
            map::pack(prot::READ | prot::WRITE, 0),
        )
    })
}

fn read_u8(address: usize) -> u8 {
    // SAFETY: `address` is inside a mapping created by `MAP` for the fixture's own address space.
    unsafe { (address as *const u8).read_volatile() }
}

fn read_u16(address: usize) -> u16 {
    // SAFETY: `address` is inside a mapping created by `MAP` for the fixture's own address space.
    unsafe { (address as *const u16).read_volatile() }
}

fn read_u32(address: usize) -> u32 {
    // SAFETY: `address` is inside a mapping created by `MAP` for the fixture's own address space.
    unsafe { (address as *const u32).read_volatile() }
}

fn write_u8(address: usize, value: u8) {
    // SAFETY: `address` is inside a mapping created by `MAP` for the fixture's own address space.
    unsafe { (address as *mut u8).write_volatile(value) }
}

fn write_u16(address: usize, value: u16) {
    // SAFETY: `address` is inside the ECAM mapping; the register is 2-byte aligned.
    unsafe { (address as *mut u16).write_volatile(value) }
}

fn write_u32(address: usize, value: u32) {
    // SAFETY: `address` is inside a device mapping created by `MAP`; the register is 4-byte aligned.
    unsafe { (address as *mut u32).write_volatile(value) }
}

fn expect_err<T>(result: Result<T, Errno>, expected: Errno) -> Result<(), Errno> {
    match result {
        Err(error) if error == expected => Ok(()),
        _ => Err(Errno::EIO),
    }
}

fn find_edu(ecam: usize) -> Option<u32> {
    let mut device = 0usize;
    while device < 32 {
        let config = ecam.saturating_add(device.saturating_mul(0x8000));
        let vendor = read_u16(config);
        let ident = read_u16(config.saturating_add(2));
        if vendor == EDU_VENDOR && ident == EDU_DEVICE {
            let command = read_u16(config.saturating_add(0x04));
            let bar = read_u32(config.saturating_add(0x10)) & !0xf;
            // UEFI leaves non-boot devices with memory decoding off on some builds; a driver
            // enables the function's memory space before touching its BAR.
            if command & 0x2 == 0 {
                write_u16(config.saturating_add(0x04), command | 0x2);
            }
            if bar != 0 {
                return Some(bar);
            }
        }
        device = device.saturating_add(1);
    }
    None
}

fn wait_for_rtc(rtc: usize, irq: Handle) -> Result<usize, Errno> {
    let now = read_u32(rtc);
    write_u32(rtc.saturating_add(PL031_RTCMR), now.saturating_add(1));
    write_u32(rtc.saturating_add(PL031_RTCCR), 1);
    write_u32(rtc.saturating_add(PL031_RTCICR), 1);
    write_u32(rtc.saturating_add(PL031_RTCIMSC), 1);
    let count = irq_wait(irq, 5_000_000_000)?;
    if count == 0 {
        return Err(Errno::ETIMEDOUT);
    }
    // Disarm the match before re-arming the GIC line: the level source is still asserted while
    // RTCDR equals RTCMR, and a pending bit captured in that window would fire again at once.
    let now = read_u32(rtc);
    write_u32(rtc.saturating_add(PL031_RTCMR), now.wrapping_add(0x1000));
    write_u32(rtc.saturating_add(PL031_RTCICR), 1);
    irq_ack(irq)?;
    Ok(count)
}

fn run() -> Result<(), Errno> {
    // MMIO: map the ECAM bus window, find the edu BAR and read its identification register.
    let ecam_region = mmio_create(ROOT, ECAM_BASE, ECAM_BUS_BYTES, 0)?;
    let ecam = map_region(ecam_region, ECAM_BUS_BYTES)?;
    let bar = find_edu(ecam).ok_or(Errno::ENOENT)?;
    let edu_region = mmio_create(ROOT, u64::from(bar), PAGE, 0)?;
    let edu = map_region(edu_region, PAGE)?;
    let id = read_u32(edu);
    if id != EDU_ID {
        return Err(Errno::EIO);
    }
    println!("s21d: mmio id={:#010x}", id);

    // DMA: a contiguous buffer below the 32-bit bus mask, zeroed, mapped and writable.
    let dma = dma_alloc(ROOT, DMA_LEN, 0, DMA_MASK_BITS, 0)?;
    let mut info = DmaInfo {
        bus_addr: 0,
        len: 0,
        flags: 0,
        pad: 0,
    };
    dma_info(dma, &mut info)?;
    let bus_limit = 1u64 << DMA_MASK_BITS;
    if info.len != DMA_LEN as u64
        || info.bus_addr == 0
        || info.bus_addr.checked_add(info.len).is_none()
        || info.bus_addr.saturating_add(info.len) > bus_limit
    {
        return Err(Errno::EIO);
    }
    let dma_ptr = map_region(dma, DMA_LEN)?;
    if read_u8(dma_ptr) != 0 || read_u8(dma_ptr.saturating_add(DMA_LEN - 1)) != 0 {
        return Err(Errno::EIO);
    }
    let mut index = 0usize;
    while index < DMA_LEN {
        write_u8(
            dma_ptr.saturating_add(index),
            (index as u8).wrapping_mul(31).wrapping_add(7),
        );
        index = index.saturating_add(1);
    }
    if read_u8(dma_ptr) != 7
        || read_u8(dma_ptr.saturating_add(DMA_LEN - 1))
            != ((DMA_LEN - 1) as u8).wrapping_mul(31).wrapping_add(7)
    {
        return Err(Errno::EIO);
    }
    println!("s21d: dma bus={:#x} len={}", info.bus_addr, info.len);

    // IRQ: claim SPI 2, receive the PL031 match and re-arm through IRQ_ACK.
    let rtc_region = mmio_create(ROOT, PL031_BASE, PAGE, 0)?;
    let rtc = map_region(rtc_region, PAGE)?;
    let irq = irq_create(ROOT, IRQ_LEGACY, PL031_IRQ, 0)?;
    match irq_create(ROOT, IRQ_LEGACY, PL031_IRQ, 0) {
        Ok(duplicate) => {
            let _ = handle_close(duplicate);
            return Err(Errno::EIO);
        }
        Err(Errno::EBUSY) => {}
        Err(_) => return Err(Errno::EIO),
    }
    let probe = irq_wait(irq, 10_000_000)?;
    println!("s21d: irq probe={}", probe);
    let first = wait_for_rtc(rtc, irq)?;
    println!("s21d: irq spi={} count={}", PL031_IRQ, first);

    // Teardown: the SPI claim is released with the last handle.
    handle_close(irq)?;
    let again = irq_create(ROOT, IRQ_LEGACY, PL031_IRQ, 0)?;
    handle_close(again)?;
    println!("s21d: teardown ok");

    // Negatives: no capability, RAM, misalignment, mask rejection, bad SPI and unsupported MSI.
    expect_err(
        mmio_create(Handle::SELF_PROC, 0x0901_0000, PAGE, 0),
        Errno::EPERM,
    )?;
    expect_err(mmio_create(ROOT, 0x4000_0000, PAGE, 0), Errno::EPERM)?;
    expect_err(mmio_create(ROOT, 0x0901_0001, PAGE, 0), Errno::EINVAL)?;
    expect_err(dma_alloc(ROOT, 2 * DMA_LEN, 0, 20, 0), Errno::ENOMEM)?;
    expect_err(irq_create(ROOT, IRQ_LEGACY, 1020, 0), Errno::EINVAL)?;
    expect_err(irq_create(ROOT, IRQ_LEGACY, PL031_IRQ, 1), Errno::EINVAL)?;
    expect_err(irq_create(ROOT, IRQ_MSI, 2, 0), Errno::EOPNOTSUPP)?;
    println!("s21d: negatives ok");
    Ok(())
}

fn main(_args: raven_rt::Args) -> i32 {
    match run() {
        Ok(()) => 0,
        Err(error) => {
            println!("s21d: failed {}", error.description());
            1
        }
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    println!("s21d: panic");
    exit(2)
}
