// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S24 `/init` fixture: address-space primitives and a spawned child.
//!
//! The parent maps the initfs, exercises `PROC_CREATE`/`MAP`/`UNMAP`/`PROTECT`/`MEMOBJ_CREATE`,
//! builds a copy-on-write test, then creates `/child`, maps its segments and stack, starts its
//! thread and waits for exit code 7. It then repeats 100 create/exit cycles for the frame-leak
//! check.

#![no_std]
#![no_main]

use core::arch::asm;

use raven_abi::errno::Errno;
use raven_abi::layout::x86_64::{USER_END, USER_MIN};
use raven_abi::syscall::Syscall;
use raven_abi::types::{Handle, map, prot};
use raven_elf::{Elf, LoadMode, Machine, StepKind, plan_load};
use raven_initfs::{Entry, Initfs};
use raven_rt::entry;

entry!(main);

const PAGE: usize = 4096;
const INITFS_BASE: usize = 0x1000_0000;
const STACK_SCRATCH: usize = 0x2000_0000;
const COW_ADDR: usize = 0x3000_0000;
const STACK_SIZE: usize = 64 * 1024;
const STACK_TOP: usize = (USER_END as usize) - PAGE;
const STACK_BASE: usize = STACK_TOP - STACK_SIZE;
const FRAME_CYCLES: usize = 100;

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
    // SAFETY: the S24 ABI fixes the syscall register assignment; the caller passes valid args.
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

#[inline(always)]
fn syscall1(number: usize, a0: usize) -> isize {
    // SAFETY: the register convention is the S24 ABI.
    unsafe { syscall6(number, a0, 0, 0, 0, 0, 0) }
}

#[inline(always)]
fn syscall2(number: usize, a0: usize, a1: usize) -> isize {
    // SAFETY: the register convention is the S24 ABI.
    unsafe { syscall6(number, a0, a1, 0, 0, 0, 0) }
}

#[inline(always)]
fn syscall3(number: usize, a0: usize, a1: usize, a2: usize) -> isize {
    // SAFETY: the register convention is the S24 ABI.
    unsafe { syscall6(number, a0, a1, a2, 0, 0, 0) }
}

fn handle_from_ret(value: isize) -> Result<Handle, Errno> {
    let value = Errno::from_ret(value)?;
    u32::try_from(value).map(Handle).map_err(|_| Errno::EINVAL)
}

fn proc_create() -> Result<Handle, Errno> {
    handle_from_ret(syscall1(Syscall::PROC_CREATE as usize, 0))
}

fn memobj_create(len: usize) -> Result<Handle, Errno> {
    handle_from_ret(syscall2(Syscall::MEMOBJ_CREATE as usize, len, 0))
}

fn map(
    proc: Handle,
    memobj: Handle,
    offset: usize,
    addr: usize,
    len: usize,
    flags: usize,
) -> Result<usize, Errno> {
    // SAFETY: the register convention is the S24 ABI; the kernel validates every argument.
    Errno::from_ret(unsafe {
        syscall6(
            Syscall::MAP as usize,
            proc.0 as usize,
            memobj.0 as usize,
            offset,
            addr,
            len,
            flags,
        )
    })
}

fn unmap(proc: Handle, addr: usize, len: usize) -> Result<(), Errno> {
    Errno::from_ret(syscall3(
        Syscall::UNMAP as usize,
        proc.0 as usize,
        addr,
        len,
    ))
    .map(|_| ())
}

fn protect(proc: Handle, addr: usize, len: usize, protection: usize) -> Result<(), Errno> {
    // SAFETY: the register convention is the S24 ABI; the kernel validates every argument.
    Errno::from_ret(unsafe {
        syscall6(
            Syscall::PROTECT as usize,
            proc.0 as usize,
            addr,
            len,
            protection,
            0,
            0,
        )
    })
    .map(|_| ())
}

fn thread_create(
    proc: Handle,
    entry: usize,
    stack_top: usize,
    arg: usize,
) -> Result<Handle, Errno> {
    // SAFETY: the register convention is the S24 ABI; the kernel validates every argument.
    handle_from_ret(unsafe {
        syscall6(
            Syscall::THREAD_CREATE as usize,
            proc.0 as usize,
            entry,
            stack_top,
            arg,
            0,
            0,
        )
    })
}

fn proc_wait(proc: Handle, code: &mut i32) -> Result<(), Errno> {
    Errno::from_ret(syscall2(
        Syscall::PROC_WAIT as usize,
        proc.0 as usize,
        code as *mut i32 as usize,
    ))
    .map(|_| ())
}

