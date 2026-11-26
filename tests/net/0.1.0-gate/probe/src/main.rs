// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S85n guest TCP probe.
//!
//! The gate injects this binary into the release initfs as `/sbin/net-gate` and
//! starts it as a one-shot service after `netd` registers `net:`. The probe
//! waits for the static QEMU user-network interface `eth0 10.0.2.15/24`, then
//! listens on that address and echoes exactly one host payload back. It exits 0
//! only after the host acknowledgement arrives; the test-only `/etc/svcd.conf`
//! propagates that status to init and the kernel test-exit path.

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
    use core::str;

    use raven_abi::errno::Errno;
    use raven_abi::types::Handle;
    use raven_proto_net::socket::{Call, Response, SocketRequest};
    use raven_proto_net::{IF_LINK_UP, IF_UP, IfName, IfaceRequest, IfaceResponse};
    use raven_rt::entry;
    use raven_scheme::Client;

    entry!(main);

    const READ_WRITE: u32 = 3;
    const ECHO_PORT: u16 = 23085;
    const ECHO_PAYLOAD: &[u8] = b"S85N-NET-GATE-PAYLOAD";
    const ECHO_ACK: &[u8] = b"S85N-NET-GATE-ACK";
    const IFACE_ATTEMPTS: u32 = 600;
    const ACCEPT_ATTEMPTS: u32 = 1_200;
    const IO_ATTEMPTS: u32 = 2_000;

    fn main(_args: raven_rt::Args) -> i32 {
        match run() {
            Ok(()) => {
                raven_rt::println!("net-gate: done");
                0
            }
            Err(message) => {
                raven_rt::eprintln!("net-gate: error: {message}");
                1
            }
        }
    }

    fn run() -> Result<(), String> {
        let client = Client::new();
        wait_interface(&client)?;
        echo_once(&client)
    }

    /// Wait for the release profile's static QEMU address to be live on the NIC.
    fn wait_interface(client: &Client) -> Result<(), String> {
        let name = IfName::new("eth0").ok_or("invalid eth0 interface name")?;
        let mut last = String::from("no response");
        for _ in 0..IFACE_ATTEMPTS {
            let response = iface_request(client, IfaceRequest::Get { name })?;
            match IfaceResponse::decode(&response) {
                Ok(IfaceResponse::Iface(info)) => {
                    let wanted = IF_UP | IF_LINK_UP;
                    if info.addr == [10, 0, 2, 15]
                        && info.prefix == 24
                        && info.flags & wanted == wanted
                    {
                        raven_rt::println!("net-gate: interface eth0 10.0.2.15/24 up");
                        return Ok(());
                    }
                    last = format!(
                        "eth0 {}.{}.{}.{}/{} flags={:#04x}",
                        info.addr[0],
                        info.addr[1],
                        info.addr[2],
                        info.addr[3],
                        info.prefix,
                        info.flags
                    );
                }
                Ok(IfaceResponse::Err(Errno::ENODEV)) => {
                    last = String::from("eth0 is absent");
                }
                Ok(IfaceResponse::Err(error)) => {
                    return Err(format!("interface get: {}", error.description()));
                }
                Ok(other) => last = format!("unexpected interface response: {other:?}"),
                Err(_) => return Err(String::from("cannot decode interface response")),
            }
            let _ = raven_rt::nanosleep(100_000_000);
        }
        Err(format!("eth0 did not come up: {last}"))
    }

    /// Accept one hostfwd connection, echo the exact payload and read the ack.
    fn echo_once(client: &Client) -> Result<(), String> {
        let path = format!("net:tcp/10.0.2.15:{ECHO_PORT}");
        let listener = client
            .open(&path, READ_WRITE)
            .map_err(|error| format!("open echo listener: {}", error.description()))?;
        expect_ok(
            call(client, listener, Call::Listen { backlog: 4 })?,
            "listen",
        )?;
        raven_rt::println!("net-gate: listening tcp 10.0.2.15:{ECHO_PORT}");

        let id = accept_retry(client, listener)?;
        let accepted = client
            .open(&format!("net:accept/{id}"), READ_WRITE)
            .map_err(|error| format!("open accepted socket: {}", error.description()))?;
        let mut buffer = [0u8; 64];
        let length = read_exact(client, accepted, ECHO_PAYLOAD.len(), &mut buffer)?;
        let payload = buffer.get(..length).ok_or("short echo payload")?;
        if payload != ECHO_PAYLOAD {
            return Err(format!("host payload differs: {payload:?}"));
        }
        let text = str::from_utf8(payload).map_err(|_| "echo payload is not UTF-8")?;
        raven_rt::println!("net-gate: echo received {length} bytes payload={text}");

        let written = client
            .write(accepted, 0, payload)
            .map_err(|error| format!("echo write: {}", error.description()))?;
        if written != length {
            return Err(format!("echo wrote {written} of {length} bytes"));
        }
        raven_rt::println!("net-gate: echo sent {written} bytes");

        let ack_length = read_exact(client, accepted, ECHO_ACK.len(), &mut buffer)?;
        let ack = buffer.get(..ack_length).ok_or("short host ack")?;
        if ack != ECHO_ACK {
            return Err(format!("host ack differs: {ack:?}"));
        }
        let text = str::from_utf8(ack).map_err(|_| "host ack is not UTF-8")?;
        raven_rt::println!("net-gate: host ack {text}");

        let _ = client.close(accepted);
        let _ = client.close(listener);
        Ok(())
    }

    fn accept_retry(client: &Client, listener: Handle) -> Result<u32, String> {
        for _ in 0..ACCEPT_ATTEMPTS {
            match call(client, listener, Call::Accept)? {
                Response::Socket { id } => return Ok(id),
                Response::Err(Errno::EAGAIN) => {}
                Response::Err(error) => {
                    return Err(format!("accept: {}", error.description()));
                }
                other => return Err(format!("unexpected accept response: {other:?}")),
            }
            let _ = raven_rt::nanosleep(50_000_000);
        }
        Err(String::from("host echo connection timed out"))
    }

    fn read_exact(
        client: &Client,
        file: Handle,
        wanted: usize,
        buffer: &mut [u8],
    ) -> Result<usize, String> {
        let mut total = 0usize;
        let mut idle = 0u32;
        while total < wanted {
            let target = buffer
                .get_mut(total..wanted)
                .ok_or("read buffer is too small")?;
            match client.read(file, 0, target) {
                Ok(0) => return Err(String::from("peer closed before the payload completed")),
                Ok(length) => {
                    total = total.checked_add(length).ok_or("read length overflow")?;
                    idle = 0;
                }
                Err(Errno::EAGAIN) => {
                    if idle >= IO_ATTEMPTS {
                        return Err(format!("read timed out after {total} of {wanted} bytes"));
                    }
                    idle += 1;
                    let _ = raven_rt::nanosleep(5_000_000);
                }
                Err(error) => return Err(format!("read: {}", error.description())),
            }
        }
        Ok(total)
    }

    fn call(client: &Client, file: Handle, operation: Call) -> Result<Response, String> {
        let request = SocketRequest::Call(operation);
        let mut encoded = [0u8; 64];
        let length = request
            .encode(&mut encoded)
            .map_err(|_| String::from("encode socket call"))?;
        let mut response = [0u8; 128];
        let read = client
            .call(file, &encoded[..length], &mut response)
            .map_err(|error| format!("socket call: {}", error.description()))?;
        let bytes = response.get(..read).ok_or("short socket call response")?;
        Response::decode(bytes).map_err(|_| String::from("cannot decode socket call"))
    }

    fn expect_ok(response: Response, what: &str) -> Result<(), String> {
        match response {
            Response::Ok => Ok(()),
            Response::Err(error) => Err(format!("{what}: {}", error.description())),
            other => Err(format!("{what}: unexpected response {other:?}")),
        }
    }

    fn iface_request(client: &Client, request: IfaceRequest) -> Result<Vec<u8>, String> {
        let control = client
            .open("net:", READ_WRITE)
            .map_err(|error| format!("open net:: {}", error.description()))?;
        let mut encoded = [0u8; 256];
        let length = request
            .encode(&mut encoded)
            .map_err(|_| String::from("encode interface request"))?;
        let mut response = [0u8; 4096];
        let read = client
            .call(control, &encoded[..length], &mut response)
            .map_err(|error| format!("interface call: {}", error.description()))?;
        let _ = client.close(control);
        response
            .get(..read)
            .map(|bytes| bytes.to_vec())
            .ok_or_else(|| String::from("short interface response"))
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
