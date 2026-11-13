// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S43 thread-kick fixture.
//!
//! Covers a futex waiter kicked by another thread, a self-kick before
//! `NANOSLEEP`, a running thread whose next sleep is interrupted, a
//! cross-process kick with/without `SIGNAL`, and a zero-bit no-op.

#![no_std]
#![no_main]

use core::arch::asm;
use core::sync::atomic::AtomicU32;

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::{Handle, Rights};
use raven_initfs::Initfs;
use raven_rt::entry;
use raven_rt::println;
use raven_rt::spawn::{RtSys, SpawnOptions, Sys, spawn_with};
use raven_rt::sys;

entry!(main);

const CANCEL: u32 = 1 << 1;
const CHECK: u32 = 1 << 0;
const PAGE: usize = 4096;

static FUTEX_WORD: AtomicU32 = AtomicU32::new(0);

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

fn kick(thread: Handle, bits: u32) -> Result<(), Errno> {
    result(unsafe {
        syscall6(
            raven_abi::syscall::THREAD_KICK,
            thread.0 as usize,
            bits as usize,
            0,
            0,
            0,
            0,
        )
    })
    .map(|_| ())
}

fn proc_wait(process: Handle) -> i32 {
    let mut code = 0i32;
    let raw = unsafe {
        syscall6(
            Syscall::PROC_WAIT as usize,
            process.0 as usize,
            &mut code as *mut i32 as usize,
            0,
            0,
            0,
            0,
        )
    };
    if raw < 0 { -1 } else { code }
}

fn main(args: raven_rt::Args) -> i32 {
    match args.get(1) {
        None | Some("init") => controller(),
        Some("blocked") => blocked_child(),
        Some(_) => 2,
    }
}

fn controller() -> i32 {
    let image = match sys::map_initfs() {
        Ok(image) => image,
        Err(_) => {
            println!("kick-eintr: initfs map failed");
            return 1;
        }
    };
    let initfs = match Initfs::parse(image) {
        Ok(initfs) => initfs,
        Err(_) => {
            println!("kick-eintr: initfs parse failed");
            return 1;
        }
    };

    let (child, child_thread) = match spawn_blocked(&initfs) {
        Ok(pair) => pair,
        Err(_) => {
            println!("kick-eintr: cross spawn failed");
            return 1;
        }
    };
    let no_signal = match sys::handle_dup(child_thread, Rights::READ) {
        Ok(handle) => handle,
        Err(_) => {
            println!("kick-eintr: thread dup failed");
            return 1;
        }
    };
    match kick(no_signal, CANCEL) {
        Err(Errno::EPERM) => println!("kick-eintr: nokick=eperm"),
        _ => {
            println!("kick-eintr: nokick unexpected");
            return 1;
        }
    }
    if kick(child_thread, CANCEL).is_err() {
        println!("kick-eintr: cross kick failed");
        return 1;
    }
    println!("kick-eintr: cross kick=0");
    let child_code = proc_wait(child);
    println!("kick-eintr: cross child exit={child_code}");
    if child_code != 0 {
        return 1;
    }

    let futex_stack = match sys::map_anonymous(2 * PAGE) {
        Ok(stack) => stack,
        Err(_) => return 1,
    };
    let futex = match sys::thread_create(
        Handle::SELF_PROC,
        futex_waiter as *const () as usize,
        futex_stack + 2 * PAGE,
        0,
    ) {
        Ok(handle) => handle,
        Err(_) => return 1,
    };
    let kicker_stack = match sys::map_anonymous(2 * PAGE) {
        Ok(stack) => stack,
        Err(_) => return 1,
    };
    let _ = sys::thread_create(
        Handle::SELF_PROC,
        kicker as *const () as usize,
        kicker_stack + 2 * PAGE,
        futex.0 as usize,
    );

    let self_stack = match sys::map_anonymous(2 * PAGE) {
        Ok(stack) => stack,
        Err(_) => return 1,
    };
    let _ = sys::thread_create(
        Handle::SELF_PROC,
        self_kicker as *const () as usize,
        self_stack + 2 * PAGE,
        0,
    );

    let running_stack = match sys::map_anonymous(2 * PAGE) {
        Ok(stack) => stack,
        Err(_) => return 1,
    };
    let running = match sys::thread_create(
        Handle::SELF_PROC,
        running_waiter as *const () as usize,
        running_stack + 2 * PAGE,
        0,
    ) {
        Ok(handle) => handle,
        Err(_) => return 1,
    };
    if kick(running, CHECK).is_err() {
        println!("kick-eintr: running kick failed");
        return 1;
    }

    let _ = sys::nanosleep(1_000_000_000);
    if kick(Handle::NONE, 0).is_err() {
        println!("kick-eintr: zero kick failed");
        return 1;
    }
    match sys::nanosleep(20_000_000) {
        Ok(()) => println!("kick-eintr: zero=ok"),
        Err(Errno::EINTR) => {
            println!("kick-eintr: zero interrupted");
            return 1;
        }
        Err(_) => return 1,
    }
    println!("kick-eintr: done");
    0
}

