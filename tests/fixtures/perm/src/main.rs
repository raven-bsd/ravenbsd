// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! U3a permission-matrix fixture.
//!
//! Runs as root, creates the matrix under `/tmp`, checks the root bypass/execute rules, drops to
//! uid/gid 1000, and checks the non-root read, directory, execute and sticky-delete rules through
//! `file:` (vfsd) and directly through `ramfs:`. The kernel attaches the process credentials to
//! every scheme request, so this exercises the real server path rather than a fake context.

#![no_std]
#![no_main]

extern crate alloc;

use alloc::format;
use core::arch::asm;

use raven_abi::errno::Errno;
use raven_initfs::Initfs;
use raven_proto_fs::{ACCESS_EXEC, ACCESS_READ, O_CREAT, O_RDONLY, O_TRUNC, O_WRONLY};
use raven_rt::entry;
use raven_rt::fs;
use raven_rt::println;
use raven_rt::spawn::{SpawnOptions, spawn};

entry!(main);

const PROC_SETCRED_EX: usize = 0x3B;

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

fn fail(step: &str) -> i32 {
    println!("perm: fail {step}");
    1
}

fn create(path: &str, data: &[u8], mode: u32) -> Result<(), Errno> {
    let open_path = format!("file:{path}");
    let mut file = fs::open(&open_path, O_WRONLY | O_CREAT | O_TRUNC)?;
    let written = file.write(data)?;
    if written != data.len() {
        return Err(Errno::EIO);
    }
    file.close()?;
    fs::chmod(path, mode)
}

fn mkdir(path: &str, mode: u32) -> Result<(), Errno> {
    match fs::mkdir(path, mode) {
        Ok(()) | Err(Errno::EEXIST) => Ok(()),
        Err(error) => Err(error),
    }
}

fn main(_args: raven_rt::Args) -> i32 {
    if let Err(step) = start_services() {
        return fail(step);
    }
    if let Err(step) = root_setup() {
        return fail(step);
    }
    if let Err(step) = root_checks() {
        return fail(step);
    }
    if let Err(step) = drop_credentials() {
        return fail(step);
    }
    if let Err(step) = user_checks() {
        return fail(step);
    }
    println!("perm: ok");
    0
}

fn start_services() -> Result<(), &'static str> {
    let image = raven_rt::map_initfs().map_err(|_| "initfs map")?;
    let initfs = Initfs::parse(image).map_err(|_| "initfs parse")?;
    for path in ["/sbin/initfsd", "/sbin/ramfsd", "/sbin/vfsd"] {
        spawn(
            &initfs,
            path,
            &SpawnOptions {
                args: &[],
                env: &[],
            },
        )
        .map_err(|_| "spawn service")?;
        let scheme = match path {
            "/sbin/initfsd" => "initfs:",
            "/sbin/ramfsd" => "ramfs:",
            _ => "file:",
        };
        wait_scheme(scheme)?;
    }
    println!("perm: services ok");
    Ok(())
}

fn wait_scheme(scheme: &str) -> Result<(), &'static str> {
    let mut attempt = 0usize;
    while attempt < 3000 {
        if fs::open(scheme, O_RDONLY).is_ok() {
            return Ok(());
        }
        let _ = raven_rt::nanosleep(10_000_000);
        attempt = attempt.saturating_add(1);
    }
    Err("scheme timeout")
}

fn root_setup() -> Result<(), &'static str> {
    mkdir("/tmp/perm", 0o755).map_err(|_| "mkdir perm")?;
    mkdir("/tmp/perm/dir755", 0o755).map_err(|_| "mkdir dir755")?;
    mkdir("/tmp/perm/dir700", 0o700).map_err(|_| "mkdir dir700")?;
    mkdir("/tmp/sticky", 0o1777).map_err(|_| "mkdir sticky")?;
    create("/tmp/perm/root600", b"root-secret", 0o600).map_err(|_| "create root600")?;
    create("/tmp/perm/world644", b"world-readable", 0o644).map_err(|_| "create world644")?;
    create("/tmp/perm/exec644", b"#!/bin/sh\n", 0o644).map_err(|_| "create exec644")?;
    create("/tmp/sticky/rootfile", b"sticky", 0o644).map_err(|_| "create sticky file")?;
    Ok(())
}

fn root_checks() -> Result<(), &'static str> {
    if fs::read_to_vec("file:/tmp/perm/root600").is_err() {
        return Err("root 0600 read");
    }
    if fs::access("/tmp/perm/exec644", ACCESS_EXEC as u32).is_ok() {
        return Err("root 0644 exec");
    }
    if fs::access("/tmp/perm/dir755", ACCESS_EXEC as u32).is_err() {
        return Err("root 0755 exec");
    }
    println!("perm: root read=ok exec=denied");
    Ok(())
}

fn drop_credentials() -> Result<(), &'static str> {
    setcred(5, 1000).map_err(|_| "setgid 1000")?;
    setcred(1, 1000).map_err(|_| "setuid 1000")?;
    Ok(())
}

fn user_checks() -> Result<(), &'static str> {
    if fs::read_to_vec("file:/tmp/perm/root600").is_ok() {
        return Err("uid1000 0600 read");
    }
    if fs::read_to_vec("file:/tmp/perm/world644").as_deref() != Ok(b"world-readable") {
        return Err("uid1000 0644 read");
    }
    println!("perm: uid1000 0600=denied 0644=ok");

    if fs::read_dir("file:/tmp/perm/dir755").is_err() {
        return Err("uid1000 dir755");
    }
    if fs::read_dir("file:/tmp/perm/dir700").is_ok() {
        return Err("uid1000 dir700");
    }
    println!("perm: uid1000 dirs 0755=ok 0700=denied");

    if fs::access("/tmp/perm/exec644", ACCESS_EXEC as u32).is_ok() {
        return Err("uid1000 0644 exec");
    }
    if fs::access("/tmp/perm/exec644", ACCESS_READ as u32).is_err() {
        return Err("uid1000 0644 read access");
    }
    println!("perm: uid1000 exec 0644=denied");

    if fs::unlink("/tmp/sticky/rootfile") != Err(Errno::EPERM) {
        return Err("uid1000 sticky unlink");
    }
    println!("perm: uid1000 sticky=denied");

    if fs::read_to_vec("ramfs:/perm/root600").is_ok() {
        return Err("uid1000 ramfs 0600");
    }
    if fs::read_to_vec("ramfs:/perm/world644").as_deref() != Ok(b"world-readable") {
        return Err("uid1000 ramfs 0644");
    }
    if fs::read_dir("ramfs:/perm/dir700").is_ok() {
        return Err("uid1000 ramfs dir700");
    }
    println!("perm: ramfs 0600=denied 0644=ok dir700=denied");
    Ok(())
}
