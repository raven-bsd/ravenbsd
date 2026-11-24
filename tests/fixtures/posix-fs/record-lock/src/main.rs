// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! P5c/P5c2 record-lock qtest parent.
//!
//! Starts the core file services, then coordinates one child process per case: conflict,
//! `F_GETLK`, blocking, close-release and exit-release. The child reports through `/tmp/result`.
//! The raw cases use the `RECORD_LOCK` syscall on a `file:` descriptor; the libc cases call
//! `fcntl(2)` on a libc descriptor over the same kernel lock table.

#![no_std]
#![no_main]

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};

use core::ffi::{c_char, c_int};

use raven_abi::errno::Errno;
use raven_abi::types::Handle;
use raven_abi::types::fcntl::{GETLK, RDLCK, RecordLock, SETLK, UNLCK, WRLCK};
use raven_initfs::Initfs;
use raven_libc_abi::fcntl::{F_GETLK, F_RDLCK, F_SETLK, F_SETLKW, F_UNLCK, F_WRLCK, O_RDWR};
use raven_libc_abi::structs::Flock;
use raven_proto_fs::{O_CREAT, O_RDONLY, O_RDWR as RAW_RDWR};
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
const LIBC_PATH: &[u8] = b"/tmp/record-lock/lockfile\0";
const LIBC_START: i64 = 20;
const LIBC_LEN: i64 = 10;
const PROC_HANDLE_INFO: usize = 0x3E;
const PROC_SIGNAL: usize = 0x4A;
const SIGKILL: usize = 9;

unsafe extern "C" {
    fn open(path: *const c_char, flags: c_int, mode: c_int) -> c_int;
    fn close(fd: c_int) -> c_int;
    fn fcntl(fd: c_int, command: c_int, lock: *mut Flock) -> c_int;
    fn __errno_location() -> *mut c_int;
}

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
    close_any(&initfs, &client, &handle)?;
    close_release(&initfs, &client, &handle)?;

    // The close-release phase closed the original descriptor.
    let handle = client
        .open("file:/tmp/record-lock/lockfile", READ_WRITE)
        .map_err(|error| format!("reopen lockfile: {}", error.description()))?;
    exit_release(&initfs, &handle)?;
    read_read(&initfs, &handle)?;
    disjoint(&initfs, &handle)?;
    getlk_pid(&initfs, &handle)?;
    kill_release(&initfs, &handle)?;
    libc_getlk_free()?;
    libc_getlk_conflict(&initfs)?;
    libc_setlk_conflict(&initfs)?;
    libc_conflict(&initfs)?;
    libc_blocking(&initfs)?;
    libc_close_release(&initfs)?;
    libc_exit_release(&initfs)?;
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
    let mut file = fs::open(&scheme(LOCKFILE), RAW_RDWR | O_CREAT)
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

/// Close a second descriptor for the same file; POSIX releases the process's locks on it.
fn close_any(initfs: &Initfs<'_>, client: &Client, handle: &Handle) -> Result<(), String> {
    set(handle, WRLCK, 0, 10)?;
    let second = client
        .open("file:/tmp/record-lock/lockfile", READ_WRITE)
        .map_err(|error| format!("open second descriptor: {}", error.description()))?;
    if *handle == second {
        return Err("second descriptor reused the locked handle".to_string());
    }
    clear(RESULT);
    clear(PHASE);
    let child = spawn_child(initfs, "try 0 10")?;
    client
        .close(second)
        .map_err(|error| format!("close second descriptor: {}", error.description()))?;
    write_text(PHASE, "go")?;
    let text = wait_for(RESULT, "granted")?;
    wait_child(child)?;
    if text.trim() != "granted" {
        return Err(format!("close-any child reported {text}"));
    }
    println!("record-lock: close-any ok");
    Ok(())
}

