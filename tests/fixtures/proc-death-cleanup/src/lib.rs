// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Shared helpers for the S69 process-death cleanup fixture.
//!
//! The controller starts a resource owner only after it has mapped the control page and
//! transferred every hardware object into the child's table.  This is the same stopped-PROC_CREATE
//! pattern the S36 lifecycle fixture uses, and it removes the "child started before its resources
//! were visible" race.

#![no_std]

use raven_abi::errno::Errno;
use raven_abi::types::{Handle, map, prot};
use raven_elf::{Elf, LoadMode, Machine, StepKind, plan_load};
use raven_initfs::Initfs;
use raven_rt::sys;
use raven_scheme::Client;

/// Page size used by the fixture.
pub const PAGE: usize = 4096;
/// Fixed address of the shared controller/owner control page.
pub const CTRL_ADDR: usize = 0x3000_0000;
/// Fixed address of the shared mapping the owner writes through.
pub const SHARED_ADDR: usize = 0x3100_0000;
/// Controller scratch window used while preparing the child stack.
pub const STACK_SCRATCH: usize = 0x3200_0000;
/// User stack object size.
pub const STACK_SIZE: usize = 256 * 1024;
/// Top of the user stack range (the guard page below `USER_END`).
pub const STACK_TOP: usize = 0x0000_7FFF_FFFF_E000;
/// Bottom of the fixed stack window.
pub const STACK_BASE: usize = STACK_TOP - STACK_SIZE;

/// Control-page word indices.
pub const CTRL_READY: usize = 0;
/// IRQ handle transferred into the owner.
pub const CTRL_IRQ: usize = 1;
/// DMA handle transferred into the owner.
pub const CTRL_DMA: usize = 2;
/// MMIO handle transferred into the owner.
pub const CTRL_MMIO: usize = 3;
/// Timer handle transferred into the owner.
pub const CTRL_TIMER: usize = 4;
/// Shared memory-object handle transferred into the owner.
pub const CTRL_MEMOBJ: usize = 5;
/// Owner status bits.
pub const CTRL_STATUS: usize = 6;
/// Cycle number (0 for the first resource cycle).
pub const CTRL_CYCLE: usize = 7;

/// Owner has started its futex helper.
pub const STATUS_FUTEX: u64 = 1 << 0;
/// Owner has started its pipe reader.
pub const STATUS_PIPE: u64 = 1 << 1;
/// Owner has started its signal waiter.
pub const STATUS_SIGNAL: u64 = 1 << 2;
/// Owner has started its sleeper.
pub const STATUS_SLEEP: u64 = 1 << 3;
/// Owner has started its IRQ waiter (x86-64 only).
pub const STATUS_IRQ: u64 = 1 << 4;
/// Owner has started its IPC blocked write.
pub const STATUS_IPC: u64 = 1 << 5;
/// Owner has created and registered its event queue.
pub const STATUS_EVENT: u64 = 1 << 6;
/// Owner status: every helper is up; the main thread is about to wait.
pub const STATUS_READY: u64 = 1 << 7;

/// Read one control word.
pub fn ctrl_read(index: usize) -> u64 {
    let address = CTRL_ADDR + index * 8;
    // SAFETY: the controller maps exactly one page at CTRL_ADDR and `index` is a fixture constant
    // below 512.
    unsafe { core::ptr::read_volatile(address as *const u64) }
}

/// Write one control word.
pub fn ctrl_write(index: usize, value: u64) {
    let address = CTRL_ADDR + index * 8;
    // SAFETY: the controller maps exactly one writable page at CTRL_ADDR and `index` is a fixture
    // constant below 512.
    unsafe { core::ptr::write_volatile(address as *mut u64, value) }
}

/// Map and parse the boot initfs.
pub fn map_initfs() -> Option<Initfs<'static>> {
    let image = raven_rt::map_initfs().ok()?;
    Initfs::parse(image).ok()
}

