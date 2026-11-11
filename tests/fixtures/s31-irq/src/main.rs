// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S31 IRQ lifecycle fixture.
//!
//! Three checks against the edu device and the legacy IOAPIC path:
//!
//! 1. an MSI vector closed while its source is still armed is quarantined; a new IRQ gets a
//!    different vector and none of the old device's messages reach it;
//! 2. closing a handle while a second thread blocks in `IRQ_WAIT` wakes that thread with `EIO`,
//!    the process stays alive and can create a new IRQ (the waiter blocks with a bounded
//!    timeout and revalidates, so a lost wake cannot strand it);
//! 3. closing and reopening a legacy GSI leaves the line masked after the last close.
//!
//! The kernel's `s30-irq-probe` and spurious-vector reports supply the serial evidence; the
//! fixture only drives syscalls and the edu registers.

#![no_std]
#![no_main]

use core::arch::asm;
use core::sync::atomic::{AtomicBool, AtomicI32, AtomicUsize, Ordering};

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

const EDU_VENDOR: u16 = 0x1234;
const EDU_DEVICE: u16 = 0x11e8;
const Q35_VENDOR: u16 = 0x8086;
const Q35_DEVICE: u16 = 0x29c0;

const EDU_INTR_RAISE: usize = 0x60;
const EDU_INTR_ACK: usize = 0x64;

const IRQ_LEGACY: u32 = 1;
const IRQ_MSI: u32 = 2;
const LEGACY_IRQ: u32 = 0;

const WAITER_STACK_SIZE: usize = 16 * 1024;

static WAIT_HANDLE: AtomicUsize = AtomicUsize::new(usize::MAX);
static WAIT_STARTED: AtomicBool = AtomicBool::new(false);
static WAIT_DONE: AtomicBool = AtomicBool::new(false);
static WAIT_RESULT: AtomicI32 = AtomicI32::new(-1);

#[repr(align(16))]
struct ThreadStack([u8; WAITER_STACK_SIZE]);

static mut WAITER_STACK: ThreadStack = ThreadStack([0; WAITER_STACK_SIZE]);

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
    println!("s31: failed {message}");
    exit(1)
}

fn exit(code: i32) -> ! {
    let _ = syscall1(Syscall::EXIT as usize, code as usize);
    loop {
        core::hint::spin_loop();
    }
}

fn thread_exit() -> ! {
    let _ = syscall1(Syscall::THREAD_EXIT as usize, 0);
    loop {
        core::hint::spin_loop();
    }
}

fn nanosleep(ns: u64) {
    let _ = syscall1(Syscall::NANOSLEEP as usize, ns as usize);
}

