// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K480 guest fixture: a handlerless `THREAD_KICK` must not look like a service exit.
//!
//! The fixture is PID 1. It starts a child service, blocks the real
//! `raven_init::worker::wait_child_status` waiter in `PROC_WAIT`, kicks the waiter with
//! `THREAD_KICK`, and feeds the result into the real `Supervisor` state machine. A correct
//! waiter stays blocked and the service state remains `Running`; the old `else { 1 }` path
//! publishes exit 1 and the supervisor restarts a live service.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![allow(unsafe_code)] // raw `raven_rt::sys` calls carry SAFETY comments below.

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use core::sync::atomic::{AtomicI32, AtomicU32, Ordering};

    use raven_abi::errno::Errno;
    use raven_abi::types::Handle;
    use raven_init::state::Supervisor;
    use raven_init::{WAIT_ERROR_CODE, wait_child_status};
    use raven_initfs::Initfs;
    use raven_proto_svc::State;
    use raven_rt::entry;
    use raven_rt::spawn::{RtSys, SpawnOptions, Sys, spawn_with};
    use raven_rt::sys;
    use raven_svc_manifest::Manifest;

    entry!(main);

    const PAGE: usize = 4096;
    const POLL_MS: usize = 50;
    const THREAD_KICK: usize = 0x81;
    const PROC_HANDLE_INFO: usize = 0x3E;
    const PROC_SIGNAL: usize = 0x4A;
    const SIGKILL: i32 = 9;

    static WAITER_DONE: AtomicU32 = AtomicU32::new(0);
    static WAITER_CODE: AtomicI32 = AtomicI32::new(0);

    fn checked(raw: isize) -> Result<usize, Errno> {
        if raw < 0 {
            Err(Errno(raw.unsigned_abs() as u16))
        } else {
            Ok(raw as usize)
        }
    }

    fn kick(thread: Handle, bits: u32) -> Result<(), Errno> {
        // SAFETY: THREAD_KICK takes the caller's thread handle and a scalar kick word; the
        // kernel validates the handle, rights and bits.
        checked(unsafe { sys::syscall2(THREAD_KICK, thread.0 as usize, bits as usize) }).map(|_| ())
    }

    fn handle_pid(handle: Handle) -> Result<i32, Errno> {
        // SAFETY: PROC_HANDLE_INFO takes a scalar process handle and writes no user memory.
        checked(unsafe { sys::syscall2(PROC_HANDLE_INFO, handle.0 as usize, 0) })
            .and_then(|pid| i32::try_from(pid).map_err(|_| Errno::EINVAL))
    }

    fn signal(pid: i32, signo: i32) -> Result<(), Errno> {
        // SAFETY: PROC_SIGNAL takes a pid and signal number; the kernel checks the caller's
        // permission against the target process.
        checked(unsafe { sys::syscall2(PROC_SIGNAL, pid as usize, signo as usize) }).map(|_| ())
    }

    fn sleep_ms(ms: usize) {
        let _ = sys::nanosleep((ms as u64).saturating_mul(1_000_000));
    }

    fn service() -> i32 {
        loop {
            let _ = sys::nanosleep(1_000_000_000);
        }
    }

    extern "C" fn waiter(arg: usize) -> ! {
        let code = match wait_child_status(Handle(arg as u32)) {
            Ok(code) => code,
            Err(_) => WAIT_ERROR_CODE,
        };
        WAITER_CODE.store(code, Ordering::Release);
        WAITER_DONE.store(1, Ordering::Release);
        sys::thread_exit(0)
    }

    struct SpawnSys;

    impl Sys for SpawnSys {
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
            RtSys.map(proc, memobj, offset, addr, len, flags)
        }

        fn unmap(&mut self, proc: Handle, addr: usize, len: usize) -> Result<(), Errno> {
            RtSys.unmap(proc, addr, len)
        }

        fn protect(
            &mut self,
            proc: Handle,
            addr: usize,
            len: usize,
            protection: u8,
        ) -> Result<(), Errno> {
            RtSys.protect(proc, addr, len, protection)
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
            RtSys.write_parent_mem(addr, bytes)
        }
    }

    fn spawn_service(initfs: &Initfs<'_>) -> Result<Handle, Errno> {
        let options = SpawnOptions {
            args: &["init", "service"],
            env: &[],
        };
        let mut spawer = SpawnSys;
        spawn_with(initfs, "/init", &options, &mut spawer).map_err(|_| Errno::EIO)
    }

    fn spawn_waiter(child: Handle) -> Result<Handle, Errno> {
        let stack = sys::map_anonymous(2 * PAGE)?;
        sys::thread_create(
            Handle::SELF_PROC,
            waiter as *const () as usize,
            stack + 2 * PAGE,
            child.0 as usize,
        )
    }

    fn wait_done(ms: usize) -> Option<i32> {
        for _ in 0..ms {
            if WAITER_DONE.load(Ordering::Acquire) != 0 {
                return Some(WAITER_CODE.load(Ordering::Acquire));
            }
            sleep_ms(1);
        }
        None
    }

    fn make_supervisor(child: Handle) -> (Supervisor, u64) {
        let text = "name = \"svc\"\nexec = \"/init\"\nargs = [\"service\"]\n";
        let manifest = Manifest::parse(text).expect("manifest");
        let mut supervisor = Supervisor::new(alloc::vec![manifest]).expect("supervisor");
        let generation = supervisor.start(0, 0).expect("start");
        let notices = supervisor.spawned(0, generation, child, 0);
        assert!(
            notices
                .iter()
                .any(|notice| matches!(notice, raven_init::state::Notice::Up(0)))
        );
        (supervisor, generation)
    }

    fn controller() -> i32 {
        let image = match sys::map_initfs() {
            Ok(image) => image,
            Err(_) => {
                raven_rt::println!("init-worker-eintr: initfs map failed");
                return 1;
            }
        };
        let initfs = match Initfs::parse(image) {
            Ok(initfs) => initfs,
            Err(_) => {
                raven_rt::println!("init-worker-eintr: initfs parse failed");
                return 1;
            }
        };

        let child = match spawn_service(&initfs) {
            Ok(child) => child,
            Err(_) => {
                raven_rt::println!("init-worker-eintr: spawn failed");
                return 1;
            }
        };
        let (mut supervisor, generation) = make_supervisor(child);
        let waiter = match spawn_waiter(child) {
            Ok(waiter) => waiter,
            Err(_) => {
                raven_rt::println!("init-worker-eintr: waiter create failed");
                return 1;
            }
        };
        sleep_ms(POLL_MS);
        if kick(waiter, 1).is_err() {
            raven_rt::println!("init-worker-eintr: kick failed");
            return 1;
        }
        sleep_ms(POLL_MS);

        match wait_done(100) {
            Some(code) => {
                // The old product path publishes 1 for the interrupted wait. The supervisor then
                // restarts a service that is still alive; this is the red row.
                raven_rt::println!("init-worker-eintr: kick=exit{code}");
                let outcome = supervisor.exited(0, generation, code, POLL_MS as u64);
                if outcome.restart.is_some() {
                    raven_rt::println!("init-worker-eintr: restart=yes");
                } else {
                    raven_rt::println!("init-worker-eintr: restart=no");
                }
                let _ = sys::proc_terminate(child, 0);
                raven_rt::println!("init-worker-eintr: fail");
                return 1;
            }
            None => raven_rt::println!("init-worker-eintr: kick=retry"),
        }

        if supervisor.runtime()[0].state != State::Running {
            raven_rt::println!(
                "init-worker-eintr: state={:?}",
                supervisor.runtime()[0].state
            );
            return 1;
        }
        raven_rt::println!("init-worker-eintr: state=running");

        if sys::proc_terminate(child, 7).is_err() {
            raven_rt::println!("init-worker-eintr: exit7 terminate failed");
            return 1;
        }
        match wait_done(500) {
            Some(7) => raven_rt::println!("init-worker-eintr: exit=7"),
            Some(code) => {
                raven_rt::println!("init-worker-eintr: exit={code} expected=7");
                return 1;
            }
            None => {
                raven_rt::println!("init-worker-eintr: exit7 waiter blocked");
                return 1;
            }
        }
        let outcome = supervisor.exited(0, generation, 7, 100);
        if outcome.restart.is_none() || supervisor.runtime()[0].last_exit != Some(7) {
            raven_rt::println!("init-worker-eintr: supervisor exit7 wrong");
            return 1;
        }
        raven_rt::println!("init-worker-eintr: exit7-restart=yes");

        let child2 = match spawn_service(&initfs) {
            Ok(child) => child,
            Err(_) => {
                raven_rt::println!("init-worker-eintr: second spawn failed");
                return 1;
            }
        };
        let generation2 = supervisor.start(0, 200).expect("second start");
        supervisor.spawned(0, generation2, child2, 200);
        WAITER_DONE.store(0, Ordering::Release);
        WAITER_CODE.store(0, Ordering::Release);
        let _waiter2 = match spawn_waiter(child2) {
            Ok(waiter) => waiter,
            Err(_) => {
                raven_rt::println!("init-worker-eintr: second waiter failed");
                return 1;
            }
        };
        sleep_ms(POLL_MS);
        let pid = match handle_pid(child2) {
            Ok(pid) => pid,
            Err(_) => {
                raven_rt::println!("init-worker-eintr: child pid failed");
                return 1;
            }
        };
        if signal(pid, SIGKILL).is_err() {
            raven_rt::println!("init-worker-eintr: SIGKILL failed");
            return 1;
        }
        match wait_done(500) {
            Some(137) => raven_rt::println!("init-worker-eintr: killed=137"),
            Some(code) => {
                raven_rt::println!("init-worker-eintr: killed={code} expected=137");
                return 1;
            }
            None => {
                raven_rt::println!("init-worker-eintr: killed waiter blocked");
                return 1;
            }
        }
        let killed = supervisor.exited(0, generation2, 137, 300);
        if killed.restart.is_none() || supervisor.runtime()[0].last_exit != Some(137) {
            raven_rt::println!("init-worker-eintr: supervisor killed wrong");
            return 1;
        }
        raven_rt::println!("init-worker-eintr: killed-restart=yes");
        raven_rt::println!("init-worker-eintr: ok");
        0
    }

    fn main(args: raven_rt::Args) -> i32 {
        match args.get(1) {
            Some("service") => service(),
            _ => controller(),
        }
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
