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
    use alloc::format;
    use alloc::vec::Vec;

    use linuxd::{
        CloneChild, CloneKind, FLAG_ADDR, FileKind, FileOp, LinuxProcess, OpenFile, Region,
        Sys as _, TrapAction, child_frame, classify, copy_in, decode, dispatch, dispatcher,
        encode_reply, interp_path, load, plan_image, trampoline_page,
    };
    use raven_abi::errno::Errno;
    use raven_abi::types::{Handle, clock, map, prot};
    use raven_initfs::Initfs;
    use raven_ipc::protocol::{self, Opcode, PACKET_SIZE, SchemePacket};
    use raven_linux_abi::flags::clone as clone_flags;
    use raven_linux_dispatch::Dispatcher;
    use raven_proto_linux::{LinuxFrame, REPLY_LEN};
    use raven_rt::entry;
    use raven_rt::println;
    use raven_rt::spawn::RtSys;
    use raven_rt::sys;

    use crate::{KernelSys, raw};

    entry!(main);

    /// Page size used by every clone mapping.
    const PAGE: usize = 4096;

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
        // A dynamic main names its interpreter with an absolute path inside the Linux root; the
        // B32 recipe stages that root at `/compat/linux` in the initfs.
        let interpreter = match interp_path(&image) {
            Ok(Some(interpreter)) => match read_interp(interpreter) {
                Some(bytes) => Some(bytes),
                None => {
                    println!("linuxd: interpreter {interpreter} is not in the initfs");
                    return 2;
                }
            },
            Ok(None) => None,
            Err(error) => {
                println!("linuxd: image plan failed: {error:?}");
                return 2;
            }
        };
        let config = linuxd::Config {
            main_pie_base: linuxd::MAIN_PIE_BASE,
            interp_base: linuxd::INTERP_BASE,
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
        let argv_refs: Vec<&[u8]> = argv.iter().map(Vec::as_slice).collect();
        let spec = match plan_image(
            &image,
            interpreter.as_deref(),
            &config,
            &argv_refs,
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
        if spec.interp_steps.is_empty() {
            println!("linuxd: dynamic=0");
        } else {
            println!(
                "linuxd: dynamic=1 interp-entry={:#x} interp-bias={:#x}",
                spec.entry, spec.interp_bias
            );
        }

        let mut sys = KernelSys;
        let mut loaded = match load(&spec, &image, interpreter.as_deref(), &mut sys) {
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

        // The clone cooperation page must exist before the Linux thread starts: a fork child has
        // no process identity in the trap packet and signals its parent through this shared page.
        let flag_object = match setup_flag_page(&mut sys, loaded.proc) {
            Ok(object) => object,
            Err(error) => {
                println!("linuxd: clone flag page failed: {}", error.description());
                return 7;
            }
        };
        loaded.regions.push(Region {
            start: FLAG_ADDR,
            len: PAGE as u64,
            backing: flag_object,
            backing_offset: 0,
        });

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
        let mut native_tid = 0u32;
        let thread =
            match raw::thread_create_ex(loaded.proc, entry, stack, 0, 0, &mut native_tid, 0) {
                Ok(thread) => thread,
                Err(error) => {
                    println!("linuxd: thread create failed: {}", error.description());
                    return 6;
                }
            };

        let _watchdog = raven_rt::thread::spawn(watchdog, 0, 64 * 1024);
        let trace = args.get(2).is_some_and(|argument| argument == "trace");
        serve(sys, loaded, handler, thread, native_tid, path, argv, trace)
    }

    /// Map the shared clone cooperation page into `proc` at [`linuxd::FLAG_ADDR`].
    fn setup_flag_page(sys: &mut KernelSys, proc: Handle) -> Result<Handle, Errno> {
        let object = sys.memobj_create(PAGE)?;
        let flags = map::pack(prot::READ | prot::WRITE, map::FIXED | map::SHARED);
        let address = usize::try_from(FLAG_ADDR).map_err(|_| Errno::EINVAL)?;
        if let Err(error) = sys.map(proc, object, 0, address, PAGE, flags) {
            let _ = sys.handle_close(object);
            return Err(error);
        }
        Ok(object)
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

    /// Read a `PT_INTERP` image. The path is absolute inside the Linux root; the recipe stages the
    /// root under [`linuxd::LINUX_ROOT`], so try the literal path first and then the rooted one.
    fn read_interp(path: &str) -> Option<Vec<u8>> {
        let mapped = sys::map_initfs().ok()?;
        let initfs = Initfs::parse(mapped).ok()?;
        if let Some(entry) = initfs.get(path) {
            return Some(entry.data.to_vec());
        }
        let rooted = format!("{}{}", linuxd::LINUX_ROOT, path);
        initfs.get(&rooted).map(|entry| entry.data.to_vec())
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

    #[allow(clippy::too_many_arguments)]
    fn serve(
        mut sys: KernelSys,
        loaded: linuxd::Loaded,
        handler: Handle,
        thread: Handle,
        native_tid: u32,
        path: &str,
        argv: Vec<Vec<u8>>,
        trace: bool,
    ) -> i32 {
        let mut dispatcher = dispatcher();
        let mut process = LinuxProcess::new(
            1,
            native_tid,
            loaded.proc,
            thread,
            loaded.brk,
            loaded.brk_limit,
            loaded.mmap,
            loaded.mmap_limit,
        );
        process.regions = loaded.regions.clone();
        process.set_identity(path.as_bytes(), argv);
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
                    let reply = reply_packet(packet, -(raven_linux_abi::errno::EPROTO as i64));
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
                trace,
            );
            if let Some(code) = exit {
                cleanup_clones(&mut sys, &mut process);
                print_stats(&dispatcher, handler);
                let _ = sys::proc_terminate(loaded.proc, code);
                let mut rt = RtSys;
                let mut status = code;
                let _ = rt.proc_wait(loaded.proc, &mut status);
                println!("linuxd: hello exit={status}");
                // Propagate the Linux status to this server's parent: the boot init's test-exit
                // observes linuxd itself, while a supervisor (the linux-root fixture) can print
                // the Linux program's status without parsing the console.
                return status;
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
        trace: bool,
    ) -> Option<i32> {
        process.user_buffer.clear();
        // B21a carries the saved user frame in the packet path. A malformed or absent frame leaves
        // `None`; only `clone` needs it, and it answers `-ENOSYS` without one.
        process.frame = LinuxFrame::decode(&packet.path).ok();
        if let Some((argument, len)) = copy_in(&call) {
            process.user_buffer.resize(len, 0);
            let source = call.args.get(argument).copied().unwrap_or(0);
            if let Err(error) = raw::copy_in(sys, &loaded.regions, source, &mut process.user_buffer)
            {
                let reply = reply_packet(packet, linuxd::linux_error(error) as i64);
                let _ = sys::send(request, &reply);
                return None;
            }
        }
        let mut action = dispatch(process, dispatcher, &call);
        // The Linux thread is blocked in this trap, so a queued `ARCH_SET_FS` is installed in it
        // before the reply is sent. A failing native call replaces the success with `-errno`.
        if let Some(base) = process.pending_tls.take() {
            match raw::set_tls(process.thread, base) {
                Ok(()) => process.fs_base = base,
                Err(error) => action = TrapAction::Reply(linuxd::linux_error(error)),
            }
        }
        if let Some(op) = process.pending_file.take() {
            match run_file_op(sys, process, loaded, op) {
                Ok(value) => action = TrapAction::Reply(value),
                Err(error) => action = TrapAction::Reply(linuxd::linux_error(error)),
            }
        }
        // `clone` is the one handler that needs kernel work: the trap loop maps the trampoline,
        // creates the native task and replaces the handler's placeholder with the child pid/tid.
        if let Some(request) = process.pending_clone.take() {
            match run_clone(sys, process, request) {
                Ok(value) => action = TrapAction::Reply(value),
                Err(error) => {
                    println!("linuxd: clone failed: {}", error.description());
                    action = TrapAction::Reply(linuxd::linux_error(error));
                }
            }
        }
        // The per-trap trace is opt-in (`linuxd /hello trace`): the B30 fixture emits its own
        // lines and an interleaved trace would split them. Statistics are always printed at exit.
        if trace {
            match action {
                TrapAction::Reply(value) => println!("linuxd: trap nr={} -> {value}", call.nr),
                TrapAction::Exit(code) => println!("linuxd: trap nr={} -> exit {code}", call.nr),
            }
        }
        if !process.output.is_empty() {
            let output = process.take_output();
            let _ = sys::debug_write(&output);
        }
        if let Some((address, pending)) = process.pending_write.take()
            && raw::copy_out(sys, &loaded.regions, address, &pending).is_err()
        {
            let reply = reply_packet(packet, -(raven_linux_abi::errno::EFAULT as i64));
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

    /// Perform one queued file operation and return the Linux syscall result.
    fn run_file_op(
        sys: &mut KernelSys,
        process: &mut LinuxProcess,
        loaded: &linuxd::Loaded,
        op: FileOp,
    ) -> Result<isize, Errno> {
        match op {
            FileOp::Open {
                scheme,
                path,
                flags,
            } => {
                let text = core::str::from_utf8(&scheme).map_err(|_| Errno::EINVAL)?;
                let handle = raw::open_file(text, flags)?;
                Ok(process.open_remote(path, handle, flags) as isize)
            }
            FileOp::Read { fd, address, len } => {
                let handle = process
                    .file(fd)
                    .and_then(OpenFile::remote)
                    .ok_or(Errno::EBADF)?;
                let mut data = alloc::vec![0u8; len];
                let read = raw::read_file(handle, &mut data)?;
                raw::copy_out(sys, &loaded.regions, address, &data[..read])?;
                Ok(read as isize)
            }
            FileOp::Write { fd, data } => {
                let handle = process
                    .file(fd)
                    .and_then(OpenFile::remote)
                    .ok_or(Errno::EBADF)?;
                let written = raw::write_file(handle, &data)?;
                Ok(written as isize)
            }
            FileOp::Pread {
                fd,
                address,
                len,
                offset,
            } => {
                let handle = process
                    .file(fd)
                    .and_then(OpenFile::remote)
                    .ok_or(Errno::EBADF)?;
                let mut data = alloc::vec![0u8; len];
                let read = raw::pread_file(handle, offset, &mut data)?;
                raw::copy_out(sys, &loaded.regions, address, &data[..read])?;
                Ok(read as isize)
            }
            FileOp::Pwrite { fd, data, offset } => {
                let handle = process
                    .file(fd)
                    .and_then(OpenFile::remote)
                    .ok_or(Errno::EBADF)?;
                let written = raw::pwrite_file(handle, offset, &data)?;
                Ok(written as isize)
            }
            FileOp::Seek { fd, offset, whence } => {
                let handle = process
                    .file(fd)
                    .and_then(OpenFile::remote)
                    .ok_or(Errno::EBADF)?;
                let new_offset = raw::seek_file(handle, offset, whence)?;
                isize::try_from(new_offset).map_err(|_| Errno::EOVERFLOW)
            }
            FileOp::Close { fd } => {
                let remote = process.file(fd).and_then(OpenFile::remote);
                let result = match remote {
                    Some(handle) => raw::close_file(handle),
                    None => Ok(()),
                };
                let _ = process.close_file(fd);
                result.map(|()| 0)
            }
            FileOp::Dup3 {
                oldfd,
                newfd,
                cloexec,
            } => {
                let file = duplicate_file(process, oldfd, cloexec)?;
                if let Some(handle) = process.file(newfd).and_then(OpenFile::remote) {
                    let _ = raw::close_file(handle);
                }
                if !process.install_file(newfd, file) {
                    return Err(Errno::EBADF);
                }
                Ok(newfd as isize)
            }
            FileOp::FcntlDup { fd, min, cloexec } => {
                let target = process.lowest_free_fd(min);
                let file = duplicate_file(process, fd, cloexec)?;
                if let Some(handle) = process.file(target).and_then(OpenFile::remote) {
                    let _ = raw::close_file(handle);
                }
                if !process.install_file(target, file) {
                    return Err(Errno::EBADF);
                }
                Ok(target as isize)
            }
        }
    }

    /// Clone one descriptor, duplicating a remote scheme handle so the server cursor is shared.
    fn duplicate_file(process: &LinuxProcess, fd: i32, cloexec: bool) -> Result<OpenFile, Errno> {
        let old = process.file(fd).ok_or(Errno::EBADF)?.clone();
        let kind = match old.remote() {
            Some(handle) => FileKind::Remote(raw::dup_file(handle)?),
            None => old.kind.clone(),
        };
        Ok(OpenFile {
            path: old.path,
            kind,
            offset: old.offset,
            flags: old.flags,
            fd_flags: if cloexec {
                linuxd::files::FD_CLOEXEC
            } else {
                0
            },
        })
    }

    /// Perform one validated `clone`: map the trampoline, create the native task, write the tid
    /// outputs and return the child pid/tid for the parent's reply.
    fn run_clone(
        sys: &mut KernelSys,
        process: &mut LinuxProcess,
        request: linuxd::CloneRequest,
    ) -> Result<isize, Errno> {
        let kind = classify(&request)?;
        let frame = child_frame(&request, kind)?;
        let page = trampoline_page(&frame);
        let address = map_trampoline(sys, process, &page)?;
        let tp = if request.flags & clone_flags::CLONE_SETTLS != 0 {
            request.tls
        } else {
            request.frame.fs_base
        };
        let clear_tid = if request.flags & clone_flags::CLONE_CHILD_CLEARTID != 0 {
            request.ctid
        } else {
            0
        };
        match kind {
            CloneKind::Thread => {
                let mut native_tid = 0u32;
                let thread = raw::thread_create_ex(
                    process.proc,
                    address as usize,
                    request.child_stack as usize,
                    0,
                    tp,
                    &mut native_tid,
                    clear_tid,
                )?;
                let task = process.next_task_id();
                process.clones.push(CloneChild {
                    task,
                    native_tid,
                    process: None,
                    thread: Some(thread),
                });
                if request.flags & clone_flags::CLONE_PARENT_SETTID != 0 {
                    write_word(sys, process, request.ptid, task)?;
                }
                if request.flags & clone_flags::CLONE_CHILD_SETTID != 0 {
                    write_word(sys, process, request.ctid, task)?;
                }
                Ok(task as isize)
            }
            CloneKind::Fork => {
                let child = raw::proc_fork(process.proc)?;
                // `THREAD_CREATE` requires a 16-byte-aligned initial stack. The trampoline
                // immediately replaces RSP with the exact frame value, so only this admission
                // check sees the rounded address.
                let stack = if request.child_stack != 0 {
                    request.child_stack
                } else {
                    request.frame.rsp & !0xf
                };
                let mut native_tid = 0u32;
                let thread = match raw::thread_create_ex(
                    child,
                    address as usize,
                    stack as usize,
                    0,
                    tp,
                    &mut native_tid,
                    clear_tid,
                ) {
                    Ok(thread) => thread,
                    Err(error) => {
                        let _ = sys::proc_terminate(child, 0);
                        let mut rt = RtSys;
                        let mut status = 0;
                        let _ = rt.proc_wait(child, &mut status);
                        let _ = sys.handle_close(child);
                        return Err(error);
                    }
                };
                let _ = sys.handle_close(thread);
                let task = process.next_task_id();
                process.clones.push(CloneChild {
                    task,
                    native_tid,
                    process: Some(child),
                    thread: None,
                });
                if request.flags & clone_flags::CLONE_PARENT_SETTID != 0 {
                    write_word(sys, process, request.ptid, task)?;
                }
                Ok(task as isize)
            }
        }
    }

    /// Map one executable trampoline page into the Linux process at a fresh fixed address.
    ///
    /// The page content is written through a shared mapping in linuxd's own address space; the
    /// process mapping is read/execute only, as the kernel rejects a writable+executable mapping.
    fn map_trampoline(
        sys: &mut KernelSys,
        process: &mut LinuxProcess,
        page: &[u8; PAGE],
    ) -> Result<u64, Errno> {
        for _ in 0..64 {
            let address = process.next_trampoline();
            let object = sys.memobj_create(PAGE)?;
            let scratch_flags = map::pack(prot::READ | prot::WRITE, map::SHARED);
            let scratch = match sys.map(Handle::SELF_PROC, object, 0, 0, PAGE, scratch_flags) {
                Ok(scratch) => scratch,
                Err(error) => {
                    let _ = sys.handle_close(object);
                    return Err(error);
                }
            };
            // SAFETY: `scratch` is a PAGE-byte read/write mapping of `object` in this process and
            // `page` is exactly PAGE bytes; the ranges do not overlap.
            unsafe {
                core::ptr::copy_nonoverlapping(page.as_ptr(), scratch as *mut u8, PAGE);
            }
            let _ = sys.unmap(Handle::SELF_PROC, scratch, PAGE);
            // Generic memory objects carry no EXEC right in ABI v1, so map read/write and change
            // the target VMA to read/execute; the loader uses the same pattern for ELF text.
            let target_flags = map::pack(prot::READ | prot::WRITE, map::FIXED | map::SHARED);
            let fixed = usize::try_from(address).map_err(|_| Errno::EINVAL)?;
            match sys.map(process.proc, object, 0, fixed, PAGE, target_flags) {
                Ok(_) => {}
                Err(Errno::EEXIST) => {
                    let _ = sys.handle_close(object);
                    continue;
                }
                Err(error) => {
                    let _ = sys.handle_close(object);
                    return Err(error);
                }
            }
            let _ = sys.handle_close(object);
            let protection = (prot::READ | prot::EXEC) as u8;
            if let Err(error) = sys.protect(process.proc, fixed, PAGE, protection) {
                let _ = sys.unmap(process.proc, fixed, PAGE);
                return Err(error);
            }
            return Ok(address);
        }
        Err(Errno::ENOMEM)
    }

    /// Store a four-byte Linux task id in the parent's address space.
    fn write_word(
        sys: &mut KernelSys,
        process: &LinuxProcess,
        address: u64,
        value: u32,
    ) -> Result<(), Errno> {
        if address == 0 {
            return Ok(());
        }
        raw::copy_out(sys, &process.regions, address, &value.to_le_bytes())
    }

    /// Reap every task `clone` created before the parent goes away.
    fn cleanup_clones(sys: &mut KernelSys, process: &mut LinuxProcess) {
        for child in process.clones.drain(..) {
            if let Some(handle) = child.process {
                let _ = sys::proc_terminate(handle, 0);
                let mut rt = RtSys;
                let mut status = 0;
                let _ = rt.proc_wait(handle, &mut status);
                let _ = sys.handle_close(handle);
            }
            if let Some(handle) = child.thread {
                let _ = sys.handle_close(handle);
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