/// Spawn a child from the initfs with `role` as argv[1].
///
/// `argv[0]` is the role too, so `raven_rt::spawn`'s P2r command-name path gives the child a
/// stable `s69-*` command before its first thread starts.
pub fn spawn_child(initfs: &Initfs<'_>, path: &str, role: &str) -> Result<Handle, Errno> {
    raven_rt::spawn::spawn(
        initfs,
        path,
        &raven_rt::spawn::SpawnOptions {
            args: &[role, role],
            env: &[],
        },
    )
    .map_err(|_| Errno::EIO)
}

/// Wait for a child and return its exit code.
pub fn wait(process: Handle) -> Result<i32, Errno> {
    let mut code = 0i32;
    // SAFETY: PROC_WAIT writes exactly one i32 to the live stack slot and validates the handle.
    Errno::from_ret(unsafe {
        sys::syscall2(
            raven_abi::syscall::Syscall::PROC_WAIT as usize,
            process.0 as usize,
            &mut code as *mut i32 as usize,
        )
    })?;
    Ok(code)
}

/// Create an empty process and return its handle.
pub fn proc_create(flags: usize) -> Result<Handle, Errno> {
    // SAFETY: PROC_CREATE takes only scalar flags.
    let raw = unsafe { sys::syscall1(raven_abi::syscall::Syscall::PROC_CREATE as usize, flags) };
    let value = Errno::from_ret(raw)?;
    u32::try_from(value).map(Handle).map_err(|_| Errno::EINVAL)
}

/// Terminate a child and wait for its status.
pub fn terminate_and_wait(process: Handle, code: i32) -> Result<i32, Errno> {
    sys::proc_terminate(process, code)?;
    wait(process)
}

/// Open a path, retrying while the server registers.
pub fn open_retry(path: &str, flags: u32) -> Result<Handle, Errno> {
    let client = Client::new();
    let mut attempt = 0usize;
    while attempt < 200_000 {
        match client.open(path, flags) {
            Ok(handle) => return Ok(handle),
            Err(Errno::ENOENT) | Err(Errno::EIO) => {
                sys::yield_now();
                attempt = attempt.saturating_add(1);
            }
            Err(error) => return Err(error),
        }
    }
    Err(Errno::ETIMEDOUT)
}

/// Poll the server's held-request count through `s69:state`.
pub fn held_count(state: Handle) -> Result<u32, Errno> {
    let mut response = [0u8; 16];
    let length = Client::new().call(state, b"held", &mut response)?;
    if length < 8 {
        return Err(Errno::EPROTO);
    }
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&response[..8]);
    u32::try_from(u64::from_le_bytes(bytes)).map_err(|_| Errno::EPROTO)
}

/// Wait until the controller-owned control page reports the owner ready.
pub fn wait_owner_ready() -> Result<u64, Errno> {
    let mut attempt = 0usize;
    while attempt < 2_000_000 {
        let status = ctrl_read(CTRL_STATUS);
        if status & STATUS_READY != 0 && ctrl_read(CTRL_READY) == 2 {
            return Ok(status);
        }
        sys::yield_now();
        attempt = attempt.saturating_add(1);
    }
    Err(Errno::ETIMEDOUT)
}

/// A process created with `PROC_CREATE` and loaded, but with no thread yet.
pub struct StoppedChild {
    /// Child process handle.
    pub process: Handle,
    /// ELF entry address.
    pub entry: usize,
    /// Initial user stack pointer.
    pub stack: usize,
}

/// Create and load `/init` for `role`, but do not start its first thread.
pub fn spawn_stopped(initfs: &Initfs<'_>, role: &str) -> Result<StoppedChild, Errno> {
    let entry = initfs.get("/init").ok_or(Errno::ENOENT)?;
    let machine = target_machine()?;
    let (user_min, user_end) = user_range();
    let elf = Elf::parse(entry.data, machine, user_min..user_end).map_err(|_| Errno::ENOEXEC)?;
    let child = proc_create(0)?;
    let result = load_child(child, entry.data_offset, &elf, role);
    match result {
        Ok(stack) => Ok(StoppedChild {
            process: child,
            entry: elf.entry() as usize,
            stack,
        }),
        Err(error) => {
            let _ = sys::proc_terminate(child, -1);
            let _ = wait(child);
            let _ = sys::handle_close(child);
            Err(error)
        }
    }
}