/// SIGKILL is an owner death with no `EXIT` syscall: the lock must not survive it.
fn kill_release(initfs: &Initfs<'_>, handle: &Handle) -> Result<(), String> {
    clear(RESULT);
    let child = spawn_child(initfs, "hold 0 10")?;
    let pid = handle_pid(child)?;
    wait_for(RESULT, "locked")?;
    signal(pid, SIGKILL)?;
    let code = wait_child_code(child)?;
    if code != 128 + SIGKILL as i32 {
        return Err(format!("SIGKILL child exited {code}"));
    }
    let after = get(handle, WRLCK, 0, 10)?;
    if after.l_type != UNLCK {
        return Err(format!(
            "stale lock after SIGKILL: type={} pid={}",
            after.l_type, after.l_pid
        ));
    }
    println!("record-lock: kill ok");
    Ok(())
}

fn read_read(initfs: &Initfs<'_>, handle: &Handle) -> Result<(), String> {
    set(handle, RDLCK, 0, 10)?;
    clear(RESULT);
    let child = spawn_child(initfs, "readpair 0 10")?;
    let text = wait_for(RESULT, "readpair ")?;
    wait_child(child)?;
    unlock(handle, 0, 10)?;
    if text.trim() != "readpair granted conflict" {
        return Err(format!("read/read child reported `{text}`"));
    }
    println!("record-lock: read-read ok");
    Ok(())
}

fn disjoint(initfs: &Initfs<'_>, handle: &Handle) -> Result<(), String> {
    set(handle, WRLCK, 0, 10)?;
    clear(RESULT);
    let disjoint_child = spawn_child(initfs, "conflict 20 10")?;
    let disjoint_text = wait_for(RESULT, "granted")?;
    wait_child(disjoint_child)?;
    clear(RESULT);
    let overlap_child = spawn_child(initfs, "conflict 5 10")?;
    let overlap_text = wait_for(RESULT, "conflict")?;
    wait_child(overlap_child)?;
    unlock(handle, 0, 10)?;
    if disjoint_text.trim() != "granted" || overlap_text.trim() != "conflict" {
        return Err(format!(
            "disjoint child reported `{}` / `{}`",
            disjoint_text, overlap_text
        ));
    }
    println!("record-lock: disjoint ok");
    Ok(())
}

fn getlk_pid(initfs: &Initfs<'_>, handle: &Handle) -> Result<(), String> {
    clear(RESULT);
    let child = spawn_child(initfs, "hold 0 10")?;
    let pid = handle_pid(child)?;
    wait_for(RESULT, "locked")?;
    let held = get(handle, WRLCK, 0, 10)?;
    if held.l_type != WRLCK || held.l_pid != pid as i32 {
        return Err(format!(
            "getlk pid mismatch: type={} pid={} expected={pid}",
            held.l_type, held.l_pid
        ));
    }
    wait_child(child)?;
    let after = get(handle, WRLCK, 0, 10)?;
    if after.l_type != UNLCK {
        return Err(format!(
            "stale lock after exit: type={} pid={}",
            after.l_type, after.l_pid
        ));
    }
    println!("record-lock: getlk-pid ok");
    Ok(())
}

fn libc_getlk_free() -> Result<(), String> {
    let fd = libc_open()?;
    let result = libc_fcntl(fd, F_GETLK, F_WRLCK as i16, LIBC_START, LIBC_LEN);
    libc_close(fd)?;
    let lock = result.map_err(|error| format!("libc getlk: {}", error.description()))?;
    if lock.l_type != F_UNLCK as i16 {
        return Err(format!("libc free range reported type {}", lock.l_type));
    }
    println!("record-lock: libc-getlk-free ok");
    Ok(())
}

