// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S37 controller: run the client scenarios, then the client-death/transfer case.
//!
//! The death client is a one-shot scheme server that returns an unlinked open as an `Open`
//! `REPLY_HANDLE` attachment and exits. This controller loads the death reader, transfers the
//! attached reference into it, and starts it. The reader's close is the last reference.

#![no_std]
#![no_main]

extern crate alloc;

use alloc::format;
use alloc::string::String;

use raven_abi::errno::Errno;
use raven_abi::layout::x86_64::{USER_END, USER_MIN};
use raven_abi::types::{Handle, Rights, map, prot};
use raven_elf::{Elf, LoadMode, Machine, StepKind, plan_load};
use raven_initfs::Initfs;
use raven_proto_fs::OpenFlags;
use raven_rt::entry;
use raven_rt::println;
use raven_rt::spawn::{SpawnOptions, spawn};
use raven_s37_fs::{
    DEATH_STACK_BASE, DEATH_STACK_SIZE, PAGE, call, close, handle_close, handle_transfer, map,
    map_initfs, open, proc_create, proc_wait, thread_create,
};

entry!(main);

const SERVER: &str = "/server";
const CLIENT: &str = "/client";
const DEATH_CLIENT: &str = "/death-client";
const DEATH_READER: &str = "/death-reader";
const RETRIES: usize = 2000;
const DEATH_RETRIES: usize = 200;

fn open_retry(path: &str) -> Result<Handle, Errno> {
    let mut attempt = 0usize;
    while attempt < RETRIES {
        match open(path, OpenFlags::READ_WRITE.to_wire()) {
            Ok(handle) => return Ok(handle),
            Err(Errno::ENOENT) => {
                let _ = raven_s37_fs::nanosleep(1_000_000);
                attempt = attempt.saturating_add(1);
            }
            Err(error) => return Err(error),
        }
    }
    Err(Errno::ETIMEDOUT)
}

fn spawn_child(initfs: &Initfs<'_>, path: &str, args: &[&str]) -> Result<Handle, Errno> {
    let options = SpawnOptions { args, env: &[] };
    spawn(initfs, path, &options).map_err(|_| Errno::EIO)
}

fn wait_ok(child: Handle) -> bool {
    matches!(proc_wait(child), Ok(0))
}

fn query(control: Handle) -> Option<String> {
    let mut response = [0u8; 64];
    let length = call(control, b"count", &mut response).ok()?;
    let text = core::str::from_utf8(response.get(..length)?).ok()?;
    Some(format!("{text}"))
}

fn field(text: &str, name: &str) -> Option<usize> {
    let rest = text.split(name).nth(1)?;
    rest.split_whitespace().next()?.parse().ok()
}

fn wait_released(control: Handle) -> Option<String> {
    let mut attempt = 0usize;
    while attempt < DEATH_RETRIES {
        let text = query(control)?;
        if field(&text, "released=").unwrap_or(0) >= 1 {
            return Some(text);
        }
        let _ = raven_s37_fs::nanosleep(50_000_000);
        attempt = attempt.saturating_add(1);
    }
    None
}

