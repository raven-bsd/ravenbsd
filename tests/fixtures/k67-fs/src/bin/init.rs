// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K67 conformance controller: starts the servers and runs the lifetime loops.

#![no_std]
#![no_main]

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};

use raven_abi::errno::Errno;
use raven_abi::types::{Handle, Rights};
use raven_initfs::Initfs;
use raven_proto_fs::Call;
use raven_rt::entry;
use raven_rt::println;
use raven_rt::spawn::{RtSys, SpawnOptions, spawn};
use raven_scheme::Client;

use raven_k67_fs::{READ, WRITE, call_status, directory, read_all, statfs, write_all};

entry!(main);

const OPEN_RETRIES: usize = 500;
const CYCLES: usize = 10_000;
const BUDGET: usize = 256;

fn main(_args: raven_rt::Args) -> i32 {
    match run() {
        Ok(()) => {
            println!("k67: done");
            0
        }
        Err(message) => {
            raven_rt::eprintln!("k67: {message}");
            1
        }
    }
}

fn run() -> Result<(), String> {
    let image = raven_rt::map_initfs().map_err(|error| format!("initfs: {error:?}"))?;
    let initfs = Initfs::parse(image).map_err(|_| "initfs parse".to_string())?;
    spawn_server(&initfs, "/sbin/initfsd", &["INITFS_HANDLE=1"])?;
    wait_scheme("initfs:")?;
    spawn_server(&initfs, "/sbin/ramfsd", &[])?;
    wait_scheme("ramfs:")?;
    spawn_server(&initfs, "/sbin/vfsd", &[])?;
    wait_scheme("file:")?;
    let client = Client::new();

    write_all(&client, "ramfs:cycle", b"x")?;
    let baseline = statfs(&client, "ramfs:")
        .map_err(|error| format!("baseline statfs: {error}"))?
        .free_files;
    println!("k67: baseline {baseline}");

    for _ in 0..CYCLES {
        let handle = client
            .open("ramfs:cycle", READ)
            .map_err(|error| format!("cycle open: {error:?}"))?;
        let duplicate = client
            .dup(handle, Rights::ALL.bits())
            .map_err(|error| format!("cycle dup: {error:?}"))?;
        let _ = client.close(duplicate);
        let _ = client.close(handle);
    }
    let after_cycles = statfs(&client, "ramfs:")
        .map_err(|error| format!("cycle statfs: {error}"))?
        .free_files;
    println!("k67: cycles free {after_cycles}");
    if after_cycles != baseline {
        return Err("cycle inode count drifted".to_string());
    }
    println!("k67: cycles ok");

    let child = spawn_server(&initfs, "/client", &[])?;
    let mut code = -1;
    RtSys
        .proc_wait(child, &mut code)
        .map_err(|error| format!("client wait: {error:?}"))?;
    let _ = raven_rt::handle_close(child);
    if code != 0 {
        return Err(format!("abandoned client exited {code}"));
    }
    let mut after = 0;
    let mut settled = false;
    for _ in 0..200 {
        match statfs(&client, "ramfs:") {
            Ok(statfs) => {
                after = statfs.free_files;
                if after == baseline {
                    settled = true;
                    break;
                }
            }
            Err(message) if message.contains("too many open files") => {}
            Err(message) => return Err(format!("abandoned statfs: {message}")),
        }
        let _ = raven_rt::nanosleep(10_000_000);
    }
    if !settled {
        return Err(format!(
            "abandoned opens did not return to baseline: {after} != {baseline}"
        ));
    }
    println!("k67: abandoned free {after}");
    println!("k67: abandoned ok");

    let mut handles = [Handle::NONE; BUDGET];
    for (index, handle) in handles.iter_mut().enumerate() {
        *handle = client
            .open("ramfs:cycle", READ)
            .map_err(|error| format!("budget open {index}: {error:?}"))?;
    }
    match client.open("ramfs:cycle", READ) {
        Err(Errno::EMFILE) => {}
        _ => return Err("budget did not return EMFILE".to_string()),
    }
    for handle in handles {
        let _ = client.close(handle);
    }
    let after_budget = statfs(&client, "ramfs:")
        .map_err(|error| format!("budget statfs: {error}"))?
        .free_files;
    println!("k67: budget free {after_budget}");
    if after_budget != baseline {
        return Err("budget handles leaked".to_string());
    }
    println!("k67: budget ok");

    unlink_lifetime(&client)?;
    println!("k67: unlink ok");
    dup_cursor(&client)?;
    println!("k67: dup ok");
    readdir_invalidation(&client)?;
    println!("k67: readdir ok");
    vfsd_round_trip(&client)?;
    println!("k67: vfsd ok");
    Ok(())
}

