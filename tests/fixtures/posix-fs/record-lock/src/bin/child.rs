// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! P5c/P5c2 record-lock qtest child: one lock operation per invocation.
//!
//! The `conflict`, `getlk`, `block`, `hold`, `try` and `exit` modes use the raw `RECORD_LOCK`
//! syscall; `libc-exit` opens and locks through libc `fcntl(2)` and leaves the lock to the
//! process-exit release path.

#![no_std]
#![no_main]

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};

use core::ffi::{c_char, c_int};

use raven_abi::errno::Errno;
use raven_abi::types::Handle;
use raven_abi::types::fcntl::{GETLK, RDLCK, RecordLock, SETLK, SETLKW, WRLCK};
use raven_libc_abi::fcntl::{F_SETLK, F_WRLCK, O_RDWR};
use raven_libc_abi::structs::Flock;
use raven_proto_fs::{O_CREAT, O_RDWR as RAW_RDWR};
use raven_rt::entry;
use raven_rt::fs;
use raven_rt::println;
use raven_scheme::Client;

entry!(main);

const RESULT: &str = "/tmp/record-lock/result";
const PHASE: &str = "/tmp/record-lock/phase";
const LIBC_PATH: &[u8] = b"/tmp/record-lock/lockfile\0";

unsafe extern "C" {
    fn open(path: *const c_char, flags: c_int, mode: c_int) -> c_int;
    fn fcntl(fd: c_int, command: c_int, lock: *mut Flock) -> c_int;
    fn __errno_location() -> *mut c_int;
}

fn main(args: raven_rt::Args) -> i32 {
    let mode = args.get(1).unwrap_or("none");
    let start = args
        .get(2)
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(0);
    let len = args
        .get(3)
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(10);
    match run(mode, start, len) {
        Ok(()) => 0,
        Err(message) => {
            println!("record-lock-child: fail {message}");
            1
        }
    }
}

fn run(mode: &str, start: i64, len: i64) -> Result<(), String> {
    if mode == "libc-exit" {
        return libc_exit(start, len);
    }
    let client = Client::new();
    let handle = client
        .open("file:/tmp/record-lock/lockfile", 3)
        .map_err(|error| format!("open lockfile: {}", error.description()))?;
    match mode {
        "conflict" => {
            let mut lock = lock(WRLCK, start, len);
            match record_lock(&handle, SETLK, &mut lock) {
                Err(Errno::EAGAIN) => write_text(RESULT, "conflict")?,
                Ok(()) => write_text(RESULT, "granted")?,
                Err(error) => write_text(RESULT, &format!("error {}", error.0))?,
            }
        }
        "readpair" => {
            // The parent holds a read lock: another read is compatible, a write is not.
            let mut read = lock(RDLCK, start, len);
            let read_result = match record_lock(&handle, SETLK, &mut read) {
                Ok(()) => "granted",
                Err(Errno::EAGAIN) => "conflict",
                Err(error) => return Err(format!("readpair read: {}", error.0)),
            };
            let mut write = lock(WRLCK, start, len);
            let write_result = match record_lock(&handle, SETLK, &mut write) {
                Ok(()) => "granted",
                Err(Errno::EAGAIN) => "conflict",
                Err(error) => return Err(format!("readpair write: {}", error.0)),
            };
            write_text(RESULT, &format!("readpair {read_result} {write_result}"))?;
        }
        "getlk" => {
            let mut lock = lock(WRLCK, start, len);
            record_lock(&handle, GETLK, &mut lock)
                .map_err(|error| format!("getlk: {}", error.description()))?;
            write_text(
                RESULT,
                &format!(
                    "getlk {} {} {} {}",
                    lock.l_type, lock.l_start, lock.l_len, lock.l_pid
                ),
            )?;
        }
        "block" => {
            write_text(RESULT, "waiting")?;
            let mut lock = lock(WRLCK, start, len);
            record_lock(&handle, SETLKW, &mut lock)
                .map_err(|error| format!("setlkw: {}", error.description()))?;
            write_text(RESULT, "acquired")?;
        }
        "hold" => {
            let mut lock = lock(WRLCK, start, len);
            record_lock(&handle, SETLK, &mut lock)
                .map_err(|error| format!("setlk: {}", error.description()))?;
            write_text(RESULT, "locked")?;
            let _ = raven_rt::nanosleep(500_000_000);
        }
        "try" => {
            wait_for(PHASE, "go")?;
            let mut lock = lock(WRLCK, start, len);
            match record_lock(&handle, SETLK, &mut lock) {
                Ok(()) => write_text(RESULT, "granted")?,
                Err(error) => write_text(RESULT, &format!("busy {}", error.0))?,
            }
        }
        "exit" => {
            let mut lock = lock(WRLCK, start, len);
            record_lock(&handle, SETLK, &mut lock)
                .map_err(|error| format!("setlk: {}", error.description()))?;
            write_text(RESULT, "locked")?;
            // Return without closing: EXIT must release the held lock.
            return Ok(());
        }
        other => return Err(format!("unknown mode {other}")),
    }
    let _ = client.close(handle);
    Ok(())
}

fn libc_exit(start: i64, len: i64) -> Result<(), String> {
    // SAFETY: LIBC_PATH is NUL terminated and the libc open contract takes three arguments.
    let fd = unsafe { open(LIBC_PATH.as_ptr().cast(), O_RDWR, 0) };
    if fd < 0 {
        return Err(format!("libc open: errno {}", libc_errno()));
    }
    let mut lock = Flock {
        l_type: F_WRLCK as i16,
        l_whence: 0,
        l_start: start,
        l_len: len,
        l_pid: 0,
    };
    // SAFETY: the pointer is a live struct flock for the duration of the call.
    let result = unsafe { fcntl(fd, F_SETLK, &mut lock) };
    if result < 0 {
        return Err(format!("libc setlk: errno {}", libc_errno()));
    }
    write_text(RESULT, "locked")?;
    // Return without closing: EXIT must release the libc-held lock.
    Ok(())
}

fn libc_errno() -> i32 {
    // SAFETY: libc returns the calling thread's errno slot.
    unsafe { *__errno_location() }
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

fn wait_for(path: &str, prefix: &str) -> Result<(), String> {
    for _ in 0..1000 {
        if let Ok(data) = fs::read_to_vec(&scheme(path)) {
            let text = String::from_utf8_lossy(&data).to_string();
            if text.starts_with(prefix) {
                return Ok(());
            }
        }
        let _ = raven_rt::nanosleep(10_000_000);
    }
    Err(format!("{path} did not report {prefix}"))
}

fn write_text(path: &str, text: &str) -> Result<(), String> {
    let mut file = fs::open(&scheme(path), RAW_RDWR | O_CREAT)
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