fn load_reader(initfs: &Initfs<'_>) -> Result<(Handle, u64), &'static str> {
    let entry = initfs.get(DEATH_READER).ok_or("reader missing")?;
    let elf = Elf::parse(entry.data, Machine::X86_64, USER_MIN..USER_END)
        .map_err(|_| "reader ELF invalid")?;
    let child = proc_create().map_err(|_| "reader proc_create failed")?;
    for step in plan_load(&elf, PAGE as u64, LoadMode::Strict).map_err(|_| "reader plan failed")? {
        let flags = map::pack(step.prot as usize, map::FIXED);
        let vaddr = usize::try_from(step.vaddr).map_err(|_| "reader vaddr")?;
        let len = usize::try_from(step.len).map_err(|_| "reader len")?;
        let loaded = match step.kind {
            StepKind::File { offset } => {
                let offset = entry
                    .data_offset
                    .checked_add(offset)
                    .ok_or("reader offset")?;
                let offset = usize::try_from(offset).map_err(|_| "reader offset usize")?;
                map(child, Handle::INITFS, offset, vaddr, len, flags)
            }
            StepKind::Zero => map(child, Handle::NONE, 0, vaddr, len, flags),
            StepKind::CopyTail { .. } => Err(Errno::EINVAL),
        };
        loaded.map_err(|_| "reader segment map failed")?;
    }
    map(
        child,
        Handle::NONE,
        0,
        DEATH_STACK_BASE,
        DEATH_STACK_SIZE,
        map::pack(prot::READ | prot::WRITE, map::FIXED),
    )
    .map_err(|_| "reader stack map failed")?;
    Ok((child, elf.entry()))
}

fn main(_args: raven_rt::Args) -> i32 {
    let image = match map_initfs(Handle::INITFS) {
        Ok(image) => image,
        Err(_) => {
            println!("s37: initfs map failed");
            return 1;
        }
    };
    let initfs = match Initfs::parse(image) {
        Ok(initfs) => initfs,
        Err(_) => {
            println!("s37: initfs parse failed");
            return 1;
        }
    };

    let _server = match spawn_child(&initfs, SERVER, &[]) {
        Ok(handle) => handle,
        Err(_) => {
            println!("s37: server spawn failed");
            return 1;
        }
    };
    let control = match open_retry("fs:ctl") {
        Ok(handle) => handle,
        Err(_) => {
            println!("s37: server did not register");
            return 1;
        }
    };
    let client = match spawn_child(&initfs, CLIENT, &[]) {
        Ok(handle) => handle,
        Err(_) => {
            println!("s37: client spawn failed");
            return 1;
        }
    };
    if !wait_ok(client) {
        println!("s37: client failed");
        return 1;
    }
    match query(control) {
        Some(text) => println!("s37: client count {text}"),
        None => {
            println!("s37: client count failed");
            return 1;
        }
    }

    let death = match spawn_child(&initfs, DEATH_CLIENT, &[]) {
        Ok(handle) => handle,
        Err(_) => {
            println!("s37: death client spawn failed");
            return 1;
        }
    };
    let attached = match open_retry("death:file") {
        Ok(handle) => handle,
        Err(_) => {
            println!("s37: death attachment failed");
            return 1;
        }
    };
    if !wait_ok(death) {
        println!("s37: death client failed");
        return 1;
    }
    let (reader, reader_entry) = match load_reader(&initfs) {
        Ok(child) => child,
        Err(message) => {
            println!("s37: {message}");
            return 1;
        }
    };
    let transferred = match handle_transfer(
        reader,
        attached,
        Rights::READ.bits() | Rights::DUP.bits() | Rights::TRANSFER.bits(),
    ) {
        Ok(handle) => handle,
        Err(_) => {
            println!("s37: death transfer failed");
            return 1;
        }
    };
    let thread = match thread_create(
        reader,
        usize::try_from(reader_entry).unwrap_or(usize::MAX),
        DEATH_STACK_BASE.saturating_add(DEATH_STACK_SIZE),
        transferred.0 as usize,
    ) {
        Ok(thread) => thread,
        Err(_) => {
            println!("s37: death reader thread failed");
            return 1;
        }
    };
    let _ = handle_close(thread);
    let _ = close(attached);
    if !wait_ok(reader) {
        println!("s37: death reader failed");
        return 1;
    }
    let text = match wait_released(control) {
        Some(text) => text,
        None => {
            println!("s37: death reader did not release");
            return 1;
        }
    };
    if field(&text, "opens=") != Some(0) {
        println!("s37: death count {text}");
        return 1;
    }
    println!("s37: death count {text}");
    println!("s37: death ok");

    let _ = close(control);
    println!("s37: lifecycle ok");
    0
}
