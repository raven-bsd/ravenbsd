// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! P5c raw `RECORD_LOCK` qtest parent.
//!
//! Starts the core file services, then coordinates one child process per case: conflict,
//! `F_GETLK`, blocking, close-release and exit-release. The child reports through `/tmp/result`
//! and the parent uses the raw `RECORD_LOCK` syscall on a `file:` descriptor.

#![no_std]
#![no_main]

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};

use raven_abi::errno::Errno;
use raven_abi::types::Handle;
use raven_abi::types::fcntl::{GETLK, RecordLock, SETLK, UNLCK, WRLCK};
use raven_initfs::Initfs;
use raven_proto_fs::{O_CREAT, O_RDONLY, O_RDWR};
use raven_rt::entry;
use raven_rt::fs;
use raven_rt::println;
use raven_rt::spawn::{RtSys, SpawnOptions, spawn};
use raven_scheme::Client;

entry!(main);

const DIRECTORY: &str = "/tmp/record-lock";
const LOCKFILE: &str = "/tmp/record-lock/lockfile";
const RESULT: &str = "/tmp/record-lock/result";
const PHASE: &str = "/tmp/record-lock/phase";
const READ_WRITE: u32 = 3;

fn main(_args: raven_rt::Args) -> i32 {
    match run() {
        Ok(()) => {
            println!("record-lock: ok");
            0
        }
        Err(message) => {
            println!("record-lock: fail {message}");
            1
        }
    }
}

fn run() -> Result<(), String> {
    let initfs = start_services()?;
    setup()?;

    let client = Client::new();
    let handle = client
        .open("file:/tmp/record-lock/lockfile", READ_WRITE)
        .map_err(|error| format!("open lockfile: {}", error.description()))?;

    getlk_free(&handle)?;
    conflict(&initfs, &handle)?;
    getlk_conflict(&initfs)?;
    blocking(&initfs, &handle)?;
    close_release(&initfs, &client, &handle)?;

    // The close-release phase closed the original descriptor.
    let handle = client
        .open("file:/tmp/record-lock/lockfile", READ_WRITE)
        .map_err(|error| format!("reopen lockfile: {}", error.description()))?;
    exit_release(&initfs, &handle)?;
    let _ = client.close(handle);
    Ok(())
}

fn start_services() -> Result<Initfs<'static>, String> {
    let image = raven_rt::map_initfs().map_err(|_| "initfs map".to_string())?;
    let initfs = Initfs::parse(image).map_err(|_| "initfs parse".to_string())?;
    for (path, scheme) in [
        ("/sbin/initfsd", "initfs:"),
        ("/sbin/ramfsd", "ramfs:"),
        ("/sbin/vfsd", "file:"),
    ] {
        spawn(
            &initfs,
            path,
            &SpawnOptions {
                args: &[],
                env: &[],
            },
        )
        .map_err(|_| format!("spawn {path}"))?;
        wait_scheme(scheme)?;
    }
    println!("record-lock: services ok");
    Ok(initfs)
}

fn wait_scheme(scheme: &str) -> Result<(), String> {
    for _ in 0..3000 {
        if fs::open(scheme, O_RDONLY).is_ok() {
            return Ok(());
        }
        let _ = raven_rt::nanosleep(10_000_000);
    }
    Err(format!("scheme {scheme} timeout"))
}

fn setup() -> Result<(), String> {
    match fs::mkdir(DIRECTORY, 0o755) {
        Ok(()) | Err(Errno::EEXIST) => {}
        Err(error) => return Err(format!("mkdir root: {}", error.description())),
    }
    let mut file = fs::open(&scheme(LOCKFILE), O_RDWR | O_CREAT)
        .map_err(|error| format!("create lockfile: {}", error.description()))?;
    file.write(&[0u8; 100])
        .map_err(|error| format!("write lockfile: {}", error.description()))?;
    file.close()
        .map_err(|error| format!("close lockfile: {}", error.description()))?;
    clear(RESULT);
    clear(PHASE);
    Ok(())
}

fn getlk_free(handle: &Handle) -> Result<(), String> {
    let mut lock = lock(WRLCK, 0, 10);
    record_lock(handle, GETLK, &mut lock)
        .map_err(|error| format!("getlk: {}", error.description()))?;
    if lock.l_type != UNLCK {
        return Err(format!("free range reported type {}", lock.l_type));
    }
    println!("record-lock: getlk-free ok");
    Ok(())
}

fn conflict(initfs: &Initfs<'_>, handle: &Handle) -> Result<(), String> {
    set(handle, WRLCK, 0, 10)?;
    clear(RESULT);
    let child = spawn_child(initfs, "conflict")?;
    let text = wait_for(RESULT, "conflict")?;
    wait_child(child)?;
    if text.trim() != "conflict" {
        return Err(format!("conflict child reported {text}"));
    }
    println!("record-lock: conflict ok");
    Ok(())
}