/// Start the first thread of a stopped child and close the controller's thread handle.
pub fn start_child(child: &StoppedChild) -> Result<(), Errno> {
    let thread = sys::thread_create(child.process, child.entry, child.stack, 0)?;
    sys::handle_close(thread)
}

fn load_child(child: Handle, data_offset: u64, elf: &Elf<'_>, role: &str) -> Result<usize, Errno> {
    let steps = plan_load(elf, PAGE as u64, LoadMode::Strict).map_err(|_| Errno::ENOEXEC)?;
    for step in steps {
        match step.kind {
            StepKind::File { offset } => {
                let offset = data_offset.checked_add(offset).ok_or(Errno::EINVAL)?;
                let offset = usize::try_from(offset).map_err(|_| Errno::EINVAL)?;
                sys::map_object_in(
                    child,
                    Handle::INITFS,
                    offset,
                    step.vaddr as usize,
                    step.len as usize,
                    step.prot as usize,
                    map::FIXED,
                )?;
            }
            StepKind::Zero => {
                sys::map_object_in(
                    child,
                    Handle::NONE,
                    0,
                    step.vaddr as usize,
                    step.len as usize,
                    step.prot as usize,
                    map::FIXED,
                )?;
            }
            StepKind::CopyTail { .. } => return Err(Errno::ENOEXEC),
        }
    }
    let stack_object = memobj_create(STACK_SIZE)?;
    let scratch = sys::map_object_in(
        Handle::SELF_PROC,
        stack_object,
        0,
        STACK_SCRATCH,
        STACK_SIZE,
        prot::READ | prot::WRITE,
        map::FIXED | map::SHARED,
    )?;
    sys::map_object_in(
        child,
        stack_object,
        0,
        STACK_BASE,
        STACK_SIZE,
        prot::READ | prot::WRITE,
        map::FIXED | map::SHARED,
    )?;
    let stack = build_stack(scratch, role);
    sys::unmap(Handle::SELF_PROC, STACK_SCRATCH, STACK_SIZE)?;
    let _ = sys::handle_close(stack_object);
    Ok(stack)
}

fn memobj_create(len: usize) -> Result<Handle, Errno> {
    // SAFETY: MEMOBJ_CREATE takes a page-aligned length and zero flags.
    let raw = unsafe { sys::syscall2(raven_abi::syscall::Syscall::MEMOBJ_CREATE as usize, len, 0) };
    let value = Errno::from_ret(raw)?;
    u32::try_from(value).map(Handle).map_err(|_| Errno::EINVAL)
}

fn build_stack(base: usize, role: &str) -> usize {
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
        ((base + STACK_TOP - STACK_BASE - 0x40) as *mut u8).write(b'i');
        ((base + STACK_TOP - STACK_BASE - 0x3f) as *mut u8).write(0);
        let role_start = base + STACK_TOP - STACK_BASE - 0x40 + 2;
        let mut index = 0usize;
        while index < role.len() {
            ((role_start + index) as *mut u8).write(*role.as_bytes().get(index).unwrap_or(&0));
            index = index.saturating_add(1);
        }
        ((role_start + role.len()) as *mut u8).write(0);
    }
    stack
}

fn target_machine() -> Result<Machine, Errno> {
    Ok(native_machine())
}

#[cfg(target_arch = "x86_64")]
const fn native_machine() -> Machine {
    Machine::X86_64
}

#[cfg(target_arch = "aarch64")]
const fn native_machine() -> Machine {
    Machine::Aarch64
}

#[cfg(target_arch = "x86_64")]
const fn user_range() -> (u64, u64) {
    (
        raven_abi::layout::x86_64::USER_MIN,
        raven_abi::layout::x86_64::USER_END,
    )
}

#[cfg(target_arch = "aarch64")]
const fn user_range() -> (u64, u64) {
    (
        raven_abi::layout::aarch64::USER_MIN,
        raven_abi::layout::aarch64::USER_END,
    )
}
