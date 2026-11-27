// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S730 raw-client conformance fixture.
//!
//! `/init` proves the kernel file-creation mask is authoritative state: the raw `proc_umask`
//! swap, `ALLPERMS` normalisation, fork inheritance, exec preservation and concurrent swaps. It
//! then serves one scheme `Open` and proves `proc_creation_context` returns the *kernel-recorded*
//! client context, rejects a short record, a forged handle, a non-creation request and a request
//! that was already retired. `/cred-client` is the unprivileged caller: it can neither obtain the
//! server's request handle nor turn its own file handle into a context.

#![no_std]
#![no_main]
#![allow(unsafe_code)] // Raw syscall assembly; every block states the kernel-validated invariant.

use core::arch::asm;
use core::sync::atomic::{AtomicUsize, Ordering};

use raven_abi::errno::Errno;
use raven_abi::syscall::{PROC_CREATION_CONTEXT, PROC_UMASK, Syscall};
use raven_abi::types::creation;
use raven_abi::types::{Handle, map, prot};
use raven_initfs::Initfs;
use raven_ipc::protocol::{Opcode, PACKET_SIZE, SchemePacket, decode, encode};
use raven_rt::entry;
use raven_rt::println;
use raven_rt::spawn::{SpawnOptions, spawn};

entry!(main);

/// Page size.
const PAGE: usize = 4096;
/// Shared control page address.
const CONTROL: usize = 0x4000_0000;
/// Fixed initfs mapping address (same as the other fixtures).
const INITFS_BASE: usize = 0x1000_0000;
/// Thread stack size.
const STACK: usize = 64 * 1024;
/// Swaps performed by each concurrent thread.
const SWAPS: usize = 512;

/// `PROC_FORK`: raw number already used by the fixtures until the ABI crate carries it.
const PROC_FORK: usize = 0x39;
/// `PROC_HANDLE_INFO`: pid behind a process handle.
const PROC_HANDLE_INFO: usize = 0x3E;

// Control-page slots.
const SLOT_FORK_OLD: usize = 0;
const SLOT_FORK_NOW: usize = 1;
const SLOT_BAD0: usize = 2;
const SLOT_BAD1: usize = 3;
const SLOT_DONE0: usize = 4;
const SLOT_DONE1: usize = 5;

#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(
    r#"
    .section .text
    .global cred_fork_entry
    .type cred_fork_entry, @function
cred_fork_entry:
    call cred_fork_body
    mov edi, eax
    mov eax, 0x21
    syscall
    ud2
    .size cred_fork_entry, . - cred_fork_entry

    .global cred_swap_entry
    .type cred_swap_entry, @function
cred_swap_entry:
    call cred_swap_body
    mov edi, eax
    mov eax, 0x21
    syscall
    ud2
    .size cred_swap_entry, . - cred_swap_entry

    .global cred_exec_entry
    .type cred_exec_entry, @function
cred_exec_entry:
    call cred_exec_body
    mov edi, eax
    mov eax, 0x21
    syscall
    ud2
    .size cred_exec_entry, . - cred_exec_entry
"#
);

#[cfg(target_arch = "aarch64")]
core::arch::global_asm!(
    r#"
    .section .text
    .global cred_fork_entry
    .type cred_fork_entry, %function
cred_fork_entry:
    bl cred_fork_body
    mov x8, #0x21
    svc #0
1:
    b 1b
    .size cred_fork_entry, . - cred_fork_entry

    .global cred_swap_entry
    .type cred_swap_entry, %function
cred_swap_entry:
    bl cred_swap_body
    mov x8, #0x21
    svc #0
1:
    b 1b
    .size cred_swap_entry, . - cred_swap_entry

    .global cred_exec_entry
    .type cred_exec_entry, %function
cred_exec_entry:
    bl cred_exec_body
    mov x8, #0x21
    svc #0
1:
    b 1b
    .size cred_exec_entry, . - cred_exec_entry
"#
);

unsafe extern "C" {
    fn cred_fork_entry();
    fn cred_swap_entry();
    fn cred_exec_entry();
}

/// Image of `/umask-client`, resolved before the fork. The forked child has a COW copy of these
/// words and of the initfs mapping.
static EXEC_IMAGE: AtomicUsize = AtomicUsize::new(0);
/// Image length.
static EXEC_IMAGE_LEN: AtomicUsize = AtomicUsize::new(0);

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