fn yield_now() {
    let _ = syscall1(Syscall::YIELD as usize, 0);
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

fn thread_create(entry: usize, stack_top: usize) -> Result<Handle, Errno> {
    handle(unsafe {
        syscall6(
            Syscall::THREAD_CREATE as usize,
            Handle::SELF_PROC.0 as usize,
            entry,
            stack_top,
            0,
            0,
            0,
        )
    })
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

fn irq_ack(irq: Handle) -> Result<(), Errno> {
    result(syscall1(Syscall::IRQ_ACK as usize, irq.0 as usize)).map(|_| ())
}

fn read_u16(address: usize) -> u16 {
    // SAFETY: the fixture reads its own device mappings, which the kernel mapped read/write.
    u16::from_le(unsafe { (address as *const u16).read_unaligned() })
}

fn read_u8(address: usize) -> u8 {
    // SAFETY: the fixture reads its own device mappings, which the kernel mapped read/write.
    unsafe { (address as *const u8).read() }
}

fn read_u32(address: usize) -> u32 {
    // SAFETY: the fixture reads its own device mappings, which the kernel mapped read/write.
    u32::from_le(unsafe { (address as *const u32).read_unaligned() })
}

fn read_u64(address: usize) -> u64 {
    // SAFETY: the fixture reads its own blob mapping, which the kernel mapped read-only.
    u64::from_le(unsafe { (address as *const u64).read_unaligned() })
}

fn write_u16(address: usize, value: u16) {
    // SAFETY: the fixture writes its own device mappings, which the kernel mapped read/write.
    unsafe { (address as *mut u16).write_unaligned(value.to_le()) }
}

fn write_u32(address: usize, value: u32) {
    // SAFETY: the fixture writes its own device mappings, which the kernel mapped read/write.
    unsafe { (address as *mut u32).write_unaligned(value.to_le()) }
}

/// Stack top for the waiter thread.
fn waiter_stack_top() -> usize {
    // SAFETY: only the address of the static is taken; the fixture never creates a reference to
    // the bytes and only the waiter thread runs on this stack.
    let base = unsafe { core::ptr::addr_of_mut!(WAITER_STACK.0) as usize };
    base.saturating_add(WAITER_STACK_SIZE)
}

/// Second thread: block in `IRQ_WAIT` until the main thread closes the handle.
///
/// The close is expected to wake the blocked waiter; the bounded timeout is a safety net, not
/// the mechanism: it guarantees that a lost wake revalidates the handle instead of stranding
/// the thread (and the case) forever.  Each new `IRQ_WAIT` re-checks the cancel state first,
/// so the waiter still reports the close as `EIO`.
extern "C" fn waiter_thread(_argument: usize) -> ! {
    WAIT_STARTED.store(true, Ordering::Release);
    let handle = Handle(WAIT_HANDLE.load(Ordering::Acquire) as u32);
    let code = loop {
        match irq_wait(handle, 250_000_000) {
            Err(error) => break i32::from(error.0),
            // Timeout: revalidate and block again.
            Ok(0) => continue,
            // The victim source never fires; a positive count means a foreign message reached
            // this object, which the errno assertion below turns into a test failure.
            Ok(count) => break i32::try_from(count).unwrap_or(i32::MAX),
        }
    };
    WAIT_RESULT.store(code, Ordering::Release);
    WAIT_DONE.store(true, Ordering::Release);
    thread_exit()
}

/// Locate the edu device: ACPI MCFG, the ECAM window, then BAR0.
fn find_edu(admin: Handle) -> Result<(usize, usize), &'static str> {
    let acpi_blob = boot_blob(admin, BLOB_ACPI).map_err(|_| "acpi blob")?;
    let blob = map_memory(
        Handle::SELF_PROC,
        acpi_blob,
        0,
        0,
        PAGE,
        prot::READ as usize,
    )
    .map_err(|_| "blob map")?;
    if read_u32(blob) != BLOB_MAGIC {
        return Err("blob magic");
    }
    let count = read_u32(blob + 16) as usize;
    if count == 0 || count > 128 {
        return Err("blob count");
    }
    let mut mcfg_offset = 0usize;
    let mut mcfg_length = 0usize;
    let mut index = 0usize;
    while index < count {
        let entry = blob + BLOB_HEADER + index * BLOB_ENTRY;
        if read_u32(entry) == u32::from_le_bytes(*b"MCFG") {
            mcfg_offset = read_u32(entry + 8) as usize;
            mcfg_length = read_u32(entry + 12) as usize;
        }
        index = index.saturating_add(1);
    }
    if mcfg_offset == 0 || mcfg_length < 56 || mcfg_offset.saturating_add(mcfg_length) > PAGE {
        return Err("mcfg entry");
    }
    let mcfg = blob + mcfg_offset;
    let ecam_base = read_u64(mcfg + 44);
    let segment = read_u16(mcfg + 52);
    // SAFETY: the MCFG copy lives inside the mapped read-only blob page range.
    let start_bus = u16::from(unsafe { ((mcfg + 54) as *const u8).read() });
    // SAFETY: same mapped page.
    let end_bus = u16::from(unsafe { ((mcfg + 55) as *const u8).read() });
    if segment != 0 || start_bus > 0 || end_bus < 1 || ecam_base == 0 {
        return Err("mcfg range");
    }

    let region =
        mmio_create(admin, ecam_base, ECAM_BUS_BYTES as usize, 0).map_err(|_| "ecam mmio")?;
    let ecam = map_memory(
        Handle::SELF_PROC,
        region,
        0,
        0,
        ECAM_BUS_BYTES as usize,
        map::pack(prot::READ | prot::WRITE, 0),
    )
    .map_err(|_| "ecam map")?;

    let mut host_bridge = false;
    let mut edu_device = None;
    let mut device = 0usize;
    while device < 32 {
        let config = ecam + device * ECAM_DEVICE_STRIDE;
        let vendor = read_u16(config);
        let ident = read_u16(config + 2);
        if vendor == Q35_VENDOR && ident == Q35_DEVICE {
            host_bridge = true;
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
        return Err("host bridge");
    }
    let Some(edu_device) = edu_device else {
        return Err("edu not found");
    };
    let edu_config = ecam + edu_device * ECAM_DEVICE_STRIDE;
    let bar0 = read_u32(edu_config + 0x10);
    if bar0 & 1 != 0 || bar0 & 0x6 != 0 {
        return Err("edu bar0 io");
    }
    let bar0_phys = u64::from(bar0 & 0xFFFF_FFF0);
    let edu_region = mmio_create(admin, bar0_phys, PAGE, 0).map_err(|_| "edu mmio")?;
    let edu = map_memory(
        Handle::SELF_PROC,
        edu_region,
        0,
        0,
        PAGE,
        map::pack(prot::READ | prot::WRITE, 0),
    )
    .map_err(|_| "edu map")?;
    if read_u32(edu) != 0x0100_00ed {
        return Err("edu id");
    }
    Ok((edu, edu_config))
}

/// Find the MSI capability and program `info` into it, returning the capability offset.
fn program_msi(config: usize, info: &IrqInfo) -> Result<usize, &'static str> {
    let mut cap = usize::from(read_u8(config + 0x34) & 0xFC);
    while cap != 0 {
        if read_u8(config + cap) == 0x05 {
            let control = read_u16(config + cap + 2);
            let wide = control & (1 << 7) != 0;
            write_u32(config + cap + 4, info.address_lo);
            if wide {
                write_u32(config + cap + 8, info.address_hi);
                write_u32(config + cap + 12, info.data);
            } else {
                write_u32(config + cap + 8, info.data);
            }
            write_u16(config + cap + 2, control | 1);
            return Ok(cap);
        }
        cap = usize::from(read_u8(config + cap + 1) & 0xFC);
    }
    Err("no msi capability")
}

fn disable_msi(config: usize, capability: usize) {
    let control = read_u16(config + capability + 2);
    write_u16(config + capability + 2, control & !1);
}

fn raise(edu: usize) {
    write_u32(edu + EDU_INTR_RAISE, 0x1);
}

fn device_ack(edu: usize) {
    write_u32(edu + EDU_INTR_ACK, 0x1);
}

fn default_info() -> IrqInfo {
    IrqInfo {
        vector: 0,
        address_lo: 0,
        address_hi: 0,
        data: 0,
    }
}

/// Close while the device keeps firing: the old vector must be retired, and the new object must
/// not observe the old source.
fn quarantine_part(admin: Handle, edu: usize, edu_config: usize) -> Result<(), &'static str> {
    let mut old = default_info();
    let old_handle = irq_create(admin, IRQ_MSI, 0, Some(&mut old)).map_err(|_| "old create")?;
    let capability = program_msi(edu_config, &old)?;

    let mut count = 0u64;
    while count < 10_000 {
        raise(edu);
        let got = irq_wait(old_handle, 5_000_000_000).map_err(|_| "old wait")?;
        if got == 0 {
            return Err("old timeout");
        }
        device_ack(edu);
        count = count.saturating_add(got as u64);
    }
    if count != 10_000 {
        return Err("old count drift");
    }
    println!("s31: msi fired count={}", count);

    // Arm a burst, then close without consuming it or acknowledging the source.  The kernel
    // cannot prove the device is quiet, so the vector is retired instead of released.
    let mut i = 0usize;
    while i < 10 {
        raise(edu);
        i = i.saturating_add(1);
    }
    handle_close(old_handle);
    let mut i = 0usize;
    while i < 10 {
        raise(edu);
        i = i.saturating_add(1);
    }
    nanosleep(50_000_000);

    let mut fresh = default_info();
    let fresh_handle =
        irq_create(admin, IRQ_MSI, 0, Some(&mut fresh)).map_err(|_| "fresh create")?;
    if fresh.vector == old.vector {
        return Err("quarantined vector reused");
    }
    println!(
        "s31: quarantine ok old=0x{:02x} new=0x{:02x}",
        old.vector, fresh.vector
    );

    // The old device is still enabled and still points at the retired vector.  None of these
    // messages may reach the new object.
    let mut i = 0usize;
    while i < 100 {
        raise(edu);
        i = i.saturating_add(1);
    }
    nanosleep(50_000_000);
    if irq_wait(fresh_handle, 1_000_000).map_err(|_| "fresh wait")? != 0 {
        return Err("old source reached new irq");
    }
    println!("s31: isolation ok");

    // Force the spurious report out of the kernel while the evidence is fresh: an inert create
    // flushes the interrupt-context records.
    let mut dummy = default_info();
    let dummy_handle =
        irq_create(admin, IRQ_MSI, 0, Some(&mut dummy)).map_err(|_| "flush create")?;
    println!("s31: spurious logged");

    disable_msi(edu_config, capability);
    irq_ack(fresh_handle).map_err(|_| "fresh ack")?;
    handle_close(fresh_handle);
    handle_close(dummy_handle);
    println!("s31: source disabled");
    Ok(())
}

