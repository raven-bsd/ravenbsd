// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S52 PG-23 fixture.
//!
//! A server thread registers the `pg23` scheme in the same process. The main thread opens
//! `pg23:file` write-only (so the handle carries `WRITE`, not `READ`), then:
//!
//! * `FSTAT` and `CALL` must succeed on the write-only handle (PG-23);
//! * revoking `WRITE` must not break them (any valid SchemeFile handle is enough);
//! * a non-SchemeFile handle must still be `EBADF`;
//! * a closed handle must be `EBADF`.
//!
//! Before PG-23 the kernel required `Rights::READ` for both calls, so the first `FSTAT` would
//! fail with `EBADF` and the test would stop there.

#![no_std]
#![no_main]

use core::arch::asm;
use core::sync::atomic::{AtomicU32, Ordering};

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::{Handle, Rights, map, prot};
use raven_proto_fs::{OpenFlags, S_IFREG, Stat};
use raven_rt::entry;
use raven_rt::println;
use raven_scheme::{Ctx, Handler, HandlerError, HandlerResult, Opened, Server};

entry!(main);

const PAGE: usize = 4096;
const STACK_ADDR: usize = 0x7000_0000;
const STACK_PAGES: usize = 16;
const STACK_LEN: usize = STACK_PAGES * PAGE;
const STACK_TOP: usize = STACK_ADDR + STACK_LEN;

/// Raw S11 write-only open flag (`raven_proto_fs::WIRE_OPEN_WRITE`).
const OPEN_WRITE: u32 = 1 << 1;

/// Server thread state: 0 starting, 1 registered, 2 failed.
static SERVER_READY: AtomicU32 = AtomicU32::new(0);

#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(
    r#"
    .section .text
    .global pg23_server_entry
    .type pg23_server_entry, @function
pg23_server_entry:
    call pg23_server_body
    mov edi, eax
    mov eax, 0x21
    syscall
    ud2
    .size pg23_server_entry, . - pg23_server_entry
"#
);

#[cfg(target_arch = "aarch64")]
core::arch::global_asm!(
    r#"
    .section .text
    .global pg23_server_entry
    .type pg23_server_entry, %function
pg23_server_entry:
    bl pg23_server_body
    mov x8, #0x21
    svc #0
1:
    b 1b
    .size pg23_server_entry, . - pg23_server_entry
"#
);

