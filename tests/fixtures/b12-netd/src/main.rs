// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Loopback client/server probe for the B12a `netd` qtest.
//!
//! The probe uses the public scheme/client API only. It prints one line per checked area so the
//! qtest can pin the protocol and lifecycle results.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::format;
    use alloc::string::{String, ToString};
    use alloc::vec::Vec;

    use raven_abi::errno::Errno;
    use raven_abi::types::Handle;
    use raven_proto_net::socket::{Call, Endpoint, Response, SocketRequest};
    use raven_proto_net::{IfaceRequest, IfaceResponse, Shutdown, SockOpt};
    use raven_rt::entry;

    entry!(main);

    const READ_WRITE: u32 = 3;
    const TCP_SERVER: Endpoint = Endpoint {
        ip: [127, 0, 0, 1],
        port: 7000,
    };
    const UDP_SERVER: Endpoint = Endpoint {
        ip: [127, 0, 0, 1],
        port: 7001,
    };
    const UDP_CLIENT: Endpoint = Endpoint {
        ip: [127, 0, 0, 1],
        port: 7002,
    };

    fn main(args: raven_rt::Args) -> i32 {
        let phase = args.get(1).unwrap_or("first");
        match run(phase) {
            Ok(()) => 0,
            Err(message) => {
                raven_rt::eprintln!("netd: {message}");
                1
            }
        }
    }

    fn run(phase: &str) -> Result<(), String> {
        let client = raven_scheme::Client::new();
        check_control(&client)?;
        check_tcp(&client, phase)?;
        check_udp(&client)?;
        raven_rt::println!(
            "netd: {} ok",
            if phase == "second" {
                "restart"
            } else {
                "probe"
            }
        );
        Ok(())
    }

    fn check_control(client: &raven_scheme::Client) -> Result<(), String> {
        let handle = client
            .open("net:", READ_WRITE)
            .map_err(|error| format!("control open: {}", error.description()))?;
        let request = IfaceRequest::List;
        let mut encoded = [0u8; 64];
        let length = request
            .encode(&mut encoded)
            .map_err(|_| "iface encode".to_string())?;
        let mut response = [0u8; 256];
        let read = client
            .call(handle, &encoded[..length], &mut response)
            .map_err(|error| format!("control call: {}", error.description()))?;
        let response = &response[..read];
        let count = IfaceResponse::decode(response.get(..8).ok_or("short count")?)
            .map_err(|_| "count decode".to_string())?;
        if count != (IfaceResponse::Count { count: 1 }) {
            return Err("count != 1".to_string());
        }
        let name_len = *response.get(14).ok_or("short iface")? as usize;
        let iface_len = 21usize.checked_add(name_len).ok_or("iface length")?;
        let iface = IfaceResponse::decode(response.get(8..8 + iface_len).ok_or("short iface")?)
            .map_err(|_| "iface decode".to_string())?;
        let IfaceResponse::Iface(info) = iface else {
            return Err("list did not return an interface".to_string());
        };
        if info.name.as_str() != "lo" || info.addr != [127, 0, 0, 1] || info.prefix != 8 {
            return Err("interface is not lo 127.0.0.1/8".to_string());
        }
        let _ = client.close(handle);
        raven_rt::println!("netd: iface lo 127.0.0.1/8");
        Ok(())
    }

    fn check_tcp(client: &raven_scheme::Client, phase: &str) -> Result<(), String> {
        let listener = client
            .open("net:tcp/127.0.0.1:7000", READ_WRITE)
            .map_err(|error| format!("tcp listener open: {}", error.description()))?;
        expect_ok(
            call(client, listener, Call::Listen { backlog: 4 })?,
            "listen",
        )?;
        let socket = client
            .open("net:tcp/127.0.0.1:7000", READ_WRITE)
            .map_err(|error| format!("tcp client open: {}", error.description()))?;
        expect_ok(call(client, socket, Call::Connect(TCP_SERVER))?, "connect")?;

        let id = accept(client, listener)?;
        let accepted = client
            .open(&format!("net:accept/{id}"), READ_WRITE)
            .map_err(|error| format!("accepted open: {}", error.description()))?;

        let payload: &[u8] = if phase == "second" {
            b"second"
        } else {
            b"first"
        };
        client
            .write(socket, 0, payload)
            .map_err(|error| format!("tcp send: {}", error.description()))?;
        let mut buffer = [0u8; 64];
        let read = read_retry(client, accepted, &mut buffer)?;
        if &buffer[..read] != payload {
            return Err("tcp request bytes differ".to_string());
        }
        client
            .write(accepted, 0, &buffer[..read])
            .map_err(|error| format!("tcp echo: {}", error.description()))?;
        let read = read_retry(client, socket, &mut buffer)?;
        if &buffer[..read] != payload {
            return Err("tcp response bytes differ".to_string());
        }

        expect_ok(
            call(
                client,
                socket,
                Call::SetSockOpt {
                    opt: SockOpt::RcvBuf,
                    value: 8192,
                },
            )?,
            "setsockopt",
        )?;
        if call(
            client,
            socket,
            Call::GetSockOpt {
                opt: SockOpt::RcvBuf,
            },
        )? != Response::Value(8192)
        {
            return Err("getsockopt differs".to_string());
        }
        if call(client, listener, Call::GetSockName)? != Response::Endpoint(TCP_SERVER) {
            return Err("getsockname differs".to_string());
        }
        expect_ok(
            call(
                client,
                socket,
                Call::Shutdown {
                    how: Shutdown::Write,
                },
            )?,
            "shutdown",
        )?;
        let read = read_retry(client, accepted, &mut buffer)?;
        if read != 0 {
            return Err("orderly close did not return EOF".to_string());
        }
        let _ = client.close(accepted);
        let _ = client.close(socket);
        let _ = client.close(listener);
        raven_rt::println!("netd: tcp echo ok");
        Ok(())
    }

    fn check_udp(client: &raven_scheme::Client) -> Result<(), String> {
        let server = client
            .open("net:udp/127.0.0.1:7001", READ_WRITE)
            .map_err(|error| format!("udp server open: {}", error.description()))?;
        let peer = client
            .open("net:udp/127.0.0.1:7002", READ_WRITE)
            .map_err(|error| format!("udp client open: {}", error.description()))?;
        expect_ok(
            call(client, server, Call::Bind(UDP_SERVER))?,
            "udp bind server",
        )?;
        expect_ok(
            call(client, peer, Call::Bind(UDP_CLIENT))?,
            "udp bind client",
        )?;

        let request = SocketRequest::Write {
            data: b"udp",
            to: Some(UDP_SERVER),
        };
        let mut encoded = [0u8; 64];
        let length = request.encode(&mut encoded).map_err(|_| "udp write")?;
        let mut response = [0u8; 128];
        let read = client
            .call(peer, &encoded[..length], &mut response)
            .map_err(|error| format!("udp write call: {}", error.description()))?;
        if Response::decode(&response[..read]) != Ok(Response::Write { len: 3 }) {
            return Err("udp write response differs".to_string());
        }

        let (data, from) = read_typed(client, server, 32)?;
        if data != b"udp" || from != Some(UDP_CLIENT) {
            return Err("udp receive bytes/source differ".to_string());
        }
        let endpoint = from.ok_or("missing udp source")?;
        let request = SocketRequest::Write {
            data: b"udp",
            to: Some(endpoint),
        };
        let length = request.encode(&mut encoded).map_err(|_| "udp echo")?;
        let read = client
            .call(server, &encoded[..length], &mut response)
            .map_err(|error| format!("udp echo call: {}", error.description()))?;
        if Response::decode(&response[..read]) != Ok(Response::Write { len: 3 }) {
            return Err("udp echo response differs".to_string());
        }
        let (data, from) = read_typed(client, peer, 32)?;
        if data != b"udp" || from != Some(UDP_SERVER) {
            return Err("udp echo bytes/source differ".to_string());
        }
        let _ = client.close(peer);
        let _ = client.close(server);
        raven_rt::println!("netd: udp echo ok");
        Ok(())
    }

    fn accept(client: &raven_scheme::Client, listener: Handle) -> Result<u32, String> {
        for _ in 0..400 {
            match call(client, listener, Call::Accept)? {
                Response::Socket { id } => return Ok(id),
                Response::Err(Errno::EAGAIN) => {
                    let _ = raven_rt::nanosleep(5_000_000);
                }
                other => return Err(format!("accept response {other:?}")),
            }
        }
        Err("accept timed out".to_string())
    }

    fn read_retry(
        client: &raven_scheme::Client,
        file: Handle,
        buffer: &mut [u8],
    ) -> Result<usize, String> {
        for _ in 0..400 {
            match client.read(file, 0, buffer) {
                Ok(read) => return Ok(read),
                Err(Errno::EAGAIN) => {
                    let _ = raven_rt::nanosleep(5_000_000);
                }
                Err(error) => return Err(format!("read: {}", error.description())),
            }
        }
        Err("read timed out".to_string())
    }

    fn read_typed(
        client: &raven_scheme::Client,
        file: Handle,
        len: u32,
    ) -> Result<(Vec<u8>, Option<Endpoint>), String> {
        let request = SocketRequest::Read { len };
        let mut encoded = [0u8; 32];
        let length = request.encode(&mut encoded).map_err(|_| "read encode")?;
        let mut response = [0u8; 256];
        let read = client
            .call(file, &encoded[..length], &mut response)
            .map_err(|error| format!("typed read call: {}", error.description()))?;
        let response = &response[..read];
        if response.get(4..6) == Some(&[6, 0]) {
            return Err(format!(
                "typed read error: {:?}",
                Response::decode(response)
            ));
        }
        let flags = *response.get(6).ok_or("typed read flags")?;
        let header = if flags & 1 != 0 { 17 } else { 11 };
        let header_response = Response::decode(response.get(..header).ok_or("typed header")?)
            .map_err(|_| "typed header decode".to_string())?;
        let Response::Read { len, from } = header_response else {
            return Err("typed read response differs".to_string());
        };
        let payload = response
            .get(header..header + len as usize)
            .ok_or("typed payload")?
            .to_vec();
        Ok((payload, from))
    }

    fn call(client: &raven_scheme::Client, file: Handle, call: Call) -> Result<Response, String> {
        let request = SocketRequest::Call(call);
        let mut encoded = [0u8; 64];
        let length = request.encode(&mut encoded).map_err(|_| "call encode")?;
        let mut response = [0u8; 128];
        let read = client
            .call(file, &encoded[..length], &mut response)
            .map_err(|error| format!("call: {}", error.description()))?;
        Response::decode(&response[..read]).map_err(|_| "call response decode".to_string())
    }

    fn expect_ok(response: Response, what: &str) -> Result<(), String> {
        if response == Response::Ok {
            Ok(())
        } else {
            Err(format!("{what}: {response:?}"))
        }
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
