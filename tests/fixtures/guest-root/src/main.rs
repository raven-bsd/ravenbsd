// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S777 fixture.
//!
//! `/init` starts the real initfsd and vfsd plus a second ramfs instance whose root mode is the
//! FreeBSD donor `/root` mode (0700), runs the G1 wrapper's `/root` setup path, and proves the
//! target guest root carries the FreeBSD base skeleton: `/root` is a directory with mode 0700,
//! and `/root/.profile` is a non-empty regular file with mode 0644, both root-owned. This is the
//! root the target `t_link` `link_perm` case sees; it is test input, not a product change.

#![no_std]
#![no_main]
#![forbid(unsafe_code)]

use raven_initfs::Initfs;
use raven_proto_fs::{O_DIRECTORY, O_RDONLY, OpenFlags, S_IFDIR, S_IFMT, S_IFREG};
use raven_rt::entry;
use raven_rt::println;
use raven_rt::spawn::{RtSys, SpawnOptions, spawn};
use raven_scheme::Client;

entry!(main);

/// `/root/.profile` content written by the G1 wrapper; the probe compares it byte for byte.
const PROFILE: &[u8] = include_bytes!("../profile");
/// The default ramfs root is 0755; the donor `/root` is 0700, so the guest starts a second one.
const ROOT_SCHEME: &str = "rootfs";

fn wait_open(client: &Client, path: &str, flags: u32) -> Result<(), &'static str> {
    for _ in 0..1000 {
        let flags = OpenFlags::from_linux(flags).map_err(|_| "open flags")?;
        if let Ok(handle) = client.open_flags(path, flags) {
            let _ = client.close(handle);
            return Ok(());
        }
        let _ = raven_rt::sys::nanosleep(10_000_000);
    }
    Err("server did not register")
}

fn stat_path(
    client: &Client,
    path: &str,
    flags: u32,
) -> Result<raven_proto_fs::Stat, &'static str> {
    let flags = OpenFlags::from_linux(flags).map_err(|_| "open flags")?;
    let handle = client.open_flags(path, flags).map_err(|_| "open")?;
    let stat = client.fstat(handle).map_err(|_| "fstat")?;
    let _ = client.close(handle);
    Ok(stat)
}

fn run() -> Result<(), &'static str> {
    let image = raven_rt::sys::map_initfs().map_err(|_| "map initfs")?;
    let initfs_image = Initfs::parse(image).map_err(|_| "parse initfs")?;
    let options = SpawnOptions {
        args: &[],
        env: &[],
    };
    let _initfsd = spawn(&initfs_image, "/sbin/initfsd", &options).map_err(|_| "spawn initfsd")?;

    let client = Client::new();
    wait_open(&client, "initfs:/", O_RDONLY | O_DIRECTORY)?;

    // The second ramfs is what supplies the donor `/root` mode; it must register before vfsd so
    // vfsd's namespace snapshot can resolve it.
    let rootfs_args = ["ramfsd", "--scheme", ROOT_SCHEME, "--root-mode", "700"];
    let rootfs_options = SpawnOptions {
        args: &rootfs_args,
        env: &[],
    };
    let _rootfsd = spawn(&initfs_image, "/sbin/ramfsd", &rootfs_options)
        .map_err(|_| "spawn rootfsd")?;
    wait_open(&client, "rootfs:", O_RDONLY | O_DIRECTORY)?;

    let _vfsd = spawn(&initfs_image, "/sbin/vfsd", &options).map_err(|_| "spawn vfsd")?;
    wait_open(&client, "file:/", O_RDONLY | O_DIRECTORY)?;

    let root_args = ["g1wrap", "root"];
    let root_options = SpawnOptions {
        args: &root_args,
        env: &[],
    };
    let setup = spawn(&initfs_image, "/sbin/g1wrap", &root_options).map_err(|_| "spawn g1wrap")?;
    let mut code = -1i32;
    RtSys
        .proc_wait(setup, &mut code)
        .map_err(|_| "wait g1wrap")?;
    if code != 0 {
        println!("guest-root: g1wrap root exited {code}");
        return Err("root-setup");
    }

    let root_stat = stat_path(&client, "file:/root", O_RDONLY | O_DIRECTORY)?;
    println!(
        "guest-root: /root dir={} mode={:04o} uid={} gid={}",
        u8::from(root_stat.mode & S_IFMT == S_IFDIR),
        root_stat.mode & 0o7777,
        root_stat.uid,
        root_stat.gid
    );
    let root_type = root_stat.mode & S_IFMT == S_IFDIR;
    let root_mode = root_stat.mode & 0o7777 == 0o700;
    let root_owner = root_stat.uid == 0 && root_stat.gid == 0;

    let profile_stat = stat_path(&client, "file:/root/.profile", O_RDONLY)?;
    println!(
        "guest-root: /root/.profile reg={} mode={:04o} size={} uid={} gid={}",
        u8::from(profile_stat.mode & S_IFMT == S_IFREG),
        profile_stat.mode & 0o7777,
        profile_stat.size,
        profile_stat.uid,
        profile_stat.gid
    );
    let profile_type = profile_stat.mode & S_IFMT == S_IFREG;
    let profile_mode = profile_stat.mode & 0o7777 == 0o644;
    let profile_owner = profile_stat.uid == 0 && profile_stat.gid == 0;
    let profile_size = profile_stat.size != 0 && profile_stat.size == PROFILE.len() as u64;
    let mut contents = [0u8; 1024];
    let flags = OpenFlags::from_linux(O_RDONLY).map_err(|_| "open flags")?;
    let profile = client
        .open_flags("file:/root/.profile", flags)
        .map_err(|_| "open /root/.profile")?;
    let length = client
        .read(profile, 0, &mut contents)
        .map_err(|_| "read /root/.profile")?;
    let _ = client.close(profile);
    let length = length.min(contents.len());
    let profile_content = contents.get(..length) == Some(PROFILE);

    // The initfs synthesizes every directory as 0755 (initfsd `Model::stat`), so it cannot carry
    // the donor 0700 `/root`. That provider limitation is separate from this ticket and must not
    // be masked; record the synthesized-directory mode as a note.
    match stat_path(&client, "initfs:/sbin", O_RDONLY | O_DIRECTORY) {
        Ok(initfs_dir) => println!(
            "guest-root: initfs-dir mode={:04o}",
            initfs_dir.mode & 0o7777
        ),
        Err(_) => println!("guest-root: initfs-dir missing"),
    }

    println!(
        "guest-root: contract root_type={} root_mode={} root_owner={} \
         profile_type={} profile_mode={} profile_owner={} profile_size={} profile_content={}",
        u8::from(root_type),
        u8::from(root_mode),
        u8::from(root_owner),
        u8::from(profile_type),
        u8::from(profile_mode),
        u8::from(profile_owner),
        u8::from(profile_size),
        u8::from(profile_content)
    );
    if !root_type
        || !root_mode
        || !root_owner
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