unsafe extern "C" {
    fn pg23_server_entry();
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

fn result(raw: isize) -> Result<usize, Errno> {
    if raw < 0 {
        Err(Errno(raw.unsigned_abs() as u16))
    } else {
        Ok(raw as usize)
    }
}

fn map_raw(address: usize, len: usize, flags: usize) -> Result<usize, Errno> {
    // SAFETY: scalar syscall arguments; the kernel validates the user address.
    result(unsafe {
        syscall6(
            Syscall::MAP as usize,
            Handle::SELF_PROC.0 as usize,
            Handle::NONE.0 as usize,
            0,
            address,
            len,
            flags,
        )
    })
}

fn thread_create(entry: usize, stack_top: usize) -> Result<Handle, Errno> {
    // SAFETY: the kernel validates the entry point and the stack top.
    result(unsafe {
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
    .map(|value| Handle(value as u32))
}

fn open_write(path: &str) -> Result<Handle, Errno> {
    // SAFETY: the path slice is live for the duration of the call.
    result(unsafe {
        syscall6(
            Syscall::OPEN as usize,
            path.as_ptr() as usize,
            path.len(),
            OPEN_WRITE as usize,
            0,
            0,
            0,
        )
    })
    .map(|value| Handle(value as u32))
}

fn fstat(handle: Handle, out: &mut [u8; 64]) -> Result<(), Errno> {
    // SAFETY: `out` is a live 64-byte writable buffer.
    result(unsafe {
        syscall6(
            Syscall::FSTAT as usize,
            handle.0 as usize,
            out.as_mut_ptr() as usize,
            out.len(),
            0,
            0,
            0,
        )
    })
    .map(|_| ())
}

fn call(handle: Handle, request: &[u8], response: &mut [u8]) -> Result<usize, Errno> {
    // SAFETY: both slices are live for the duration of the call.
    result(unsafe {
        syscall6(
            Syscall::CALL as usize,
            handle.0 as usize,
            request.as_ptr() as usize,
            request.len(),
            response.as_mut_ptr() as usize,
            response.len(),
            0,
        )
    })
}

fn handle_revoke(handle: Handle, rights: Rights) -> Result<(), Errno> {
    // SAFETY: scalar syscall arguments.
    result(unsafe {
        syscall6(
            Syscall::HANDLE_REVOKE as usize,
            handle.0 as usize,
            rights.bits() as usize,
            0,
            0,
            0,
            0,
        )
    })
    .map(|_| ())
}

fn handle_close(handle: Handle) -> Result<(), Errno> {
    // SAFETY: the handle value is copied; no pointer is dereferenced.
    result(unsafe {
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

fn memobj_create(len: usize) -> Result<Handle, Errno> {
    // SAFETY: the length is scalar; the kernel validates it.
    result(unsafe { syscall6(Syscall::MEMOBJ_CREATE as usize, len, 0, 0, 0, 0, 0) })
        .map(|value| Handle(value as u32))
}

/// The in-process scheme server. It serves exactly one file and one control call.
struct Files;

impl Handler for Files {
    fn open_with(
        &mut self,
        path: &str,
        flags: OpenFlags,
        _mode: u32,
        _ctx: &Ctx,
    ) -> HandlerResult<Opened> {
        if flags.directory() || path.trim_start_matches('/') != "file" {
            return Err(HandlerError::from(Errno::ENOENT));
        }
        Ok(Opened::File(1))
    }

    fn fstat(&mut self, file: u64) -> HandlerResult<Stat> {
        if file != 1 {
            return Err(HandlerError::from(Errno::EBADF));
        }
        Ok(Stat {
            ino: 1,
            size: 0,
            blocks: 0,
            mtime: 0,
            ctime: 0,
            atime: 0,
            mode: S_IFREG | 0o644,
            nlink: 1,
            uid: 0,
            gid: 0,
        })
    }

    fn call(&mut self, file: u64, request: &[u8], response: &mut [u8]) -> HandlerResult<usize> {
        if file != 1 {
            return Err(HandlerError::from(Errno::EBADF));
        }
        if request != [7] || response.is_empty() {
            return Err(HandlerError::from(Errno::EINVAL));
        }
        response[0] = 0xa5;
        Ok(1)
    }

    fn close(&mut self, file: u64) -> HandlerResult<()> {
        if file == 1 {
            Ok(())
        } else {
            Err(HandlerError::from(Errno::EBADF))
        }
    }
}

/// Server thread body. Registers the scheme, publishes readiness, then serves forever.
#[unsafe(no_mangle)]
pub extern "C" fn pg23_server_body() -> i32 {
    let mut server = match Server::register("pg23") {
        Ok(server) => server,
        Err(_) => {
            SERVER_READY.store(2, Ordering::Release);
            return 1;
        }
    };
    SERVER_READY.store(1, Ordering::Release);
    server.run(&mut Files);
}

fn wait_ready() -> Result<(), &'static str> {
    for _ in 0..1000 {
        match SERVER_READY.load(Ordering::Acquire) {
            1 => return Ok(()),
            2 => return Err("server registration failed"),
            _ => {}
        }
        // 1 ms; yields so a single-CPU QEMU can run the server thread.
        // SAFETY: the timeout is a scalar nanosecond count.
        let _ = result(unsafe { syscall6(Syscall::NANOSLEEP as usize, 1_000_000, 0, 0, 0, 0, 0) });
    }
    Err("server did not register")
}

fn run() -> Result<(), &'static str> {
    if map_raw(
        STACK_ADDR,
        STACK_LEN,
        map::pack(prot::READ | prot::WRITE, map::FIXED),
    ) != Ok(STACK_ADDR)
    {
        return Err("server stack map");
    }
    thread_create(pg23_server_entry as *const () as usize, STACK_TOP)
        .map_err(|_| "server thread create")?;
    wait_ready()?;

    // Write-only: rights are WRITE|DUP|TRANSFER, never READ.
    let file = open_write("pg23:file").map_err(|_| "write-only open")?;
    let mut stat = [0u8; 64];
    fstat(file, &mut stat).map_err(|_| "write-only fstat")?;
    let mode = u32::from_le_bytes([stat[48], stat[49], stat[50], stat[51]]);
    if mode & 0o170000 != S_IFREG {
        return Err("fstat mode");
    }

    let mut response = [0u8; 8];
    if call(file, &[7], &mut response).map_err(|_| "write-only call")? != 1 || response[0] != 0xa5 {
        return Err("call reply");
    }

    // Revoke the only data right: the handle is still a valid SchemeFile.
    handle_revoke(file, Rights::WRITE).map_err(|_| "revoke write")?;
    fstat(file, &mut stat).map_err(|_| "revoked fstat")?;
    if call(file, &[7], &mut response).map_err(|_| "revoked call")? != 1 {
        return Err("revoked call reply");
    }

    // Rights::NONE still means "any valid SchemeFile", not "any handle".
    let memory = memobj_create(PAGE).map_err(|_| "memobj create")?;
    if fstat(memory, &mut stat) != Err(Errno::EBADF) {
        return Err("fstat accepted a memory handle");
    }
    if call(memory, &[7], &mut response) != Err(Errno::EBADF) {
        return Err("call accepted a memory handle");
    }

    // A closed handle is no longer a SchemeFile handle.
    handle_close(file).map_err(|_| "file close")?;
    if fstat(file, &mut stat) != Err(Errno::EBADF) {
        return Err("fstat accepted a closed handle");
    }
    Ok(())
}

fn main(_args: raven_rt::Args) -> i32 {
    match run() {
        Ok(()) => {
            println!("libc-rights: ok");
            0
        }
        Err(message) => {
            println!("libc-rights: fail {}", message);
            1
        }
    }
}