/// Close the last handle while another thread blocks in `IRQ_WAIT`.
fn cancel_part(admin: Handle) -> Result<(), &'static str> {
    let mut info = default_info();
    let victim = irq_create(admin, IRQ_MSI, 0, Some(&mut info)).map_err(|_| "victim create")?;
    WAIT_HANDLE.store(victim.0 as usize, Ordering::Release);
    WAIT_STARTED.store(false, Ordering::Release);
    WAIT_DONE.store(false, Ordering::Release);
    WAIT_RESULT.store(-1, Ordering::Release);
    let thread_entry: extern "C" fn(usize) -> ! = waiter_thread;
    let thread =
        thread_create(thread_entry as usize, waiter_stack_top()).map_err(|_| "thread create")?;

    let mut spins = 0usize;
    while !WAIT_STARTED.load(Ordering::Acquire) {
        yield_now();
        spins = spins.saturating_add(1);
        if spins > 1_000_000 {
            return Err("waiter did not start");
        }
    }
    nanosleep(50_000_000);

    // The source was never programmed.  Acknowledge it so the close can release the vector
    // instead of quarantining it, then close the last handle under the blocked waiter.
    irq_ack(victim).map_err(|_| "victim ack")?;
    handle_close(victim);

    let mut waits = 0usize;
    while !WAIT_DONE.load(Ordering::Acquire) {
        nanosleep(1_000_000);
        waits = waits.saturating_add(1);
        if waits > 2_000 {
            return Err("waiter did not return");
        }
    }
    let code = WAIT_RESULT.load(Ordering::Acquire);
    if code != i32::from(Errno::EIO.0) {
        return Err("waiter errno");
    }
    println!("s31: waiter cancelled errno={}", code);
    handle_close(thread);

    let mut after = default_info();
    let handle = irq_create(admin, IRQ_MSI, 0, Some(&mut after)).map_err(|_| "after create")?;
    handle_close(handle);
    println!("s31: process alive");
    Ok(())
}

