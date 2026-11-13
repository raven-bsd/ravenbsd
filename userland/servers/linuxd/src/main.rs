// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `linuxd`: the RavenBSD Linux personality server.
//!
//! The skeleton registers `linux:`, loads one static Linux image from the initfs, maps it with
//! `linux-elf`, marks the new process with `PROC_SET_PERSONALITY` and serves its traps. It also
//! prints the dispatcher statistics before it exits, which is what the `linux-hello` qtest reads.
//!
//! Run as `/init` (no arguments) it serves `/hello`; a first argument selects another initfs path.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod raw;

/// `linuxd::Sys` over the real `raven_rt` syscalls.
///
/// Empty on purpose: `RtSys` is stateless, and the adapter exists so the loader and its tests never
/// link `raven-rt`'s `_start` into a host test binary (the crate's public surface stays host-safe).
#[cfg(target_os = "none")]
pub(crate) struct KernelSys;

#[cfg(target_os = "none")]
impl linuxd::Sys for KernelSys {
    fn proc_create(&mut self) -> Result<raven_abi::types::Handle, raven_abi::errno::Errno> {
        let mut sys = raven_rt::spawn::RtSys;
        raven_rt::spawn::Sys::proc_create(&mut sys)
    }

    fn memobj_create(
        &mut self,
        len: usize,
    ) -> Result<raven_abi::types::Handle, raven_abi::errno::Errno> {
        let mut sys = raven_rt::spawn::RtSys;
        raven_rt::spawn::Sys::memobj_create(&mut sys, len)
    }

    fn map(
        &mut self,
        proc: raven_abi::types::Handle,
        object: raven_abi::types::Handle,
        offset: usize,
        addr: usize,
        len: usize,
        flags: usize,
    ) -> Result<usize, raven_abi::errno::Errno> {
        let mut sys = raven_rt::spawn::RtSys;
        raven_rt::spawn::Sys::map(&mut sys, proc, object, offset, addr, len, flags)
    }

    fn unmap(
        &mut self,
        proc: raven_abi::types::Handle,
        addr: usize,
        len: usize,
    ) -> Result<(), raven_abi::errno::Errno> {
        let mut sys = raven_rt::spawn::RtSys;
        raven_rt::spawn::Sys::unmap(&mut sys, proc, addr, len)
    }

    fn protect(
        &mut self,
        proc: raven_abi::types::Handle,
        addr: usize,
        len: usize,
        protection: u8,
    ) -> Result<(), raven_abi::errno::Errno> {
        let mut sys = raven_rt::spawn::RtSys;
        raven_rt::spawn::Sys::protect(&mut sys, proc, addr, len, protection)
    }

    fn handle_close(
        &mut self,
        handle: raven_abi::types::Handle,
    ) -> Result<(), raven_abi::errno::Errno> {
        let mut sys = raven_rt::spawn::RtSys;
        raven_rt::spawn::Sys::handle_close(&mut sys, handle)
    }

    fn write_parent_mem(&mut self, addr: usize, bytes: &[u8]) {
        let mut sys = raven_rt::spawn::RtSys;
        raven_rt::spawn::Sys::write_parent_mem(&mut sys, addr, bytes);
    }
}

#[cfg(not(target_os = "none"))]
fn main() {
    println!("linuxd: a RavenBSD server; build it for x86_64-unknown-none");
}

#[cfg(target_os = "none")]
mod program {
    use alloc::vec::Vec;

    use linuxd::{
        LinuxProcess, TrapAction, copy_in_len, decode, dispatch, dispatcher, encode_reply, load,
        plan_image,
    };
    use raven_abi::errno::Errno;
    use raven_abi::types::{Handle, clock};
    use raven_initfs::Initfs;
    use raven_ipc::protocol::{self, Opcode, PACKET_SIZE, SchemePacket};
    use raven_linux_dispatch::Dispatcher;
    use raven_proto_linux::REPLY_LEN;
    use raven_rt::entry;
    use raven_rt::println;
    use raven_rt::spawn::RtSys;
    use raven_rt::sys;

    use crate::{KernelSys, raw};

    entry!(main);

    /// A Linux process that stops before its first syscall is a failure, not a hang.
    const WATCHDOG_SECS: u64 = 20;