fn handle_close(handle: Handle) -> Result<(), Errno> {
    Errno::from_ret(syscall1(Syscall::HANDLE_CLOSE as usize, handle.0 as usize)).map(|_| ())
}

fn write(bytes: &[u8]) {
    let _ = syscall3(
        Syscall::DEBUG_WRITE as usize,
        bytes.as_ptr() as usize,
        bytes.len(),
        0,
    );
}

fn write_hex(mut value: usize) {
    let mut buffer = [0u8; 18];
    buffer[0] = b'0';
    buffer[1] = b'x';
    let mut index = 16;
    while index >= 2 {
        let digit = (value & 0xf) as u8;
        buffer[index] = if digit < 10 {
            b'0' + digit
        } else {
            b'a' + digit - 10
        };
        value >>= 4;
        if index == 2 {
            break;
        }
        index -= 1;
    }
    write(&buffer);
}

fn fail(name: &str, error: Errno) -> ! {
    write(b"s24: negative failed: ");
    write(name.as_bytes());
    write(b" got ");
    write_hex(error.0 as usize);
    write(b"\n");
    exit(1)
}

fn fail_text(text: &[u8]) -> ! {
    write(text);
    exit(1)
}

fn expect(name: &str, result: Result<usize, Errno>, expected: Errno) -> bool {
    match result {
        Err(error) if error == expected => true,
        Err(error) => {
            write(b"s24: negative failed: ");
            write(name.as_bytes());
            write(b" got ");
            write_hex(error.0 as usize);
            write(b" want ");
            write_hex(expected.0 as usize);
            write(b"\n");
            false
        }
        Ok(_) => {
            write(b"s24: negative failed: ");
            write(name.as_bytes());
            write(b" unexpectedly succeeded\n");
            false
        }
    }
}

fn expect_unit(name: &str, result: Result<(), Errno>, expected: Errno) -> bool {
    expect(name, result.map(|_| 0), expected)
}

