// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S41 Linux-personality trap fixture.
//!
//! The controller spawns a native handler that registers `linux:`, a Linux
//! child that opens the handler before switching personality, a child that
//! switches personality with no handler, and a native child that proves
//! syscall number 1 is still `EXIT` for Native processes.

#![no_std]
#![no_main]

use core::arch::asm;

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::Handle;
use raven_initfs::Initfs;
use raven_ipc::protocol::{self, Opcode, PACKET_SIZE, SchemePacket};
use raven_proto_linux::{LinuxReplyV1, LinuxSyscallV1};
use raven_rt::entry;
use raven_rt::println;
use raven_rt::spawn::{RtSys, SpawnOptions, Sys, spawn, spawn_with};

entry!(main);

const O_RDWR: u32 = 3;
const PERSONALITY_LINUX: usize = 1;
const RETRIES: usize = 2_000;

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
    // SAFETY: the caller supplies a valid RavenBSD syscall number and arguments; the kernel
    // validates every handle and pointer.
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
    // SAFETY: the caller supplies a valid RavenBSD syscall number and arguments; the kernel
    // validates every handle and pointer.
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

fn raw_number(number: usize, a0: usize, a1: usize, a2: usize) -> isize {
    // SAFETY: scalar arguments only; used by the Linux child after its personality switch.
    unsafe { syscall6(number, a0, a1, a2, 0, 0, 0) }
}