fn libc_getlk_conflict(initfs: &Initfs<'_>) -> Result<(), String> {
    clear(RESULT);
    let child = spawn_child(initfs, &hold_mode())?;
    wait_for(RESULT, "locked")?;
    let fd = libc_open()?;
    let result = libc_fcntl(fd, F_GETLK, F_RDLCK as i16, LIBC_START, LIBC_LEN);
    libc_close(fd)?;
    wait_child(child)?;
    let lock = result.map_err(|error| format!("libc getlk: {}", error.description()))?;
    if lock.l_type != F_WRLCK as i16
        || lock.l_start != LIBC_START
        || lock.l_len != LIBC_LEN
        || lock.l_pid == 0
    {
        return Err(format!(
            "libc getlk reported type={} start={} len={} pid={}",
            lock.l_type, lock.l_start, lock.l_len, lock.l_pid
        ));
    }
    println!("record-lock: libc-getlk-conflict ok");
    Ok(())
}

fn libc_setlk_conflict(initfs: &Initfs<'_>) -> Result<(), String> {
    clear(RESULT);
    let child = spawn_child(initfs, &hold_mode())?;
    wait_for(RESULT, "locked")?;
    let fd = libc_open()?;
    let result = libc_fcntl(fd, F_SETLK, F_WRLCK as i16, LIBC_START, LIBC_LEN);
    libc_close(fd)?;
    wait_child(child)?;
    match result {
        Err(Errno::EAGAIN) => {}
        Err(error) => return Err(format!("libc setlk: {}", error.description())),
        Ok(_) => return Err("libc setlk granted while the child held the range".to_string()),
    }
    println!("record-lock: libc-setlk-conflict ok");
    Ok(())
}

fn libc_conflict(initfs: &Initfs<'_>) -> Result<(), String> {
    let fd = libc_open()?;
    libc_fcntl(fd, F_SETLK, F_WRLCK as i16, LIBC_START, LIBC_LEN)
        .map_err(|error| format!("libc setlk: {}", error.description()))?;
    clear(RESULT);
    let child = spawn_child(initfs, &format!("conflict {LIBC_START} {LIBC_LEN}"))?;
    let text = wait_for(RESULT, "conflict")?;
    wait_child(child)?;
    let _ = libc_fcntl(fd, F_SETLK, F_UNLCK as i16, LIBC_START, LIBC_LEN);
    libc_close(fd)?;
    if text.trim() != "conflict" {
        return Err(format!("libc conflict child reported {text}"));
    }
    println!("record-lock: libc-conflict ok");
    Ok(())
}

fn libc_blocking(initfs: &Initfs<'_>) -> Result<(), String> {
    clear(RESULT);
    let child = spawn_child(initfs, &hold_mode())?;
    wait_for(RESULT, "locked")?;
    let fd = libc_open()?;
    libc_fcntl(fd, F_SETLKW, F_WRLCK as i16, LIBC_START, LIBC_LEN)
        .map_err(|error| format!("libc setlkw: {}", error.description()))?;
    wait_child(child)?;
    let _ = libc_fcntl(fd, F_SETLK, F_UNLCK as i16, LIBC_START, LIBC_LEN);
    libc_close(fd)?;
    println!("record-lock: libc-setlkw ok");
    Ok(())
}

fn libc_close_release(initfs: &Initfs<'_>) -> Result<(), String> {
    let fd = libc_open()?;
    libc_fcntl(fd, F_SETLK, F_WRLCK as i16, LIBC_START, LIBC_LEN)
        .map_err(|error| format!("libc setlk: {}", error.description()))?;
    clear(RESULT);
    clear(PHASE);
    let child = spawn_child(initfs, &format!("try {LIBC_START} {LIBC_LEN}"))?;
    libc_close(fd)?;
    write_text(PHASE, "go")?;
    let text = wait_for(RESULT, "granted")?;
    wait_child(child)?;
    if text.trim() != "granted" {
        return Err(format!("libc close-release child reported {text}"));
    }
    println!("record-lock: libc-close ok");
    Ok(())
}

