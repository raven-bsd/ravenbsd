// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K68 service lifecycle fixture.
//!
//! `worker` registers the `k68:` scheme and sleeps; `crash` exits immediately; `client` drives
//! stop/start/restart through `svc:` and checks the PROCESS termination authorization matrix.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::format;
    use alloc::string::String;
    use alloc::vec::Vec;
    use core::sync::atomic::{AtomicI32, Ordering};

    use raven_abi::errno::Errno;
    use raven_abi::types::{Handle, Rights};
    use raven_initfs::Initfs;
    use raven_proto_svc::{State, Status};
    use raven_rt::entry;
    use raven_rt::spawn::{RtSys, SpawnOptions, Sys, spawn_with};
    use raven_scheme::{Client, Ctx, Handler, HandlerResult, Opened, Server};

    entry!(main);

    const READ: u32 = 1;
    const POLLS: usize = 500;
    const POLL_NS: u64 = 10_000_000;
    const GRANTED_INITFS: Handle = Handle::new(3, 0);
    const CLIENT_STACK: usize = 256 * 1024;
    static CLIENT_STATUS: AtomicI32 = AtomicI32::new(1);

    struct ReadOnlyScheme;

    impl Handler for ReadOnlyScheme {
        fn open(
            &mut self,
            _path: &str,
            _flags: u32,
            _mode: u32,
            _ctx: &Ctx,
        ) -> HandlerResult<Opened> {
            Ok(Opened::File(1))
        }

        fn read(&mut self, _file: u64, _offset: u64, _buffer: &mut [u8]) -> HandlerResult<usize> {
            Ok(0)
        }

        fn close(&mut self, _file: u64) -> HandlerResult<()> {
            Ok(())
        }
    }

    /// Use the manifest grant at handle 3 for executable mappings; a service child's handle 1 is
    /// READ|MAP only.
    struct GrantSys;

    impl Sys for GrantSys {
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

    fn main(args: raven_rt::Args) -> i32 {
        let mode = args.get(0).unwrap_or("client");
        match mode {
            "worker" => worker(),
            "crash" => 1,
            "client" => {
                let thread = match raven_rt::thread::spawn(client_thread, 0, CLIENT_STACK) {
                    Ok(thread) => thread,
                    Err(error) => {
                        raven_rt::eprintln!("k68: client spawn: {}", error.description());
                        return 1;
                    }
                };
                while !thread.is_finished() {
                    let _ = raven_rt::nanosleep(1_000_000);
                }
                // Thread::join races the worker's thread exit; the process exits right after.
                core::mem::forget(thread);
                let status = CLIENT_STATUS.load(Ordering::Acquire);
                if status == 0 {
                    raven_rt::println!("k68: ok");
                }
                status
            }
            _ => 2,
        }
    }

    fn client_thread(_arg: usize) {
        let status = match client() {
            Ok(()) => 0,
            Err(message) => {
                raven_rt::eprintln!("k68: {message}");
                1
            }
        };
        CLIENT_STATUS.store(status, Ordering::Release);
    }

    fn worker() -> i32 {
        let Ok(mut server) = Server::register("k68") else {
            return 1;
        };
        let mut handler = ReadOnlyScheme;
        server.run(&mut handler)
    }

    fn client() -> Result<(), String> {
        let image = raven_rt::map_initfs().map_err(|error| format!("initfs: {error:?}"))?;
        let initfs = Initfs::parse(image).map_err(|error| format!("initfs parse: {error:?}"))?;
        let client = Client::new();
        expect_state(&client, "worker", &[State::Running])?;

        svcctl(&initfs, &["stop", "worker"], &[0])?;
        expect_state(&client, "worker", &[State::Stopped])?;
        svcctl(&initfs, &["start", "worker"], &[0])?;
        expect_state(&client, "worker", &[State::Running])?;

        svcctl(&initfs, &["stop", "worker"], &[0])?;
        expect_state(&client, "worker", &[State::Stopped])?;

        let child = RtSys
            .proc_create()
            .map_err(|error| format!("proc_create: {error:?}"))?;
        let weak = raven_rt::handle_dup(child, Rights::READ)
            .map_err(|error| format!("handle_dup: {error:?}"))?;
        match raven_rt::proc_terminate(weak, 0) {
            Err(Errno::EPERM) => {}
            other => return Err(alloc::format!("weak terminate: {other:?}")),
        }
        raven_rt::proc_terminate(child, 0).map_err(|error| format!("terminate: {error:?}"))?;
        match raven_rt::proc_terminate(child, 0) {
            Err(Errno::ESRCH) => {}
            other => return Err(alloc::format!("second terminate: {other:?}")),
        }
        let _ = raven_rt::handle_close(weak);
        let _ = raven_rt::handle_close(child);

        // The crash service is a one-shot failure; restarting it must fail cleanly, not fight a
        // stale scheme name.
        expect_state(
            &client,
            "crash",
            &[State::GaveUp, State::Failed, State::Stopped],
        )?;
        svcctl(&initfs, &["start", "crash"], &[0, 1])?;
        expect_state(
            &client,
            "crash",
            &[State::GaveUp, State::Failed, State::Stopped],
        )?;

        svcctl(&initfs, &["stop", "worker"], &[0])?;
        expect_state(&client, "worker", &[State::Stopped])?;
        Ok(())
    }

    fn svcctl(initfs: &Initfs<'_>, args: &[&str], allowed: &[i32]) -> Result<(), String> {
        let mut argv = Vec::with_capacity(args.len() + 1);
        argv.push("svcctl");
        argv.extend_from_slice(args);
        let child = spawn_with(
            initfs,
            "/bin/svcctl",
            &SpawnOptions {
                args: &argv,
                env: &[],
            },
            &mut GrantSys,
        )
        .map_err(|error| format!("svcctl spawn: {error:?}"))?;
        let mut code = -1i32;
        RtSys
            .proc_wait(child, &mut code)
            .map_err(|error| format!("svcctl wait: {error:?}"))?;
        if allowed.contains(&code) {
            Ok(())
        } else {
            Err(format!("svcctl exit {code}"))
        }
    }

    fn expect_state(client: &Client, name: &str, expected: &[State]) -> Result<(), String> {
        for _ in 0..POLLS {
            let state = state(client, name)?;
            if expected.contains(&state) {
                return Ok(());
            }
            let _ = raven_rt::nanosleep(POLL_NS);
        }
        Err(format!("{name}: state timeout"))
    }

    fn state(client: &Client, name: &str) -> Result<State, String> {
        let handle = open(client, name)?;
        let mut buffer = [0u8; 128];
        let read = client
            .read(handle, 0, &mut buffer)
            .map_err(|error| format!("{name}: {}", error.description()))?;
        let _ = client.close(handle);
        let line = buffer
            .get(..read)
            .ok_or_else(|| format!("{name}: short read"))?;
        let status = Status::decode(line).map_err(|_| format!("{name}: bad status"))?;
        Ok(status.state)
    }

    fn open(client: &Client, name: &str) -> Result<Handle, String> {
        let path = alloc::format!("svc:/{name}");
        client
            .open(&path, READ)
            .map_err(|error| format!("{name}: {}", error.description()))
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
