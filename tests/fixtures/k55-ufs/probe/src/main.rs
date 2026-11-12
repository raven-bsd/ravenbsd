// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! One-shot read-write UFS client for the K55 qtest.
//!
//! It creates/updates/removes files, syncs, stops the server, runs the
//! `ufs-core` read-only checker on the same block device, restarts the server
//! and verifies that the data persisted.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::format;
    use alloc::string::{String, ToString};

    use fsd_ufs::block::open_partition;
    use raven_abi::errno::Errno;
    use raven_proto_fs::{
        Call, CallRequest, CallResponse, O_APPEND, O_CREAT, O_RDWR, O_WRONLY, S_IFMT, S_IFREG,
    };
    use raven_proto_svc::{Op, State, Status};
    use raven_rt::entry;
    use raven_rt::fs;
    use raven_scheme::Client;
    use ufs_core::check::{Mode, check};

    entry!(main);

    const WRITE_PATH: &str = "file:/ufs/write.txt";
    const MOVED_PATH: &str = "file:/ufs/rwdir/moved.txt";
    const PERSIST_PATH: &str = "file:/ufs/persist.txt";

    fn main(_args: raven_rt::Args) -> i32 {
        match run() {
            Ok(()) => {
                raven_rt::println!("fsd-ufs-rw: probe ok");
                0
            }
            Err(message) => {
                raven_rt::eprintln!("fsd-ufs-rw: probe: {message}");
                1
            }
        }
    }

    fn run() -> Result<(), String> {
        fs::mount("/ufs", "ufs:").map_err(|error| format!("mount: {}", error.description()))?;
        raven_rt::println!("fsd-ufs-rw: probe mount ok");

        create_write_truncate()?;
        namespace()?;
        persistence()?;
        Ok(())
    }

    fn create_write_truncate() -> Result<(), String> {
        let mut file = fs::open(WRITE_PATH, O_RDWR | O_CREAT)
            .map_err(|error| format!("create: {}", error.description()))?;
        file.write(b"first\n")
            .map_err(|error| format!("write: {}", error.description()))?;
        file.close()
            .map_err(|error| format!("close: {}", error.description()))?;
        let data = fs::read_to_vec(WRITE_PATH)
            .map_err(|error| format!("read back: {}", error.description()))?;
        if data.as_slice() != b"first\n" {
            return Err(format!("create/write read {} bytes", data.len()));
        }
        raven_rt::println!("fsd-ufs-rw: probe create/write ok");

        truncate_file(WRITE_PATH, 3)?;
        let data = fs::read_to_vec(WRITE_PATH)
            .map_err(|error| format!("read truncated: {}", error.description()))?;
        if data.as_slice() != b"fir" {
            return Err(format!("truncate left {} bytes", data.len()));
        }
        let mut file = fs::open(WRITE_PATH, O_WRONLY | O_APPEND)
            .map_err(|error| format!("append open: {}", error.description()))?;
        file.write(b"st\n")
            .map_err(|error| format!("append write: {}", error.description()))?;
        file.close()
            .map_err(|error| format!("append close: {}", error.description()))?;
        let data = fs::read_to_vec(WRITE_PATH)
            .map_err(|error| format!("read appended: {}", error.description()))?;
        if data.as_slice() != b"first\n" {
            return Err(format!("append left {} bytes", data.len()));
        }
        raven_rt::println!("fsd-ufs-rw: probe truncate/append ok");
        Ok(())
    }

    /// `raven_rt::fs::truncate` opens a write-only handle and then sends a
    /// CALL, which the kernel denies because CALL needs READ.  Open read-write
    /// directly so the truncate path itself is exercised.
    fn truncate_file(path: &str, size: u64) -> Result<(), String> {
        let client = Client::new();
        let handle = client
            .open(path, 3)
            .map_err(|error| format!("truncate open: {}", error.description()))?;
        let request = CallRequest::new(Call::Truncate { size });
        let mut encoded = [0u8; 32];
        let length = request
            .encode(&mut encoded)
            .map_err(|_| "encode TRUNCATE".to_string())?;
        let mut response = [0u8; 8];
        let result = client.call(handle, &encoded[..length], &mut response);
        let _ = client.close(handle);
        let length = result.map_err(|error| format!("truncate call: {}", error.description()))?;
        match CallResponse::decode(request.op(), &response[..length])
            .map_err(|_| "decode TRUNCATE".to_string())?
        {
            CallResponse::Status(0) => Ok(()),
            CallResponse::Status(status) => Err(format!("truncate status {status}")),
            other => Err(format!("truncate returned {other:?}")),
        }
    }

    fn namespace() -> Result<(), String> {
        fs::mkdir("/ufs/rwdir", 0o755)
            .map_err(|error| format!("mkdir: {}", error.description()))?;
        fs::rename("/ufs/write.txt", "/ufs/rwdir/moved.txt")
            .map_err(|error| format!("rename: {}", error.description()))?;
        let data = fs::read_to_vec(MOVED_PATH)
            .map_err(|error| format!("read renamed: {}", error.description()))?;
        if data.as_slice() != b"first\n" {
            return Err(format!("renamed file has {} bytes", data.len()));
        }

        fs::symlink("/ufs/rwdir/link", "moved.txt")
            .map_err(|error| format!("symlink: {}", error.description()))?;
        let target = fs::readlink("/ufs/rwdir/link")
            .map_err(|error| format!("readlink: {}", error.description()))?;
        if target != "moved.txt" {
            return Err(format!("readlink returned {target}"));
        }
        let data = fs::read_to_vec("file:/ufs/rwdir/link")
            .map_err(|error| format!("read through link: {}", error.description()))?;
        if data.as_slice() != b"first\n" {
            return Err(format!("link target has {} bytes", data.len()));
        }

        fs::chmod("/ufs/rwdir/moved.txt", 0o600)
            .map_err(|error| format!("chmod: {}", error.description()))?;
        let stat = fs::stat(MOVED_PATH)
            .map_err(|error| format!("stat renamed: {}", error.description()))?;
        if stat.mode & 0o777 != 0o600 || stat.mode & S_IFMT != S_IFREG {
            return Err(format!("renamed mode is {:o}", stat.mode));
        }
        raven_rt::println!("fsd-ufs-rw: probe mkdir/rename/symlink/chmod ok");
        Ok(())
    }

    fn persistence() -> Result<(), String> {
        let mut file = fs::open(PERSIST_PATH, O_RDWR | O_CREAT)
            .map_err(|error| format!("persist create: {}", error.description()))?;
        file.write(b"persist\n")
            .map_err(|error| format!("persist write: {}", error.description()))?;
        file.close()
            .map_err(|error| format!("persist close: {}", error.description()))?;

        fs::unlink("/ufs/rwdir/moved.txt")
            .map_err(|error| format!("unlink: {}", error.description()))?;
        match fs::open(MOVED_PATH, O_RDWR) {
            Err(Errno::ENOENT) => {}
            Err(error) => return Err(format!("unlinked open: {}", error.description())),
            Ok(_) => return Err("unlinked file still opens".to_string()),
        }
        fs::unlink("/ufs/rwdir/link")
            .map_err(|error| format!("unlink link: {}", error.description()))?;
        fs::rmdir("/ufs/rwdir").map_err(|error| format!("rmdir: {}", error.description()))?;
        let entries = fs::read_dir("file:/ufs")
            .map_err(|error| format!("readdir: {}", error.description()))?;
        if entries.iter().any(|entry| entry.name == "rwdir") {
            return Err("rwdir survived rmdir".to_string());
        }
        raven_rt::println!("fsd-ufs-rw: probe unlink/rmdir ok");

        fs::sync().map_err(|error| format!("sync: {}", error.description()))?;
        raven_rt::println!("fsd-ufs-rw: probe fsync ok");

        fs::umount("/ufs").map_err(|error| format!("umount: {}", error.description()))?;
        raven_rt::println!("fsd-ufs-rw: probe umount ok");

        let client = Client::new();
        svc_control(&client, "fsd-ufs", Op::Stop)?;
        wait_state(&client, "fsd-ufs", State::Stopped)?;
        raven_rt::println!("fsd-ufs-rw: probe server stopped");

        let mut disk = open_partition("block:0", 1)?;
        let report =
            check(&mut disk, Mode::ReadOnly).map_err(|error| format!("ufs-fsck: {error}"))?;
        if !report.clean_after {
            for finding in &report.findings {
                raven_rt::println!(
                    "fsd-ufs-rw: fsck phase={} problem={:?} inode={:?} block={:?}",
                    finding.phase,
                    finding.problem,
                    finding.inode,
                    finding.block
                );
            }
            return Err(format!(
                "ufs-fsck reported {} findings",
                report.findings.len()
            ));
        }
        raven_rt::println!("fsd-ufs-rw: ufs-fsck -n clean");

        svc_control(&client, "fsd-ufs", Op::Start)?;
        wait_state(&client, "fsd-ufs", State::Running)?;
        raven_rt::println!("fsd-ufs-rw: probe server restarted");

        fs::mount("/ufs", "ufs:").map_err(|error| format!("remount: {}", error.description()))?;
        let data = fs::read_to_vec(PERSIST_PATH)
            .map_err(|error| format!("read persisted: {}", error.description()))?;
        if data.as_slice() != b"persist\n" {
            return Err(format!("persisted file has {} bytes", data.len()));
        }
        let entries = fs::read_dir("file:/ufs")
            .map_err(|error| format!("remount readdir: {}", error.description()))?;
        if entries.iter().any(|entry| entry.name == "rwdir") {
            return Err("rwdir reappeared after remount".to_string());
        }
        fs::umount("/ufs").map_err(|error| format!("remount umount: {}", error.description()))?;
        raven_rt::println!("fsd-ufs-rw: probe remount ok");
        Ok(())
    }

    fn svc_control(client: &Client, name: &str, operation: Op) -> Result<(), String> {
        let path = format!("svc:/{name}");
        let handle = client
            .open(&path, 1)
            .map_err(|error| format!("{name}: {}", error.description()))?;
        let mut response = [0u8; 1];
        let result = client.call(handle, &[operation.as_u8()], &mut response);
        let _ = client.close(handle);
        result
            .map(|_| ())
            .map_err(|error| format!("{name}: {}", error.description()))
    }

    fn wait_state(client: &Client, name: &str, wanted: State) -> Result<(), String> {
        for _ in 0..200 {
            if service_state(client, name)? == wanted {
                return Ok(());
            }
            let _ = raven_rt::nanosleep(10_000_000);
        }
        Err(format!("{name}: timed out waiting for {:?}", wanted))
    }

    fn service_state(client: &Client, name: &str) -> Result<State, String> {
        let path = format!("svc:/{name}");
        let handle = client
            .open(&path, 1)
            .map_err(|error| format!("{name}: {}", error.description()))?;
        let mut buffer = [0u8; 128];
        let read = client
            .read(handle, 0, &mut buffer)
            .map_err(|error| format!("{name}: {}", error.description()))?;
        let _ = client.close(handle);
        let line = buffer
            .get(..read)
            .ok_or_else(|| format!("{name}: short status"))?;
        let status = Status::decode(line).map_err(|_| format!("{name}: bad status line"))?;
        Ok(status.state)
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
