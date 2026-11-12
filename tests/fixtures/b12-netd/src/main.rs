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
    use raven_rt::event::{EDGE, EventQueue, EvqEvent, SIGNALED};

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
        let (listener_event, listener_queue) = event_for(client, listener)?;
        let socket = client
            .open("net:tcp/127.0.0.1:7000", READ_WRITE)
            .map_err(|error| format!("tcp client open: {}", error.description()))?;
        expect_ok(call(client, socket, Call::Connect(TCP_SERVER))?, "connect")?;
        let (client_event, client_queue) = event_for(client, socket)?;

        let id = accept_event(client, listener, &listener_queue)?;
        raven_rt::println!("netd: accept path net:accept/{id}");
        let accepted = client
            .open(&format!("net:accept/{id}"), READ_WRITE)
            .map_err(|error| format!("accepted open: {}", error.description()))?;
        let (accepted_event, accepted_queue) = event_for(client, accepted)?;

        let payload: &[u8] = if phase == "second" {
            b"second"
        } else {
            b"first"
        };
        client
            .write(socket, 0, payload)
            .map_err(|error| format!("tcp send: {}", error.description()))?;
        let mut buffer = [0u8; 64];
        let read = read_event(client, accepted, &accepted_queue, &mut buffer)?;
        if &buffer[..read] != payload {
            return Err("tcp request bytes differ".to_string());
        }
        client
            .write(accepted, 0, &buffer[..read])
            .map_err(|error| format!("tcp echo: {}", error.description()))?;
        let read = read_event(client, socket, &client_queue, &mut buffer)?;
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
        let client_local = call(client, socket, Call::GetSockName)?;
        if call(client, accepted, Call::GetPeerName)? != client_local {
            return Err("accepted peer endpoint differs".to_string());
        }
        raven_rt::println!("netd: endpoints ok");
        raven_rt::println!("netd: options ok");
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
        let read = read_event(client, accepted, &accepted_queue, &mut buffer)?;
        if read != 0 {
            return Err("orderly close did not return EOF".to_string());
        }
        let _ = raven_rt::handle_close(accepted_event);
        let _ = raven_rt::handle_close(client_event);
        let _ = raven_rt::handle_close(listener_event);
        let _ = raven_rt::handle_close(listener_queue.handle());
        let _ = raven_rt::handle_close(client_queue.handle());
        let _ = raven_rt::handle_close(accepted_queue.handle());
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
        let (server_event, server_queue) = event_for(client, server)?;
        let (peer_event, peer_queue) = event_for(client, peer)?;

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

        let (data, from) = read_typed_event(client, server, &server_queue, 32)?;
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
        let (data, from) = read_typed_event(client, peer, &peer_queue, 32)?;
        if data != b"udp" || from != Some(UDP_SERVER) {
            return Err("udp echo bytes/source differ".to_string());
        }
        let _ = raven_rt::handle_close(server_event);
        let _ = raven_rt::handle_close(peer_event);
        let _ = raven_rt::handle_close(server_queue.handle());
        let _ = raven_rt::handle_close(peer_queue.handle());
        let _ = client.close(peer);
        let _ = client.close(server);
        raven_rt::println!("netd: udp echo ok");
        Ok(())
    }

    fn accept_event(
        client: &raven_scheme::Client,
        listener: Handle,
        queue: &EventQueue,
    ) -> Result<u32, String> {
        for _ in 0..100 {
            match call(client, listener, Call::Accept)? {
                Response::Socket { id } => return Ok(id),
                Response::Err(Errno::EAGAIN) => wait_event(queue)?,
                other => return Err(format!("accept response {other:?}")),
            }
        }
        Err("accept timed out".to_string())
    }

    fn read_event(
        client: &raven_scheme::Client,
        file: Handle,
        queue: &EventQueue,
        buffer: &mut [u8],
    ) -> Result<usize, String> {
        for _ in 0..100 {
            match client.read(file, 0, buffer) {
                Ok(read) => return Ok(read),
                Err(Errno::EAGAIN) => wait_event(queue)?,
                Err(error) => return Err(format!("read: {}", error.description())),
            }
        }
        Err("read timed out".to_string())
    }

    fn read_typed_event(
        client: &raven_scheme::Client,
        file: Handle,
        queue: &EventQueue,
        len: u32,
    ) -> Result<(Vec<u8>, Option<Endpoint>), String> {
        for _ in 0..100 {
            let response = raw_request(client, file, &SocketRequest::Read { len })?;
            if response.get(4..6) == Some(&[6, 0]) {
                let status = u16::from_le_bytes(
                    response
                        .get(2..4)
                        .ok_or("short typed error")?
                        .try_into()
                        .map_err(|_| "short typed error")?,
                );
                if status == Errno::EAGAIN.0 {
                    wait_event(queue)?;
                    continue;
                }
                return Err(format!(
                    "typed read error: {:?}",
                    Response::decode(&response)
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
            return Ok((payload, from));
        }
        Err("typed read timed out".to_string())
    }

    fn event_for(
        client: &raven_scheme::Client,
        file: Handle,
    ) -> Result<(Handle, EventQueue), String> {
        let response = raw_request(client, file, &SocketRequest::Call(Call::Event))?;
        raven_rt::println!("netd: readiness event");
        if Response::decode(response.get(..6).ok_or("short event response")?) != Ok(Response::Ok) {
            return Err("event call was not OK".to_string());
        }
        let handle = u32::from_le_bytes(
            response
                .get(6..10)
                .ok_or("short event handle")?
                .try_into()
                .map_err(|_| "short event handle")?,
        );
        let handle = Handle(handle);
        let queue = EventQueue::create().map_err(|error| format!("event queue: {error:?}"))?;
        queue
            .add(handle, SIGNALED | EDGE, 0)
            .map_err(|error| format!("event add: {error:?}"))?;
        Ok((handle, queue))
    }

    fn wait_event(queue: &EventQueue) -> Result<(), String> {
        let mut events = [EvqEvent::default(); 1];
        let count = queue
            .wait(&mut events, 100_000_000)
            .map_err(|error| format!("event wait: {error:?}"))?;
        if count == 0 {
            return Err("event wait timed out".to_string());
        }
        Ok(())
    }

    fn raw_request(
        client: &raven_scheme::Client,
        file: Handle,
        request: &SocketRequest<'_>,
    ) -> Result<Vec<u8>, String> {
        let mut encoded = [0u8; 128];
        let length = request.encode(&mut encoded).map_err(|_| "request encode")?;
        let mut response = [0u8; 256];
        let read = client
            .call(file, &encoded[..length], &mut response)
            .map_err(|error| format!("call: {}", error.description()))?;
        Ok(response[..read].to_vec())
    }

    fn call(client: &raven_scheme::Client, file: Handle, call: Call) -> Result<Response, String> {
        let response = raw_request(client, file, &SocketRequest::Call(call))?;
        Response::decode(&response).map_err(|_| "call response decode".to_string())
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
