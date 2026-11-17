// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! P2e procd fixture client.
//!
//! Starts `initfsd` and `procd`, then speaks the version-1 `proc:` protocol. The mode file
//! selects the spawn/wait case or the kill/wait-signal case.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::format;
    use alloc::string::String;
    use alloc::vec;
    use alloc::vec::Vec;

    use raven_abi::errno::Errno;
    use raven_abi::types::{Handle, Rights};
    use raven_initfs::Initfs;
    use raven_proto_fs::OpenFlags;
    use raven_proto_proc::{
        Body, FdMapEntry, HelloRequest, KillRequest, Op, Request, Response, SetPgidRequest,
        SpawnRequest, VERSION, WaitRequest, decode_response, encode_request,
    };
    use raven_rt::entry;
    use raven_rt::fs;
    use raven_rt::spawn::{SpawnOptions, spawn};
    use raven_scheme::Client;

    entry!(main);

    const SPAWN_SETPGROUP: u32 = 2;
    const SPAWN_SETSIGDEF: u32 = 4;
    const SPAWN_SETSIGMASK: u32 = 8;
    const SPAWN_CLOEXEC_DEFAULT: u32 = 1 << 17;
    const PROC_SELF_INFO: usize = 0x35;
    const PROC_PID_INFO: usize = 0x3A;
    const INFO_WORDS: usize = 41;
    const SIGKILL: u32 = 9;
    const SIGUSR1: u32 = 10;
    const SIGUSR2: u32 = 12;
    const SIGTERM: u32 = 15;

    fn main(_args: raven_rt::Args) -> i32 {
        match run() {
            Ok(()) => {
                raven_rt::println!("procd-test: ok");
                0
            }
            Err(step) => {
                raven_rt::println!("procd-test: fail {step}");
                1
            }
        }
    }

    fn run() -> Result<(), &'static str> {
        start_service("/sbin/initfsd", "initfs:")?;
        start_service("/sbin/procd", "proc:")?;
        let mode = fs::read_to_vec("initfs:/etc/procd-mode").map_err(|_| "read mode")?;
        let mode = core::str::from_utf8(&mode).map_err(|_| "mode utf8")?.trim();
        let client = Client::new();
        let handle = client
            .open("proc:", u32::from(OpenFlags::READ_WRITE.to_wire()))
            .map_err(|_| "open proc")?;
        let mut seq = 1u64;
        hello(&client, handle, &mut seq)?;
        match mode {
            "exec" => exec_cases(&client, handle, &mut seq)?,
            "signal" => signal_cases(&client, handle, &mut seq)?,
            "attrs" => attrs_cases(&client, handle, &mut seq)?,
            _ => return Err("mode"),
        }
        let _ = client.close(handle);
        Ok(())
    }

    fn start_service(path: &str, scheme: &str) -> Result<(), &'static str> {
        let image = raven_rt::map_initfs().map_err(|_| "initfs map")?;
        let initfs = Initfs::parse(image).map_err(|_| "initfs parse")?;
        spawn(
            &initfs,
            path,
            &SpawnOptions {
                args: &[],
                env: &[],
            },
        )
        .map_err(|_| "spawn service")?;
        wait_scheme(scheme)
    }

    fn wait_scheme(path: &str) -> Result<(), &'static str> {
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

    fn hello(client: &Client, handle: Handle, seq: &mut u64) -> Result<(), &'static str> {
        let response = call(
            client,
            handle,
            seq,
            &Request::Hello(HelloRequest { client_version: 1 }),
        )?;
        match response.body {
            Body::Hello {
                server_version,
                pid,
            } => {
                raven_rt::println!("procd-test: hello version={server_version} pid={pid}");
                Ok(())
            }
            _ => Err("hello body"),
        }
    }

    fn exec_cases(client: &Client, handle: Handle, seq: &mut u64) -> Result<(), &'static str> {
        let pid = spawn_child(
            client,
            handle,
            seq,
            &["procd-child", "exec"],
            &["TEST=value"],
        )?;
        raven_rt::println!("procd-test: spawn pid={pid}");
        let status = wait_child(client, handle, seq, pid)?;
        if exited(status) != Some(7) {
            return Err("exec exit");
        }
        raven_rt::println!("procd-test: wait exit=7");
        let unknown = wait_raw(client, handle, seq, 999_999)?;
        if unknown.status != -(Errno::ECHILD.0 as i32) {
            return Err("unknown wait status");
        }
        raven_rt::println!("procd-test: unknown=echild");
        Ok(())
    }

    fn signal_cases(client: &Client, handle: Handle, seq: &mut u64) -> Result<(), &'static str> {
        let pid = spawn_child(client, handle, seq, &["procd-child", "sleep"], &[])?;
        raven_rt::println!("procd-test: spawn pid={pid}");
        let _ = raven_rt::nanosleep(100_000_000);

        let response = call(
            client,
            handle,
            seq,
            &Request::SetPgid(SetPgidRequest { pid, pgid: 0 }),
        )?;
        if response.status < 0 {
            return Err("setpgid");
        }
        raven_rt::println!("procd-test: setpgid pid={pid}");
        let pgid = call(
            client,
            handle,
            seq,
            &Request::GetPgid(raven_proto_proc::GetPgidRequest { pid }),
        )?;
        if pgid.value != i64::from(pid) {
            return Err("getpgid value");
        }
        raven_rt::println!("procd-test: getpgid={}", pgid.value);
        let sid = call(
            client,
            handle,
            seq,
            &Request::GetSid(raven_proto_proc::GetSidRequest { pid }),
        )?;
        if sid.value <= 0 {
            return Err("getsid");
        }

        let response = call(
            client,
            handle,
            seq,
            &Request::Kill(KillRequest {
                pid: i64::from(pid),
                sig: 15,
                flags: 0,
                value: None,
            }),
        )?;
        if response.status < 0 {
            return Err("kill");
        }
        raven_rt::println!("procd-test: kill signal=15");
        let status = wait_child(client, handle, seq, pid)?;
        if exited(status) != Some(143) {
            return Err("signal wait");
        }
        raven_rt::println!("procd-test: wait exit=143");
        let unknown = wait_raw(client, handle, seq, 999_999)?;
        if unknown.status != -(Errno::ECHILD.0 as i32) {
            return Err("unknown wait status");
        }
        raven_rt::println!("procd-test: unknown=echild");

        let response = call_header(client, handle, seq, Op::SetCred)?;
        if response.status != -(Errno::ENOSYS.0 as i32) {
            return Err("setcred status");
        }
        raven_rt::println!("procd-test: unsupported=enosys");
        Ok(())
    }

    fn attrs_cases(client: &Client, handle: Handle, seq: &mut u64) -> Result<(), &'static str> {
        let (read, write) = raven_rt::pipe::pipe().map_err(|_| "pipe")?;
        raven_rt::sys::write(write, b"ok\n").map_err(|_| "pipe write")?;
        let mut invalid = 0u32;

        let source = transfer(client, handle, read)?;
        let mut request = base_spawn();
        request.fdmap = vec![
            FdMapEntry {
                fd: 7,
                handle: source,
            },
            FdMapEntry {
                fd: 7,
                handle: source,
            },
        ];
        expect_error(client, handle, seq, &Request::Spawn(request), Errno::EINVAL)?;
        invalid = invalid.saturating_add(1);

        let source = transfer(client, handle, read)?;
        let mut request = base_spawn();
        request.fdmap = vec![FdMapEntry {
            fd: -1,
            handle: source,
        }];
        expect_error(client, handle, seq, &Request::Spawn(request), Errno::EINVAL)?;
        invalid = invalid.saturating_add(1);

        let source = transfer(client, handle, read)?;
        let mut request = base_spawn();
        request.fdmap = vec![FdMapEntry {
            fd: 1024,
            handle: source,
        }];
        expect_error(client, handle, seq, &Request::Spawn(request), Errno::EINVAL)?;
        invalid = invalid.saturating_add(1);

        let mut request = base_spawn();
        request.fdmap = vec![FdMapEntry {
            fd: 7,
            handle: 0x00ff_ffff,
        }];
        expect_error(client, handle, seq, &Request::Spawn(request), Errno::EBADF)?;
        invalid = invalid.saturating_add(1);

        let mut request = base_spawn();
        request.spawn_flags = SPAWN_SETSIGMASK;
        set_signal(&mut request.sigmask, SIGKILL);
        expect_error(client, handle, seq, &Request::Spawn(request), Errno::EINVAL)?;
        invalid = invalid.saturating_add(1);

        let mut request = base_spawn();
        set_signal(&mut request.sigdefault, SIGTERM);
        expect_error(client, handle, seq, &Request::Spawn(request), Errno::EINVAL)?;
        invalid = invalid.saturating_add(1);

        let nohang = call_raw(
            client,
            handle,
            seq,
            &Request::Wait(WaitRequest {
                idtype: 0,
                id: 0,
                options: 1,
            }),
        )?;
        let no_child =
            nohang.status == -(Errno::ECHILD.0 as i32) || (nohang.status == 0 && nohang.value == 0);
        if !no_child {
            return Err("invalid spawn created a child");
        }
        raven_rt::println!("procd-test: attrs invalid={invalid}");

        let source = transfer(client, handle, read)?;
        let mut request = base_spawn();
        request.spawn_flags =
            SPAWN_SETPGROUP | SPAWN_SETSIGMASK | SPAWN_SETSIGDEF | SPAWN_CLOEXEC_DEFAULT;
        request.pgroup = 0;
        request.fdmap = vec![FdMapEntry {
            fd: 7,
            handle: source,
        }];
        set_signal(&mut request.sigmask, SIGUSR1);
        set_signal(&mut request.sigdefault, SIGTERM);
        set_signal(&mut request.ignored, SIGUSR2);
        request.argv = vec![
            b"procd-child".to_vec(),
            b"attrs".to_vec(),
            b"-1".to_vec(),
            b"1".to_vec(),
        ];
        let pid = spawn_request(client, handle, seq, request)?;
        raven_rt::println!("procd-test: attrs spawn1={pid}");
        let kernel_ok = kernel_command_is(pid, "procd-child");
        let registry_ok = registry_has(client, pid)?;
        raven_rt::println!(
            "procd-test: command registry={} kernel={}",
            u8::from(registry_ok),
            u8::from(kernel_ok)
        );
        let status = wait_child(client, handle, seq, pid)?;
        if exited(status) != Some(0) {
            return Err("attrs spawn1 exit");
        }

        let mut request = base_spawn();
        request.argv = vec![b"procd-child".to_vec(), b"exec-name".to_vec()];
        let exec_pid = spawn_request(client, handle, seq, request)?;
        let status = wait_child(client, handle, seq, exec_pid)?;
        if exited(status) != Some(0) {
            return Err("exec-name exit");
        }
        raven_rt::println!("procd-test: command exec=1");

        let pgid = self_pgid()?;
        raven_rt::sys::write(write, b"ok\n").map_err(|_| "pipe write 2")?;
        let source = transfer(client, handle, read)?;
        let mut request = base_spawn();
        request.spawn_flags = SPAWN_SETPGROUP;
        request.pgroup = pgid;
        request.fdmap = vec![FdMapEntry {
            fd: 7,
            handle: source,
        }];
        request.argv = vec![
            b"procd-child".to_vec(),
            b"attrs".to_vec(),
            format!("{pgid}").into_bytes(),
            b"0".to_vec(),
        ];
        let pid = spawn_request(client, handle, seq, request)?;
        let status = wait_child(client, handle, seq, pid)?;
        if exited(status) != Some(0) {
            return Err("attrs spawn2 exit");
        }
        raven_rt::println!("procd-test: attrs spawn2={pid} pgid={pgid}");

        let _ = client.close(read);
        let _ = client.close(write);
        Ok(())
    }

    fn base_spawn() -> SpawnRequest {
        SpawnRequest {
            spawn_flags: 0,
            pgroup: 0,
            sigdefault: [0u8; 128],
            sigmask: [0u8; 128],
            ignored: [0u8; 128],
            sched_policy: 0,
            sched_prio: 0,
            umask: 0o022,
            tcpgrp_fd: -1,
            path: b"/bin/procd-child".to_vec(),
            cwd: b"/".to_vec(),
            argv: vec![b"procd-child".to_vec(), b"attrs".to_vec()],
            envp: vec![],
            fdmap: vec![],
        }
    }

    fn transfer(client: &Client, proc_handle: Handle, source: Handle) -> Result<u32, &'static str> {
        client
            .transfer_to_server(proc_handle, source, Rights::ALL.0)
            .map(|handle| handle.0)
            .map_err(|_| "transfer")
    }

    fn expect_error(
        client: &Client,
        handle: Handle,
        seq: &mut u64,
        request: &Request,
        expected: Errno,
    ) -> Result<(), &'static str> {
        let response = call_raw(client, handle, seq, request)?;
        if response.status != -(expected.0 as i32) {
            return Err("unexpected spawn error");
        }
        Ok(())
    }

    fn spawn_request(
        client: &Client,
        handle: Handle,
        seq: &mut u64,
        request: SpawnRequest,
    ) -> Result<i32, &'static str> {
        let response = call_raw(client, handle, seq, &Request::Spawn(request))?;
        if response.status < 0 {
            raven_rt::println!("procd-test: spawn-status={}", response.status);
            return Err("spawn status");
        }
        i32::try_from(response.value).map_err(|_| "spawn pid")
    }

    fn set_signal(set: &mut [u8; 128], signo: u32) {
        let bit = usize::try_from(signo.saturating_sub(1)).unwrap_or(0);
        if let Some(byte) = set.get_mut(bit / 8) {
            *byte |= 1 << (bit % 8);
        }
    }

    fn self_pgid() -> Result<i32, &'static str> {
        let mut words = [0u64; 8];
        // SAFETY: PROC_SELF_INFO writes eight u64 words to the live array owned by the caller.
        let raw = unsafe {
            raven_rt::sys::syscall2(PROC_SELF_INFO, words.as_mut_ptr() as usize, words.len())
        };
        Errno::from_ret(raw).map_err(|_| "self info")?;
        i32::try_from(words[2]).map_err(|_| "pgid")
    }

    fn registry_has(client: &Client, pid: i32) -> Result<bool, &'static str> {
        let registry = client
            .open("proc:registry", u32::from(OpenFlags::READ.to_wire()))
            .map_err(|_| "open registry")?;
        let mut text = Vec::new();
        let mut buffer = [0u8; 512];
        let mut offset = 0u64;
        loop {
            let read = client
                .read(registry, offset, &mut buffer)
                .map_err(|_| "read registry")?;
            if read == 0 {
                break;
            }
            text.extend_from_slice(&buffer[..read]);
            offset = offset.saturating_add(read as u64);
            if text.len() > 64 * 1024 {
                break;
            }
        }
        let _ = client.close(registry);
        let text = core::str::from_utf8(&text).map_err(|_| "registry utf8")?;
        let pid_text = format!("{pid}");
        Ok(text.lines().any(|line| {
            let mut fields = line.split_whitespace();
            fields.next() == Some(pid_text.as_str()) && fields.any(|field| field == "procd-child")
        }))
    }

    fn kernel_command_is(pid: i32, expected: &str) -> bool {
        let mut words = [0u64; INFO_WORDS];
        // SAFETY: PROC_PID_INFO writes INFO_WORDS u64 words to the live caller-owned array.
        let raw = unsafe {
            raven_rt::sys::syscall3(
                PROC_PID_INFO,
                pid as isize as usize,
                words.as_mut_ptr() as usize,
                words.len(),
            )
        };
        if Errno::from_ret(raw).is_err() {
            return false;
        }
        command_from_words(&words).as_deref() == Some(expected)
    }

    fn command_from_words(words: &[u64; INFO_WORDS]) -> Option<String> {
        let mut bytes = Vec::new();
        for word in words.iter().skip(8) {
            for byte in word.to_le_bytes() {
                if byte == 0 {
                    return core::str::from_utf8(&bytes).ok().map(String::from);
                }
                bytes.push(byte);
            }
        }
        core::str::from_utf8(&bytes).ok().map(String::from)
    }

    fn spawn_child(
        client: &Client,
        handle: Handle,
        seq: &mut u64,
        argv: &[&str],
        env: &[&str],
    ) -> Result<i32, &'static str> {
        let request = SpawnRequest {
            spawn_flags: 0,
            pgroup: 0,
            sigdefault: [0u8; 128],
            sigmask: [0u8; 128],
            ignored: [0u8; 128],
            sched_policy: 0,
            sched_prio: 0,
            umask: 0o022,
            tcpgrp_fd: -1,
            path: b"/bin/procd-child".to_vec(),
            cwd: b"/".to_vec(),
            argv: argv.iter().map(|value| value.as_bytes().to_vec()).collect(),
            envp: env.iter().map(|value| value.as_bytes().to_vec()).collect(),
            fdmap: vec![],
        };
        let response = call(client, handle, seq, &Request::Spawn(request))?;
        i32::try_from(response.value).map_err(|_| "spawn pid")
    }

    fn wait_child(
        client: &Client,
        handle: Handle,
        seq: &mut u64,
        pid: i32,
    ) -> Result<i32, &'static str> {
        let response = call_raw(
            client,
            handle,
            seq,
            &Request::Wait(WaitRequest {
                idtype: 1,
                id: i64::from(pid),
                options: 0,
            }),
        )?;
        if response.status < 0 {
            raven_rt::println!("procd-test: wait-status={}", response.status);
            return Err("wait status");
        }
        match response.body {
            Body::Wait { status, .. } => Ok(status),
            _ => Err("wait body"),
        }
    }

    fn call(
        client: &Client,
        handle: Handle,
        seq: &mut u64,
        request: &Request,
    ) -> Result<Response, &'static str> {
        let response = call_raw(client, handle, seq, request)?;
        if response.status < 0 {
            return Err("call status");
        }
        Ok(response)
    }

    fn call_raw(
        client: &Client,
        handle: Handle,
        seq: &mut u64,
        request: &Request,
    ) -> Result<Response, &'static str> {
        let mut encoded = [0u8; 4096];
        let length = encode_request(request, *seq, &mut encoded).map_err(|_| "encode")?;
        *seq = seq.saturating_add(1);
        let mut response = [0u8; 1024];
        let length = client
            .call(handle, &encoded[..length], &mut response)
            .map_err(|_| "call")?;
        decode_response(request.op(), &response[..length]).map_err(|_| "decode response")
    }

    fn wait_raw(
        client: &Client,
        handle: Handle,
        seq: &mut u64,
        pid: i32,
    ) -> Result<Response, &'static str> {
        call_raw(
            client,
            handle,
            seq,
            &Request::Wait(WaitRequest {
                idtype: 1,
                id: i64::from(pid),
                options: 0,
            }),
        )
    }

    fn call_header(
        client: &Client,
        handle: Handle,
        seq: &mut u64,
        op: Op,
    ) -> Result<Response, &'static str> {
        let mut encoded = [0u8; 16];
        encoded[0..2].copy_from_slice(&VERSION.to_le_bytes());
        encoded[2..4].copy_from_slice(&(op as u16).to_le_bytes());
        encoded[8..16].copy_from_slice(&seq.to_le_bytes());
        *seq = seq.saturating_add(1);
        let mut response = [0u8; 1024];
        let length = client
            .call(handle, &encoded, &mut response)
            .map_err(|_| "call header")?;
        decode_response(op, &response[..length]).map_err(|_| "decode header")
    }

    fn exited(status: i32) -> Option<i32> {
        if status & 0x7f == 0 {
            Some((status >> 8) & 0xff)
        } else {
            None
        }
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
