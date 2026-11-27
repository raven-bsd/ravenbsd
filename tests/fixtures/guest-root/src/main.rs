// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S777 fixture.
//!
//! `/init` spawns the real `initfsd` and proves the target guest root carries the FreeBSD base
//! `/root` skeleton: `/root` is a directory with mode 0700, and `/root/.profile` is a non-empty
//! regular file with mode 0644. Both are owned by root. This is the root the target `t_link`
//! `link_perm` case sees; it is test input, not a product change.

#![no_std]
#![no_main]
#![forbid(unsafe_code)]

use raven_initfs::Initfs;
use raven_proto_fs::{O_DIRECTORY, O_RDONLY, OpenFlags, S_IFDIR, S_IFMT, S_IFREG};
use raven_rt::entry;
use raven_rt::println;
use raven_rt::spawn::{SpawnOptions, spawn};
use raven_scheme::Client;

entry!(main);

/// `/root/.profile` content packed into the initfs; the probe compares it byte for byte.
const PROFILE: &[u8] = include_bytes!("../profile");

fn wait_server(client: &Client) -> Result<(), &'static str> {
    for _ in 0..1000 {
        if let Ok(handle) = client.open_flags(
            "initfs:/",
            OpenFlags::from_linux(O_RDONLY | O_DIRECTORY).unwrap(),
        ) {
            let _ = client.close(handle);
            return Ok(());
        }
        let _ = raven_rt::sys::nanosleep(10_000_000);
    }
    Err("server did not register")
}

fn run() -> Result<(), &'static str> {
    let image = raven_rt::sys::map_initfs().map_err(|_| "map initfs")?;
    let initfs_image = Initfs::parse(image).map_err(|_| "parse initfs")?;
    let options = SpawnOptions {
        args: &[],
        env: &[],
    };
    let _initfsd = spawn(&initfs_image, "/initfsd", &options).map_err(|_| "spawn initfsd")?;

    let client = Client::new();
    wait_server(&client)?;

    let root = match client.open_flags(
        "initfs:/root",
        OpenFlags::from_linux(O_RDONLY | O_DIRECTORY).map_err(|_| "root open flags")?,
    ) {
        Ok(handle) => handle,
        Err(error) => {
            println!("guest-root: /root error={:?}", error);
            return Err("root-open");
        }
    };
    let root_stat = client.fstat(root).map_err(|_| "stat /root")?;
    let _ = client.close(root);
    println!(
        "guest-root: /root dir={} mode={:04o} uid={} gid={}",
        u8::from(root_stat.mode & S_IFMT == S_IFDIR),
        root_stat.mode & 0o7777,
        root_stat.uid,
        root_stat.gid
    );
    let mut root_type = false;
    let mut root_mode = false;
    let mut root_owner = false;
    if root_stat.mode & S_IFMT != S_IFDIR {
        root_type = true;
    }
    if root_stat.mode & 0o7777 != 0o700 {
        root_mode = true;
    }
    if root_stat.uid != 0 || root_stat.gid != 0 {
        root_owner = true;
    }

    let profile = match client.open_flags("initfs:/root/.profile", OpenFlags::READ) {
        Ok(handle) => handle,
        Err(error) => {
            println!("guest-root: /root/.profile error={:?}", error);
            println!(
                "guest-root: contract root_type={} root_mode={} root_owner={} profile=missing",
                u8::from(root_type),
                u8::from(root_mode),
                u8::from(root_owner)
            );
            return Err("profile-open");
        }
    };
    let profile_stat = client.fstat(profile).map_err(|_| "stat /root/.profile")?;
    println!(
        "guest-root: /root/.profile reg={} mode={:04o} size={} uid={} gid={}",
        u8::from(profile_stat.mode & S_IFMT == S_IFREG),
        profile_stat.mode & 0o7777,
        profile_stat.size,
        profile_stat.uid,
        profile_stat.gid
    );
    let mut contents = [0u8; 1024];
    let length = client
        .read(profile, 0, &mut contents)
        .map_err(|_| "read /root/.profile")?;
    let _ = client.close(profile);
    let length = length.min(contents.len());

    let profile_type = profile_stat.mode & S_IFMT == S_IFREG;
    let profile_mode = profile_stat.mode & 0o7777 == 0o644;
    let profile_owner = profile_stat.uid == 0 && profile_stat.gid == 0;
    let profile_size = profile_stat.size != 0 && profile_stat.size == PROFILE.len() as u64;
    let profile_content = contents.get(..length) == Some(PROFILE);
    println!(
        "guest-root: contract root_type={} root_mode={} root_owner={} \
         profile_type={} profile_mode={} profile_owner={} profile_size={} profile_content={}",
        u8::from(!root_type),
        u8::from(!root_mode),
        u8::from(!root_owner),
        u8::from(profile_type),
        u8::from(profile_mode),
        u8::from(profile_owner),
        u8::from(profile_size),
        u8::from(profile_content)
    );
    if root_type
        || root_mode
        || root_owner
        || !profile_type
        || !profile_mode
        || !profile_owner
        || !profile_size
        || !profile_content
    {
        return Err("contract");
    }
    Ok(())
}

fn main(_args: raven_rt::Args) -> i32 {
    match run() {
        Ok(()) => {
            println!("guest-root: ok");
            0
        }
        Err(message) => {
            println!("guest-root: fail {message}");
            1
        }
    }
}