fn spawn_server(initfs: &Initfs<'_>, path: &str, env: &[&str]) -> Result<Handle, String> {
    spawn(initfs, path, &SpawnOptions { args: &[], env }).map_err(|_| format!("spawn {path}"))
}

fn wait_scheme(scheme: &str) -> Result<(), String> {
    let client = Client::new();
    for _ in 0..OPEN_RETRIES {
        match client.open(scheme, READ) {
            Ok(handle) => {
                let _ = client.close(handle);
                return Ok(());
            }
            Err(Errno::ENOENT) => {
                let _ = raven_rt::nanosleep(10_000_000);
            }
            Err(error) => return Err(format!("scheme {scheme}: {error:?}")),
        }
    }
    Err(format!("scheme {scheme} timed out"))
}

fn unlink_lifetime(client: &Client) -> Result<(), String> {
    write_all(client, "ramfs:u", b"data")?;
    let first = client
        .open("ramfs:u", READ | WRITE)
        .map_err(|error| format!("unlink open 1: {error:?}"))?;
    let second = client
        .open("ramfs:u", READ)
        .map_err(|error| format!("unlink open 2: {error:?}"))?;
    let root = client
        .open("ramfs:", READ)
        .map_err(|error| format!("unlink root: {error:?}"))?;
    call_status(client, root, Call::Unlink { path: b"/u" })?;
    let _ = client.close(root);
    let _ = client.close(first);
    let mut data = [0u8; 4];
    let length = client
        .read(second, 0, &mut data)
        .map_err(|error| format!("survivor read: {error:?}"))?;
    if length != 4 || &data != b"data" {
        return Err("survivor lost unlinked data".to_string());
    }
    let _ = client.close(second);
    match client.open("ramfs:u", READ) {
        Err(Errno::ENOENT) => Ok(()),
        _ => Err("unlinked file still resolves".to_string()),
    }
}

fn dup_cursor(client: &Client) -> Result<(), String> {
    write_all(client, "ramfs:d", b"abcdef")?;
    let handle = client
        .open("ramfs:d", READ)
        .map_err(|error| format!("dup open: {error:?}"))?;
    let mut first = [0u8; 3];
    client
        .read(handle, 0, &mut first)
        .map_err(|error| format!("dup read 1: {error:?}"))?;
    let duplicate = client
        .dup(handle, Rights::ALL.bits())
        .map_err(|error| format!("dup: {error:?}"))?;
    let _ = client.close(handle);
    let mut rest = [0u8; 3];
    let length = client
        .read(duplicate, 0, &mut rest)
        .map_err(|error| format!("dup read 2: {error:?}"))?;
    let _ = client.close(duplicate);
    if &first != b"abc" || length != 3 || &rest != b"def" {
        return Err("dup cursor was not shared".to_string());
    }
    Ok(())
}

fn readdir_invalidation(client: &Client) -> Result<(), String> {
    let root = client
        .open("ramfs:", READ)
        .map_err(|error| format!("readdir root: {error:?}"))?;
    call_status(
        client,
        root,
        Call::Mkdir {
            mode: 0o755,
            path: b"/dir",
        },
    )?;
    write_all(client, "ramfs:dir/a", b"a")?;
    write_all(client, "ramfs:dir/b", b"b")?;
    let dir = client
        .open("ramfs:dir", READ)
        .map_err(|error| format!("readdir open: {error:?}"))?;
    let mut buffer = [0u8; 64];
    let first = client
        .read(dir, 0, &mut buffer)
        .map_err(|error| format!("readdir read 1: {error:?}"))?;
    if first == 0 {
        return Err("empty directory".to_string());
    }
    write_all(client, "ramfs:dir/c", b"c")?;
    match client.read(dir, 0, &mut buffer) {
        Err(Errno::EINVAL) => {}
        _ => return Err("directory mutation was not invalidated".to_string()),
    }
    let _ = client.close(dir);
    let _ = client.close(root);
    Ok(())
}

fn vfsd_round_trip(client: &Client) -> Result<(), String> {
    write_all(client, "file:/tmp/x", b"vfsd")?;
    if read_all(client, "file:/tmp/x")? != b"vfsd" {
        return Err("file:/tmp/x round trip failed".to_string());
    }
    let root = directory(client, "file:/")?;
    if !root.iter().any(|name| name == "tmp") {
        return Err("file:/ listing missing tmp".to_string());
    }
    if directory(client, "file:/sbin")?.is_empty() {
        return Err("file:/sbin is empty".to_string());
    }
    Ok(())
}