    fn main(args: raven_rt::Args) -> i32 {
        let path = args.get(1).unwrap_or("/hello");
        let image = match image_from_initfs(path) {
            Ok(image) => image,
            Err(code) => return code,
        };
        let config = linuxd::Config {
            main_pie_base: 0,
            interp_base: 0,
            page: 4096,
            uid: 0,
            euid: 0,
            gid: 0,
            egid: 0,
            secure: false,
            hwcap: 0,
            platform: "x86_64",
            clktck: 100,
            sysinfo_ehdr: None,
            random: random_bytes(),
        };
        let spec = match plan_image(
            &image,
            &config,
            &[path.as_bytes()],
            &[],
            path.as_bytes(),
            raven_abi::layout::x86_64::USER_END,
        ) {
            Ok(spec) => spec,
            Err(error) => {
                println!("linuxd: image plan failed: {error:?}");
                return 2;
            }
        };
        println!(
            "linuxd: entry={:#x} phdr={:#x} phnum={} brk={:#x}",
            spec.entry, spec.phdr, spec.phnum, spec.brk_start
        );

        let mut sys = KernelSys;
        let loaded = match load(&spec, &image, &mut sys) {
            Ok(loaded) => loaded,
            Err(error) => {
                println!("linuxd: load failed: {error:?}");
                return 3;
            }
        };
        println!(
            "linuxd: mapped {} regions, sp={:#x}",
            loaded.regions.len(),
            loaded.sp
        );

        let handler = match raw::scheme_register("linux") {
            Ok(handle) => handle,
            Err(error) => {
                println!("linuxd: scheme register failed: {}", error.description());
                return 4;
            }
        };
        if let Err(error) = raw::set_personality(loaded.proc, handler) {
            println!(
                "linuxd: PROC_SET_PERSONALITY failed: {}",
                error.description()
            );
            return 5;
        }
        let entry = match usize::try_from(loaded.entry) {
            Ok(entry) => entry,
            Err(_) => return 6,
        };
        let stack = match usize::try_from(loaded.sp) {
            Ok(stack) => stack,
            Err(_) => return 6,
        };
        if let Err(error) = sys::thread_create(loaded.proc, entry, stack, 0) {
            println!("linuxd: thread create failed: {}", error.description());
            return 6;
        }

        let _watchdog = raven_rt::thread::spawn(watchdog, 0, 64 * 1024);
        serve(sys, loaded, handler)
    }

    fn image_from_initfs(path: &str) -> Result<Vec<u8>, i32> {
        let mapped = match sys::map_initfs() {
            Ok(mapped) => mapped,
            Err(error) => {
                println!("linuxd: initfs map failed: {}", error.description());
                return Err(2);
            }
        };
        let initfs = match Initfs::parse(mapped) {
            Ok(initfs) => initfs,
            Err(_) => {
                println!("linuxd: initfs parse failed");
                return Err(2);
            }
        };
        match initfs.get(path) {
            Some(entry) => Ok(entry.data.to_vec()),
            None => {
                println!("linuxd: {path} is not in the initfs");
                Err(2)
            }
        }
    }

    fn random_bytes() -> [u8; 16] {
        let mut bytes = [0u8; 16];
        if let Ok(now) = sys::clock_get(clock::MONOTONIC) {
            let seed = (now.tv_sec as u64)
                .rotate_left(32)
                .wrapping_add(now.tv_nsec as u64)
                .wrapping_mul(0x9E37_79B9_7F4A_7C15);
            bytes[..8].copy_from_slice(&seed.to_le_bytes());
            bytes[8..].copy_from_slice(&seed.rotate_left(17).to_le_bytes());
        }
        bytes
    }