fn getlk_conflict(initfs: &Initfs<'_>) -> Result<(), String> {
    clear(RESULT);
    let child = spawn_child(initfs, "getlk")?;
    let text = wait_for(RESULT, "getlk ")?;
    wait_child(child)?;
    let fields = text.split_whitespace().collect::<alloc::vec::Vec<_>>();
    if fields.len() != 5 {
        return Err(format!("getlk child reported `{text}`"));
    }
    let kind = fields[1]
        .parse::<i16>()
        .map_err(|_| "getlk type".to_string())?;
    let start = fields[2]
        .parse::<i64>()
        .map_err(|_| "getlk start".to_string())?;
    let len = fields[3]
        .parse::<i64>()
        .map_err(|_| "getlk len".to_string())?;
    let pid = fields[4]
        .parse::<i32>()
        .map_err(|_| "getlk pid".to_string())?;
    if kind != WRLCK || start != 0 || len != 10 || pid == 0 {
        return Err(format!(
            "getlk reported type={kind} start={start} len={len} pid={pid}"
        ));
    }
    println!("record-lock: getlk-conflict ok");
    Ok(())
}

fn blocking(initfs: &Initfs<'_>, handle: &Handle) -> Result<(), String> {
    clear(RESULT);
    let child = spawn_child(initfs, "block")?;
    wait_for(RESULT, "waiting")?;
    unlock(handle, 0, 10)?;
    wait_for(RESULT, "acquired")?;
    wait_child(child)?;
    println!("record-lock: block ok");
    Ok(())
}

fn close_release(initfs: &Initfs<'_>, client: &Client, handle: &Handle) -> Result<(), String> {
    set(handle, WRLCK, 0, 10)?;
    clear(RESULT);
    clear(PHASE);
    let child = spawn_child(initfs, "try")?;
    client
        .close(*handle)
        .map_err(|error| format!("close lockfile: {}", error.description()))?;
    write_text(PHASE, "go")?;
    let text = wait_for(RESULT, "granted")?;
    wait_child(child)?;
    if text.trim() != "granted" {
        return Err(format!("close-release child reported {text}"));
    }
    println!("record-lock: close ok");
    Ok(())
}

fn exit_release(initfs: &Initfs<'_>, handle: &Handle) -> Result<(), String> {
    clear(RESULT);
    let child = spawn_child(initfs, "exit")?;
    wait_for(RESULT, "locked")?;
    wait_child(child)?;
    set(handle, WRLCK, 0, 10)?;
    unlock(handle, 0, 10)?;
    println!("record-lock: exit ok");
    Ok(())
}

fn spawn_child(initfs: &Initfs<'_>, mode: &str) -> Result<Handle, String> {
    spawn(
        initfs,
        "/sbin/record-lock-child",
        &SpawnOptions {
            args: &["record-lock-child", mode],
            env: &[],
        },
    )
    .map_err(|_| format!("spawn child {mode}"))
}

fn wait_child(handle: Handle) -> Result<(), String> {
    let mut code = 0i32;
    RtSys
        .proc_wait(handle, &mut code)
        .map_err(|error| format!("wait child: {}", error.description()))?;
    if code != 0 {
        return Err(format!("child exited {code}"));
    }
    Ok(())
}

fn lock(kind: i16, start: i64, len: i64) -> RecordLock {
    RecordLock {
        l_type: kind,
        l_whence: 0,
        l_start: start,
        l_len: len,
        l_pid: 0,
    }
}

fn record_lock(handle: &Handle, cmd: u32, lock: &mut RecordLock) -> Result<(), Errno> {
    // SAFETY: the kernel validates the 32-byte stack record range before copying to/from it.
    let result = unsafe {
        raven_rt::sys::syscall3(
            raven_abi::syscall::RECORD_LOCK,
            handle.0 as usize,
            cmd as usize,
            lock as *mut RecordLock as usize,
        )
    };
    Errno::from_ret(result).map(|_| ())
}

fn set(handle: &Handle, kind: i16, start: i64, len: i64) -> Result<(), String> {
    let mut value = lock(kind, start, len);
    record_lock(handle, SETLK, &mut value)
        .map_err(|error| format!("set lock: {}", error.description()))
}

fn unlock(handle: &Handle, start: i64, len: i64) -> Result<(), String> {
    set(handle, UNLCK, start, len)
}

fn wait_for(path: &str, prefix: &str) -> Result<String, String> {
    for _ in 0..1000 {
        if let Ok(data) = fs::read_to_vec(&scheme(path)) {
            let text = String::from_utf8_lossy(&data).to_string();
            if text.starts_with(prefix) {
                return Ok(text);
            }
        }
        let _ = raven_rt::nanosleep(10_000_000);
    }
    Err(format!("{path} did not report {prefix}"))
}

fn clear(path: &str) {
    let _ = fs::unlink(path);
}

fn write_text(path: &str, text: &str) -> Result<(), String> {
    let mut file = fs::open(&scheme(path), O_RDWR | O_CREAT)
        .map_err(|error| format!("open {path}: {}", error.description()))?;
    file.write(text.as_bytes())
        .map_err(|error| format!("write {path}: {}", error.description()))?;
    file.close()
        .map_err(|error| format!("close {path}: {}", error.description()))
}

fn scheme(path: &str) -> String {
    format!("file:{path}")
}

#[cfg(not(target_os = "none"))]
fn main() {}
