// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S34 usercopy fixture: kernel writes into private copy-on-write pages.
//!
//! Each page is a private mapping of a memory object, so its PTE starts read-only while the VMA
//! is writable.  `uname`, `clock_get` and an event-queue record must resolve COW and deliver the
//! data; the same calls into a genuinely read-only mapping must return `EFAULT` without touching
//! the page.

#![no_std]
#![no_main]

use core::arch::asm;

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::{Handle, clock, map, prot};
use raven_ipc::protocol::{Opcode, PACKET_SIZE};
use raven_rt::entry;
use raven_rt::println;

entry!(main);

const PAGE: usize = 4096;
const SIGNALED: usize = 8;
const EVQ_ADD: usize = 1;
const SERVER_STACK_SIZE: usize = 16 * 1024;

#[repr(align(16))]
struct ServerStack([u8; SERVER_STACK_SIZE]);

static mut SERVER_STACK: ServerStack = ServerStack([0; SERVER_STACK_SIZE]);

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

fn exit(code: i32) -> ! {
    let _ = syscall1(Syscall::EXIT as usize, code as usize);
    loop {
        core::hint::spin_loop();
    }
}

fn fail(message: &str) -> ! {
    println!("usercopy-cow: failed {message}");
    exit(1)
}

fn handle_close(handle: Handle) {
    let _ = syscall1(Syscall::HANDLE_CLOSE as usize, handle.0 as usize);
}