/// Legacy close/reopen: the line is masked after the last close and re-routed masked.
fn legacy_part(admin: Handle) -> Result<(), &'static str> {
    let first = irq_create(admin, IRQ_LEGACY, LEGACY_IRQ, None).map_err(|_| "legacy create")?;
    irq_ack(first).map_err(|_| "legacy ack")?;
    handle_close(first);
    let second = irq_create(admin, IRQ_LEGACY, LEGACY_IRQ, None).map_err(|_| "legacy reopen")?;
    handle_close(second);
    println!("s31: legacy close masked");
    Ok(())
}

fn main(_args: raven_rt::Args) -> i32 {
    let admin = match handle_dup(Handle::ROOT_DEVICE, Rights::ADMIN.union(Rights::DUP).0) {
        Ok(handle) => handle,
        Err(_) => fail("root dup"),
    };
    let (edu, edu_config) = match find_edu(admin) {
        Ok(found) => found,
        Err(message) => fail(message),
    };
    if let Err(message) = quarantine_part(admin, edu, edu_config) {
        fail(message);
    }
    if let Err(message) = cancel_part(admin) {
        fail(message);
    }
    if let Err(message) = legacy_part(admin) {
        fail(message);
    }
    println!("s31: lifecycle ok");
    0
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    exit(2)
}