fn proc_wait(process: Handle) -> i32 {
    let mut code = 0i32;
    // SAFETY: PROC_WAIT writes one i32 into the valid stack slot.
    let result = unsafe {
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
    if result < 0 { -1 } else { code }
}

fn main(args: raven_rt::Args) -> i32 {
    match args.get(1) {
        None => controller(),
        Some("handler") => handler(),
        Some("linux") => linux_child(),
        Some("nohandler") => nohandler_child(),
        Some("native") => native_child(),
        Some("flag") => flag_child(),
        Some(_) => 2,
    }
}

/// A spawner that creates a child with `PROC_CREATE` flag bit 0 (Linux), so
/// the flag path is tested independently of the self-set path.
struct FlagSys(RtSys);

impl Sys for FlagSys {
    fn proc_create(&mut self) -> Result<Handle, Errno> {
        raw_handle(Syscall::PROC_CREATE as usize, 1, 0, 0)
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
        self.0.thread_create(proc, entry, stack_top, arg)
    }

    fn handle_close(&mut self, handle: Handle) -> Result<(), Errno> {
        self.0.handle_close(handle)
    }

    fn write_parent_mem(&mut self, addr: usize, bytes: &[u8]) {
        self.0.write_parent_mem(addr, bytes);
    }
}

fn controller() -> i32 {
    let image = match raven_rt::sys::map_initfs() {
        Ok(image) => image,
        Err(_) => {
            println!("linux-trap: initfs map failed");
            return 1;
        }
    };
    let initfs = match Initfs::parse(image) {
        Ok(initfs) => initfs,
        Err(_) => {
            println!("linux-trap: initfs parse failed");
            return 1;
        }
    };
    let handler = match spawn_role(&initfs, "handler") {
        Ok(handle) => handle,
        Err(_) => {
            println!("linux-trap: handler spawn failed");
            return 1;
        }
    };
    let _ = handler;
    match open_retry("linux:") {
        Ok(handle) => {
            let _ = raven_rt::sys::close(handle);
        }
        Err(_) => {
            println!("linux-trap: handler did not register");
            return 1;
        }
    }

    let linux = match spawn_role(&initfs, "linux") {
        Ok(handle) => handle,
        Err(_) => {
            println!("linux-trap: linux child spawn failed");
            return 1;
        }
    };
    let nohandler = match spawn_role(&initfs, "nohandler") {
        Ok(handle) => handle,
        Err(_) => {
            println!("linux-trap: no-handler child spawn failed");
            return 1;
        }
    };
    let native = match spawn_role(&initfs, "native") {
        Ok(handle) => handle,
        Err(_) => {
            println!("linux-trap: native child spawn failed");
            return 1;
        }
    };
    let flag = match spawn_flag(&initfs) {
        Ok(handle) => handle,
        Err(_) => {
            println!("linux-trap: flag child spawn failed");
            return 1;
        }
    };

    let native_code = proc_wait(native);
    println!("linux-trap: native exit={native_code}");
    let nohandler_code = proc_wait(nohandler);
    println!("linux-trap: nohandler exit={nohandler_code}");
    let flag_code = proc_wait(flag);
    println!("linux-trap: flag exit={flag_code}");
    let _ = raven_rt::sys::nanosleep(500_000_000);
    let _ = raven_rt::sys::proc_terminate(linux, 0);
    let linux_code = proc_wait(linux);
    println!("linux-trap: linux terminated={linux_code}");
    if native_code != 7 || nohandler_code != -5 || flag_code != -5 || linux_code != 0 {
        println!("linux-trap: unexpected exit codes");
        return 1;
    }
    println!("linux-trap: done");
    0
}

fn spawn_role(initfs: &Initfs<'_>, role: &str) -> Result<Handle, Errno> {
    let options = SpawnOptions {
        args: &["init", role],
        env: &[],
    };
    spawn(initfs, "/init", &options).map_err(|_| Errno::EIO)
}

fn spawn_flag(initfs: &Initfs<'_>) -> Result<Handle, Errno> {
    let options = SpawnOptions {
        args: &["init", "flag"],
        env: &[],
    };
    let mut sys = FlagSys(RtSys);
    spawn_with(initfs, "/init", &options, &mut sys).map_err(|_| Errno::EIO)
}

fn open_retry(path: &str) -> Result<Handle, Errno> {
    let mut attempt = 0usize;
    while attempt < RETRIES {
        match raven_rt::sys::open(path, O_RDWR) {
            Ok(handle) => return Ok(handle),
            Err(Errno::ENOENT) => {
                let _ = raven_rt::sys::nanosleep(1_000_000);
                attempt = attempt.saturating_add(1);
            }
            Err(error) => return Err(error),
        }
    }
    Err(Errno::ETIMEDOUT)
}

fn handler() -> i32 {
    if raven_rt::sys::scheme_register("linux").is_err() {
        println!("linux-trap: handler register failed");
        return 1;
    }
    let mut bytes = [0u8; PACKET_SIZE];
    loop {
        let request = match raven_rt::sys::recv(&mut bytes) {
            Ok(handle) => handle,
            Err(_) => continue,
        };
        let packet = match protocol::decode(&bytes) {
            Ok(packet) => packet,
            Err(_) => continue,
        };
        let reply = if packet.opcode == Opcode::Call.as_u16() {
            match LinuxSyscallV1::decode(packet.data_bytes()) {
                Ok(call) => {
                    println!(
                        "linux-trap: req arch={} nr={} a0={}",
                        call.arch, call.nr, call.args[0] as i64
                    );
                    let value = match call.nr {
                        1 => 42,
                        9999 => -(Errno::ENOSYS.0 as i64),
                        9998 => 7,
                        _ => 0,
                    };
                    println!("linux-trap: reply nr={} value={value}", call.nr);
                    reply_packet(packet, Some(value))
                }
                Err(_) => reply_packet(packet, Some(-(Errno::EPROTO.0 as i64))),
            }
        } else {
            // Open/Close/Dup notifications on `linux:` are acknowledged so the
            // client's OPEN can complete; they carry no Linux request.
            reply_packet(packet, None)
        };
        let _ = raven_rt::sys::send(request, &reply);
    }
}

fn reply_packet(mut packet: SchemePacket, value: Option<i64>) -> [u8; PACKET_SIZE] {
    packet.status = 0;
    packet.attach_count = 0;
    packet.data.fill(0);
    packet.buffer_len = 8;
    packet.response_len = 9;
    match value {
        Some(value) => {
            let mut encoded = [0u8; 16];
            let length = LinuxReplyV1::new(value).encode(&mut encoded).unwrap_or(0);
            packet.data[..length].copy_from_slice(&encoded[..length]);
            packet.data_len = length as u32;
        }
        None => packet.data_len = 0,
    }
    let mut out = [0u8; PACKET_SIZE];
    let _ = protocol::encode(&packet, &mut out);
    out
}

fn linux_child() -> i32 {
    let handler = match open_retry("linux:") {
        Ok(handle) => handle,
        Err(_) => {
            println!("linux-trap: linux child handler open failed");
            return 1;
        }
    };
    let set = raw_number(
        raven_abi::syscall::PROC_SET_PERSONALITY,
        Handle::SELF_PROC.0 as usize,
        PERSONALITY_LINUX,
        handler.0 as usize,
    );
    if set != 0 {
        println!("linux-trap: personality set failed");
        return 1;
    }
    // Linux write(1, ...): the handler must see number 1, not native EXIT.
    let first = raw_number(1, 0, 0, 0);
    // Feed the returned value back as an argument so the handler proves the
    // value reached rax; then repeat with the unknown-number result.
    let second = raw_number(9999, first as usize, 0, 0);
    let _ = raw_number(9998, second as usize, 0, 0);
    loop {
        core::hint::spin_loop();
    }
}

fn nohandler_child() -> i32 {
    let set = raw_number(
        raven_abi::syscall::PROC_SET_PERSONALITY,
        Handle::SELF_PROC.0 as usize,
        PERSONALITY_LINUX,
        Handle::NONE.0 as usize,
    );
    if set != 0 {
        return 1;
    }
    // The kernel must terminate this process with -EIO, not run native EXIT.
    let _ = raw_number(1, 0, 0, 0);
    loop {
        core::hint::spin_loop();
    }
}

fn native_child() -> i32 {
    // Native process: number 1 is still EXIT and carries the requested code.
    let _ = raw_number(1, 7, 0, 0);
    0
}

fn flag_child() -> i32 {
    // Created with PROC_CREATE bit 0: already Linux, with no handler.
    let _ = raw_number(1, 0, 0, 0);
    loop {
        core::hint::spin_loop();
    }
}

fn raw_handle(number: usize, a0: usize, a1: usize, a2: usize) -> Result<Handle, Errno> {
    let result = raw_number(number, a0, a1, a2);
    if result < 0 {
        Err(Errno(result.unsigned_abs() as u16))
    } else {
        u32::try_from(result as usize)
            .map(Handle)
            .map_err(|_| Errno::EINVAL)
    }
}
