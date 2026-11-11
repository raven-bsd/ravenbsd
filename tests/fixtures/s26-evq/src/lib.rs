// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Shared S26 fixture helpers: raw syscalls and deterministic child creation.

#![no_std]
#![allow(unsafe_code)] // Raw syscall assembly, initfs mapping and the shared stack; every block has SAFETY.

use core::arch::asm;

use raven_abi::errno::Errno;
use raven_abi::layout::x86_64::{USER_END, USER_MIN};
use raven_abi::syscall::Syscall;
use raven_abi::types::{Handle, Rights, clock, map, prot};
use raven_elf::{Elf, LoadMode, Machine, StepKind, plan_load};
use raven_initfs::Initfs;
use raven_ipc::protocol::{self, PACKET_SIZE, SchemePacket};

/// Page size.
pub const PAGE: usize = 4096;
/// Fixed address where the controller maps the initfs for parsing.
pub const INITFS_BASE: usize = 0x1000_0000;
/// Fixed scratch address used while building a child stack.
pub const STACK_SCRATCH: usize = 0x2000_0000;
/// Child stack size.
pub const STACK_SIZE: usize = 64 * 1024;
/// Highest stack address.
pub const STACK_TOP: usize = (USER_END as usize) - PAGE;
/// Lowest stack address.
pub const STACK_BASE: usize = STACK_TOP - STACK_SIZE;
/// Open flag: read.
pub const O_READ: u32 = 1;
/// Open flag: write.
pub const O_WRITE: u32 = 2;
/// Open flag: read and write.
pub const O_RDWR: u32 = O_READ | O_WRITE;
/// A source is readable.
pub const READABLE: usize = 1;
/// A source has hung up.
pub const HUP: usize = 4;
/// A notification-style source fired.
pub const SIGNALED: usize = 8;
/// Report readiness transitions instead of levels.
pub const EDGE: usize = 0x8000_0000;

/// One event record written by `EVQ_WAIT`.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct EvqEvent {
    /// Opaque value supplied at registration.
    pub cookie: u64,
    /// Ready bits.
    pub events: u32,
    /// Padding.
    pub pad: u32,
}

/// One capability installed into a freshly created child before its first thread runs.
pub struct Grant {
    /// Source object in the controller.
    pub object: Handle,
    /// Rights to keep for the child.
    pub rights: Rights,
}

