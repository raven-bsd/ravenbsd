// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S204 qtest probe: an unprivileged `file:` client creates with two caller
//! umasks and reads the resulting mode and owner back from `FSTAT`.
//!
//! The client runs as root as a one-shot svcd service, then switches to the
//! non-root caller ids before the create, so the kernel stamps the vfsd request
//! with the caller's credentials. Protocol version 1 has no create mode on the
//! OPEN wire; like libc, the probe applies `0666 & !mask` with a `CHMOD` CALL
//! after the create and then stats the path.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::string::String;

    use raven_proto_fs::{O_CREAT, O_RDWR};
    use raven_rt::entry;

    /// Non-root caller user id the probe switches to before the create.
    const CALLER_UID: u32 = 1000;
    /// Non-root caller group id the probe switches to before the create.
    const CALLER_GID: u32 = 1000;
    /// Permission bits requested by a POSIX `open(path, O_CREAT, 0666)`.
    const CREATE_MODE: u32 = 0o666;

    entry!(main);

    fn main(_args: raven_rt::Args) -> i32 {
        match run() {
            Ok(()) => {
                raven_rt::println!("vfs-umask: probe ok");
                0
            }
            Err(message) => {
                raven_rt::eprintln!("vfs-umask: {message}");
                1
            }
        }
    }

    fn run() -> Result<(), String> {
        raven_rt::proc::setgroups(&[])
            .map_err(|error| alloc::format!("setgroups: {}", error.description()))?;
        raven_rt::proc::setresgid(CALLER_GID)
            .map_err(|error| alloc::format!("setresgid: {}", error.description()))?;
        raven_rt::proc::setresuid(CALLER_UID)
            .map_err(|error| alloc::format!("setresuid: {}", error.description()))?;

        for (mask, path) in [(0o027, "/tmp/vfs-umask-027"), (0o077, "/tmp/vfs-umask-077")] {
            let scheme_path = alloc::format!("file:{path}");
            let file = raven_rt::fs::open(&scheme_path, O_RDWR | O_CREAT)
                .map_err(|error| alloc::format!("open {path}: {}", error.description()))?;
            let created = file
                .fstat()
                .map_err(|error| alloc::format!("fstat {path}: {}", error.description()))?;
            file.close()
                .map_err(|error| alloc::format!("close {path}: {}", error.description()))?;

            let wanted = CREATE_MODE & !mask;
            let chmod = raven_rt::fs::chmod(path, wanted);
            let after = raven_rt::fs::stat(&scheme_path)
                .map_err(|error| alloc::format!("stat {path}: {}", error.description()))?;

            raven_rt::println!(
                "vfs-umask: {mask:03o} mode={:04o} uid={} gid={}",
                after.mode & 0o777,
                after.uid,
                after.gid
            );
            raven_rt::println!(
                "vfs-umask: {mask:03o} created-mode={:04o} created-uid={} created-gid={}",
                created.mode & 0o777,
                created.uid,
                created.gid
            );
            if let Err(error) = chmod {
                raven_rt::println!("vfs-umask: {mask:03o} chmod-errno={}", error.0);
            }
        }
        Ok(())
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
