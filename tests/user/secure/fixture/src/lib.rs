// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Shared target helpers for the U3e `su`/`passwd` credential-transition fixture.

#![cfg_attr(target_os = "none", no_std)]
#![allow(unsafe_code)] // The raw wait syscall is the only unsafe surface; its contract is local.

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod target {
    use alloc::format;
    use alloc::string::{String, ToString};
    use alloc::vec::Vec;

    use raven_abi::errno::Errno;
    use raven_abi::syscall::Syscall;
    use raven_abi::types::Handle;
    use raven_initfs::Initfs;
    use raven_proto_fs::{O_CREAT, O_TRUNC, O_WRONLY, OpenFlags};
    use raven_rt::fs;
    use raven_rt::proc;
    use raven_rt::spawn::{SpawnError, SpawnOptions, spawn, spawn_file};
    use raven_scheme::Client;

    /// The fixture's non-root uid.
    pub const UID: u32 = 1000;
    /// The fixture's non-root gid.
    pub const GID: u32 = 1000;
    /// The caller group the target session must not inherit.
    pub const CALLER_GROUP: u32 = 0;

    /// Environment entries a hostile caller tries to inject into the session.
    pub const HOSTILE_ENV: &[&str] = &[
        "PATH=/evil",
        "HOME=/evil",
        "SHELL=/evil",
        "USER=evil",
        "LOGNAME=evil",
        "LD_PRELOAD=/evil.so",
        "TERM=vt220",
    ];

    /// The environment keys the probe reports.
    pub const ENV_KEYS: &[&str] = &[
        "HOME",
        "SHELL",
        "USER",
        "LOGNAME",
        "PATH",
        "PWD",
        "TERM",
        "LD_PRELOAD",
    ];

    /// Wait for `process` and return its raw exit code.
    pub fn wait(process: Handle) -> Result<i32, Errno> {
        let mut code = 0i32;
        // SAFETY: PROC_WAIT takes a process handle and a writable i32 pointer owned by the caller.
        let raw = unsafe {
            raven_rt::sys::syscall2(
                Syscall::PROC_WAIT as usize,
                process.0 as usize,
                &mut code as *mut i32 as usize,
            )
        };
        Errno::from_ret(raw).map(|_| code)
    }

    /// Start `path` as a normal (non-setuid) initfs program.
    pub fn spawn_plain(
        initfs: &Initfs<'_>,
        path: &str,
        args: &[&str],
        env: &[&str],
    ) -> Result<Handle, SpawnError> {
        spawn(initfs, path, &SpawnOptions { args, env })
    }

    /// Start a setuid-capable binary through an open `initfs:` handle.
    ///
    /// The native exec path applies the initfs mode bits only when it receives the open file
    /// handle, not the plain initfs path.
    pub fn spawn_setuid(path: &str, args: &[&str], env: &[&str]) -> Result<Handle, SpawnError> {
        let client = Client::new();
        let open_path = format!("initfs:{path}");
        let handle = client
            .open(&open_path, u32::from(OpenFlags::READ.to_wire()))
            .map_err(SpawnError::Sys)?;
        let child = spawn_file(handle, path, &SpawnOptions { args, env }, false);
        let _ = client.close(handle);
        child
    }

    /// Wait until `path` opens successfully.
    pub fn wait_scheme(path: &str) -> Result<(), &'static str> {
        let client = Client::new();
        let mut attempt = 0usize;
        while attempt < 3000 {
            if let Ok(handle) = client.open(path, u32::from(OpenFlags::READ.to_wire())) {
                let _ = client.close(handle);
                return Ok(());
            }
            let _ = raven_rt::nanosleep(10_000_000);
            attempt = attempt.saturating_add(1);
        }
        Err("scheme timeout")
    }

    /// Start the filesystem and console services the tools need.
    pub fn start_services() -> Result<(), &'static str> {
        let image = raven_rt::map_initfs().map_err(|_| "initfs map")?;
        let initfs = Initfs::parse(image).map_err(|_| "initfs parse")?;
        for (path, probe) in [
            ("/sbin/initfsd", "initfs:"),
            ("/sbin/ramfsd", "ramfs:"),
            ("/sbin/vfsd", "file:"),
            ("/sbin/consd", "tty:console"),
        ] {
            spawn_plain(&initfs, path, &[], &[]).map_err(|_| "spawn service")?;
            wait_scheme(probe)?;
        }
        Ok(())
    }

    /// Copy the initfs account files into the writable `/etc` ramfs mount.
    ///
    /// This uses the same plain `/etc/...` paths `raven-userdb`'s target adapter uses. It is the
    /// point where the current `raven-rt` plain-path defect blocks the target qtest; the case is
    /// skipped with that reason rather than weakening the assertion.
    pub fn install_accounts() -> Result<(), &'static str> {
        for (name, mode) in [("passwd", 0o644), ("group", 0o644), ("shadow", 0o600)] {
            let source = format!("initfs:/etc/{name}");
            let data = fs::read_to_vec(&source).map_err(|_| "read account")?;
            let target = format!("/etc/{name}");
            write_file(&target, &data, mode)?;
        }
        // The target shells run as the account uid; give them one writable marker directory.
        match fs::mkdir("/tmp/secure", 0o777) {
            Ok(()) | Err(Errno::EEXIST) => {}
            Err(_) => return Err("mkdir marker"),
        }
        fs::chmod("/tmp/secure", 0o777).map_err(|_| "chmod marker")?;
        Ok(())
    }

    /// Write a whole file and apply its final mode before the contents are visible.
    pub fn write_file(path: &str, data: &[u8], mode: u32) -> Result<(), &'static str> {
        let mut file = fs::open(path, O_WRONLY | O_CREAT | O_TRUNC).map_err(|_| "open file")?;
        fs::chmod(path, mode).map_err(|_| "chmod file")?;
        let mut offset = 0usize;
        while offset < data.len() {
            let written = file
                .write(data.get(offset..).ok_or("file offset")?)
                .map_err(|_| "write file")?;
            if written == 0 {
                return Err("short write");
            }
            offset = offset.saturating_add(written);
        }
        file.close().map_err(|_| "close file")
    }

    /// Read a whole `file:` path.
    pub fn read_file(path: &str) -> Result<Vec<u8>, &'static str> {
        fs::read_to_vec(path).map_err(|_| "read file")
    }

    /// Remove the shell-run marker if it exists.
    pub fn remove_marker() {
        let _ = fs::unlink("/tmp/secure/ran");
    }

    /// True when a target shell ran.
    pub fn marker_exists() -> bool {
        fs::stat("/tmp/secure/ran").is_ok()
    }

    /// True when the account-database lock survived a failed transaction.
    pub fn lock_exists() -> bool {
        fs::stat("file:/etc/.pwd.lock").is_ok()
    }

    /// Create the shell-run marker.
    pub fn write_marker() {
        let _ = write_file("/tmp/secure/ran", b"ran", 0o644);
    }

    /// Read the caller's supplementary groups.
    pub fn read_groups() -> Result<Vec<u32>, Errno> {
        let mut empty = [];
        let count = proc::groups(&mut empty)?;
        let mut buffer = [0u32; 16];
        if count > buffer.len() {
            return Err(Errno::ERANGE);
        }
        let count = proc::groups(&mut buffer)?;
        Ok(buffer.get(..count).unwrap_or(&[]).to_vec())
    }

    /// Print the six credential words.
    pub fn print_ids(label: &str) -> Result<(), Errno> {
        let ids = proc::credentials()?;
        raven_rt::println!(
            "{label}: uid={} euid={} suid={} gid={} egid={} sgid={}",
            ids.uid,
            ids.euid,
            ids.suid,
            ids.gid,
            ids.egid,
            ids.sgid
        );
        Ok(())
    }

    /// Print the supplementary groups as a comma-separated list.
    pub fn print_groups(label: &str) -> Result<(), Errno> {
        let groups = read_groups()?;
        let mut line = String::from(label);
        line.push_str(": groups=");
        for (index, gid) in groups.iter().enumerate() {
            if index > 0 {
                line.push(',');
            }
            line.push_str(&gid.to_string());
        }
        raven_rt::println!("{line}");
        Ok(())
    }

    /// Print the selected environment keys.
    pub fn print_environment(label: &str, args: &raven_rt::Args) {
        for key in ENV_KEYS {
            match args.env_get(key) {
                Some(value) => raven_rt::println!("{label}: env {key}={value}"),
                None => raven_rt::println!("{label}: env {key}=<unset>"),
            }
        }
    }

    /// Rebuild the environment the shell passes to the command probe.
    pub fn inherited_environment(args: &raven_rt::Args) -> Vec<String> {
        let mut environment = Vec::new();
        for key in ENV_KEYS {
            if let Some(value) = args.env_get(key) {
                environment.push(format!("{key}={value}"));
            }
        }
        environment
    }

    /// The exit status of the child, or `-1` when the wait failed.
    pub fn exit_code(process: Handle) -> i32 {
        wait(process).unwrap_or(-1)
    }
}

#[cfg(target_os = "none")]
pub use target::*;
