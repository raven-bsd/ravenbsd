// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! B21a fixture: prove the kernel captures the x86-64 Linux trap frame and resumes the thread at a
//! chosen RIP/RSP.
//!
//! Roles, selected by the first argument (the same binary is packed as `/init`):
//! * controller: spawns the handler, waits for `linux:`, spawns the Linux child, watches a shared
//!   resume flag and reaps the child;
//! * handler: registers `linux:`, decodes the frame from the request packet's `path` field, replies
//!   with a frame whose RIP/RSP point at the child's resume target and stack;
//! * linux: opens `linux:`, sets the Linux personality and a nonzero FS base, then issues a custom
//!   syscall; it resumes at the target written by the handler and sets the shared flag.

#![no_std]
#![no_main]

extern crate alloc;

use core::arch::asm;

use raven_abi::errno::Errno;
use raven_abi::types::{Handle, map, prot};
use raven_initfs::Initfs;
use raven_ipc::protocol::{self, Opcode, PACKET_SIZE, SchemePacket};
use raven_proto_linux::{FRAME_LEN, LinuxFrame, LinuxReplyV1, LinuxSyscallV1, REPLY_LEN};
use raven_rt::entry;
use raven_rt::println;
use raven_rt::spawn::{RtSys, SpawnOptions, Sys, spawn};
use raven_rt::sys;

entry!(main);

const O_RDWR: u32 = 3;
const PERSONALITY_LINUX: usize = 1;
const CUSTOM_NR: u64 = 7777;
const RETRIES: usize = 2000;
const PAGE: usize = 4096;
/// Address the controller maps the shared resume page at in the child.
const FLAG_ADDR: usize = 0x0000_5000_0000_0000;
const RESUME_MAGIC: u64 = 0x5245_5355_4d45_0001;
const FAIL_MAGIC: u64 = 0x4641_494c_0001;
const TLS_BASE: usize = 0x7000;

static STACK: [u8; 32 * 1024] = [0; 32 * 1024];

fn main(args: raven_rt::Args) -> i32 {
    match args.get(1) {
        None => controller(),
        Some("handler") => handler(),
        Some("linux") => linux_child(),
        Some(_) => 2,
    }
}

fn controller() -> i32 {
    let image = match sys::map_initfs() {
        Ok(image) => image,
        Err(_) => {
            println!("linux-frame: FAIL initfs map");
            return 1;
        }
    };
    let initfs = match Initfs::parse(image) {
        Ok(initfs) => initfs,
        Err(_) => {
            println!("linux-frame: FAIL initfs parse");
            return 1;
        }
    };
    let handler = match spawn_role(&initfs, "handler") {
        Ok(handle) => handle,
        Err(_) => {
            println!("linux-frame: FAIL handler spawn");
            return 1;
        }
    };
    if open_retry("linux:").is_err() {
        println!("linux-frame: FAIL handler did not register");
        return 1;
    }
    let child = match spawn_role(&initfs, "linux") {
        Ok(handle) => handle,
        Err(_) => {
            println!("linux-frame: FAIL child spawn");
            return 1;
        }
    };

    let mut rt = RtSys;
    let object = match rt.memobj_create(PAGE) {
        Ok(object) => object,
        Err(_) => {
            println!("linux-frame: FAIL memobj");
            return 1;
        }
    };
    let child_flags = map::pack(prot::READ | prot::WRITE, map::SHARED | map::FIXED);
    if rt
        .map(child, object, 0, FLAG_ADDR, PAGE, child_flags)
        .is_err()
    {
        println!("linux-frame: FAIL shared map");
        return 1;
    }
    let local_flags = map::pack(prot::READ | prot::WRITE, map::SHARED);
    let local = match rt.map(Handle::SELF_PROC, object, 0, 0, PAGE, local_flags) {
        Ok(address) => address,
        Err(_) => {
            println!("linux-frame: FAIL local map");
            return 1;
        }
    };
    rt.write_parent_mem(local, &0u64.to_le_bytes());

    let mut seen = false;
    let mut failed = false;
    let mut attempt = 0usize;
    while attempt < RETRIES {
        // SAFETY: `local` is the controller's read/write mapping of the shared page.
        let value = unsafe { core::ptr::read_volatile(local as *const u64) };
        if value == RESUME_MAGIC {
            seen = true;
            break;
        }
        if value == FAIL_MAGIC {
            failed = true;
            break;
        }
        let _ = sys::nanosleep(1_000_000);
        attempt = attempt.saturating_add(1);
    }
    if seen {
        println!("linux-frame: resumed");
    } else if failed {
        println!("linux-frame: FAIL not resumed");
    } else {
        println!("linux-frame: FAIL timeout");
    }
    let _ = sys::proc_terminate(child, 0);
    let mut status = 0i32;
    let _ = rt.proc_wait(child, &mut status);
    println!("linux-frame: child exit={status}");
    let _ = sys::proc_terminate(handler, 0);
    let _ = rt.proc_wait(handler, &mut status);
    if seen && status == 0 {
        println!("linux-frame: done");
        0
    } else {
        1
    }
}

