// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S53 PG-22 fixture.
//!
//! `/init` spawns the real `initfsd` and `ramfsd` servers, then exercises the client API:
//! sequential `read`/`write` keep the server cursor, `seek` moves it with `SEEK_SET`/`CUR`/`END`,
//! and `pread`/`pwrite` use an explicit offset without moving it. The ramfs case writes past EOF
//! to create a hole; the initfs case proves the same seek/pread path over the read-only server.

#![no_std]
#![no_main]

use raven_abi::errno::Errno;
use raven_abi::types::Handle;
use raven_initfs::Initfs;
use raven_proto_fs::{O_CREAT, O_RDONLY, O_RDWR, O_WRONLY, OpenFlags};
use raven_rt::entry;
use raven_rt::println;
use raven_rt::spawn::{SpawnOptions, spawn};
use raven_scheme::Client;

entry!(main);

const SEEK_SET: u32 = 0;
const SEEK_CUR: u32 = 1;
const SEEK_END: u32 = 2;

fn wait_open(client: &Client, path: &str, flags: OpenFlags) -> Result<Handle, &'static str> {
    for _ in 0..1000 {
        if let Ok(handle) = client.open_flags(path, flags) {
            return Ok(handle);
        }
        let _ = raven_rt::sys::nanosleep(10_000_000);
    }
    Err("server did not register")
}

fn expect_err<T>(result: Result<T, Errno>, error: Errno) -> Result<(), &'static str> {
    match result {
        Err(value) if value == error => Ok(()),
        _ => Err("wrong error"),
    }
}

fn ramfs(client: &Client) -> Result<(), &'static str> {
    let file = wait_open(
        client,
        "ramfs:seek",
        OpenFlags::from_linux(O_RDWR | O_CREAT).map_err(|_| "open flags")?,
    )?;

    if client
        .pwrite(file, 0, b"abcdefghij")
        .map_err(|_| "pwrite data")?
        != 10
    {
        return Err("pwrite length");
    }
    if client.seek(file, 0, SEEK_CUR).map_err(|_| "cursor")? != 0 {
        return Err("pwrite moved cursor");
    }

    let mut data = [0u8; 8];
    if client
        .pread(file, 2, &mut data[..3])
        .map_err(|_| "pread cde")?
        != 3
        || &data[..3] != b"cde"
    {
        return Err("pread contents");
    }
    if client.seek(file, 0, SEEK_CUR).map_err(|_| "cursor")? != 0 {
        return Err("pread moved cursor");
    }

    // A sequential read moves the cursor; a positional operation around it does not.
    if client.seek(file, 4, SEEK_SET).map_err(|_| "seek set")? != 4 {
        return Err("seek set value");
    }
    if client.read(file, 0, &mut data[..2]).map_err(|_| "read")? != 2
        || &data[..2] != b"ef"
    {
        return Err("read contents");
    }
    if client.seek(file, 0, SEEK_CUR).map_err(|_| "cursor")? != 6 {
        return Err("read cursor");
    }
    if client.seek(file, -2, SEEK_END).map_err(|_| "seek end")? != 8 {
        return Err("seek end value");
    }
    if client.read(file, 0, &mut data[..2]).map_err(|_| "read tail")? != 2
        || &data[..2] != b"ij"
    {
        return Err("read tail contents");
    }
    if client.seek(file, 0, SEEK_END).map_err(|_| "seek end")? != 10 {
        return Err("seek end size");
    }
    if client.seek(file, 100, SEEK_SET).map_err(|_| "seek past eof")? != 100 {
        return Err("seek past eof value");
    }
    if client.read(file, 0, &mut data).map_err(|_| "read past eof")? != 0 {
        return Err("read past eof");
    }

    // Positional write past EOF creates a zero-filled hole and does not move the cursor.
    if client
        .pwrite(file, 15, b"XY")
        .map_err(|_| "pwrite hole")?
        != 2
    {
        return Err("pwrite hole length");
    }
    if client.fstat(file).map_err(|_| "fstat")?.size != 17 {
        return Err("hole size");
    }
    if client.seek(file, 0, SEEK_CUR).map_err(|_| "cursor")? != 100 {
        return Err("pwrite moved cursor");
    }
    let mut hole = [0xffu8; 7];
    if client
        .pread(file, 10, &mut hole)
        .map_err(|_| "pread hole")?
        != 7
        || hole != [0, 0, 0, 0, 0, b'X', b'Y']
    {
        return Err("hole contents");
    }
    if client.seek(file, -2, SEEK_END).map_err(|_| "seek end")? != 15 {
        return Err("seek end after hole");
    }
    if client.read(file, 0, &mut data[..2]).map_err(|_| "read hole")? != 2
        || &data[..2] != b"XY"
    {
        return Err("read hole contents");
    }

    // Invalid seek targets and access modes.
    if expect_err(client.seek(file, -1, SEEK_SET), Errno::EINVAL).is_err() {
        return Err("negative seek accepted");
    }
    if expect_err(client.seek(file, 0, 9), Errno::EINVAL).is_err() {
        return Err("unknown whence accepted");
    }
    let reader = client
        .open_flags("ramfs:seek", OpenFlags::from_linux(O_RDONLY).unwrap())
        .map_err(|_| "reader open")?;
    if expect_err(client.pwrite(reader, 0, b"x"), Errno::EACCES).is_err() {
        return Err("pwrite on reader accepted");
    }
    let writer = client
        .open_flags("ramfs:seek", OpenFlags::from_linux(O_WRONLY).unwrap())
        .map_err(|_| "writer open")?;
    if expect_err(client.pread(writer, 0, &mut data[..1]), Errno::EACCES).is_err() {
        return Err("pread on writer accepted");
    }
    Ok(())
}