fn libc_exit_release(initfs: &Initfs<'_>) -> Result<(), String> {
    clear(RESULT);
    let child = spawn_child(initfs, &format!("libc-exit {LIBC_START} {LIBC_LEN}"))?;
    wait_for(RESULT, "locked")?;
    wait_child(child)?;
    let fd = libc_open()?;
    let result = libc_fcntl(fd, F_SETLK, F_WRLCK as i16, LIBC_START, LIBC_LEN);
    libc_close(fd)?;
    result.map_err(|error| format!("libc exit release: {}", error.description()))?;
    println!("record-lock: libc-exit ok");
    Ok(())
}

fn hold_mode() -> String {
    format!("hold {LIBC_START} {LIBC_LEN}")
}

fn libc_open() -> Result<c_int, String> {
    // SAFETY: LIBC_PATH is NUL terminated and the libc open contract takes three arguments.
    let fd = unsafe { open(LIBC_PATH.as_ptr().cast(), O_RDWR, 0) };
    if fd < 0 {
        return Err(format!("libc open: errno {}", libc_errno()));
    }
    Ok(fd)
}

fn libc_close(fd: c_int) -> Result<(), String> {
    // SAFETY: fd came from the libc open above.
    let result = unsafe { close(fd) };
    if result < 0 {
        return Err(format!("libc close: errno {}", libc_errno()));
    }
    Ok(())
}

fn libc_errno() -> i32 {
    // SAFETY: libc returns the calling thread's errno slot.
    unsafe { *__errno_location() }
}

fn libc_fcntl(fd: c_int, command: i32, kind: i16, start: i64, len: i64) -> Result<Flock, Errno> {
    let mut lock = Flock {
        l_type: kind,
        l_whence: 0,
        l_start: start,
        l_len: len,
        l_pid: 0,
    };
    // SAFETY: the pointer is a live struct flock for the duration of the call.
    let result = unsafe { fcntl(fd, command, &mut lock) };
    if result < 0 {
        let code = u16::try_from(libc_errno()).unwrap_or(Errno::EIO.0);
        return Err(Errno(code));
    }
    Ok(lock)
}

fn spawn_child(initfs: &Initfs<'_>, mode: &str) -> Result<Handle, String> {
    let mut arguments = alloc::vec!["record-lock-child"];
    arguments.extend(mode.split_whitespace());
    spawn(
        initfs,
        "/sbin/record-lock-child",
        &SpawnOptions {
            args: &arguments,
            env: &[],
        },
    )
    .map_err(|_| format!("spawn child {mode}"))
}

fn wait_child(handle: Handle) -> Result<(), String> {
    let code = wait_child_code(handle)?;
    if code != 0 {
        return Err(format!("child exited {code}"));
    }
    Ok(())
}

fn wait_child_code(handle: Handle) -> Result<i32, String> {
    let mut code = 0i32;
    RtSys
        .proc_wait(handle, &mut code)
        .map_err(|error| format!("wait child: {}", error.description()))?;
    Ok(code)
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

fn get(handle: &Handle, kind: i16, start: i64, len: i64) -> Result<RecordLock, String> {
    let mut value = lock(kind, start, len);
    record_lock(handle, GETLK, &mut value)
        .map_err(|error| format!("get lock: {}", error.description()))?;
    Ok(value)
}

fn handle_pid(handle: Handle) -> Result<u32, String> {
    // SAFETY: PROC_HANDLE_INFO takes a process handle and returns its pid.
    let result = unsafe { raven_rt::sys::syscall2(PROC_HANDLE_INFO, handle.0 as usize, 0) };
    let pid =
        Errno::from_ret(result).map_err(|error| format!("handle info: {}", error.description()))?;
    u32::try_from(pid).map_err(|_| "handle pid too large".to_string())
}

fn signal(pid: u32, signo: usize) -> Result<(), String> {
    // SAFETY: PROC_SIGNAL takes the target pid and a signal number.
    let result = unsafe { raven_rt::sys::syscall2(PROC_SIGNAL, pid as usize, signo) };
    Errno::from_ret(result)
        .map(|_| ())
        .map_err(|error| format!("signal {pid}: {}", error.description()))
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