fn handler() -> i32 {
    if sys::scheme_register("linux").is_err() {
        println!("linux-frame: FAIL handler register");
        return 1;
    }
    let mut bytes = [0u8; PACKET_SIZE];
    loop {
        let request = match sys::recv(&mut bytes) {
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
                    match LinuxFrame::decode(&packet.path) {
                        Ok(frame) => println!(
                            "linux-frame: handler nr={} rip=0x{:x} rsp=0x{:x} fs=0x{:x}",
                            call.nr, frame.rip, frame.rsp, frame.fs_base
                        ),
                        Err(_) => println!("linux-frame: handler no frame"),
                    }
                    if call.nr == CUSTOM_NR {
                        let mut frame = LinuxFrame::decode(&packet.path).unwrap_or_default();
                        frame.rip = call.args[0];
                        frame.rsp = call.args[1];
                        frame.gprs[0] = 0;
                        let mut encoded = [0u8; FRAME_LEN];
                        let _ = frame.encode(&mut encoded);
                        println!(
                            "linux-frame: handler target=0x{:x} stack=0x{:x}",
                            frame.rip, frame.rsp
                        );
                        reply_packet(packet, Some(0), Some(&encoded))
                    } else {
                        reply_packet(packet, Some(0), None)
                    }
                }
                Err(_) => reply_packet(packet, Some(-(Errno::EPROTO.0 as i64)), None),
            }
        } else {
            reply_packet(packet, None, None)
        };
        let _ = sys::send(request, &reply);
    }
}

fn linux_child() -> i32 {
    let handler = match open_retry("linux:") {
        Ok(handle) => handle,
        Err(_) => {
            println!("linux-frame: FAIL child open");
            return 1;
        }
    };
    // The controller maps the shared resume page after it gets the spawn handle; let it win the
    // race while this process is still native.
    let _ = sys::nanosleep(50_000_000);
    // A nonzero TLS base proves the FS field is captured; the target does not use it.
    let tls = raw_number(
        raven_abi::syscall::THREAD_SET_TLS,
        Handle::NONE.0 as usize,
        TLS_BASE,
    );
    if tls != 0 {
        println!("linux-frame: FAIL tls base");
        return 1;
    }
    let set = raw_number3(
        raven_abi::syscall::PROC_SET_PERSONALITY,
        Handle::SELF_PROC.0 as usize,
        PERSONALITY_LINUX,
        handler.0 as usize,
    );
    if set != 0 {
        println!("linux-frame: FAIL personality");
        return 1;
    }
    let target = resumed as *const () as usize;
    let stack = STACK.as_ptr() as usize + STACK.len();
    let _ = raw_number(CUSTOM_NR as usize, target, stack);
    // The custom syscall returned here only if the handler did not supply a resume frame.
    // SAFETY: the controller mapped a writable shared page at FLAG_ADDR before the child ran.
    unsafe {
        core::ptr::write_volatile(FLAG_ADDR as *mut u64, FAIL_MAGIC);
    }
    loop {
        core::hint::spin_loop();
    }
}

extern "C" fn resumed() -> ! {
    // SAFETY: the controller mapped a writable shared page at FLAG_ADDR before the child ran.
    unsafe {
        core::ptr::write_volatile(FLAG_ADDR as *mut u64, RESUME_MAGIC);
    }
    loop {
        core::hint::spin_loop();
    }
}

fn spawn_role(initfs: &Initfs<'_>, role: &str) -> Result<Handle, Errno> {
    let options = SpawnOptions {
        args: &["init", role],
        env: &[],
    };
    spawn(initfs, "/init", &options).map_err(|_| Errno::EIO)
}

fn open_retry(path: &str) -> Result<Handle, Errno> {
    let mut attempt = 0usize;
    while attempt < RETRIES {
        match sys::open(path, O_RDWR) {
            Ok(handle) => return Ok(handle),
            Err(Errno::ENOENT) => {
                let _ = sys::nanosleep(1_000_000);
                attempt = attempt.saturating_add(1);
            }
            Err(error) => return Err(error),
        }
    }
    Err(Errno::ETIMEDOUT)
}

fn reply_packet(
    mut packet: SchemePacket,
    value: Option<i64>,
    frame: Option<&[u8]>,
) -> [u8; PACKET_SIZE] {
    packet.status = 0;
    packet.attach_count = 0;
    packet.data.fill(0);
    packet.buffer_len = 8;
    packet.response_len = REPLY_LEN as u64;
    match value {
        Some(value) => {
            let mut encoded = [0u8; 16];
            let length = LinuxReplyV1::new(value).encode(&mut encoded).unwrap_or(0);
            packet.data[..length].copy_from_slice(&encoded[..length]);
            packet.data_len = length as u32;
        }
        None => packet.data_len = 0,
    }
    match frame {
        Some(frame) => {
            if let Some(path) = packet.path.get_mut(..frame.len()) {
                path.copy_from_slice(frame);
            }
        }
        None => packet.path.fill(0),
    }
    let mut out = [0u8; PACKET_SIZE];
    let _ = protocol::encode(&packet, &mut out);
    out
}

#[inline(always)]
fn raw_number(number: usize, a0: usize, a1: usize) -> isize {
    let result: isize;
    // SAFETY: the caller supplies a valid RavenBSD syscall number and user arguments; the kernel
    // validates every handle, pointer and range.
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") number => result,
            in("rdi") a0,
            in("rsi") a1,
            in("rdx") 0usize,
            in("r10") 0usize,
            in("r8") 0usize,
            in("r9") 0usize,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack),
        );
    }
    result
}

#[inline(always)]
fn raw_number3(number: usize, a0: usize, a1: usize, a2: usize) -> isize {
    let result: isize;
    // SAFETY: as in `raw_number`; PROC_SET_PERSONALITY takes three scalar words.
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") number => result,
            in("rdi") a0,
            in("rsi") a1,
            in("rdx") a2,
            in("r10") 0usize,
            in("r8") 0usize,
            in("r9") 0usize,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack),
        );
    }
    result
}
