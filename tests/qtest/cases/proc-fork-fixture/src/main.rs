// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S50b `PROC_FORK` fixture.
//!
//! The parent maps a private data page, a `NOFORK` page and a `ZERO_ON_FORK` page, plus a
//! shared control object. It forks with raw `PROC_FORK` (0x39), changes the data page, then
//! creates a child thread with `THREAD_CREATE`. The child checks the COW copy, writes its own
//! value, maps a duplicated memory-object handle over the absent `NOFORK` range and reads the
//! zeroed page. The parent waits and checks that neither of its pages changed.
//!
//! A short fork/exit loop follows for the kernel's frame/lineage stability evidence.

#![no_std]
#![no_main]

use core::arch::asm;

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::{Handle, map, prot};
use raven_rt::entry;
use raven_rt::println;

entry!(main);

const PAGE: usize = 4096;

const DATA_ADDR: usize = 0x5000_0000;
const CONTROL_ADDR: usize = 0x5100_0000;
const NOFORK_ADDR: usize = 0x5200_0000;
const ZERO_ADDR: usize = 0x5300_0000;
const NOFORK_OBJ_ADDR: usize = 0x5400_0000;
const INVALID_ADDR: usize = 0x5500_0000;

const PRE_MARKER: u32 = 0x1111_2222;
const PARENT_MARKER: u32 = 0x3333_4444;
const CHILD_MARKER: u32 = 0x5555_6666;
const ZERO_MARKER: u32 = 0x7777_8888;

const CONTROL_DATA_SAW: usize = 4;
const CONTROL_MAP_OK: usize = 8;
const CONTROL_ZERO_OK: usize = 12;
const CONTROL_CHILD_WROTE: usize = 16;
const CONTROL_LOOP: usize = 20;
const CONTROL_NOFORK_HANDLE: usize = 24;
const CONTROL_HANDLE_ABSENT: usize = 28;
const CONTROL_MAPPING_ABSENT: usize = 32;

/// Raw `proc_fork(src, flags) -> Handle` number (the ABI crate is reviewer-owned).
const PROC_FORK: usize = 0x39;

#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(
    r#"
    .section .text
    .global proc_fork_child_entry
    .type proc_fork_child_entry, @function
proc_fork_child_entry:
    call proc_fork_child_body
    mov edi, eax
    mov eax, 0x21
    syscall
    ud2
    .size proc_fork_child_entry, . - proc_fork_child_entry

    .global proc_fork_quick_entry
    .type proc_fork_quick_entry, @function
proc_fork_quick_entry:
    call proc_fork_quick_body
    mov edi, eax
    mov eax, 0x21
    syscall
    ud2
    .size proc_fork_quick_entry, . - proc_fork_quick_entry
"#
);

#[cfg(target_arch = "aarch64")]
core::arch::global_asm!(
    r#"
    .section .text
    .global proc_fork_child_entry
    .type proc_fork_child_entry, %function
proc_fork_child_entry:
    bl proc_fork_child_body
    mov x8, #0x21
    svc #0
1:
    b 1b
    .size proc_fork_child_entry, . - proc_fork_child_entry

    .global proc_fork_quick_entry
    .type proc_fork_quick_entry, %function
proc_fork_quick_entry:
    bl proc_fork_quick_body
    mov x8, #0x21
    svc #0
1:
    b 1b
    .size proc_fork_quick_entry, . - proc_fork_quick_entry
"#
);

unsafe extern "C" {
    fn proc_fork_child_entry();
    fn proc_fork_quick_entry();
}

#[cfg(target_arch = "x86_64")]
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
    // SAFETY: scalar syscall arguments; the kernel validates handles and pointers.
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

#[cfg(target_arch = "aarch64")]
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
    // SAFETY: scalar syscall arguments; the kernel validates handles and pointers.
    unsafe {
        asm!(
            "svc #0",
            in("x8") number,
            inlateout("x0") a0 => result,
            in("x1") a1,
            in("x2") a2,
            in("x3") a3,
            in("x4") a4,
            in("x5") a5,
            options(nostack),
        );
    }
    result
}

