// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! P5c raw `RECORD_LOCK` qtest child: one lock operation per invocation.

#![no_std]
#![no_main]

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};

use raven_abi::errno::Errno;
use raven_abi::types::Handle;
use raven_abi::types::fcntl::{GETLK, RecordLock, SETLK, SETLKW, WRLCK};
use raven_proto_fs::{O_CREAT, O_RDWR};
use raven_rt::entry;
use raven_rt::fs;
use raven_rt::println;
use raven_scheme::Client;

entry!(main);

const RESULT: &str = "/tmp/record-lock/result";
const PHASE: &str = "/tmp/record-lock/phase";

fn main(args: raven_rt::Args) -> i32 {
    let mode = args.get(1).unwrap_or("none");
    match run(mode) {
        Ok(()) => 0,
        Err(message) => {
            println!("record-lock-child: fail {message}");
            1
        }
    }
}

fn run(mode: &str) -> Result<(), String> {
    let client = Client::new();
    let handle = client
        .open("file:/tmp/record-lock/lockfile", 3)
        .map_err(|error| format!("open lockfile: {}", error.description()))?;
    match mode {
        "conflict" => {
            let mut lock = lock(WRLCK, 0, 10);
            match record_lock(&handle, SETLK, &mut lock) {
                Err(Errno::EAGAIN) => write_text(RESULT, "conflict")?,
                Ok(()) => write_text(RESULT, "granted")?,
                Err(error) => write_text(RESULT, &format!("error {}", error.0))?,
            }
        }
        "getlk" => {
            let mut lock = lock(WRLCK, 0, 10);
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
            let mut lock = lock(WRLCK, 0, 10);
            record_lock(&handle, SETLKW, &mut lock)
                .map_err(|error| format!("setlkw: {}", error.description()))?;
            write_text(RESULT, "acquired")?;
        }
        "try" => {
            wait_for(PHASE, "go")?;
            let mut lock = lock(WRLCK, 0, 10);
            match record_lock(&handle, SETLK, &mut lock) {
                Ok(()) => write_text(RESULT, "granted")?,
                Err(error) => write_text(RESULT, &format!("busy {}", error.0))?,
            }
        }
        "exit" => {
            let mut lock = lock(WRLCK, 0, 10);
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
