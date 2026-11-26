// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K537 devfs-node fixture: symlink follow/no-follow, dangling, device mode/owner, traversal,
//! unlink-while-open and node-name validation through `file:` -> vfsd -> ramfsd.
//!
//! The fixture starts the core services, builds a small device tree under `/tmp/devfs`, then
//! drops to uid 1000 for the mode-0600 denial row. Every printed line is a hard assertion.

#![no_std]
#![no_main]

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use raven_abi::errno::Errno;
use raven_initfs::Initfs;
use raven_proto_fs::{
    Call, CallRequest, CallResponse, O_CREAT, O_NOFOLLOW, O_RDONLY, O_WRONLY, S_IFCHR, S_IFMT,
};
use raven_rt::entry;
use raven_rt::fs;
use raven_rt::println;
use raven_rt::proc::{setresgid, setresuid};
use raven_rt::spawn::{SpawnOptions, spawn};
use raven_scheme::Client;

entry!(main);

const START: &str = "/tmp/devfs";
const READ_ONLY: u32 = 1;

fn main(_args: raven_rt::Args) -> i32 {
    match run() {
        Ok(()) => {
            println!("devfs: ok");
            0
        }
        Err(message) => {
            println!("devfs: fail {message}");
            1
        }
    }
}

fn run() -> Result<(), String> {
    start_services()?;
    setup()?;
    nofollow()?;
    follow()?;
    dangling()?;
    mode_owner()?;
    traversal()?;
    unlink_open()?;
    names()?;
    nonroot()?;
    Ok(())
}

fn start_services() -> Result<(), String> {
    let image = raven_rt::map_initfs().map_err(|_| "initfs map".to_string())?;
    let initfs = Initfs::parse(image).map_err(|_| "initfs parse".to_string())?;
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
    println!("devfs: services ok");
    Ok(())
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
    match fs::mkdir(START, 0o755) {
        Ok(()) | Err(Errno::EEXIST) => {}
        Err(error) => return Err(format!("mkdir start: {}", error.description())),
    }
    write_file(&format!("{START}/target"), b"live")?;
    fs::symlink(&format!("{START}/link"), "target")
        .map_err(|error| format!("link: {}", error.description()))?;
    fs::symlink(&format!("{START}/linknull"), "null")
        .map_err(|error| format!("linknull: {}", error.description()))?;
    fs::symlink(&format!("{START}/dangling"), "missing")
        .map_err(|error| format!("dangling: {}", error.description()))?;
    call_status(Call::Mkdir {
        mode: 0o755,
        path: b"dir",
    })?;
    call_status(Call::Mknod {
        mode: S_IFCHR | 0o777,
        dev: 0,
        path: b"null",
    })?;
    call_status(Call::Mknod {
        mode: S_IFCHR | 0o600,
        dev: 0,
        path: b"secure",
    })?;
    Ok(())
}

fn nofollow() -> Result<(), String> {
    match file_open(&format!("{START}/linknull"), O_RDONLY | O_NOFOLLOW) {
        Err(Errno::ELOOP) => {
            println!("devfs: nofollow ok");
            Ok(())
        }
        Err(error) => Err(format!("no-follow error {}", error.description())),
        Ok(_) => Err("no-follow followed the link".to_string()),
    }
}

fn follow() -> Result<(), String> {
    let data = file_read(&format!("{START}/link"))
        .map_err(|error| format!("follow read: {}", error.description()))?;
    if data != b"live" {
        return Err(format!("follow read {:?}", data));
    }
    println!("devfs: follow ok");
    Ok(())
}

fn dangling() -> Result<(), String> {
    if file_stat(&format!("{START}/dangling")) != Err(Errno::ENOENT) {
        return Err("dangling link resolved".to_string());
    }
    println!("devfs: dangling ok");
    Ok(())
}

fn mode_owner() -> Result<(), String> {
    let stat = file_stat(&format!("{START}/secure"))
        .map_err(|error| format!("secure stat: {}", error.description()))?;
    if stat.mode & S_IFMT != S_IFCHR {
        return Err(format!("secure type {:o}", stat.mode));
    }
    if stat.mode & 0o777 != 0o600 {
        return Err(format!("secure mode {:o}", stat.mode & 0o777));
    }
    if stat.uid != 0 || stat.gid != 0 {
        return Err(format!("secure owner {}:{}", stat.uid, stat.gid));
    }
    println!("devfs: mode-owner ok");
    Ok(())
}

fn traversal() -> Result<(), String> {
    match file_open(&format!("{START}/null/x"), O_RDONLY) {
        Err(Errno::ENOTDIR) | Err(Errno::ENOENT) => {
            println!("devfs: traversal ok");
            Ok(())
        }
        Err(error) => Err(format!("traversal error {}", error.description())),
        Ok(_) => Err("traversal through a node succeeded".to_string()),
    }
}

