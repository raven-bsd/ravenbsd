// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S731 end-to-end create semantics.
//!
//! Starts `initfsd`, `ramfsd` and `vfsd`, then creates objects through `file:` with explicit
//! modes and process masks. Every row checks the real inode mode and owner reported by the
//! serving filesystem, not a PASS label: the requested mode reaches the server, the kernel mask
//! is applied at creation, the creator owns the inode, an `O_CREAT` on an existing file changes
//! nothing, and `mkdir` uses the same authenticated path.

#![no_std]
#![no_main]
#![allow(unsafe_code)] // Raw credential syscalls; every block states the kernel-validated invariant.

extern crate alloc;

use alloc::format;
use alloc::string::String;

use core::arch::asm;

use raven_abi::errno::Errno;
use raven_abi::types::Handle;
use raven_initfs::Initfs;
use raven_libc_sys::sys;
use raven_proto_fs::{O_CREAT, O_RDWR, OpenFlags, S_IFMT, Stat};
use raven_rt::entry;
use raven_rt::fs;
use raven_rt::println;
use raven_rt::spawn::{SpawnOptions, spawn};
use raven_scheme::Client;

entry!(main);

const PROC_SETCRED_EX: usize = 0x3B;

#[cfg(target_arch = "x86_64")]
#[inline(always)]
unsafe fn syscall6(number: usize, a0: usize, a1: usize, a2: usize, a3: usize, a4: usize, a5: usize) -> isize {
    let result: isize;
    // SAFETY: scalar syscall arguments; the kernel validates pointers and values.
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
unsafe fn syscall6(number: usize, a0: usize, a1: usize, a2: usize, a3: usize, a4: usize, a5: usize) -> isize {
    let result: isize;
    // SAFETY: scalar syscall arguments; the kernel validates pointers and values.
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

fn setcred(which: usize, value: u32) -> Result<(), Errno> {
    let raw = unsafe { syscall6(PROC_SETCRED_EX, which, value as usize, 0, 0, 0, 0) };
    Errno::from_ret(raw).map(|_| ())
}

fn setgroups(groups: &[u32]) -> Result<(), Errno> {
    let raw = unsafe {
        syscall6(
            PROC_SETCRED_EX,
            9,
            0,
            0,
            0,
            groups.as_ptr() as usize,
            groups.len(),
        )
    };
    Errno::from_ret(raw).map(|_| ())
}

fn main(_args: raven_rt::Args) -> i32 {
    match run() {
        Ok(()) => {
            println!("create-umask: ok");
            0
        }
        Err(message) => {
            println!("create-umask: fail {message}");
            1
        }
    }
}

fn run() -> Result<(), String> {
    start_services()?;
    let base = "/tmp/create";
    match fs::mkdir(base, 0o755) {
        Ok(()) | Err(Errno::EEXIST) => {}
        Err(error) => return Err(format!("mkdir base {error:?}")),
    }
    fs::chmod(base, 0o777).map_err(|error| format!("chmod base {error:?}"))?;

    set_umask(0o027)?;
    let first = create(&format!("file:{base}/mask027"), 0o666)?;
    check(&first, 0o640, 0, 0, "mask027")?;
    println!("create-umask: mask027 mode=0640 owner=0:0");

    set_umask(0o077)?;
    let second = create(&format!("file:{base}/mask077"), 0o666)?;
    check(&second, 0o600, 0, 0, "mask077")?;
    println!("create-umask: mask077 mode=0600 owner=0:0");

    set_umask(0)?;
    let requested = create(&format!("file:{base}/requested"), 0o600)?;
    check(&requested, 0o600, 0, 0, "requested mode")?;
    println!("create-umask: requested mode=0600 owner=0:0");

    set_umask(0)?;
    let keep = create(&format!("file:{base}/keep"), 0o640)?;
    check(&keep, 0o640, 0, 0, "keep create")?;
    let again = create(&format!("file:{base}/keep"), 0o777)?;
    check(&again, 0o640, 0, 0, "existing unchanged")?;
    println!("create-umask: existing unchanged mode=0640 owner=0:0");

    setgroups(&[1000]).map_err(|error| format!("setgroups {error:?}"))?;
    setcred(5, 1000).map_err(|error| format!("setgid 1000 {error:?}"))?;
    setcred(1, 1000).map_err(|error| format!("setuid 1000 {error:?}"))?;
    set_umask(0o027)?;
    let unprivileged = create(&format!("file:{base}/by-1000"), 0o666)?;
    check(&unprivileged, 0o640, 1000, 1000, "uid1000 create")?;
    println!("create-umask: uid1000 mode=0640 owner=1000:1000");

    let directory = format!("{base}/dir");
    fs::mkdir(&directory, 0o777).map_err(|error| format!("mkdir dir {error:?}"))?;
    let stat = fs::stat(&format!("file:{directory}")).map_err(|error| format!("stat dir {error:?}"))?;
    if stat.mode & S_IFMT == 0 {
        return Err(String::from("mkdir has no directory type"));
    }
    if stat.mode & 0o7777 != 0o750 {
        return Err(format!("mkdir mode {:04o}, expected 0750", stat.mode & 0o7777));
    }
    if (stat.uid, stat.gid) != (1000, 1000) {
        return Err(format!("mkdir owner {}:{}", stat.uid, stat.gid));
    }
    println!("create-umask: mkdir mode=0750 owner=1000:1000");
    Ok(())
}

fn start_services() -> Result<(), String> {
    let image = raven_rt::map_initfs().map_err(|_| String::from("initfs map"))?;
    let initfs = Initfs::parse(image).map_err(|_| String::from("initfs parse"))?;
    for path in ["/sbin/initfsd", "/sbin/ramfsd", "/sbin/vfsd"] {
        spawn(
            &initfs,
            path,
            &SpawnOptions {
                args: &[],
                env: &[],
            },
        )
        .map_err(|_| format!("spawn {path}"))?;
        let scheme = match path {
            "/sbin/initfsd" => "initfs:",
            "/sbin/ramfsd" => "ramfs:",
            _ => "file:",
        };
        wait_scheme(scheme)?;
    }
    println!("create-umask: services ok");
    Ok(())
}

fn wait_scheme(scheme: &str) -> Result<(), String> {
    for _ in 0..3000 {
        if sys::open(scheme.as_bytes(), u32::from(OpenFlags::READ.to_wire()), 0).is_ok() {
            return Ok(());
        }
        let _ = raven_rt::nanosleep(10_000_000);
    }
    Err(format!("scheme {scheme} timeout"))
}

fn set_umask(mask: u32) -> Result<(), String> {
    sys::umask(mask)
        .map(|_| ())
        .map_err(|error| format!("umask {mask:04o}: {error:?}"))
}

fn create(path: &str, mode: u32) -> Result<Handle, String> {
    let wire = OpenFlags::from_linux(O_RDWR | O_CREAT)
        .map_err(|_| String::from("open flags"))?
        .to_wire();
    sys::open(path.as_bytes(), u32::from(wire), mode)
        .map_err(|error| format!("create {path}: {error:?}"))
}

fn check(handle: &Handle, mode: u32, uid: u32, gid: u32, label: &str) -> Result<(), String> {
    let client = Client::new();
    let stat: Stat = client
        .fstat(*handle)
        .map_err(|error| format!("fstat {label}: {error:?}"))?;
    if stat.mode & 0o7777 != mode {
        return Err(format!(
            "{label}: mode {:04o}, expected {mode:04o}",
            stat.mode & 0o7777
        ));
    }
    if (stat.uid, stat.gid) != (uid, gid) {
        return Err(format!(
            "{label}: owner {}:{}, expected {uid}:{gid}",
            stat.uid, stat.gid
        ));
    }
    client
        .close(*handle)
        .map_err(|error| format!("close {label}: {error:?}"))
}