fn initfs(client: &Client) -> Result<(), &'static str> {
    let file = wait_open(client, "initfs:init", OpenFlags::READ)?;
    let size = client.fstat(file).map_err(|_| "initfs fstat")?.size;
    if size < 4 {
        return Err("initfs file too small");
    }

    let mut magic = [0u8; 4];
    if client
        .pread(file, 0, &mut magic)
        .map_err(|_| "initfs pread")?
        != 4
        || magic != *b"\x7fELF"
    {
        return Err("initfs pread magic");
    }
    if client.seek(file, 0, SEEK_CUR).map_err(|_| "initfs cursor")? != 0 {
        return Err("initfs pread moved cursor");
    }
    if client
        .seek(file, -4, SEEK_END)
        .map_err(|_| "initfs seek end")?
        != size - 4
    {
        return Err("initfs seek end value");
    }
    let mut tail = [0u8; 4];
    if client.read(file, 0, &mut tail).map_err(|_| "initfs read tail")? != 4 {
        return Err("initfs read tail length");
    }
    if client.seek(file, 0, SEEK_CUR).map_err(|_| "initfs cursor")? != size {
        return Err("initfs cursor after tail");
    }
    Ok(())
}

fn run() -> Result<(), &'static str> {
    let image = raven_rt::sys::map_initfs().map_err(|_| "map initfs")?;
    let initfs_image = Initfs::parse(image).map_err(|_| "parse initfs")?;
    let options = SpawnOptions {
        args: &[],
        env: &[],
    };
    let _initfsd = spawn(&initfs_image, "/initfsd", &options).map_err(|_| "spawn initfsd")?;
    let _ramfsd = spawn(&initfs_image, "/ramfsd", &options).map_err(|_| "spawn ramfsd")?;

    let client = Client::new();
    ramfs(&client)?;
    initfs(&client)?;
    Ok(())
}

fn main(_args: raven_rt::Args) -> i32 {
    match run() {
        Ok(()) => {
            println!("libc-seek: ok");
            0
        }
        Err(message) => {
            println!("libc-seek: fail {}", message);
            1
        }
    }
}