/// Raw `syscall` instruction.
#[inline(always)]
pub fn syscall6(
    number: usize,
    a0: usize,
    a1: usize,
    a2: usize,
    a3: usize,
    a4: usize,
    a5: usize,
) -> isize {
    let result: isize;
    // SAFETY: the caller passes valid registers for the RavenBSD ABI; the kernel validates
    // every pointer and handle.
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

/// One-argument syscall.
pub fn syscall1(number: usize, a0: usize) -> isize {
    syscall6(number, a0, 0, 0, 0, 0, 0)
}

/// Two-argument syscall.
pub fn syscall2(number: usize, a0: usize, a1: usize) -> isize {
    syscall6(number, a0, a1, 0, 0, 0, 0)
}

/// Three-argument syscall.
pub fn syscall3(number: usize, a0: usize, a1: usize, a2: usize) -> isize {
    syscall6(number, a0, a1, a2, 0, 0, 0)
}

/// Convert a raw return value.
pub fn result(raw: isize) -> Result<usize, Errno> {
    Errno::from_ret(raw)
}

fn handle_from_ret(raw: isize) -> Result<Handle, Errno> {
    let value = result(raw)?;
    u32::try_from(value).map(Handle).map_err(|_| Errno::EINVAL)
}

/// Exit the process.
pub fn exit(code: i32) -> ! {
    let _ = syscall1(Syscall::EXIT as usize, code as usize);
    loop {
        core::hint::spin_loop();
    }
}

/// Sleep for `ns` nanoseconds.
pub fn nanosleep(ns: u64) -> Result<(), Errno> {
    result(syscall1(Syscall::NANOSLEEP as usize, ns as usize)).map(|_| ())
}

/// Debug-console write.
pub fn debug_write(bytes: &[u8]) -> Result<usize, Errno> {
    result(syscall3(
        Syscall::DEBUG_WRITE as usize,
        bytes.as_ptr() as usize,
        bytes.len(),
        0,
    ))
}

/// Create an event queue.
pub fn evq_create() -> Result<Handle, Errno> {
    handle_from_ret(syscall1(Syscall::EVQ_CREATE as usize, 0))
}

/// Add, modify or delete one registration.
pub fn evq_ctl(
    queue: Handle,
    operation: usize,
    source: Handle,
    events: usize,
    cookie: u64,
) -> Result<(), Errno> {
    result(syscall6(
        Syscall::EVQ_CTL as usize,
        queue.0 as usize,
        operation,
        source.0 as usize,
        events,
        cookie as usize,
        0,
    ))
    .map(|_| ())
}

/// Wait for up to `max` ready records.
pub fn evq_wait(queue: Handle, out: &mut [EvqEvent], timeout_ns: u64) -> Result<usize, Errno> {
    result(syscall6(
        Syscall::EVQ_WAIT as usize,
        queue.0 as usize,
        out.as_mut_ptr() as usize,
        out.len(),
        timeout_ns as usize,
        0,
        0,
    ))
}

/// Create a disarmed monotonic timer.
pub fn timer_create() -> Result<Handle, Errno> {
    handle_from_ret(syscall2(
        Syscall::TIMER_CREATE as usize,
        clock::MONOTONIC as usize,
        0,
    ))
}

/// Arm a one-shot or periodic timer.
pub fn timer_set(timer: Handle, initial_ns: u64, interval_ns: u64) -> Result<(), Errno> {
    result(syscall3(
        Syscall::TIMER_SET as usize,
        timer.0 as usize,
        initial_ns as usize,
        interval_ns as usize,
    ))
    .map(|_| ())
}

/// Create a user-event doorbell.
pub fn event_create() -> Result<Handle, Errno> {
    handle_from_ret(syscall1(Syscall::EVENT_CREATE as usize, 0))
}

/// Set a doorbell's signalled level.
pub fn event_signal(event: Handle) -> Result<(), Errno> {
    result(syscall2(
        Syscall::EVENT_SIGNAL as usize,
        event.0 as usize,
        0,
    ))
    .map(|_| ())
}

/// Register a scheme name for the calling process and return the server handle.
pub fn scheme_register(name: &str) -> Result<Handle, Errno> {
    handle_from_ret(syscall3(
        Syscall::SCHEME_REGISTER as usize,
        name.as_ptr() as usize,
        name.len(),
        Handle::SELF_PROC.0 as usize,
    ))
}

/// Open a scheme path.
pub fn open(path: &str, flags: u32) -> Result<Handle, Errno> {
    handle_from_ret(syscall6(
        Syscall::OPEN as usize,
        path.as_ptr() as usize,
        path.len(),
        flags as usize,
        0,
        0,
        0,
    ))
}

/// Write to a scheme file.
pub fn write(handle: Handle, buffer: &[u8]) -> Result<usize, Errno> {
    result(syscall3(
        Syscall::WRITE as usize,
        handle.0 as usize,
        buffer.as_ptr() as usize,
        buffer.len(),
    ))
}

/// Receive one request packet; returns the server-side request handle.
pub fn recv(packet: &mut [u8]) -> Result<Handle, Errno> {
    handle_from_ret(syscall2(
        Syscall::RECV as usize,
        packet.as_mut_ptr() as usize,
        packet.len(),
    ))
}

/// Send one response packet.
pub fn send(request: Handle, packet: &[u8]) -> Result<(), Errno> {
    result(syscall3(
        Syscall::SEND as usize,
        request.0 as usize,
        packet.as_ptr() as usize,
        packet.len(),
    ))
    .map(|_| ())
}

/// Create an empty child process.
pub fn proc_create() -> Result<Handle, Errno> {
    handle_from_ret(syscall1(Syscall::PROC_CREATE as usize, 0))
}

/// Create an anonymous zero-filled memory object.
pub fn memobj_create(len: usize) -> Result<Handle, Errno> {
    handle_from_ret(syscall2(Syscall::MEMOBJ_CREATE as usize, len, 0))
}

/// Map memory into a process.
#[allow(clippy::too_many_arguments)] // Mirrors the six-register MAP syscall.
pub fn map_memory(
    process: Handle,
    object: Handle,
    offset: usize,
    address: usize,
    len: usize,
    flags: usize,
) -> Result<usize, Errno> {
    result(syscall6(
        Syscall::MAP as usize,
        process.0 as usize,
        object.0 as usize,
        offset,
        address,
        len,
        flags,
    ))
}

/// Unmap memory in a process.
pub fn unmap(process: Handle, address: usize, len: usize) -> Result<(), Errno> {
    result(syscall3(
        Syscall::UNMAP as usize,
        process.0 as usize,
        address,
        len,
    ))
    .map(|_| ())
}

/// Transfer a capability into another process before its first thread starts.
pub fn handle_transfer(process: Handle, object: Handle, rights: Rights) -> Result<Handle, Errno> {
    handle_from_ret(syscall3(
        Syscall::HANDLE_TRANSFER as usize,
        process.0 as usize,
        object.0 as usize,
        rights.0 as usize,
    ))
}

/// Create the first thread of a process.
pub fn thread_create(process: Handle, entry: usize, stack: usize) -> Result<Handle, Errno> {
    handle_from_ret(syscall6(
        Syscall::THREAD_CREATE as usize,
        process.0 as usize,
        entry,
        stack,
        stack,
        0,
        0,
    ))
}

/// Close a handle.
pub fn handle_close(handle: Handle) -> Result<(), Errno> {
    result(syscall1(Syscall::HANDLE_CLOSE as usize, handle.0 as usize)).map(|_| ())
}

/// Wait for a process and return its exit code.
pub fn proc_wait(process: Handle) -> Result<i32, Errno> {
    let mut code = -1i32;
    result(syscall2(
        Syscall::PROC_WAIT as usize,
        process.0 as usize,
        &mut code as *mut i32 as usize,
    ))?;
    Ok(code)
}

/// Map the initfs image read-only and return its bytes.
pub fn map_initfs_image() -> Result<&'static [u8], Errno> {
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

/// Encode a scheme packet.
pub fn encode_packet(packet: &SchemePacket) -> Result<[u8; PACKET_SIZE], Errno> {
    let mut bytes = [0u8; PACKET_SIZE];
    protocol::encode(packet, &mut bytes).map_err(|_| Errno::EPROTO)?;
    Ok(bytes)
}

/// Decode a scheme packet.
pub fn decode_packet(bytes: &[u8]) -> Result<SchemePacket, Errno> {
    protocol::decode(bytes).map_err(|_| Errno::EPROTO)
}

/// Spawn `/path` from `initfs` with optional capabilities installed before the first thread.
pub fn spawn_process(initfs: &Initfs<'_>, path: &str, grants: &[Grant]) -> Result<Handle, Errno> {
    let entry = stage_error("entry", initfs.get(path).ok_or(Errno::ENOENT))?;
    let elf = stage_error(
        "elf",
        Elf::parse(entry.data, Machine::X86_64, USER_MIN..USER_END).map_err(|_| Errno::EINVAL),
    )?;
    let child = stage_error("proc", proc_create())?;
    let steps = stage_error(
        "plan",
        plan_load(&elf, PAGE as u64, LoadMode::Strict).map_err(|_| Errno::EINVAL),
    )?;
    for step in steps {
        let flags = map::pack(step.prot as usize, map::FIXED);
        match step.kind {
            StepKind::File { offset } => {
                let offset = stage_error(
                    "file offset",
                    entry.data_offset.checked_add(offset).ok_or(Errno::EINVAL),
                )?;
                let offset = stage_error(
                    "file offset usize",
                    usize::try_from(offset).map_err(|_| Errno::EINVAL),
                )?;
                stage_error(
                    "file map",
                    map_memory(
                        child,
                        Handle::INITFS,
                        offset,
                        step.vaddr as usize,
                        step.len as usize,
                        flags,
                    ),
                )?;
            }
            StepKind::Zero => {
                stage_error(
                    "zero map",
                    map_memory(
                        child,
                        Handle::NONE,
                        0,
                        step.vaddr as usize,
                        step.len as usize,
                        flags,
                    ),
                )?;
            }
            StepKind::CopyTail { .. } => return Err(Errno::EINVAL),
        }
    }
    let stack = stage_error("memobj", memobj_create(STACK_SIZE))?;
    let scratch = stage_error(
        "scratch map",
        map_memory(
            Handle::SELF_PROC,
            stack,
            0,
            STACK_SCRATCH,
            STACK_SIZE,
            map::pack(prot::READ | prot::WRITE, map::FIXED | map::SHARED),
        ),
    )?;
    stage_error(
        "child stack map",
        map_memory(
            child,
            stack,
            0,
            STACK_BASE,
            STACK_SIZE,
            map::pack(prot::READ | prot::WRITE, map::FIXED | map::SHARED),
        ),
    )?;
    let stack_pointer = build_stack(scratch);
    for grant in grants {
        let _ = stage_error(
            "transfer",
            handle_transfer(child, grant.object, grant.rights),
        )?;
    }
    let thread = stage_error(
        "thread",
        thread_create(child, elf.entry() as usize, stack_pointer),
    )?;
    let _ = handle_close(thread);
    stage_error(
        "scratch unmap",
        unmap(Handle::SELF_PROC, STACK_SCRATCH, STACK_SIZE),
    )?;
    let _ = handle_close(stack);
    Ok(child)
}

fn stage_error<T>(stage: &str, value: Result<T, Errno>) -> Result<T, Errno> {
    if let Err(error) = &value {
        raven_rt::println!("s26: spawn failed stage={} errno={}", stage, error.0);
    }
    value
}

fn build_stack(base: usize) -> usize {
    let stack = STACK_TOP - 0x100;
    let mut offset = stack - STACK_BASE;
    for value in [1u64, STACK_TOP as u64 - 0x40, 0, 0, 6, PAGE as u64, 0, 0] {
        // SAFETY: the shared stack mapping is writable and `offset` stays inside its fixed size.
        unsafe { ((base + offset) as *mut u64).write_unaligned(value.to_le()) };
        offset += 8;
    }
    // SAFETY: the argument string and its terminator are inside the writable shared mapping.
    unsafe {
        ((base + STACK_TOP - STACK_BASE - 0x40) as *mut u8).write(b'x');
        ((base + STACK_TOP - STACK_BASE - 0x3f) as *mut u8).write(0);
    }
    stack
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    exit(2)
}