fn checked(raw: isize) -> Result<usize, Errno> {
    Errno::from_ret(raw)
}

fn read64(index: usize) -> u64 {
    // SAFETY: the fixture owns the shared control mapping.
    unsafe { core::ptr::read_volatile((CONTROL + index * 8) as *const u64) }
}

fn write64(index: usize, value: u64) {
    // SAFETY: the fixture owns the shared control mapping.
    unsafe {
        core::ptr::write_volatile((CONTROL + index * 8) as *mut u64, value);
    }
}

fn umask(mask: u32) -> Result<u32, Errno> {
    checked(unsafe { syscall6(PROC_UMASK, mask as usize, 0, 0, 0, 0, 0) })
        .map(|value| value as u32)
}

fn creation_context(
    request: Handle,
    mode: u32,
    out: &mut creation::Context,
    len: usize,
) -> Result<(), Errno> {
    checked(unsafe {
        syscall6(
            PROC_CREATION_CONTEXT,
            request.0 as usize,
            mode as usize,
            out as *mut creation::Context as usize,
            len,
            0,
            0,
        )
    })
    .map(|_| ())
}

/// Fork child body: report the inherited mask and prove the child's swap is local.
#[unsafe(no_mangle)]
pub extern "C" fn cred_fork_body() -> i32 {
    let Ok(old) = umask(0o022) else {
        return 1;
    };
    write64(SLOT_FORK_OLD, u64::from(old));
    let Ok(now) = umask(0o027) else {
        return 2;
    };
    write64(SLOT_FORK_NOW, u64::from(now));
    0
}

/// Concurrent swap body: every observed value must be one of the two legal masks.
#[unsafe(no_mangle)]
pub extern "C" fn cred_swap_body(slot: usize) -> i32 {
    let mut next = if slot == 0 { 0o022 } else { 0o077 };
    let mut bad = 0u64;
    for _ in 0..SWAPS {
        let Ok(old) = umask(next) else {
            return 1;
        };
        if old != 0o022 && old != 0o077 {
            bad = bad.saturating_add(1);
        }
        next = if next == 0o022 { 0o077 } else { 0o022 };
    }
    write64(if slot == 0 { SLOT_BAD0 } else { SLOT_BAD1 }, bad);
    write64(if slot == 0 { SLOT_DONE0 } else { SLOT_DONE1 }, 1);
    0
}

/// Exec child body: replace this image with `/umask-client`; the mask must survive the exec.
#[unsafe(no_mangle)]
pub extern "C" fn cred_exec_body() -> i32 {
    let pointer = EXEC_IMAGE.load(Ordering::Acquire) as *const u8;
    let length = EXEC_IMAGE_LEN.load(Ordering::Acquire);
    if pointer.is_null() || length == 0 {
        return 1;
    }
    // SAFETY: the words name the read-only initfs mapping inherited by the fork.
    let image = unsafe { core::slice::from_raw_parts(pointer, length) };
    let argv: [&[u8]; 1] = [b"/umask-client"];
    match raven_rt::exec::exec_image(image, b"/umask-client", &argv, &[], &[]) {
        Ok(()) => 2,
        Err(_) => 3,
    }
}

fn fork_exec_preserves(initfs: &Initfs<'_>, parent_mask: u32) -> Result<(), Errno> {
    let entry = initfs.get("/umask-client").ok_or(Errno::ENOENT)?;
    EXEC_IMAGE.store(entry.data.as_ptr() as usize, Ordering::Release);
    EXEC_IMAGE_LEN.store(entry.data.len(), Ordering::Release);
    let _ = umask(parent_mask)?;
    let child = proc_fork(Handle::SELF_PROC)?;
    let anchor = 0u64;
    let stack = (&anchor as *const u64 as usize) & !0xf;
    thread_create(
        child,
        cred_exec_entry as *const () as usize,
        stack,
        0,
    )?;
    let code = proc_wait(child)?;
    if code != 0 {
        println!("proc-umask: fail exec child code={code}");
        return Err(Errno::EIO);
    }
    Ok(())
}

fn memobj_create(len: usize) -> Result<Handle, Errno> {
    checked(unsafe { syscall6(Syscall::MEMOBJ_CREATE as usize, len, 0, 0, 0, 0, 0) })
        .map(|value| Handle(value as u32))
}