    fn serve(mut sys: KernelSys, loaded: linuxd::Loaded, handler: Handle) -> i32 {
        let mut dispatcher = dispatcher();
        let mut process = LinuxProcess::new(
            1,
            loaded.brk,
            loaded.brk_limit,
            loaded.mmap,
            loaded.mmap_limit,
        );
        process.regions = loaded.regions.clone();
        let mut bytes = [0u8; PACKET_SIZE];
        loop {
            let request = match sys::recv(&mut bytes) {
                Ok(request) => request,
                Err(_) => continue,
            };
            let Some(packet) = protocol::decode(&bytes).ok() else {
                continue;
            };
            if packet.opcode != Opcode::Call.as_u16() {
                // An external `open("linux:")` is answered here so the client does not block; the
                // personality handler is the scheme-server handle, which needs no open.
                let reply = ack(packet);
                let _ = sys::send(request, &reply);
                continue;
            }
            let call = match decode(packet.data_bytes()) {
                Ok(call) => call,
                Err(_) => {
                    let reply = reply_packet(packet, -(Errno::EPROTO.0 as i64));
                    let _ = sys::send(request, &reply);
                    continue;
                }
            };
            let exit = serve_one(
                &mut sys,
                &mut dispatcher,
                &mut process,
                &loaded,
                call,
                packet,
                request,
            );
            if let Some(code) = exit {
                print_stats(&dispatcher, handler);
                let _ = sys::proc_terminate(loaded.proc, code);
                let mut rt = RtSys;
                let mut status = code;
                let _ = rt.proc_wait(loaded.proc, &mut status);
                println!("linuxd: hello exit={status}");
                return 0;
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn serve_one(
        sys: &mut KernelSys,
        dispatcher: &mut Dispatcher,
        process: &mut LinuxProcess,
        loaded: &linuxd::Loaded,
        call: raven_proto_linux::LinuxSyscallV1,
        packet: SchemePacket,
        request: Handle,
    ) -> Option<i32> {
        process.user_buffer.clear();
        if let Some(len) = copy_in_len(&call) {
            process.user_buffer.resize(len, 0);
            if let Err(error) =
                raw::copy_in(sys, &loaded.regions, call.args[1], &mut process.user_buffer)
            {
                let reply = reply_packet(packet, -(error.0 as i64));
                let _ = sys::send(request, &reply);
                return None;
            }
        }
        let action = dispatch(process, dispatcher, &call);
        // The skeleton traces every trap: the line shows which Linux syscall the fixture reached
        // and what it got back. B21+ can put it behind a quiet flag.
        match action {
            TrapAction::Reply(value) => println!("linuxd: trap nr={} -> {value}", call.nr),
            TrapAction::Exit(code) => println!("linuxd: trap nr={} -> exit {code}", call.nr),
        }
        if !process.output.is_empty() {
            let output = process.take_output();
            let _ = sys::debug_write(&output);
        }
        if let Some((address, pending)) = process.pending_write.take()
            && raw::copy_out(sys, &loaded.regions, address, &pending).is_err()
        {
            let reply = reply_packet(packet, -(Errno::EFAULT.0 as i64));
            let _ = sys::send(request, &reply);
            return None;
        }
        match action {
            TrapAction::Reply(value) => {
                let packet = reply_packet(packet, value as i64);
                let _ = sys::send(request, &packet);
                None
            }
            TrapAction::Exit(code) => {
                let packet = reply_packet(packet, 0);
                let _ = sys::send(request, &packet);
                Some(code)
            }
        }
    }

    fn print_stats(dispatcher: &Dispatcher, handler: Handle) {
        println!("linuxd: handler={}", handler.0);
        for (name, calls, errors) in dispatcher.stats() {
            println!("linuxd: syscall {name} calls={calls} errors={errors}");
        }
        for (number, name, hits) in dispatcher.unimplemented_hits() {
            println!(
                "linuxd: unimplemented nr={number} name={} hits={hits}",
                name.unwrap_or("?")
            );
        }
    }

    fn ack(mut packet: SchemePacket) -> [u8; PACKET_SIZE] {
        packet.status = 0;
        packet.attach_count = 0;
        packet.data_len = 0;
        packet.buffer_len = 0;
        packet.response_len = 0;
        let mut out = [0u8; PACKET_SIZE];
        let _ = protocol::encode(&packet, &mut out);
        out
    }

    fn reply_packet(mut packet: SchemePacket, value: i64) -> [u8; PACKET_SIZE] {
        let mut encoded = [0u8; REPLY_LEN];
        let length = encode_reply(value, &mut encoded);
        packet.status = 0;
        packet.attach_count = 0;
        packet.data.fill(0);
        packet.data[..length].copy_from_slice(&encoded[..length]);
        packet.data_len = length as u32;
        packet.buffer_len = 8;
        packet.response_len = REPLY_LEN as u64;
        let mut out = [0u8; PACKET_SIZE];
        let _ = protocol::encode(&packet, &mut out);
        out
    }

    fn watchdog(_: usize) {
        let mut slept = 0u64;
        while slept < WATCHDOG_SECS {
            let _ = sys::nanosleep(1_000_000_000);
            slept += 1;
        }
        println!(
            "linuxd: no Linux trap within {WATCHDOG_SECS}s; the fixture stopped before its first syscall"
        );
        sys::exit(1);
    }
}