fn main(_args: raven_rt::Args) -> i32 {
    // Learn the initfs size from its page-aligned header, then map the whole archive.
    let first = match map(Handle::SELF_PROC, Handle::INITFS, 0, 0, PAGE, prot::READ) {
        Ok(address) => address,
        Err(error) => fail("initfs header", error),
    };
    // SAFETY: the mapping is one readable page and C7 puts total_size at byte 40.
    let total = unsafe { ((first + 40) as *const u64).read_unaligned() } as usize;
    let _ = unmap(Handle::SELF_PROC, first, PAGE);
    if total == 0 || !total.is_multiple_of(PAGE) || INITFS_BASE.checked_add(total).is_none() {
        fail_text(b"s24: invalid initfs size\n");
    }
    let initfs_map = match map(
        Handle::SELF_PROC,
        Handle::INITFS,
        0,
        INITFS_BASE,
        total,
        map::pack(prot::READ, map::FIXED),
    ) {
        Ok(address) => address,
        Err(error) => fail("initfs map", error),
    };
    // SAFETY: the loader mapped the validated initfs bytes at `initfs_map`.
    let image = unsafe { core::slice::from_raw_parts(initfs_map as *const u8, total) };
    let initfs = match Initfs::parse(image) {
        Ok(initfs) => initfs,
        Err(_) => fail_text(b"s24: invalid initfs\n"),
    };
    let child_entry = match initfs.get("/child") {
        Some(entry) => entry,
        None => fail_text(b"s24: /child missing\n"),
    };
    let elf = match Elf::parse(child_entry.data, Machine::X86_64, USER_MIN..USER_END) {
        Ok(elf) => elf,
        Err(_) => fail_text(b"s24: invalid child ELF\n"),
    };
    if elf.interp().is_some() {
        fail_text(b"s24: child has an interpreter\n");
    }
    if plan_load(&elf, PAGE as u64, LoadMode::Strict).is_err() {
        fail_text(b"s24: child ELF is not page aligned\n");
    }

    let mut ok = true;
    let scratch = match map(
        Handle::SELF_PROC,
        Handle::NONE,
        0,
        0,
        PAGE,
        map::pack(prot::READ | prot::WRITE, 0),
    ) {
        Ok(address) => address,
        Err(error) => fail("scratch", error),
    };
    ok &= expect(
        "map overlap",
        map(
            Handle::SELF_PROC,
            Handle::NONE,
            0,
            scratch,
            PAGE,
            map::pack(prot::READ | prot::WRITE, map::FIXED),
        ),
        Errno::EEXIST,
    );
    ok &= expect(
        "map unaligned",
        map(
            Handle::SELF_PROC,
            Handle::NONE,
            0,
            scratch + 1,
            PAGE,
            map::pack(prot::READ | prot::WRITE, 0),
        ),
        Errno::EINVAL,
    );
    ok &= expect(
        "map zero length",
        map(Handle::SELF_PROC, Handle::NONE, 0, 0, 0, prot::READ),
        Errno::EINVAL,
    );
    ok &= expect(
        "map w+x",
        map(
            Handle::SELF_PROC,
            Handle::NONE,
            0,
            0,
            PAGE,
            map::pack(prot::WRITE | prot::EXEC, 0),
        ),
        Errno::EINVAL,
    );
    ok &= expect(
        "map address zero",
        map(
            Handle::SELF_PROC,
            Handle::NONE,
            0,
            0,
            PAGE,
            map::pack(prot::READ, map::FIXED),
        ),
        Errno::EINVAL,
    );
    ok &= expect(
        "map past user end",
        map(
            Handle::SELF_PROC,
            Handle::NONE,
            0,
            USER_END as usize - PAGE,
            2 * PAGE,
            map::pack(prot::READ, map::FIXED),
        ),
        Errno::EINVAL,
    );
    ok &= expect(
        "map bad process",
        map(
            Handle::new(0x00FF_FFFF, 0),
            Handle::NONE,
            0,
            0,
            PAGE,
            prot::READ,
        ),
        Errno::EBADF,
    );
    ok &= expect_unit(
        "unmap hole",
        unmap(Handle::SELF_PROC, scratch + PAGE, PAGE),
        Errno::EINVAL,
    );
    ok &= expect_unit(
        "protect hole",
        protect(Handle::SELF_PROC, scratch + PAGE, PAGE, prot::READ),
        Errno::ENOMEM,
    );
    ok &= protect(Handle::SELF_PROC, scratch, PAGE, prot::READ).is_ok();
    ok &= expect_unit(
        "protect w+x",
        protect(Handle::SELF_PROC, scratch, PAGE, prot::WRITE | prot::EXEC),
        Errno::EINVAL,
    );
    ok &= unmap(Handle::SELF_PROC, scratch, PAGE).is_ok();

    let cow_ok = cow_test(initfs_map, &child_entry);

    let code = spawn_child(&child_entry, &elf, "world");
    if code != 7 {
        write(b"s24: child exit code ");
        write_hex(code as usize);
        write(b"\n");
        exit(1);
    }
    if !ok {
        write(b"s24: parent negatives failed\n");
        exit(1);
    }
    write(b"s24: negatives ok\n");
    write(b"s24: child exit code 7\n");
    if !cow_ok {
        write(b"s24: cow failed\n");
        exit(1);
    }
    write(b"s24: cow ok\n");

    let mut cycle = 0usize;
    while cycle < FRAME_CYCLES {
        let code = spawn_child(&child_entry, &elf, "framecheck");
        if code != 7 {
            write(b"s24: frame cycle failed\n");
            exit(1);
        }
        cycle += 1;
    }
    let _ = unmap(Handle::SELF_PROC, INITFS_BASE, total);
    exit(0)
}

fn spawn_child(child: &Entry<'_>, elf: &Elf<'_>, argument: &str) -> i32 {
    let child_handle = match proc_create() {
        Ok(handle) => handle,
        Err(error) => fail("proc_create", error),
    };
    let steps = match plan_load(elf, PAGE as u64, LoadMode::Strict) {
        Ok(steps) => steps,
        Err(_) => fail_text(b"s24: child plan failed\n"),
    };
    for step in steps {
        let flags = map::pack(step.prot as usize, map::FIXED);
        let result = match step.kind {
            StepKind::File { offset } => {
                let offset = match (child.data_offset).checked_add(offset) {
                    Some(offset) => offset,
                    None => fail_text(b"s24: child offset overflow\n"),
                };
                let offset = usize::try_from(offset)
                    .unwrap_or_else(|_| fail_text(b"s24: child offset too large\n"));
                map(
                    child_handle,
                    Handle::INITFS,
                    offset,
                    step.vaddr as usize,
                    step.len as usize,
                    flags,
                )
            }
            StepKind::Zero => map(
                child_handle,
                Handle::NONE,
                0,
                step.vaddr as usize,
                step.len as usize,
                flags,
            ),
            StepKind::CopyTail { .. } => fail_text(b"s24: copy tail not page aligned\n"),
        };
        if let Err(error) = result {
            fail("child segment", error);
        }
    }

    let memobj = match memobj_create(STACK_SIZE) {
        Ok(handle) => handle,
        Err(error) => fail("memobj_create", error),
    };
    let scratch = match map(
        Handle::SELF_PROC,
        memobj,
        0,
        STACK_SCRATCH,
        STACK_SIZE,
        map::pack(prot::READ | prot::WRITE, map::SHARED | map::FIXED),
    ) {
        Ok(address) => address,
        Err(error) => fail("stack scratch", error),
    };
    let sp0 = build_stack(scratch, argument);
    if let Err(error) = map(
        child_handle,
        memobj,
        0,
        STACK_BASE,
        STACK_SIZE,
        map::pack(prot::READ | prot::WRITE, map::SHARED | map::FIXED),
    ) {
        fail("child stack", error);
    }
    if thread_create(child_handle, elf.entry() as usize, sp0, sp0).is_err() {
        fail_text(b"s24: thread_create failed\n");
    }
    let _ = unmap(Handle::SELF_PROC, scratch, STACK_SIZE);
    let _ = handle_close(memobj);

    let mut code = -1i32;
    if let Err(error) = proc_wait(child_handle, &mut code) {
        fail("proc_wait", error);
    }
    let _ = handle_close(child_handle);
    code
}