fn map_raw(
    process: Handle,
    object: Handle,
    offset: usize,
    address: usize,
    len: usize,
    flags: usize,
) -> Result<usize, Errno> {
    checked(unsafe {
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

fn scheme_register(name: &str) -> Result<(), Errno> {
    checked(unsafe {
        syscall6(
            Syscall::SCHEME_REGISTER as usize,
            name.as_ptr() as usize,
            name.len(),
            Handle::SELF_PROC.0 as usize,
            0,
            0,
            0,
        )
    })
    .map(|_| ())
}

fn open(path: &str, flags: u32) -> Result<Handle, Errno> {
    checked(unsafe {
        syscall6(
            Syscall::OPEN as usize,
            path.as_ptr() as usize,
            path.len(),
            flags as usize,
            0,
            0,
            0,
        )
    })
    .map(|value| Handle(value as u32))
}

fn close(handle: Handle) -> Result<(), Errno> {
    checked(unsafe { syscall6(Syscall::CLOSE as usize, handle.0 as usize, 0, 0, 0, 0, 0) })
        .map(|_| ())
}

fn recv(packet: &mut [u8; PACKET_SIZE]) -> Result<Handle, Errno> {
    checked(unsafe {
        syscall6(
            Syscall::RECV as usize,
            packet.as_mut_ptr() as usize,
            packet.len(),
            0,
            0,
            0,
            0,
        )
    })
    .map(|value| Handle(value as u32))
}

fn send(request: Handle, packet: &[u8; PACKET_SIZE]) -> Result<(), Errno> {
    checked(unsafe {
        syscall6(
            Syscall::SEND as usize,
            request.0 as usize,
            packet.as_ptr() as usize,
            packet.len(),
            0,
            0,
            0,
        )
    })
    .map(|_| ())
}

fn reply(request: &SchemePacket) -> Result<[u8; PACKET_SIZE], Errno> {
    let mut response = SchemePacket::request(
        Opcode::from_u16(request.opcode).ok_or(Errno::EPROTO)?,
        request.request_id,
        request.file_id,
    );
    response.status = 0;
    response.server_cookie = 1;
    let mut bytes = [0u8; PACKET_SIZE];
    encode(&response, &mut bytes).map_err(|_| Errno::EPROTO)?;
    Ok(bytes)
}

fn proc_fork(source: Handle) -> Result<Handle, Errno> {
    checked(unsafe { syscall6(PROC_FORK, source.0 as usize, 0, 0, 0, 0, 0) })
        .map(|value| Handle(value as u32))
}

fn thread_create(process: Handle, entry: usize, stack_top: usize, arg: usize) -> Result<(), Errno> {
    checked(unsafe {
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
    .map(|_| ())
}

fn proc_wait(process: Handle) -> Result<i32, Errno> {
    let mut code = 0i32;
    checked(unsafe {
        syscall6(
            Syscall::PROC_WAIT as usize,
            process.0 as usize,
            &mut code as *mut i32 as usize,
            0,
            0,
            0,
            0,
        )
    })
    .map(|_| code)
}

fn proc_handle_info(process: Handle) -> Result<u64, Errno> {
    checked(unsafe { syscall6(PROC_HANDLE_INFO, process.0 as usize, 0, 0, 0, 0, 0) })
        .map(|value| value as u64)
}

fn nanosleep(ns: u64) -> Result<(), Errno> {
    checked(unsafe { syscall6(Syscall::NANOSLEEP as usize, ns as usize, 0, 0, 0, 0, 0) })
        .map(|_| ())
}

fn map_initfs_image() -> Result<&'static [u8], Errno> {
    let page = map_raw(
        Handle::SELF_PROC,
        Handle::INITFS,
        0,
        0,
        PAGE,
        prot::READ,
    )?;
    // SAFETY: the mapping is one readable page; the C7 header puts total_size at byte 40.
    let total = unsafe { ((page + 40) as *const u64).read_unaligned() as usize };
    let _ = checked(unsafe {
        syscall6(
            Syscall::UNMAP as usize,
            Handle::SELF_PROC.0 as usize,
            page,
            PAGE,
            0,
            0,
            0,
        )
    });
    if total == 0 || !total.is_multiple_of(PAGE) {
        return Err(Errno::EINVAL);
    }
    let base = map_raw(
        Handle::SELF_PROC,
        Handle::INITFS,
        0,
        INITFS_BASE,
        total,
        map::pack(prot::READ, map::FIXED),
    )?;
    // SAFETY: the initfs mapping was validated above and is read-only for this process.
    Ok(unsafe { core::slice::from_raw_parts(base as *const u8, total) })
}

fn spawn_path(initfs: &Initfs<'_>, path: &str) -> Result<Handle, Errno> {
    let options = SpawnOptions {
        args: &[],
        env: &[],
    };
    spawn(initfs, path, &options).map_err(|_| Errno::EIO)
}

fn fork_inherits(parent_mask: u32) -> Result<(), Errno> {
    let child = proc_fork(Handle::SELF_PROC)?;
    let entry = cred_fork_entry as *const () as usize;
    let anchor = 0u64;
    let stack = (&anchor as *const u64 as usize) & !0xf;
    thread_create(child, entry, stack, 0)?;
    let code = proc_wait(child)?;
    if code != 0 {
        println!("proc-umask: fail fork child code={code}");
        return Err(Errno::EIO);
    }
    let old = read64(SLOT_FORK_OLD);
    let now = read64(SLOT_FORK_NOW);
    if old != u64::from(parent_mask) || now != 0o022 {
        println!("proc-umask: fail fork old={old:o} now={now:o}");
        return Err(Errno::EIO);
    }
    Ok(())
}

fn concurrent_swaps() -> Result<(), Errno> {
    // Both threads must observe only the two masks they install; start from 0o022.
    let _ = umask(0o022)?;
    let stack = map_raw(
        Handle::SELF_PROC,
        Handle::NONE,
        0,
        0,
        STACK,
        map::pack(prot::READ | prot::WRITE, 0),
    )?;
    let entry = cred_swap_entry as *const () as usize;
    let top0 = stack.saturating_add(STACK);
    let top1 = top0.saturating_sub(STACK / 2);
    thread_create(Handle::SELF_PROC, entry, top0, 0)?;
    thread_create(Handle::SELF_PROC, entry, top1, 1)?;
    let mut waited = 0usize;
    while (read64(SLOT_DONE0) == 0 || read64(SLOT_DONE1) == 0) && waited < 5000 {
        nanosleep(1_000_000)?;
        waited = waited.saturating_add(1);
    }
    if read64(SLOT_DONE0) == 0 || read64(SLOT_DONE1) == 0 {
        println!("proc-umask: fail swap timeout");
        return Err(Errno::ETIMEDOUT);
    }
    let bad0 = read64(SLOT_BAD0);
    let bad1 = read64(SLOT_BAD1);
    let final_mask = umask(0o027)?;
    if bad0 != 0 || bad1 != 0 || (final_mask != 0o022 && final_mask != 0o077) {
        println!("proc-umask: fail swap bad={bad0}/{bad1} final={final_mask:o}");
        return Err(Errno::EIO);
    }
    Ok(())
}

fn serve_context(client: Handle) -> Result<(), Errno> {
    let client_pid = proc_handle_info(client)?;
    let mut packet_bytes = [0u8; PACKET_SIZE];
    let request_handle = recv(&mut packet_bytes)?;
    let request = decode(&packet_bytes).map_err(|_| Errno::EPROTO)?;
    if request.opcode != Opcode::Open.as_u16() {
        println!("proc-umask: fail unexpected opcode");
        return Err(Errno::EPROTO);
    }
    let mut context = creation::Context::default();
    creation_context(request_handle, 0o666, &mut context, creation::SIZE)?;
    if context.version != creation::VERSION
        || context.size as usize != creation::SIZE
        || context.request_id != request.request_id
        || context.pid != client_pid
        || context.uid != 0
        || context.euid != 0
        || context.umask != 0o027
        || context.mode != 0o640
    {
        println!(
            "proc-umask: fail context pid={} want={client_pid} umask={:o} mode={:o}",
            context.pid, context.umask, context.mode
        );
        return Err(Errno::EIO);
    }
    if creation_context(request_handle, 0o666, &mut context, creation::SIZE - 1) != Err(Errno::EINVAL)
    {
        println!("proc-umask: fail short record accepted");
        return Err(Errno::EIO);
    }
    if creation_context(Handle(0x00FF_FFFF), 0o666, &mut context, creation::SIZE)
        != Err(Errno::EBADF)
    {
        println!("proc-umask: fail forged handle accepted");
        return Err(Errno::EIO);
    }
    let bytes = reply(&request)?;
    send(request_handle, &bytes)?;
    if creation_context(request_handle, 0o666, &mut context, creation::SIZE)
        != Err(Errno::EBADF)
    {
        println!("proc-umask: fail retired request accepted");
        return Err(Errno::EIO);
    }
    // The client now closes the returned handle: a non-creation request must not mint a context.
    let close_handle = recv(&mut packet_bytes)?;
    let close_request = decode(&packet_bytes).map_err(|_| Errno::EPROTO)?;
    if close_request.opcode != Opcode::Close.as_u16()
        || creation_context(close_handle, 0o666, &mut context, creation::SIZE) != Err(Errno::EINVAL)
    {
        println!("proc-umask: fail non-creation request accepted");
        return Err(Errno::EIO);
    }
    let bytes = reply(&close_request)?;
    send(close_handle, &bytes)?;
    let code = proc_wait(client)?;
    if code != 0 {
        println!("proc-umask: fail client code={code}");
        return Err(Errno::EIO);
    }
    Ok(())
}

fn cred_client() -> i32 {
    if umask(0o027).is_err() {
        println!("cred-client: fail umask");
        return 1;
    }
    let file = match open("cred:x", 1) {
        Ok(handle) => handle,
        Err(error) => {
            println!("cred-client: fail open errno={}", error.0);
            return 1;
        }
    };
    let mut context = creation::Context::default();
    if creation_context(file, 0o666, &mut context, creation::SIZE) != Err(Errno::EINVAL) {
        println!("cred-client: fail file handle minted a context");
        return 1;
    }
    if creation_context(Handle(0x00FF_FFFF), 0o666, &mut context, creation::SIZE)
        != Err(Errno::EBADF)
    {
        println!("cred-client: fail forged handle");
        return 1;
    }
    // Another process's request handle is a capability in the server's table only. Guessing its
    // index/generation in the client's table must fail, never mint a context.
    let mut index = 0u32;
    while index < 64 {
        let forged = Handle::new(index, 0);
        if creation_context(forged, 0o666, &mut context, creation::SIZE).is_ok() {
            println!("cred-client: fail foreign handle index={index}");
            return 1;
        }
        index = index.saturating_add(1);
    }
    if close(file).is_err() {
        println!("cred-client: fail close");
        return 1;
    }
    0
}

fn main(args: raven_rt::Args) -> i32 {
    match args.get(0) {
        Some("/cred-client") => return cred_client(),
        Some("/umask-client") => {
            let Ok(mask) = umask(0o027) else {
                println!("umask-client: fail");
                return 1;
            };
            println!("umask-client: inherited={mask:o}");
            return 0;
        }
        _ => {}
    }
    println!("proc-umask: start");
    let control = match memobj_create(PAGE) {
        Ok(handle) => handle,
        Err(_) => return 1,
    };
    if map_raw(
        Handle::SELF_PROC,
        control,
        0,
        CONTROL,
        PAGE,
        map::pack(prot::READ | prot::WRITE, map::FIXED | map::SHARED),
    )
    .is_err()
    {
        println!("proc-umask: fail control map");
        return 1;
    }
    let first = umask(0o077);
    let second = umask(0o1_7077);
    let third = umask(0o022);
    // 0o1_7077 is stored as ALLPERMS 0o7077: the out-of-range 0o10000 bit is dropped and the
    // setuid/setgid bits survive exactly as FreeBSD `sys_umask` stores `newmask & ALLPERMS`.
    if first != Ok(0o022) || second != Ok(0o077) || third != Ok(0o7077) {
        println!(
            "proc-umask: fail swap sequence a={first:?} b={second:?} c={third:?}"
        );
        return 1;
    }
    if umask(0o027).is_err() || fork_inherits(0o027).is_err() {
        return 1;
    }
    if concurrent_swaps().is_err() {
        return 1;
    }
    let Ok(image) = map_initfs_image() else {
        println!("proc-umask: fail initfs map");
        return 1;
    };
    let Ok(initfs) = Initfs::parse(image) else {
        println!("proc-umask: fail initfs parse");
        return 1;
    };
    if fork_exec_preserves(&initfs, 0o027).is_err() {
        return 1;
    }
    if scheme_register("cred").is_err() {
        println!("proc-umask: fail scheme");
        return 1;
    }
    let Ok(client) = spawn_path(&initfs, "/cred-client") else {
        println!("proc-umask: fail client spawn");
        return 1;
    };
    if let Err(error) = serve_context(client) {
        println!("proc-umask: fail context errno={}", error.0);
        return 1;
    }
    println!("proc-umask: ok");
    0
}
