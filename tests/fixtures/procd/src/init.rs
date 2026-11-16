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
    use alloc::vec;

    use raven_abi::errno::Errno;
    use raven_abi::types::Handle;
    use raven_initfs::Initfs;
    use raven_proto_fs::OpenFlags;
    use raven_proto_proc::{
        Body, HelloRequest, KillRequest, Op, Request, Response, SetPgidRequest, SpawnRequest,
        VERSION, WaitRequest, decode_response, encode_request,
    };
    use raven_rt::entry;
    use raven_rt::fs;
    use raven_rt::spawn::{SpawnOptions, spawn};
    use raven_scheme::Client;

    entry!(main);

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
        let response = call(
            client,
            handle,
            seq,
            &Request::Wait(WaitRequest {
                idtype: 1,
                id: i64::from(pid),
                options: 0,
            }),
        )?;
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
