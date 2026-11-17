// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! P2k exec child: checks argv/env, preserved identity and CLOEXEC handle closure.

#![no_std]
#![no_main]

#[path = "../common.rs"]
mod common;

use raven_abi::errno::Errno;
use raven_abi::types::Handle;
use raven_rt::entry;
use raven_rt::println;
use raven_rt::sys::{read, write};

use common::*;

entry!(main);

fn env_u64(args: &raven_rt::Args, key: &str) -> Option<u64> {
    args.env_get(key)?.parse().ok()
}

fn run(args: raven_rt::Args) -> Result<(), Errno> {
    let argv = args.get(0) == Some("/exec-child")
        && args.get(1) == Some("alpha")
        && args.get(2) == Some("beta");
    let info = self_info()?;
    let identity = env_u64(&args, "PROC_EXEC_PID") == Some(info[0])
        && env_u64(&args, "PROC_EXEC_PGID") == Some(info[2])
        && env_u64(&args, "PROC_EXEC_SID") == Some(info[3])
        && env_u64(&args, "PROC_EXEC_UID") == Some(info[4])
        && env_u64(&args, "PROC_EXEC_EUID") == Some(info[5])
        && env_u64(&args, "PROC_EXEC_GID") == Some(info[6])
        && env_u64(&args, "PROC_EXEC_EGID") == Some(info[7]);

    let plain_read = Handle(env_u64(&args, "PROC_EXEC_PLAIN_R").ok_or(Errno::EINVAL)? as u32);
    let plain_write = Handle(env_u64(&args, "PROC_EXEC_PLAIN_W").ok_or(Errno::EINVAL)? as u32);
    let close_read = Handle(env_u64(&args, "PROC_EXEC_CLOSE_R").ok_or(Errno::EINVAL)? as u32);
    let close_write = Handle(env_u64(&args, "PROC_EXEC_CLOSE_W").ok_or(Errno::EINVAL)? as u32);
    let listed_read = Handle(env_u64(&args, "PROC_EXEC_LISTED_R").ok_or(Errno::EINVAL)? as u32);
    let listed_write = Handle(env_u64(&args, "PROC_EXEC_LISTED_W").ok_or(Errno::EINVAL)? as u32);

    let mut byte = [0u8; 1];
    let plain = write(plain_write, b"K") == Ok(1)
        && read(plain_read, &mut byte) == Ok(1)
        && byte[0] == b'K';
    let cloexec = read(close_read, &mut byte) == Err(Errno::EBADF)
        && write(close_write, b"x") == Err(Errno::EBADF)
        && read(listed_read, &mut byte) == Err(Errno::EBADF)
        && write(listed_write, b"x") == Err(Errno::EBADF);

    println!("proc-exec: child argv={}", u8::from(argv));
    println!("proc-exec: child identity={}", u8::from(identity));
    println!("proc-exec: child handles={}", u8::from(plain && cloexec));
    if !(argv && identity && plain && cloexec) {
        println!("proc-exec: child FAIL");
        return Err(Errno::EINVAL);
    }
    println!("proc-exec: child ok");
    Ok(())
}

fn main(args: raven_rt::Args) -> i32 {
    match run(args) {
        Ok(()) => 0,
        Err(error) => {
            println!("proc-exec: child error {error:?}");
            1
        }
    }
}