fn spawn_blocked(initfs: &Initfs<'_>) -> Result<(Handle, Handle), Errno> {
    let options = SpawnOptions {
        args: &["init", "blocked"],
        env: &[],
    };
    let mut spawner = KickSys {
        inner: RtSys,
        thread: None,
    };
    let child = spawn_with(initfs, "/init", &options, &mut spawner).map_err(|_| Errno::EIO)?;
    let thread = spawner.thread.ok_or(Errno::EIO)?;
    Ok((child, thread))
}

struct KickSys {
    inner: RtSys,
    thread: Option<Handle>,
}

impl Sys for KickSys {
    fn proc_create(&mut self) -> Result<Handle, Errno> {
        self.inner.proc_create()
    }

    fn memobj_create(&mut self, len: usize) -> Result<Handle, Errno> {
        self.inner.memobj_create(len)
    }

    fn map(
        &mut self,
        proc: Handle,
        memobj: Handle,
        offset: usize,
        addr: usize,
        len: usize,
        flags: usize,
    ) -> Result<usize, Errno> {
        self.inner.map(proc, memobj, offset, addr, len, flags)
    }

    fn unmap(&mut self, proc: Handle, addr: usize, len: usize) -> Result<(), Errno> {
        self.inner.unmap(proc, addr, len)
    }

    fn protect(
        &mut self,
        proc: Handle,
        addr: usize,
        len: usize,
        protection: u8,
    ) -> Result<(), Errno> {
        self.inner.protect(proc, addr, len, protection)
    }

    fn thread_create(
        &mut self,
        proc: Handle,
        entry: usize,
        stack_top: usize,
        arg: usize,
    ) -> Result<Handle, Errno> {
        let thread = self.inner.thread_create(proc, entry, stack_top, arg)?;
        self.thread = Some(thread);
        Ok(thread)
    }

    fn handle_close(&mut self, handle: Handle) -> Result<(), Errno> {
        self.inner.handle_close(handle)
    }

    fn write_parent_mem(&mut self, addr: usize, bytes: &[u8]) {
        self.inner.write_parent_mem(addr, bytes);
    }
}

extern "C" fn futex_waiter(_arg: usize) -> ! {
    match sys::futex_wait(&FUTEX_WORD, 0, None) {
        Err(Errno::EINTR) => println!("kick-eintr: futex=eintr"),
        other => println!("kick-eintr: futex unexpected={other:?}"),
    }
    sys::thread_exit(0)
}

extern "C" fn kicker(arg: usize) -> ! {
    let target = Handle(arg as u32);
    let _ = sys::nanosleep(50_000_000);
    match kick(target, CANCEL) {
        Ok(()) => println!("kick-eintr: futex kick=0"),
        Err(_) => println!("kick-eintr: futex kick failed"),
    }
    sys::thread_exit(0)
}

extern "C" fn self_kicker(_arg: usize) -> ! {
    if kick(Handle::NONE, CANCEL).is_err() {
        println!("kick-eintr: self kick failed");
        sys::thread_exit(1);
    }
    match sys::nanosleep(1_000_000_000) {
        Err(Errno::EINTR) => println!("kick-eintr: self=eintr"),
        _ => println!("kick-eintr: self unexpected"),
    }
    sys::thread_exit(0)
}

extern "C" fn running_waiter(_arg: usize) -> ! {
    // No syscall: the controller kicks us while this loop runs, then our first
    // interruptible wait must consume the sticky word immediately.
    let mut spins = 0u64;
    while spins < 100_000 {
        spins = spins.saturating_add(1);
        core::hint::spin_loop();
    }
    match sys::nanosleep(1_000_000_000) {
        Err(Errno::EINTR) => println!("kick-eintr: running=eintr"),
        _ => println!("kick-eintr: running unexpected"),
    }
    sys::thread_exit(0)
}

fn blocked_child() -> i32 {
    match sys::futex_wait(&FUTEX_WORD, 0, None) {
        Err(Errno::EINTR) => {
            println!("kick-eintr: cross child=eintr");
            0
        }
        _ => {
            println!("kick-eintr: cross child unexpected");
            1
        }
    }
}
