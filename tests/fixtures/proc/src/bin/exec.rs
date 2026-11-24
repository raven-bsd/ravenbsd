// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! P2k exec fixture: parser failure, kernel-side failure, then a real image replacement.

#![no_std]
#![no_main]

extern crate alloc;

#[path = "../common.rs"]
mod common;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use raven_abi::errno::Errno;
use raven_rt::entry;
use raven_rt::exec::{ExecRequest, exec_image, exec_raw};
use raven_rt::pipe::{O_CLOEXEC, pipe, pipe2};
use raven_rt::println;

use common::*;

entry!(main);

fn run() -> Result<(), Errno> {
    let info = self_info()?;
    let (plain_read, plain_write) = pipe()?;
    let (close_read, close_write) = pipe2(O_CLOEXEC)?;
    let (listed_read, listed_write) = pipe()?;
    let env: Vec<String> = [
        ("PROC_EXEC_PID", info[0]),
        ("PROC_EXEC_PGID", info[2]),
        ("PROC_EXEC_SID", info[3]),
        ("PROC_EXEC_UID", info[4]),
        ("PROC_EXEC_EUID", info[5]),
        ("PROC_EXEC_GID", info[6]),
        ("PROC_EXEC_EGID", info[7]),
        ("PROC_EXEC_PLAIN_R", u64::from(plain_read.0)),
        ("PROC_EXEC_PLAIN_W", u64::from(plain_write.0)),
        ("PROC_EXEC_CLOSE_R", u64::from(close_read.0)),
        ("PROC_EXEC_CLOSE_W", u64::from(close_write.0)),
        ("PROC_EXEC_LISTED_R", u64::from(listed_read.0)),
        ("PROC_EXEC_LISTED_W", u64::from(listed_write.0)),
    ]
    .iter()
    .map(|(key, value)| format!("{key}={value}"))
    .collect();
    let env_refs: Vec<&[u8]> = env.iter().map(String::as_bytes).collect();

    // The parser rejects this before the kernel sees a valid image; the old image must continue.
    let bad_image = exec_image(b"not an elf", b"/bad", &[b"bad"], &[], &[]) == Err(Errno::ENOEXEC);
    // The kernel rejects an oversized request before touching the live address space.
    let dummy = [0u8; 1];
    // The libc/raw kernel bound is 16 MiB; the rt convenience wrapper keeps its own smaller
    // self-limit, so the raw path is checked against the kernel bound here.
    const KERNEL_MAX_EXEC_IMAGE: u64 = 16 * 1024 * 1024;
    let oversized = ExecRequest {
        image: dummy.as_ptr() as u64,
        image_len: KERNEL_MAX_EXEC_IMAGE.saturating_add(1),
        path: 0,
        path_len: 0,
        argv: 0,
        argc: 0,
        envp: 0,
        envc: 0,
        close_list: 0,
        close_count: 0,
        flags: 0,
    };
    let bad_kernel = exec_raw(&oversized) == Err(Errno::EFBIG);
    println!(
        "proc-exec: bad-image={} bad-kernel={}",
        u8::from(bad_image),
        u8::from(bad_kernel)
    );
    if !(bad_image && bad_kernel) {
        println!("proc-exec: FAIL preflight");
        return Err(Errno::EINVAL);
    }

    let archive = raven_rt::map_initfs()?;
    let initfs = raven_initfs::Initfs::parse(archive).map_err(|_| Errno::ENOEXEC)?;
    let child = initfs.get("/exec-child").ok_or(Errno::ENOENT)?;
    println!("proc-exec: exec-start");
    exec_image(
        child.data,
        b"/exec-child",
        &[b"/exec-child", b"alpha", b"beta"],
        &env_refs,
        &[listed_read, listed_write],
    )?;
    // A successful exec never returns.
    println!("proc-exec: FAIL exec returned");
    Err(Errno::EINVAL)
}

fn main(_args: raven_rt::Args) -> i32 {
    match run() {
        Ok(()) => 0,
        Err(error) => {
            println!("proc-exec: error {error:?}");
            1
        }
    }
}
