// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! U5 user-tools qtest fixture init.
//!
//! Starts the minimal scheme stack, mounts a ramfs over `/etc`, copies the three account files
//! from the initfs, and starts `rsh`. The seven U2 tools then run against a writable account
//! database; `su` and `passwd` are packed 4755 by the build script.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::format;

    use raven_initfs::Initfs;
    use raven_proto_fs::{O_CREAT, O_RDONLY, O_RDWR, O_TRUNC, O_WRONLY};
    use raven_rt::entry;
    use raven_rt::fs;
    use raven_rt::spawn::{RtSys, SpawnOptions, spawn};
    use raven_scheme::Client;

    entry!(main);

    fn main(_args: raven_rt::Args) -> i32 {
        match run() {
            Ok(code) => code,
            Err(message) => {
                raven_rt::eprintln!("user-tools: {message}");
                1
            }
        }
    }

    fn run() -> Result<i32, &'static str> {
        let image = raven_rt::map_initfs().map_err(|_| "map initfs")?;
        let initfs = Initfs::parse(image).map_err(|_| "parse initfs")?;

        start(&initfs, "/sbin/logd", &[])?;
        wait("log:probe", O_WRONLY)?;
        start(&initfs, "/sbin/initfsd", &["INITFS_HANDLE=1"])?;
        wait("initfs:/", O_RDONLY)?;
        start(&initfs, "/sbin/ramfsd", &[])?;
        wait("ramfs:/", O_RDONLY)?;
        start(&initfs, "/sbin/nulld", &[])?;
        start(&initfs, "/sbin/zerod", &[])?;
        start(&initfs, "/sbin/randd", &[])?;
        start(&initfs, "/sbin/vfsd", &[])?;
        wait("file:/", O_RDONLY)?;
        start(&initfs, "/sbin/consd", &[])?;
        wait("tty:console", O_RDWR)?;

        copy_accounts(&initfs)?;
        let child = spawn(
            &initfs,
            "/bin/rsh",
            &SpawnOptions {
                args: &[],
                env: &[],
            },
        )
        .map_err(|_| "spawn rsh")?;
        let mut code = 1;
        RtSys.proc_wait(child, &mut code).map_err(|_| "wait rsh")?;
        Ok(code)
    }

    fn start(initfs: &Initfs<'_>, path: &str, env: &[&str]) -> Result<(), &'static str> {
        spawn(initfs, path, &SpawnOptions { args: &[], env }).map_err(|_| "spawn service")?;
        Ok(())
    }

    fn wait(path: &str, flags: u32) -> Result<(), &'static str> {
        let client = Client::new();
        let wire = fs::s11_flags(flags).map_err(|_| "flags")?;
        for _ in 0..3000 {
            if let Ok(handle) = client.open(path, wire) {
                let _ = client.close(handle);
                return Ok(());
            }
            let _ = raven_rt::nanosleep(10_000_000);
        }
        Err("scheme timeout")
    }

    fn copy_accounts(initfs: &Initfs<'_>) -> Result<(), &'static str> {
        fs::mount("/etc", "ramfs:").map_err(|_| "mount /etc")?;
        for (name, mode) in [("passwd", 0o644u32), ("group", 0o644), ("shadow", 0o600)] {
            let source = format!("/etc/{name}");
            let entry = initfs.get(&source).ok_or("account file missing")?;
            let target = format!("/etc/{name}");
            write_all(&format!("file:{target}"), entry.data)?;
            fs::chmod(&target, mode).map_err(|_| "chmod account file")?;
        }
        Ok(())
    }

    fn write_all(path: &str, data: &[u8]) -> Result<(), &'static str> {
        let mut file =
            fs::open(path, O_WRONLY | O_CREAT | O_TRUNC).map_err(|_| "open account file")?;
        let mut offset = 0usize;
        while offset < data.len() {
            let written = file
                .write(data.get(offset..).unwrap_or(&[]))
                .map_err(|_| "write account file")?;
            if written == 0 {
                return Err("short account write");
            }
            offset = offset.saturating_add(written);
        }
        file.close().map_err(|_| "close account file")?;
        fs::sync().map_err(|_| "sync account file")
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
