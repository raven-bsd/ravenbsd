// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S42 per-thread TLS base fixture.
//!
//! One process starts two threads with distinct pages and reads the TLS marker
//! through the architectural register after a forced yield. A second process
//! gets its base set by the parent (the linuxd cross-process shape), and a
//! fresh thread records the cleared/default base of zero.

#![no_std]
#![no_main]

use core::arch::asm;

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::{Handle, map, prot};
use raven_initfs::Initfs;
use raven_rt::entry;
use raven_rt::println;
use raven_rt::spawn::{RtSys, SpawnOptions, Sys, spawn_with};
use raven_rt::sys;

entry!(main);

const PAGE: usize = 4096;
const MAGIC_A: u64 = 0x1111_2222_3333_4444;
const MAGIC_B: u64 = 0x5555_6666_7777_8888;
const MAGIC_CROSS: u64 = 0x9999_aaaa_bbbb_cccc;
const FIXED_TLS: usize = 0x7000_0000;

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
    // SAFETY: the caller supplies a valid RavenBSD syscall number and scalar arguments.
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
    // SAFETY: the caller supplies a valid RavenBSD syscall number and scalar arguments.
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

fn raw_handle(number: usize, a0: usize, a1: usize, a2: usize) -> Result<Handle, Errno> {
    result(unsafe { syscall6(number, a0, a1, a2, 0, 0, 0) })
        .and_then(|value| u32::try_from(value).map(Handle).map_err(|_| Errno::EINVAL))
}