fn read32(address: usize) -> u32 {
    // SAFETY: the fixture owns the mapping for the duration.
    unsafe { core::ptr::read_volatile(address as *const u32) }
}

fn write32(address: usize, value: u32) {
    // SAFETY: the fixture owns the mapping for the duration.
    unsafe {
        core::ptr::write_volatile(address as *mut u32, value);
    }
}

fn handle_from(raw: isize) -> Result<Handle, Errno> {
    match Errno::from_ret(raw) {
        Ok(value) => u32::try_from(value).map(Handle).map_err(|_| Errno::EINVAL),
        Err(error) => Err(error),
    }
}

fn memobj_create(len: usize) -> Result<Handle, Errno> {
    handle_from(unsafe { syscall6(Syscall::MEMOBJ_CREATE as usize, len, 0, 0, 0, 0, 0) })
}

fn map_raw(
    process: Handle,
    object: Handle,
    offset: usize,
    address: usize,
    len: usize,
    flags: usize,
) -> Result<usize, Errno> {
    Errno::from_ret(unsafe {
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

fn proc_fork(source: Handle, flags: usize) -> Result<Handle, Errno> {
    handle_from(unsafe { syscall6(PROC_FORK, source.0 as usize, flags, 0, 0, 0, 0) })
}

fn thread_create(
    process: Handle,
    entry: usize,
    stack_top: usize,
    arg: usize,
) -> Result<Handle, Errno> {
    handle_from(unsafe {
        syscall6(
            Syscall::THREAD_CREATE as usize,
            process.0 as usize,
            entry,
            stack_top,
            arg,
            0,
            0,
        )
    })
}

fn proc_wait(process: Handle, code: &mut i32) -> Result<(), Errno> {
    Errno::from_ret(unsafe {
        syscall6(
            Syscall::PROC_WAIT as usize,
            process.0 as usize,
            code as *mut i32 as usize,
            0,
            0,
            0,
            0,
        )
    })
    .map(|_| ())
}

fn close(handle: Handle) -> Result<(), Errno> {
    Errno::from_ret(unsafe {
        syscall6(
            Syscall::HANDLE_CLOSE as usize,
            handle.0 as usize,
            0,
            0,
            0,
            0,
            0,
        )
    })
    .map(|_| ())
}

/// Child thread body: check the COW copy, map the duplicated handle and read the zero page.
#[unsafe(no_mangle)]
pub extern "C" fn proc_fork_child_body() -> i32 {
    let spare = read32(CONTROL_ADDR) as u32;
    let saw = read32(DATA_ADDR) == PRE_MARKER;
    write32(CONTROL_ADDR + CONTROL_DATA_SAW, u32::from(saw));

    // This store must land in a new COW frame, not in the parent's page.
    write32(DATA_ADDR, CHILD_MARKER);

    let mapped = map_raw(
        Handle::SELF_PROC,
        Handle(spare),
        0,
        NOFORK_ADDR,
        PAGE,
        map::pack(prot::READ | prot::WRITE, map::FIXED),
    );
    let map_ok = mapped == Ok(NOFORK_ADDR);
    write32(CONTROL_ADDR + CONTROL_MAP_OK, u32::from(map_ok));
    if map_ok {
        write32(NOFORK_ADDR, CHILD_MARKER);
    }

    let zero_ok = read32(ZERO_ADDR) == 0;
    write32(CONTROL_ADDR + CONTROL_ZERO_OK, u32::from(zero_ok));
    write32(CONTROL_ADDR + CONTROL_CHILD_WROTE, CHILD_MARKER);

    // A NOFORK mapping's object handle must be absent, not merely unmapped.
    let nofork_handle = read32(CONTROL_ADDR + CONTROL_NOFORK_HANDLE);
    let handle_absent = map_raw(
        Handle::SELF_PROC,
        Handle(nofork_handle),
        0,
        NOFORK_OBJ_ADDR + 0x1000,
        PAGE,
        map::pack(prot::READ | prot::WRITE, map::FIXED),
    ) == Err(Errno::EBADF);
    write32(
        CONTROL_ADDR + CONTROL_HANDLE_ABSENT,
        u32::from(handle_absent),
    );
    let mapping_absent = map_raw(
        Handle::SELF_PROC,
        Handle(spare),
        0,
        NOFORK_OBJ_ADDR,
        PAGE,
        map::pack(prot::READ | prot::WRITE, map::FIXED),
    ) == Ok(NOFORK_OBJ_ADDR);
    write32(
        CONTROL_ADDR + CONTROL_MAPPING_ABSENT,
        u32::from(mapping_absent),
    );
    0
}

/// Loop child body: bump the shared counter and exit.
#[unsafe(no_mangle)]
pub extern "C" fn proc_fork_quick_body() -> i32 {
    let count = read32(CONTROL_ADDR + CONTROL_LOOP);
    write32(CONTROL_ADDR + CONTROL_LOOP, count.wrapping_add(1));
    0
}

fn stack_top() -> usize {
    let anchor = 0u64;
    (&anchor as *const u64 as usize) & !0xf
}

fn run() -> Result<(), Errno> {
    let control = memobj_create(PAGE)?;
    let spare = memobj_create(PAGE)?;
    let nofork_obj = memobj_create(PAGE)?;

    map_raw(
        Handle::SELF_PROC,
        control,
        0,
        CONTROL_ADDR,
        PAGE,
        map::pack(prot::READ | prot::WRITE, map::FIXED | map::SHARED),
    )?;
    for slot in 0..10 {
        write32(CONTROL_ADDR + slot * 4, 0);
    }
    let private = map::pack(prot::READ | prot::WRITE, map::FIXED);
    map_raw(Handle::SELF_PROC, Handle::NONE, 0, DATA_ADDR, PAGE, private)?;
    map_raw(
        Handle::SELF_PROC,
        Handle::NONE,
        0,
        NOFORK_ADDR,
        PAGE,
        map::pack(prot::READ | prot::WRITE, map::FIXED | map::NOFORK),
    )?;
    map_raw(
        Handle::SELF_PROC,
        Handle::NONE,
        0,
        ZERO_ADDR,
        PAGE,
        map::pack(prot::READ | prot::WRITE, map::FIXED | map::ZERO_ON_FORK),
    )?;
    map_raw(
        Handle::SELF_PROC,
        nofork_obj,
        0,
        NOFORK_OBJ_ADDR,
        PAGE,
        map::pack(
            prot::READ | prot::WRITE,
            map::FIXED | map::SHARED | map::NOFORK,
        ),
    )?;

    let invalid_combo = map_raw(
        Handle::SELF_PROC,
        Handle::NONE,
        0,
        INVALID_ADDR,
        PAGE,
        map::pack(
            prot::READ | prot::WRITE,
            map::FIXED | map::NOFORK | map::ZERO_ON_FORK,
        ),
    ) == Err(Errno::EINVAL);
    let invalid_shared = map_raw(
        Handle::SELF_PROC,
        control,
        0,
        INVALID_ADDR,
        PAGE,
        map::pack(
            prot::READ | prot::WRITE,
            map::FIXED | map::SHARED | map::ZERO_ON_FORK,
        ),
    ) == Err(Errno::EINVAL);
    let invalid_fork = proc_fork(Handle::SELF_PROC, 1) == Err(Errno::EINVAL);
    println!(
        "proc-fork: invalid combo={} shared-zero={} flags={}",
        u8::from(invalid_combo),
        u8::from(invalid_shared),
        u8::from(invalid_fork)
    );
    if !invalid_combo || !invalid_shared || !invalid_fork {
        println!("proc-fork: FAIL invalid arguments");
        return Err(Errno::EINVAL);
    }

    write32(CONTROL_ADDR, spare.0);
    write32(CONTROL_ADDR + CONTROL_NOFORK_HANDLE, nofork_obj.0);
    write32(DATA_ADDR, PRE_MARKER);
    write32(NOFORK_ADDR, PRE_MARKER);
    write32(ZERO_ADDR, ZERO_MARKER);
    write32(NOFORK_OBJ_ADDR, PRE_MARKER);

    let child = proc_fork(Handle::SELF_PROC, 0)?;
    // A parent write after the fork must not reach the child's copied page.
    write32(DATA_ADDR, PARENT_MARKER);

    let stack = stack_top();
    let thread = thread_create(child, proc_fork_child_entry as *const () as usize, stack, 0)?;
    let mut code = 0i32;
    proc_wait(child, &mut code)?;
    close(thread)?;
    if code != 0 {
        println!("proc-fork: FAIL child code={code}");
        return Err(Errno::EINVAL);
    }

    let saw = read32(CONTROL_ADDR + CONTROL_DATA_SAW);
    let map_ok = read32(CONTROL_ADDR + CONTROL_MAP_OK);
    let zero_ok = read32(CONTROL_ADDR + CONTROL_ZERO_OK);
    let child_wrote = read32(CONTROL_ADDR + CONTROL_CHILD_WROTE);
    let handle_absent = read32(CONTROL_ADDR + CONTROL_HANDLE_ABSENT);
    let mapping_absent = read32(CONTROL_ADDR + CONTROL_MAPPING_ABSENT);
    let parent_data = read32(DATA_ADDR);
    let parent_nofork = read32(NOFORK_ADDR);
    let parent_zero = read32(ZERO_ADDR);
    let parent_nofork_obj = read32(NOFORK_OBJ_ADDR);
    println!("proc-fork: spare-handle={}", spare.0);
    println!("proc-fork: child code={code}");
    println!(
        "proc-fork: data parent=0x{parent_data:08x} child-saw={saw} child-wrote=0x{child_wrote:08x}"
    );
    println!("proc-fork: nofork map={map_ok} parent=0x{parent_nofork:08x}");
    println!("proc-fork: zero child-ok={zero_ok} parent=0x{parent_zero:08x}");
    println!(
        "proc-fork: nofork-object handle-absent={handle_absent} map-absent={mapping_absent} parent=0x{parent_nofork_obj:08x}"
    );
    let ok = saw == 1
        && map_ok == 1
        && zero_ok == 1
        && handle_absent == 1
        && mapping_absent == 1
        && child_wrote == CHILD_MARKER
        && parent_data == PARENT_MARKER
        && parent_nofork == PRE_MARKER
        && parent_zero == ZERO_MARKER
        && parent_nofork_obj == PRE_MARKER;
    if !ok {
        println!("proc-fork: FAIL checks");
        return Err(Errno::EINVAL);
    }
    close(child)?;

    for _ in 0..4 {
        let child = proc_fork(Handle::SELF_PROC, 0)?;
        let thread = thread_create(child, proc_fork_quick_entry as *const () as usize, stack, 0)?;
        let mut code = 0i32;
        proc_wait(child, &mut code)?;
        close(thread)?;
        close(child)?;
        if code != 0 {
            println!("proc-fork: FAIL loop code={code}");
            return Err(Errno::EINVAL);
        }
    }
    println!(
        "proc-fork: loop done cycles=4 counter={}",
        read32(CONTROL_ADDR + CONTROL_LOOP)
    );
    Ok(())
}

fn main(_args: raven_rt::Args) -> i32 {
    match run() {
        Ok(()) => {
            println!("proc-fork: done");
            0
        }
        Err(error) => {
            println!("proc-fork: error {error:?}");
            1
        }
    }
}
