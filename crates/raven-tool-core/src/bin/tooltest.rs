// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! One-shot integration client for the base file tools.
//!
//! The service spawns every tool through `raven_rt::spawn`, waits for its exit status and checks
//! the resulting filesystem state through `raven_rt::fs`. It prints `tooltest: ok` only after the
//! whole sequence succeeds.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use core::sync::atomic::{AtomicI32, Ordering};

    use alloc::string::String;

    use raven_abi::types::Handle;
    use raven_initfs::Initfs;
    use raven_rt::entry;
    use raven_rt::fs;
    use raven_rt::spawn::{RtSys, SpawnOptions, Sys, spawn_with};
    use raven_tool_core::Errno;
    use raven_tool_core::error::line;
    use raven_tool_core::path::with_file_scheme;

    entry!(main);

    const ROOT: &str = "/tmp/k38";
    const DIR: &str = "/tmp/k38/a";
    /// `svcd` installs the manifest's `grants = ["initfs"]` before the first thread; the child
    /// already owns handles 0..=2, so the transfer lands at index 3.
    const GRANTED_INITFS: Handle = Handle::new(3, 0);
    /// `raven_rt::spawn` builds the child's stack in a 64 KiB buffer on the caller's stack.
    /// Run the test from a helper thread with enough room.
    const WORKER_STACK: usize = 256 * 1024;

    static STATUS: AtomicI32 = AtomicI32::new(1);

    /// Delegate to `RtSys`, but map the executable segments from the child's granted initfs
    /// handle (which carries `EXEC`). The well-known handle 1 only carries `READ|MAP` in a service
    /// child.
    struct ToolSys;

    impl Sys for ToolSys {
        fn proc_create(&mut self) -> Result<Handle, Errno> {
            RtSys.proc_create()
        }

        fn memobj_create(&mut self, len: usize) -> Result<Handle, Errno> {
            RtSys.memobj_create(len)
        }

        fn map(
            &mut self,
            proc: Handle,
            memobj: Handle,
            offset: usize,
            addr: usize,
            len: usize,
            flags: usize,
        ) -> Result<usize, Errno> {
            let source = if memobj == Handle::INITFS {
                GRANTED_INITFS
            } else {
                memobj
            };
            RtSys.map(proc, source, offset, addr, len, flags)
        }

        fn unmap(&mut self, proc: Handle, addr: usize, len: usize) -> Result<(), Errno> {
            RtSys.unmap(proc, addr, len)
        }

        fn thread_create(
            &mut self,
            proc: Handle,
            entry: usize,
            stack_top: usize,
            arg: usize,
        ) -> Result<Handle, Errno> {
            RtSys.thread_create(proc, entry, stack_top, arg)
        }

        fn handle_close(&mut self, handle: Handle) -> Result<(), Errno> {
            RtSys.handle_close(handle)
        }

        fn write_parent_mem(&mut self, addr: usize, bytes: &[u8]) {
            RtSys.write_parent_mem(addr, bytes);
        }
    }

    fn main(_args: raven_rt::Args) -> i32 {
        let thread = match raven_rt::thread::spawn(worker, 0, WORKER_STACK) {
            Ok(thread) => thread,
            Err(error) => {
                raven_rt::eprintln!("tooltest: worker spawn: {}", error.description());
                return 1;
            }
        };
        while !thread.is_finished() {
            let _ = raven_rt::nanosleep(1_000_000);
        }
        // `Thread::join` unmaps the worker stack as soon as the done word is set, racing the
        // worker's futex wake/thread exit (the K11 join limitation). The process is about to exit,
        // so leak the stack and control block instead.
        core::mem::forget(thread);
        let status = STATUS.load(Ordering::Acquire);
        if status == 0 {
            raven_rt::println!("tooltest: ok");
        }
        status
    }

    fn worker(_arg: usize) {
        let status = match run() {
            Ok(()) => 0,
            Err(message) => {
                raven_rt::eprintln!("tooltest: {message}");
                1
            }
        };
        STATUS.store(status, Ordering::Release);
    }

    fn run() -> Result<(), String> {
        let image = raven_rt::map_initfs().map_err(|error| line("tooltest", "initfs", error))?;
        let initfs = Initfs::parse(image).map_err(|error| alloc::format!("initfs: {error:?}"))?;

        fs::mkdir(ROOT, 0o755).map_err(|error| line("tooltest", ROOT, error))?;
        let mounts = fs::read_to_vec("file:/etc/mounts")
            .map_err(|error| line("tooltest", "file:/etc/mounts", error))?;
        if mounts.is_empty() {
            return Err("/etc/mounts is empty".into());
        }
        stat_ok("/etc/mounts")?;
        spawn_ok(&initfs, "mkdir", &["-p", DIR])?;
        stat_ok(DIR)?;
        spawn_ok(&initfs, "touch", &["/tmp/k38/a/file"])?;
        stat_ok("/tmp/k38/a/file")?;

        spawn_ok(&initfs, "cp", &["/etc/mounts", "/tmp/k38/a/mounts"])?;
        stat_ok("/tmp/k38/a/mounts")?;
        compare("/etc/mounts", "/tmp/k38/a/mounts")?;

        spawn_ok(&initfs, "cat", &["/tmp/k38/a/mounts"])?;
        spawn_ok(&initfs, "ls", &["/tmp/k38/a"])?;
        spawn_ok(&initfs, "stat", &["/tmp/k38/a/mounts"])?;
        spawn_ok(&initfs, "df", &[])?;

        spawn_ok(&initfs, "mv", &["/tmp/k38/a/mounts", "/tmp/k38/a/renamed"])?;
        stat_ok("/tmp/k38/a/renamed")?;
        match fs::stat(&with_file_scheme("/tmp/k38/a/mounts")) {
            Err(Errno::ENOENT) => {}
            _ => return Err("mv left the old name behind".into()),
        }

        spawn_ok(&initfs, "rm", &["-r", DIR])?;
        match fs::stat(&with_file_scheme(DIR)) {
            Err(Errno::ENOENT) => {}
            _ => return Err("rm left the tree behind".into()),
        }
        spawn_ok(&initfs, "rmdir", &[ROOT])?;
        match fs::stat(&with_file_scheme(ROOT)) {
            Err(Errno::ENOENT) => {}
            _ => return Err("rmdir left the root behind".into()),
        }
        Ok(())
    }

    fn spawn_ok(initfs: &Initfs<'_>, name: &str, args: &[&str]) -> Result<(), String> {
        let path = alloc::format!("/bin/{name}");
        let mut argv = alloc::vec::Vec::with_capacity(args.len() + 1);
        argv.push(name);
        argv.extend_from_slice(args);
        let process = spawn_with(
            initfs,
            &path,
            &SpawnOptions {
                args: &argv,
                env: &[],
            },
            &mut ToolSys,
        )
        .map_err(|error| alloc::format!("spawn {name}: {error:?}"))?;
        let mut code = -1i32;
        RtSys
            .proc_wait(process, &mut code)
            .map_err(|error| line("tooltest", &path, error))?;
        if code != 0 {
            return Err(alloc::format!("{name} exited {code}"));
        }
        Ok(())
    }

    fn stat_ok(path: &str) -> Result<(), String> {
        fs::stat(&with_file_scheme(path))
            .map(|_| ())
            .map_err(|error| line("tooltest", path, error))
    }

    fn compare(left: &str, right: &str) -> Result<(), String> {
        let left_data = fs::read_to_vec(&with_file_scheme(left))
            .map_err(|error| line("tooltest", left, error))?;
        let right_data = fs::read_to_vec(&with_file_scheme(right))
            .map_err(|error| line("tooltest", right, error))?;
        if left_data == right_data {
            Ok(())
        } else {
            Err("copied file differs".into())
        }
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