fn set_tls(thread: Handle, base: usize) -> Result<(), Errno> {
    result(unsafe {
        syscall6(
            raven_abi::syscall::THREAD_SET_TLS,
            thread.0 as usize,
            base,
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

fn map_fixed(address: usize) -> Result<usize, Errno> {
    let flags = prot::READ | prot::WRITE | (map::FIXED << map::SHIFT);
    result(unsafe {
        syscall6(
            Syscall::MAP as usize,
            Handle::SELF_PROC.0 as usize,
            Handle::NONE.0 as usize,
            0,
            address,
            PAGE,
            flags,
        )
    })
}

#[cfg(target_arch = "x86_64")]
fn read_tls_base() -> u64 {
    let value: u64;
    // SAFETY: rdfsbase reads the current thread's FS base; FSGSBASE is enabled
    // by the kernel when the CPU supports it.
    unsafe {
        asm!("rdfsbase {}", out(reg) value, options(nostack, preserves_flags));
    }
    value
}

#[cfg(target_arch = "aarch64")]
fn read_tls_base() -> u64 {
    let value: u64;
    // SAFETY: TPIDR_EL0 is the user TLS base register.
    unsafe {
        asm!("mrs {}, tpidr_el0", out(reg) value, options(nostack, preserves_flags));
    }
    value
}

#[cfg(target_arch = "x86_64")]
fn read_tls_marker() -> u64 {
    let value: u64;
    // SAFETY: the thread's TLS base points at a mapped page; fs:0 reads its first word.
    unsafe {
        asm!("mov {}, fs:0", out(reg) value, options(nostack, readonly, preserves_flags));
    }
    value
}

#[cfg(target_arch = "aarch64")]
fn read_tls_marker() -> u64 {
    read_u64(read_tls_base() as usize)
}

fn main(args: raven_rt::Args) -> i32 {
    match args.get(1) {
        None | Some("init") => controller(),
        Some("spawned") => spawned(),
        Some(_) => 2,
    }
}

fn controller() -> i32 {
    let image = match sys::map_initfs() {
        Ok(image) => image,
        Err(_) => {
            println!("tls-base: initfs map failed");
            return 1;
        }
    };
    let initfs = match Initfs::parse(image) {
        Ok(initfs) => initfs,
        Err(_) => {
            println!("tls-base: initfs parse failed");
            return 1;
        }
    };

    let child = match spawn_cross(&initfs) {
        Ok(handle) => handle,
        Err(_) => {
            println!("tls-base: cross spawn failed");
            return 1;
        }
    };
    let child_code = proc_wait(child);
    if child_code != 0 {
        println!("tls-base: cross child exit={child_code}");
        return 1;
    }

    let page_a = match sys::map_anonymous(PAGE) {
        Ok(page) => page,
        Err(_) => return 1,
    };
    let page_b = match sys::map_anonymous(PAGE) {
        Ok(page) => page,
        Err(_) => return 1,
    };
    write_u64(page_a, MAGIC_A);
    write_u64(page_b, MAGIC_B);
    let stack_a = match sys::map_anonymous(2 * PAGE) {
        Ok(page) => page,
        Err(_) => return 1,
    };
    let stack_b = match sys::map_anonymous(2 * PAGE) {
        Ok(page) => page,
        Err(_) => return 1,
    };
    let _ = sys::thread_create(
        Handle::SELF_PROC,
        worker_entry as *const () as usize,
        stack_a + 2 * PAGE,
        page_a,
    );
    let _ = sys::thread_create(
        Handle::SELF_PROC,
        worker_entry as *const () as usize,
        stack_b + 2 * PAGE,
        page_b,
    );
    let _ = sys::nanosleep(300_000_000);

    if set_tls(Handle::NONE, page_a).is_err() {
        println!("tls-base: clear setup failed");
        return 1;
    }
    sys::yield_now();
    if set_tls(Handle::NONE, 0).is_err() {
        println!("tls-base: clear failed");
        return 1;
    }
    sys::yield_now();
    let base = read_tls_base();
    println!("tls-base: clear base={:#x}", base);
    if base != 0 {
        return 1;
    }
    let zero_stack = match sys::map_anonymous(2 * PAGE) {
        Ok(page) => page,
        Err(_) => return 1,
    };
    let _ = sys::thread_create(
        Handle::SELF_PROC,
        zero_entry as *const () as usize,
        zero_stack + 2 * PAGE,
        0,
    );
    let _ = sys::nanosleep(100_000_000);
    println!("tls-base: done");
    0
}

fn spawn_cross(initfs: &Initfs<'_>) -> Result<Handle, Errno> {
    let options = SpawnOptions {
        args: &["init", "spawned"],
        env: &[],
    };
    let mut spawner = TlsSys(RtSys);
    spawn_with(initfs, "/init", &options, &mut spawner).map_err(|_| Errno::EIO)
}

struct TlsSys(RtSys);

impl Sys for TlsSys {
    fn proc_create(&mut self) -> Result<Handle, Errno> {
        self.0.proc_create()
    }

    fn memobj_create(&mut self, len: usize) -> Result<Handle, Errno> {
        self.0.memobj_create(len)
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
        self.0.map(proc, memobj, offset, addr, len, flags)
    }

    fn unmap(&mut self, proc: Handle, addr: usize, len: usize) -> Result<(), Errno> {
        self.0.unmap(proc, addr, len)
    }

    fn protect(
        &mut self,
        proc: Handle,
        addr: usize,
        len: usize,
        protection: u8,
    ) -> Result<(), Errno> {
        self.0.protect(proc, addr, len, protection)
    }

    fn thread_create(
        &mut self,
        proc: Handle,
        entry: usize,
        stack_top: usize,
        arg: usize,
    ) -> Result<Handle, Errno> {
        let thread = self.0.thread_create(proc, entry, stack_top, arg)?;
        set_tls(thread, FIXED_TLS)?;
        Ok(thread)
    }

    fn handle_close(&mut self, handle: Handle) -> Result<(), Errno> {
        self.0.handle_close(handle)
    }

    fn write_parent_mem(&mut self, addr: usize, bytes: &[u8]) {
        self.0.write_parent_mem(addr, bytes);
    }
}

extern "C" fn worker_entry(arg: usize) -> ! {
    if set_tls(Handle::NONE, arg).is_err() {
        println!("tls-base: thread set failed");
        sys::thread_exit(1);
    }
    sys::yield_now();
    let _ = sys::nanosleep(10_000_000);
    let base = read_tls_base();
    let marker = read_tls_marker();
    println!(
        "tls-base: thread page={:#x} base={:#x} marker={:#x}",
        arg, base, marker
    );
    sys::thread_exit(0)
}

extern "C" fn zero_entry(_arg: usize) -> ! {
    let base = read_tls_base();
    println!("tls-base: zero base={:#x}", base);
    sys::thread_exit(0)
}

fn spawned() -> i32 {
    if map_fixed(FIXED_TLS).is_err() {
        println!("tls-base: cross map failed");
        return 1;
    }
    write_u64(FIXED_TLS, MAGIC_CROSS);
    let _ = sys::nanosleep(100_000_000);
    let base = read_tls_base();
    let marker = read_tls_marker();
    println!("tls-base: cross base={:#x} marker={:#x}", base, marker);
    if base != FIXED_TLS as u64 || marker != MAGIC_CROSS {
        return 1;
    }
    0
}

#[cfg(target_arch = "aarch64")]
fn read_u64(address: usize) -> u64 {
    // SAFETY: the fixture reads its own mapped TLS page.
    unsafe { (address as *const u64).read_unaligned() }
}

fn write_u64(address: usize, value: u64) {
    // SAFETY: the fixture writes its own mapped TLS page.
    unsafe {
        (address as *mut u64).write_unaligned(value);
    }
}
