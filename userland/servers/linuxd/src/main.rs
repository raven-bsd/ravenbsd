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
    use alloc::string::String;
    use alloc::vec::Vec;

    use linuxd::{
        LinuxProcess, TrapAction, copy_in, decode, dispatch, dispatcher, encode_reply, load,
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
        let mut argv = Vec::new();
        argv.push(path.as_bytes().to_vec());
        for (index, argument) in args.iter().enumerate() {
            if index > 0 && argument != path {
                argv.push(argument.as_bytes().to_vec());
            }
        }
        let image = match image_from_initfs(path) {
            Ok(image) => image,
            Err(code) => return code,
        };
        let config = config();
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
        let mut tid = 0u32;
        let thread = match raw::thread_create_ex(loaded.proc, entry, stack, &mut tid) {
            Ok(thread) => thread,
            Err(error) => {
                println!("linuxd: thread create failed: {}", error.description());
                return 6;
            }
        };
        if tid == 0 {
            println!("linuxd: thread create returned no tid");
            return 6;
        }

        let _watchdog = raven_rt::thread::spawn(watchdog, 0, 64 * 1024);
        let trace = args.get(2).is_some_and(|argument| argument == "trace");
        let session = Session::new(1, tid, thread, loaded, path.as_bytes(), argv);
        serve(sys, session, handler, trace)
    }

    fn image_from_initfs(path: &str) -> Result<Vec<u8>, i32> {
        match image_bytes(path) {
            Ok(image) => Ok(image),
            Err(error) => {
                println!("linuxd: {path}: {}", error.description());
                Err(2)
            }
        }
    }

    /// Read one executable image from the initfs without printing; `execve` reports the errno.
    fn image_bytes(path: &str) -> Result<Vec<u8>, Errno> {
        let mapped = sys::map_initfs().map_err(|_| Errno::ENOENT)?;
        let initfs = Initfs::parse(mapped).map_err(|_| Errno::ENOEXEC)?;
        initfs
            .get(path)
            .map(|entry| entry.data.to_vec())
            .ok_or(Errno::ENOENT)
    }

    /// The fixed Linux configuration of this personality.
    fn config() -> linuxd::Config {
        linuxd::Config {
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

    /// The Linux process the server currently serves and the native mapping behind it.
    struct Session {
        loaded: linuxd::Loaded,
        process: LinuxProcess,
    }

    impl Session {
        fn new(
            pid: i32,
            tid: u32,
            thread: Handle,
            loaded: linuxd::Loaded,
            path: &[u8],
            argv: Vec<Vec<u8>>,
        ) -> Self {
            let mut process = LinuxProcess::new(
                pid,
                tid,
                thread,
                loaded.brk,
                loaded.brk_limit,
                loaded.mmap,
                loaded.mmap_limit,
            );
            process.regions = loaded.regions.clone();
            process.set_identity(path, argv);
            Self { loaded, process }
        }
    }

    /// What one served trap asks the loop to do next.
    enum Step {
        /// Reply sent; keep serving.
        Continue,
        /// The process exited with this status.
        Exit(i32),
    }

    fn serve(mut sys: KernelSys, mut session: Session, handler: Handle, trace: bool) -> i32 {
        let mut dispatcher = dispatcher();
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
            match serve_one(
                &mut sys,
                &mut dispatcher,
                &mut session,
                call,
                packet,
                request,
                trace,
                handler,
            ) {
                Step::Continue => {}
                Step::Exit(code) => {
                    print_stats(&dispatcher, handler);
                    let _ = sys::proc_terminate(session.loaded.proc, code);
                    let mut rt = RtSys;
                    let mut status = code;
                    let _ = rt.proc_wait(session.loaded.proc, &mut status);
                    println!("linuxd: hello exit={status}");
                    return 0;
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn serve_one(
        sys: &mut KernelSys,
        dispatcher: &mut Dispatcher,
        session: &mut Session,
        call: raven_proto_linux::LinuxSyscallV1,
        packet: SchemePacket,
        request: Handle,
        trace: bool,
        handler: Handle,
    ) -> Step {
        let process = &mut session.process;
        process.user_buffer.clear();
        if let Some((argument, len)) = copy_in(&call) {
            process.user_buffer.resize(len, 0);
            let source = call.args.get(argument).copied().unwrap_or(0);
            if let Err(error) = raw::copy_in(
                sys,
                &session.loaded.regions,
                source,
                &mut process.user_buffer,
            ) {
                let reply = reply_packet(packet, -(error.0 as i64));
                let _ = sys::send(request, &reply);
                return Step::Continue;
            }
        }
        let mut action = dispatch(process, dispatcher, &call);
        // The Linux thread is blocked in this trap, so a queued `ARCH_SET_FS` is installed in it
        // before the reply is sent. A failing native call replaces the success with `-errno`.
        if let Some(base) = process.pending_tls.take() {
            match raw::set_tls(process.thread, base) {
                Ok(()) => process.fs_base = base,
                Err(error) => action = TrapAction::Reply(-(error.0 as isize)),
            }
        }
        // The per-trap trace is opt-in (`linuxd /hello trace`): the B30 fixture emits its own
        // lines and an interleaved trace would split them. Statistics are always printed at exit.
        if trace {
            match action {
                TrapAction::Reply(value) => println!("linuxd: trap nr={} -> {value}", call.nr),
                TrapAction::Exit(code) => println!("linuxd: trap nr={} -> exit {code}", call.nr),
                TrapAction::Exec => println!("linuxd: trap nr={} -> exec", call.nr),
            }
        }
        if !process.output.is_empty() {
            let output = process.take_output();
            let _ = sys::debug_write(&output);
        }
        if let Some((address, pending)) = process.pending_write.take()
            && raw::copy_out(sys, &session.loaded.regions, address, &pending).is_err()
        {
            let reply = reply_packet(packet, -(Errno::EFAULT.0 as i64));
            let _ = sys::send(request, &reply);
            return Step::Continue;
        }
        match action {
            TrapAction::Reply(value) => {
                let packet = reply_packet(packet, value as i64);
                let _ = sys::send(request, &packet);
                Step::Continue
            }
            TrapAction::Exit(code) => {
                let packet = reply_packet(packet, 0);
                let _ = sys::send(request, &packet);
                Step::Exit(code)
            }
            TrapAction::Exec => match exec_image(sys, session, handler) {
                Ok(new_session) => {
                    terminate_old(sys, session);
                    println!(
                        "linuxd: exec path={} pid={} tid={}",
                        String::from_utf8_lossy(&new_session.process.exe),
                        new_session.process.pid,
                        new_session.process.tid
                    );
                    *session = new_session;
                    Step::Continue
                }
                Err(error) => {
                    let reply = reply_packet(packet, -(error.0 as i64));
                    let _ = sys::send(request, &reply);
                    Step::Continue
                }
            },
        }
    }

    /// Largest `argv`/`envp` array and string `execve` accepts from the caller.
    const MAX_EXEC_ARGS: usize = 64;
    /// Largest total `argv`+`envp` bytes read for one `execve`.
    const MAX_EXEC_BYTES: usize = 64 * 1024;
    /// Largest single `argv`/`envp` string.
    const MAX_EXEC_STRING: usize = 4096;

    /// Build the replacement process for a queued `execve`.
    ///
    /// The new image gets a fresh native process, the same personality handler and a first thread.
    /// The old process keeps running until the trap loop has a complete replacement; on any error
    /// nothing is torn down and the syscall returns `-errno` to the caller as Linux expects.
    fn exec_image(
        sys: &mut KernelSys,
        session: &mut Session,
        handler: Handle,
    ) -> Result<Session, Errno> {
        let request = session.process.pending_exec.take().ok_or(Errno::EINVAL)?;
        let path = core::str::from_utf8(&request.path).map_err(|_| Errno::ENOENT)?;
        let image = image_bytes(path)?;
        let argv = read_string_array(sys, &session.loaded.regions, request.argv)?;
        let envp = read_string_array(sys, &session.loaded.regions, request.envp)?;
        let argv_refs: Vec<&[u8]> = argv.iter().map(Vec::as_slice).collect();
        let envp_refs: Vec<&[u8]> = envp.iter().map(Vec::as_slice).collect();
        let spec = plan_image(
            &image,
            &config(),
            &argv_refs,
            &envp_refs,
            &request.path,
            raven_abi::layout::x86_64::USER_END,
        )
        .map_err(|_| Errno::ENOEXEC)?;
        let loaded = load(&spec, &image, sys).map_err(load_error)?;
        if let Err(error) = raw::set_personality(loaded.proc, handler) {
            close_regions(sys, &loaded.regions);
            let _ = sys::handle_close(loaded.proc);
            return Err(error);
        }
        let entry = match usize::try_from(loaded.entry) {
            Ok(entry) => entry,
            Err(_) => {
                close_regions(sys, &loaded.regions);
                let _ = sys::handle_close(loaded.proc);
                return Err(Errno::E2BIG);
            }
        };
        let stack = match usize::try_from(loaded.sp) {
            Ok(stack) => stack,
            Err(_) => {
                close_regions(sys, &loaded.regions);
                let _ = sys::handle_close(loaded.proc);
                return Err(Errno::E2BIG);
            }
        };
        let mut tid = 0u32;
        let thread = match raw::thread_create_ex(loaded.proc, entry, stack, &mut tid) {
            Ok(thread) => thread,
            Err(error) => {
                close_regions(sys, &loaded.regions);
                let _ = sys::handle_close(loaded.proc);
                return Err(error);
            }
        };
        if tid == 0 {
            close_regions(sys, &loaded.regions);
            let _ = sys::handle_close(loaded.proc);
            return Err(Errno::EINVAL);
        }
        let mut process = LinuxProcess::new(
            session.process.pid,
            tid,
            thread,
            loaded.brk,
            loaded.brk_limit,
            loaded.mmap,
            loaded.mmap_limit,
        );
        process.regions = loaded.regions.clone();
        process.set_identity(&request.path, argv);
        // `execve` keeps the open file descriptions; only the image is replaced.
        process.adopt_files(session.process.take_files());
        Ok(Session { loaded, process })
    }

    fn load_error(error: linuxd::LoadError) -> Errno {
        match error {
            linuxd::LoadError::Sys(error) => error,
            linuxd::LoadError::TooLarge => Errno::E2BIG,
        }
    }

    /// Stop, reap and release the process an `execve` replaced.
    fn terminate_old(sys: &mut KernelSys, session: &mut Session) {
        let _ = sys::proc_terminate(session.loaded.proc, 0);
        let mut rt = RtSys;
        let mut status = 0;
        let _ = rt.proc_wait(session.loaded.proc, &mut status);
        let _ = sys::handle_close(session.process.thread);
        let _ = sys::handle_close(session.loaded.proc);
        close_regions(sys, &session.loaded.regions);
    }

    /// Close the memory objects the loader kept for a process that is gone.
    fn close_regions(sys: &mut KernelSys, regions: &[linuxd::Region]) {
        let mut closed: Vec<Handle> = Vec::new();
        for region in regions {
            if region.backing == Handle::NONE || closed.contains(&region.backing) {
                continue;
            }
            let _ = linuxd::Sys::handle_close(sys, region.backing);
            closed.push(region.backing);
        }
    }

    /// Read a NUL-terminated pointer array (`argv`/`envp`) from the caller.
    fn read_string_array(
        sys: &mut KernelSys,
        regions: &[linuxd::Region],
        address: u64,
    ) -> Result<Vec<Vec<u8>>, Errno> {
        if address == 0 {
            return Ok(Vec::new());
        }
        let mut values = Vec::new();
        let mut total = 0usize;
        for index in 0..MAX_EXEC_ARGS {
            let slot = (index as u64)
                .checked_mul(8)
                .and_then(|offset| address.checked_add(offset))
                .ok_or(Errno::EFAULT)?;
            let pointer = read_word(sys, regions, slot)?;
            if pointer == 0 {
                return Ok(values);
            }
            let string = read_string(sys, regions, pointer)?;
            total = total.checked_add(string.len() + 1).ok_or(Errno::E2BIG)?;
            if total > MAX_EXEC_BYTES {
                return Err(Errno::E2BIG);
            }
            values.push(string);
        }
        Err(Errno::E2BIG)
    }

    fn read_word(
        sys: &mut KernelSys,
        regions: &[linuxd::Region],
        address: u64,
    ) -> Result<u64, Errno> {
        let mut bytes = [0u8; 8];
        raw::copy_in(sys, regions, address, &mut bytes)?;
        Ok(u64::from_le_bytes(bytes))
    }

    fn read_string(
        sys: &mut KernelSys,
        regions: &[linuxd::Region],
        address: u64,
    ) -> Result<Vec<u8>, Errno> {
        let mut out = Vec::new();
        loop {
            let cursor = address.checked_add(out.len() as u64).ok_or(Errno::EFAULT)?;
            let region = regions
                .iter()
                .find(|region| region.contains(cursor, 1))
                .ok_or(Errno::EFAULT)?;
            let available = region.len - (cursor - region.start);
            let want = (MAX_EXEC_STRING + 1 - out.len())
                .min(available as usize)
                .min(64);
            if want == 0 {
                return Err(Errno::E2BIG);
            }
            let mut chunk = [0u8; 64];
            let slice = chunk.get_mut(..want).ok_or(Errno::E2BIG)?;
            raw::copy_in(sys, regions, cursor, slice)?;
            if let Some(end) = slice.iter().position(|byte| *byte == 0) {
                out.extend_from_slice(slice.get(..end).unwrap_or(&[]));
                return Ok(out);
            }
            out.extend_from_slice(slice);
            if out.len() >= MAX_EXEC_STRING {
                return Err(Errno::E2BIG);
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