fn build_stack(mem_base: usize, argument: &str) -> usize {
    let sp0 = STACK_TOP - 0x200;
    let argv0 = STACK_TOP - 0x100;
    let argv1 = STACK_TOP - 0x80;
    write_cstr(mem_base, argv0 - STACK_BASE, "/child");
    write_cstr(mem_base, argv1 - STACK_BASE, argument);
    let mut offset = sp0 - STACK_BASE;
    store_u64(mem_base, offset, 2);
    offset += 8;
    store_u64(mem_base, offset, argv0 as u64);
    offset += 8;
    store_u64(mem_base, offset, argv1 as u64);
    offset += 8;
    store_u64(mem_base, offset, 0);
    offset += 8;
    store_u64(mem_base, offset, 0);
    offset += 8;
    store_u64(mem_base, offset, 6); // AT_PAGESZ
    offset += 8;
    store_u64(mem_base, offset, PAGE as u64);
    offset += 8;
    store_u64(mem_base, offset, 0);
    offset += 8;
    store_u64(mem_base, offset, 0);
    sp0
}

fn write_cstr(mem_base: usize, offset: usize, text: &str) {
    let bytes = text.as_bytes();
    if offset + bytes.len() + 1 > STACK_SIZE {
        fail_text(b"s24: stack string overflow\n");
    }
    // SAFETY: the parent owns the writable shared stack mapping and the offset was checked.
    let mut index = 0usize;
    while index < bytes.len() {
        let Some(byte) = bytes.get(index).copied() else {
            break;
        };
        // SAFETY: the offset is inside the writable shared stack mapping.
        unsafe {
            ((mem_base + offset + index) as *mut u8).write(byte);
        }
        index += 1;
    }
    // SAFETY: the NUL terminator is inside the checked string range.
    unsafe {
        ((mem_base + offset + bytes.len()) as *mut u8).write(0);
    }
}

fn store_u64(mem_base: usize, offset: usize, value: u64) {
    if offset + 8 > STACK_SIZE {
        fail_text(b"s24: stack word overflow\n");
    }
    // SAFETY: the parent owns the writable shared stack mapping and the offset was checked.
    unsafe {
        ((mem_base + offset) as *mut u64).write_unaligned(value.to_le());
    }
}

fn cow_test(initfs_map: usize, child: &Entry<'_>) -> bool {
    let offset = match usize::try_from(child.data_offset) {
        Ok(offset) => offset,
        Err(_) => return false,
    };
    let cow = match map(
        Handle::SELF_PROC,
        Handle::INITFS,
        offset,
        COW_ADDR,
        PAGE,
        map::pack(prot::READ | prot::WRITE, map::FIXED),
    ) {
        Ok(address) => address,
        Err(_) => return false,
    };
    // SAFETY: the read-only initfs mapping covers the child data page and byte 7.
    let original = unsafe { ((initfs_map + offset + 7) as *const u8).read() };
    let changed = original ^ 0x5a;
    // SAFETY: the writable COW mapping is one page and byte 7 is inside it.
    unsafe {
        ((cow + 7) as *mut u8).write(changed);
    }
    // SAFETY: the read-only initfs mapping is one page plus byte 7.
    let reread = unsafe { ((initfs_map + offset + 7) as *const u8).read() };
    // SAFETY: the writable COW mapping is one page plus byte 7.
    let cow_read = unsafe { ((cow + 7) as *const u8).read() };
    let _ = unmap(Handle::SELF_PROC, COW_ADDR, PAGE);
    reread == original && cow_read == changed
}

fn exit(code: i32) -> ! {
    let _ = syscall1(Syscall::EXIT as usize, code as usize);
    loop {
        core::hint::spin_loop();
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    exit(2)
}