fn memobj_create(len: usize) -> Result<Handle, Errno> {
    handle(syscall2(Syscall::MEMOBJ_CREATE as usize, len, 0))
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

/// Private mapping of a fresh memory object: writable VMA, read-only PTE (COW).
fn map_cow(len: usize) -> Result<usize, Errno> {
    let object = memobj_create(len)?;
    let address = map_memory(
        Handle::SELF_PROC,
        object,
        0,
        0,
        len,
        map::pack(prot::READ | prot::WRITE, 0),
    )?;
    handle_close(object);
    Ok(address)
}

/// Read-only mapping: the VMA itself has no write permission.
fn map_read_only(len: usize) -> Result<usize, Errno> {
    let object = memobj_create(len)?;
    let address = map_memory(
        Handle::SELF_PROC,
        object,
        0,
        0,
        len,
        map::pack(prot::READ, 0),
    )?;
    handle_close(object);
    Ok(address)
}

fn uname(address: usize) -> Result<(), Errno> {
    result(syscall1(Syscall::UNAME as usize, address)).map(|_| ())
}

fn clock_get(clock_id: u32, address: usize) -> Result<(), Errno> {
    // SAFETY: scalar arguments and a user pointer validated by the kernel.
    result(unsafe {
        syscall6(
            Syscall::CLOCK_GET as usize,
            clock_id as usize,
            address,
            0,
            0,
            0,
            0,
        )
    })
    .map(|_| ())
}

fn event_create() -> Result<Handle, Errno> {
    handle(syscall1(Syscall::EVENT_CREATE as usize, 0))
}

fn event_signal(event: Handle) -> Result<(), Errno> {
    result(syscall2(
        Syscall::EVENT_SIGNAL as usize,
        event.0 as usize,
        0,
    ))
    .map(|_| ())
}

fn evq_create() -> Result<Handle, Errno> {
    handle(syscall1(Syscall::EVQ_CREATE as usize, 0))
}

fn evq_ctl(
    queue: Handle,
    operation: usize,
    source: Handle,
    events: usize,
    cookie: u64,
) -> Result<(), Errno> {
    result(unsafe {
        syscall6(
            Syscall::EVQ_CTL as usize,
            queue.0 as usize,
            operation,
            source.0 as usize,
            events,
            cookie as usize,
            0,
        )
    })
    .map(|_| ())
}

fn evq_wait(queue: Handle, out: usize, max: usize, timeout_ns: u64) -> Result<usize, Errno> {
    result(unsafe {
        syscall6(
            Syscall::EVQ_WAIT as usize,
            queue.0 as usize,
            out,
            max,
            timeout_ns as usize,
            0,
            0,
        )
    })
}

fn read_i64(address: usize) -> i64 {
    // SAFETY: the caller passes an address inside its own writable mapping.
    i64::from_le(unsafe { (address as *const i64).read_unaligned() })
}

fn read_u64(address: usize) -> u64 {
    // SAFETY: the caller passes an address inside its own writable mapping.
    u64::from_le(unsafe { (address as *const u64).read_unaligned() })
}

fn read_u32(address: usize) -> u32 {
    // SAFETY: the caller passes an address inside its own writable mapping.
    u32::from_le(unsafe { (address as *const u32).read_unaligned() })
}

fn recv(packet: &mut [u8]) -> Result<Handle, Errno> {
    result(syscall2(
        Syscall::RECV as usize,
        packet.as_mut_ptr() as usize,
        packet.len(),
    ))
    .and_then(|value| u32::try_from(value).map(Handle).map_err(|_| Errno::EINVAL))
}

fn send(request: Handle, packet: &[u8]) -> Result<(), Errno> {
    result(syscall3(
        Syscall::SEND as usize,
        request.0 as usize,
        packet.as_ptr() as usize,
        packet.len(),
    ))
    .map(|_| ())
}

fn scheme_register(name: &[u8]) -> Result<(), Errno> {
    result(syscall3(
        Syscall::SCHEME_REGISTER as usize,
        name.as_ptr() as usize,
        name.len(),
        Handle::SELF_PROC.0 as usize,
    ))
    .map(|_| ())
}

fn open(path: &[u8], flags: u32) -> Result<Handle, Errno> {
    handle(unsafe {
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
}

fn fstat(file: Handle, buffer: usize, len: usize) -> Result<(), Errno> {
    result(syscall3(
        Syscall::FSTAT as usize,
        file.0 as usize,
        buffer,
        len,
    ))
    .map(|_| ())
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

fn thread_exit() -> ! {
    let _ = syscall1(Syscall::THREAD_EXIT as usize, 0);
    loop {
        core::hint::spin_loop();
    }
}

fn server_stack_top() -> usize {
    // SAFETY: only the address of the static is taken; the server thread is the only user.
    let base = unsafe { core::ptr::addr_of_mut!(SERVER_STACK.0) as usize };
    base.saturating_add(SERVER_STACK_SIZE)
}

/// Scheme server thread: answers `Open` and `Fstat` requests with inline data.
extern "C" fn server_thread(_argument: usize) -> ! {
    let mut bytes = [0u8; PACKET_SIZE];
    let mut finish = false;
    loop {
        let request = match recv(&mut bytes) {
            Ok(request) => request,
            Err(_) => continue,
        };
        let mut reply = match raven_ipc::protocol::decode(&bytes) {
            Ok(packet) => packet,
            Err(_) => continue,
        };
        reply.status = 0;
        if reply.opcode == Opcode::Open.as_u16() {
            reply.server_cookie = 1;
            reply.data_len = 0;
        } else if reply.opcode == Opcode::Fstat.as_u16() {
            let _ = reply.set_data(b"ipc-cow");
            finish = true;
        } else {
            reply.status = i32::from(Errno::EPROTO.0);
        }
        if raven_ipc::protocol::encode(&reply, &mut bytes).is_ok() {
            let _ = send(request, &bytes);
        }
        if finish {
            // The test needs the process to reach PID 1 exit; a blocked server thread would keep
            // it alive forever.
            thread_exit();
        }
    }
}

fn main(_args: raven_rt::Args) -> i32 {
    // 1. uname into a fresh COW page.
    let uname_page = match map_cow(PAGE) {
        Ok(page) => page,
        Err(_) => fail("uname map"),
    };
    if uname(uname_page).is_err() {
        fail("uname cow");
    }
    // SAFETY: the kernel just filled the page; sysname starts at offset 0.
    let sysname = unsafe { core::slice::from_raw_parts(uname_page as *const u8, 9) };
    if sysname != b"RavenBSD\0" {
        fail("uname sysname");
    }
    println!("usercopy-cow: uname cow ok");

    // 2. clock_get into another fresh COW page.
    let clock_page = match map_cow(PAGE) {
        Ok(page) => page,
        Err(_) => fail("clock map"),
    };
    if clock_get(clock::MONOTONIC, clock_page).is_err() {
        fail("clock cow");
    }
    let seconds = read_i64(clock_page);
    let nanos = read_i64(clock_page.saturating_add(8));
    if !(0..1_000_000_000).contains(&nanos) || seconds < 0 {
        fail("clock fields");
    }
    println!("usercopy-cow: clock cow ok");

    // 3. an event-queue record into a fresh COW page.
    let event_page = match map_cow(PAGE) {
        Ok(page) => page,
        Err(_) => fail("event map"),
    };
    let queue = match evq_create() {
        Ok(queue) => queue,
        Err(_) => fail("evq create"),
    };
    let event = match event_create() {
        Ok(event) => event,
        Err(_) => fail("event create"),
    };
    if evq_ctl(queue, EVQ_ADD, event, SIGNALED, 0x5347).is_err() {
        fail("evq add");
    }
    if event_signal(event).is_err() {
        fail("event signal");
    }
    let got = match evq_wait(queue, event_page, 1, 1_000_000_000) {
        Ok(got) => got,
        Err(_) => fail("evq wait"),
    };
    if got != 1 {
        fail("evq count");
    }
    if read_u64(event_page) != 0x5347 || read_u32(event_page.saturating_add(8)) != SIGNALED as u32 {
        fail("evq record");
    }
    println!("usercopy-cow: event cow ok");

    // 4. An IPC reply (inline FSTAT data) into a fresh COW page.  The scheme is registered by
    // this process and served by a second thread; the self-call capture keeps the client address,
    // so the router's final copy_to_user writes into the COW page.
    let ipc_page = match map_cow(PAGE) {
        Ok(page) => page,
        Err(_) => fail("ipc map"),
    };
    if scheme_register(b"cow").is_err() {
        fail("scheme register");
    }
    let server_entry: extern "C" fn(usize) -> ! = server_thread;
    let server = match thread_create(server_entry as usize, server_stack_top()) {
        Ok(server) => server,
        Err(_) => fail("server thread"),
    };
    let file = match open(b"cow:x", 1) {
        Ok(file) => file,
        Err(_) => fail("ipc open"),
    };
    if fstat(file, ipc_page, PAGE).is_err() {
        fail("ipc fstat");
    }
    // SAFETY: the kernel wrote the inline reply into this page.
    let reply = unsafe { core::slice::from_raw_parts(ipc_page as *const u8, 7) };
    if reply != b"ipc-cow" {
        fail("ipc data");
    }
    println!("usercopy-cow: ipc cow ok");
    handle_close(server);
    handle_close(file);

    // 5. A genuinely read-only VMA still fails with EFAULT.
    let read_only = match map_read_only(PAGE) {
        Ok(page) => page,
        Err(_) => fail("read-only map"),
    };
    if uname(read_only) != Err(Errno::EFAULT) {
        fail("uname read-only accepted");
    }
    if clock_get(clock::MONOTONIC, read_only) != Err(Errno::EFAULT) {
        fail("clock read-only accepted");
    }
    println!("usercopy-cow: read-only efault ok");

    handle_close(queue);
    handle_close(event);
    println!("usercopy-cow: ok");
    0
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    exit(2)
}