fn unlink_open() -> Result<(), String> {
    let mut file = file_open(&format!("{START}/target"), O_RDONLY)
        .map_err(|error| format!("unlink open: {}", error.description()))?;
    call_status(Call::Unlink { path: b"target" })?;
    let data = file
        .read_to_vec()
        .map_err(|error| format!("unlinked read: {}", error.description()))?;
    if data != b"live" {
        return Err(format!("unlinked fd read {:?}", data));
    }
    let entries =
        file_read_dir(START).map_err(|error| format!("readdir: {}", error.description()))?;
    if entries.iter().any(|entry| entry.name == "target") {
        return Err("unlinked target still listed".to_string());
    }
    file.close()
        .map_err(|error| format!("unlinked close: {}", error.description()))?;
    println!("devfs: unlink-open ok");
    Ok(())
}

fn names() -> Result<(), String> {
    match call_raw(Call::Mknod {
        mode: S_IFCHR | 0o600,
        dev: 0,
        path: b"bad/name",
    })? {
        Err(Errno::ENOENT) | Err(Errno::EINVAL) => {}
        other => return Err(format!("slash name {other:?}")),
    }
    match call_raw(Call::Mknod {
        mode: S_IFCHR | 0o600,
        dev: 0,
        path: b"bad\0name",
    })? {
        Err(Errno::EINVAL) => {}
        other => return Err(format!("NUL name {other:?}")),
    }
    // A slash is the hierarchy separator; it cannot address a single node name here.
    if call_raw(Call::Mknod {
        mode: S_IFCHR | 0o600,
        dev: 0,
        path: b"null/child",
    })? != Err(Errno::ENOTDIR)
    {
        return Err("slash name was accepted as a node".to_string());
    }
    println!("devfs: names ok");
    Ok(())
}

fn nonroot() -> Result<(), String> {
    setresgid(1000).map_err(|error| format!("setresgid: {}", error.description()))?;
    setresuid(1000).map_err(|error| format!("setresuid: {}", error.description()))?;
    match file_open(&format!("{START}/secure"), O_RDONLY) {
        Err(Errno::EACCES) => {
            println!("devfs: nonroot ok");
            Ok(())
        }
        Err(error) => Err(format!("nonroot error {}", error.description())),
        Ok(_) => Err("non-root opened a mode-0600 device".to_string()),
    }
}

fn write_file(path: &str, data: &[u8]) -> Result<(), String> {
    let mut file = file_open(path, O_WRONLY | O_CREAT)
        .map_err(|error| format!("open {path}: {}", error.description()))?;
    let written = file
        .write(data)
        .map_err(|error| format!("write {path}: {}", error.description()))?;
    if written != data.len() {
        return Err(format!("short write {path}"));
    }
    file.close()
        .map_err(|error| format!("close {path}: {}", error.description()))
}

fn file_open(path: &str, flags: u32) -> Result<fs::File, Errno> {
    fs::open(&format!("file:{path}"), flags)
}

fn file_stat(path: &str) -> Result<raven_proto_fs::Stat, Errno> {
    fs::stat(&format!("file:{path}"))
}

fn file_read(path: &str) -> Result<Vec<u8>, Errno> {
    fs::read_to_vec(&format!("file:{path}"))
}

fn file_read_dir(path: &str) -> Result<Vec<fs::DirEntry>, Errno> {
    fs::read_dir(&format!("file:{path}"))
}

fn call_status(call: Call<'_>) -> Result<(), String> {
    match call_raw(call)? {
        Ok(()) => Ok(()),
        Err(error) => Err(format!("status {}", error.description())),
    }
}

/// Run one typed call on the devfs directory; `Ok(())` when the server status is zero.
fn call_raw(call: Call<'_>) -> Result<Result<(), Errno>, String> {
    let client = Client::new();
    let dir = client
        .open(&format!("file:{START}"), READ_ONLY)
        .map_err(|error| format!("dir open: {}", error.description()))?;
    let request = CallRequest::new(call);
    let mut response = [0u8; 256];
    let result = client.call_typed(dir, &request, &mut response);
    let _ = client.close(dir);
    let length = match result {
        Ok(length) => length,
        Err(error) => return Ok(Err(error)),
    };
    match CallResponse::decode(request.op(), &response[..length])
        .map_err(|_| "response decode".to_string())?
    {
        CallResponse::Status(0) => Ok(Ok(())),
        CallResponse::Status(status) => Ok(Err(Errno(
            u16::try_from(status).map_err(|_| "status".to_string())?,
        ))),
        other => Err(format!("unexpected response {other:?}")),
    }
}
