// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! B70 end-to-end network probe.
//!
//! The probe runs as a one-shot service in the `hw-net` overlay of the real
//! `server` image. It reads the live DHCP lease from `net:`, verifies the
//! default route, sends ICMP echoes to the QEMU gateway, resolves a name via
//! the lease DNS server, and completes a host-to-guest TCP echo through QEMU
//! user networking and the guest socket table. It exits 0 only after every
//! step succeeds; init then exits 0, which the kernel's test-exit protocol
//! reports as QEMU status 33.

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

    use raven_abi::errno::Errno;
    use raven_abi::types::Handle;
    use raven_proto_net::socket::{Call, Response, SocketRequest};
    use raven_proto_net::{
        DHCP_TIMER_UNKNOWN, DhcpLeaseInfo, IfName, IfaceRequest, IfaceResponse, ROUTE_GATEWAY,
        RouteInfo, SOCKET_STATE_ESTABLISHED, SOCKET_TCP, SocketEntry,
    };
    use raven_rt::entry;
    use raven_scheme::Client;
    use resolvd::proto::{Address, QTYPE_A, Query, Reply, STATUS_ERROR, STATUS_OK};

    entry!(main);

    const READ: u32 = 1;
    const READ_WRITE: u32 = 3;
    const ECHO_PORT: u16 = 23070;
    const ECHO_PAYLOAD: &[u8] = b"b70-host-echo-payload";
    const ECHO_ACK: &[u8] = b"b70-host-ack";
    const PING_ID: u16 = 0x4230;
    const PING_PAYLOAD: &[u8] = b"b70-ping";
    const PING_COUNT: u16 = 3;
    const LEASE_ATTEMPTS: u32 = 600;
    const RESOLV_ATTEMPTS: u32 = 300;
    const ACCEPT_ATTEMPTS: u32 = 4_800;
    const IO_ATTEMPTS: u32 = 2_000;

    fn main(_args: raven_rt::Args) -> i32 {
        match run() {
            Ok(()) => {
                raven_rt::println!("b70: done");
                0
            }
            Err(message) => {
                raven_rt::eprintln!("b70: {message}");
                1
            }
        }
    }

    fn run() -> Result<(), String> {
        let client = Client::new();
        let control = client
            .open("net:", READ_WRITE)
            .map_err(|error| format!("open net:: {}", error.description()))?;

        let lease = check_lease(&client, control)?;
        print_lease(&lease)?;
        check_route(&client, control)?;
        check_ping(&client)?;
        check_resolver(&client)?;
        check_host_echo(&client, control)?;
        let _ = client.close(control);
        Ok(())
    }

    fn check_lease(client: &Client, control: Handle) -> Result<DhcpLeaseInfo, String> {
        let name = IfName::new("eth0").ok_or("invalid eth0 interface name")?;
        let mut last = String::from("no response");
        for _ in 0..LEASE_ATTEMPTS {
            let response = iface_request(client, control, IfaceRequest::DhcpLease { name })?;
            match IfaceResponse::decode(&response) {
                Ok(IfaceResponse::DhcpLease(lease)) => {
                    if lease.addr != [10, 0, 2, 15] || lease.prefix != 24 {
                        return Err(format!(
                            "unexpected lease {}.{}.{}.{}/{}",
                            lease.addr[0],
                            lease.addr[1],
                            lease.addr[2],
                            lease.addr[3],
                            lease.prefix
                        ));
                    }
                    if lease.router != Some([10, 0, 2, 2]) {
                        return Err(String::from("lease has no 10.0.2.2 router"));
                    }
                    if !lease.dns.contains(&[10, 0, 2, 3]) {
                        return Err(String::from("lease has no 10.0.2.3 DNS server"));
                    }
                    if lease.renew_ms == DHCP_TIMER_UNKNOWN
                        || lease.rebind_ms == DHCP_TIMER_UNKNOWN
                        || lease.expires_ms == DHCP_TIMER_UNKNOWN
                    {
                        return Err(String::from("lease timers are unknown"));
                    }
                    if !(lease.renew_ms < lease.rebind_ms && lease.rebind_ms < lease.expires_ms) {
                        return Err(String::from("lease timers are out of order"));
                    }
                    return Ok(lease);
                }
                Ok(IfaceResponse::Err(Errno::ENOENT)) => {
                    last = String::from("no live lease yet");
                }
                Ok(IfaceResponse::Err(error)) => {
                    return Err(format!("DHCP lease error: {}", error.description()));
                }
                Ok(other) => return Err(format!("unexpected DHCP response: {other:?}")),
                Err(_) => return Err(String::from("cannot decode DHCP lease response")),
            }
            let _ = raven_rt::nanosleep(100_000_000);
        }
        Err(format!("DHCP lease timed out: {last}"))
    }

    fn print_lease(lease: &DhcpLeaseInfo) -> Result<(), String> {
        let router = lease.router.ok_or("lease router missing")?;
        let dns = lease.dns.first().ok_or("lease DNS missing")?;
        raven_rt::println!(
            "b70: lease eth0 {}.{}.{}.{}/{} router={}.{}.{}.{} dns={}.{}.{}.{}",
            lease.addr[0],
            lease.addr[1],
            lease.addr[2],
            lease.addr[3],
            lease.prefix,
            router[0],
            router[1],
            router[2],
            router[3],
            dns[0],
            dns[1],
            dns[2],
            dns[3]
        );
        Ok(())
    }

    fn check_route(client: &Client, control: Handle) -> Result<(), String> {
        let response = iface_request(client, control, IfaceRequest::RouteList)?;
        let count = iface_count(&response)?;
        let mut offset = 8usize;
        let mut found = false;
        for _ in 0..count {
            let route = iface_route(&response, offset)?;
            offset = offset
                .checked_add(route_info_len(&response, offset)?)
                .ok_or("route offset overflow")?;
            if route.dest == [0, 0, 0, 0]
                && route.prefix == 0
                && route.gateway == Some([10, 0, 2, 2])
                && route.iface.as_str() == "eth0"
            {
                found = true;
            }
        }
        if offset != response.len() {
            return Err(String::from("trailing route-list bytes"));
        }
        if !found {
            return Err(String::from("no default route via 10.0.2.2 on eth0"));
        }
        raven_rt::println!("b70: route default via 10.0.2.2 dev eth0");
        Ok(())
    }

    fn check_ping(client: &Client) -> Result<(), String> {
        let file = client
            .open("net:icmp/10.0.2.2", READ_WRITE)
            .map_err(|error| format!("open icmp: {}", error.description()))?;
        for seq in 1..=PING_COUNT {
            let request = SocketRequest::IcmpEcho {
                id: PING_ID,
                seq,
                timeout_ms: 3_000,
                data: PING_PAYLOAD,
            };
            let mut encoded = [0u8; 128];
            let length = request
                .encode(&mut encoded)
                .map_err(|_| String::from("encode ICMP echo"))?;
            let mut response = [0u8; 64];
            let read = client
                .call(file, &encoded[..length], &mut response)
                .map_err(|error| format!("send ICMP echo: {}", error.description()))?;
            let bytes = response.get(..read).ok_or("short ICMP send response")?;
            match Response::decode(bytes) {
                Ok(Response::Ok) => {}
                Ok(other) => return Err(format!("unexpected ICMP send response: {other:?}")),
                Err(_) => return Err(String::from("cannot decode ICMP send response")),
            }
            let rtt = wait_icmp_reply(client, file, seq)?;
            raven_rt::println!("b70: ping 10.0.2.2 seq={seq} rtt={rtt}ms");
        }
        let _ = client.close(file);
        raven_rt::println!(
            "b70: ping {PING_COUNT} packets transmitted, {PING_COUNT} received, 0% packet loss"
        );
        Ok(())
    }

    fn wait_icmp_reply(client: &Client, file: Handle, seq: u16) -> Result<u32, String> {
        for _ in 0..300 {
            let request = SocketRequest::Read { len: 64 };
            let mut encoded = [0u8; 32];
            let length = request
                .encode(&mut encoded)
                .map_err(|_| String::from("encode ICMP read"))?;
            let mut response = [0u8; 128];
            let read = client
                .call(file, &encoded[..length], &mut response)
                .map_err(|error| format!("read ICMP reply: {}", error.description()))?;
            let bytes = response.get(..read).ok_or("short ICMP response")?;
            if bytes.len() < 22 {
                match Response::decode(bytes) {
                    Ok(Response::Err(Errno::EAGAIN)) => {}
                    Ok(Response::Err(Errno::ETIMEDOUT)) => {
                        return Err(format!("ICMP seq {seq} timed out in netd"));
                    }
                    Ok(other) => return Err(format!("unexpected ICMP read response: {other:?}")),
                    Err(_) => return Err(String::from("cannot decode ICMP read response")),
                }
                let _ = raven_rt::nanosleep(10_000_000);
                continue;
            }
            let header = Response::decode(&bytes[..22])
                .map_err(|_| String::from("cannot decode ICMP reply header"))?;
            let Response::Icmp {
                source,
                id,
                seq: reply_seq,
                rtt_ms,
                len,
            } = header
            else {
                return Err(String::from("ICMP reply header is not an echo reply"));
            };
            if source.ip != [10, 0, 2, 2] || id != PING_ID || reply_seq != seq {
                return Err(format!(
                    "ICMP reply does not match seq {seq}: id={id} seq={reply_seq}"
                ));
            }
            let payload = bytes
                .get(22..22 + usize::from(len))
                .ok_or("short ICMP reply payload")?;
            if payload != PING_PAYLOAD {
                return Err(String::from("ICMP reply payload differs"));
            }
            return Ok(rtt_ms);
        }
        Err(format!("ICMP seq {seq} reply timed out"))
    }

    fn check_resolver(client: &Client) -> Result<(), String> {
        wait_for_lease_nameserver()?;
        raven_rt::println!("b70: resolv.conf nameserver=10.0.2.3");

        let file = client
            .open("resolv:", READ)
            .map_err(|error| format!("open resolv:: {}", error.description()))?;
        let query = Query {
            qtype: QTYPE_A,
            name: "localhost",
        };
        let mut encoded = [0u8; 512];
        let length = query
            .encode(&mut encoded)
            .map_err(|_| String::from("encode DNS query"))?;
        let mut response = [0u8; 512];
        let mut last = String::from("no reply");
        let mut resolved = false;
        for _ in 0..20 {
            let read = client
                .call(file, &encoded[..length], &mut response)
                .map_err(|error| format!("resolve localhost: {}", error.description()))?;
            let bytes = response.get(..read).ok_or("short resolver reply")?;
            let reply =
                Reply::decode(bytes).map_err(|_| String::from("cannot decode resolver reply"))?;
            match (
                reply.status,
                reply.records.first().map(|record| record.address),
            ) {
                (STATUS_OK, Some(Address::V4(address))) if address == [127, 0, 0, 1] => {
                    raven_rt::println!("b70: dns localhost=127.0.0.1");
                    resolved = true;
                    break;
                }
                (STATUS_OK, Some(Address::V4(address))) => {
                    last = format!(
                        "localhost={}.{}.{}.{}",
                        address[0], address[1], address[2], address[3]
                    );
                }
                (STATUS_ERROR, _) => {
                    last = String::from("resolver returned STATUS_ERROR");
                }
                (status, address) => {
                    last = format!("localhost status={status} answer={address:?}");
                }
            }
            let _ = raven_rt::nanosleep(100_000_000);
        }
        let _ = client.close(file);
        if resolved {
            Ok(())
        } else {
            Err(format!(
                "resolver did not return localhost=127.0.0.1: {last}"
            ))
        }
    }

    fn wait_for_lease_nameserver() -> Result<(), String> {
        let mut last = String::from("no resolv.conf");
        for _ in 0..RESOLV_ATTEMPTS {
            match raven_rt::fs::read_to_vec("file:/etc/resolv.conf") {
                Ok(bytes) => match String::from_utf8(bytes) {
                    Ok(text) => {
                        if text
                            .lines()
                            .any(|line| line.trim() == "nameserver 10.0.2.3")
                        {
                            return Ok(());
                        }
                        last = String::from("10.0.2.3 is absent");
                    }
                    Err(_) => last = String::from("resolv.conf is not UTF-8"),
                },
                Err(error) => last = format!("read resolv.conf: {}", error.description()),
            }
            let _ = raven_rt::nanosleep(100_000_000);
        }
        Err(format!("lease DNS was not applied: {last}"))
    }

    fn check_host_echo(client: &Client, control: Handle) -> Result<(), String> {
        let path = format!("net:tcp/10.0.2.15:{ECHO_PORT}");
        let listener = client
            .open(&path, READ_WRITE)
            .map_err(|error| format!("open echo listener: {}", error.description()))?;
        expect_ok(
            call(client, listener, Call::Listen { backlog: 4 })?,
            "listen",
        )?;
        raven_rt::println!("b70: listening tcp 10.0.2.15:{ECHO_PORT}");

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
        let text = core::str::from_utf8(payload).map_err(|_| "echo payload is not UTF-8")?;
        raven_rt::println!("b70: echo received {length} bytes payload={text}");

        check_socket_table(client, control)?;

        let written = client
            .write(accepted, 0, payload)
            .map_err(|error| format!("echo write: {}", error.description()))?;
        if written != length {
            return Err(format!("echo wrote {written} of {length} bytes"));
        }
        raven_rt::println!("b70: echo sent {written} bytes");

        let ack_length = read_exact(client, accepted, ECHO_ACK.len(), &mut buffer)?;
        let ack = buffer.get(..ack_length).ok_or("short host ack")?;
        if ack != ECHO_ACK {
            return Err(format!("host ack differs: {ack:?}"));
        }
        let text = core::str::from_utf8(ack).map_err(|_| "host ack is not UTF-8")?;
        raven_rt::println!("b70: host ack {text}");

        let _ = client.close(accepted);
        let _ = client.close(listener);
        Ok(())
    }

    fn check_socket_table(client: &Client, control: Handle) -> Result<(), String> {
        let response = iface_request(client, control, IfaceRequest::SocketList)?;
        let count = iface_count(&response)?;
        let mut offset = 8usize;
        let mut found = false;
        for _ in 0..count {
            let entry = socket_entry(&response, offset)?;
            offset = offset
                .checked_add(socket_entry_len(&response, offset)?)
                .ok_or("socket offset overflow")?;
            if entry.transport == SOCKET_TCP
                && entry.state == SOCKET_STATE_ESTABLISHED
                && entry.local_port == ECHO_PORT
                && entry.remote.is_some()
                && entry.iface.as_str() == "eth0"
            {
                print_socket(&entry)?;
                found = true;
            }
        }
        if offset != response.len() {
            return Err(String::from("trailing socket-list bytes"));
        }
        if found {
            Ok(())
        } else {
            Err(String::from(
                "socket table does not show the established echo connection",
            ))
        }
    }

    fn print_socket(entry: &SocketEntry) -> Result<(), String> {
        let remote = entry.remote.ok_or("socket remote missing")?;
        raven_rt::println!(
            "b70: socket tcp ESTABLISHED {}.{}.{}.{}:{} remote={}.{}.{}.{}:{} iface={}",
            entry.local[0],
            entry.local[1],
            entry.local[2],
            entry.local[3],
            entry.local_port,
            remote[0],
            remote[1],
            remote[2],
            remote[3],
            entry.remote_port,
            entry.iface.as_str()
        );
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

    fn iface_request(
        client: &Client,
        control: Handle,
        request: IfaceRequest,
    ) -> Result<Vec<u8>, String> {
        let mut encoded = [0u8; 256];
        let length = request
            .encode(&mut encoded)
            .map_err(|_| String::from("encode interface request"))?;
        let mut response = [0u8; 4096];
        let read = client
            .call(control, &encoded[..length], &mut response)
            .map_err(|error| format!("interface call: {}", error.description()))?;
        response
            .get(..read)
            .map(|bytes| bytes.to_vec())
            .ok_or_else(|| String::from("short interface response"))
    }

    fn iface_count(response: &[u8]) -> Result<u16, String> {
        let head = response.get(..8).ok_or("short count response")?;
        match IfaceResponse::decode(head).map_err(|_| String::from("cannot decode count"))? {
            IfaceResponse::Count { count } => Ok(count),
            other => Err(format!("unexpected count record: {other:?}")),
        }
    }

    fn route_info_len(response: &[u8], offset: usize) -> Result<usize, String> {
        let flags_index = offset.checked_add(11).ok_or("route offset overflow")?;
        let flags = *response.get(flags_index).ok_or("short route flags")?;
        let name_index = offset.checked_add(12).ok_or("route offset overflow")?;
        let name_len = usize::from(*response.get(name_index).ok_or("short route name")?);
        let length = 13usize
            .checked_add(name_len)
            .ok_or("route length overflow")?;
        if flags & ROUTE_GATEWAY != 0 {
            length
                .checked_add(4)
                .ok_or_else(|| String::from("route gateway length overflow"))
        } else {
            Ok(length)
        }
    }

    fn iface_route(response: &[u8], offset: usize) -> Result<RouteInfo, String> {
        let length = route_info_len(response, offset)?;
        let end = offset.checked_add(length).ok_or("route end overflow")?;
        let record = response.get(offset..end).ok_or("short route record")?;
        match IfaceResponse::decode(record).map_err(|_| String::from("cannot decode route"))? {
            IfaceResponse::RouteInfo(route) => Ok(route),
            other => Err(format!("unexpected route record: {other:?}")),
        }
    }

    fn socket_entry_len(response: &[u8], offset: usize) -> Result<usize, String> {
        let name_index = offset.checked_add(12).ok_or("socket offset overflow")?;
        let name_len = usize::from(*response.get(name_index).ok_or("short socket name")?);
        let remote_index = 19usize
            .checked_add(name_len)
            .and_then(|value| offset.checked_add(value))
            .ok_or("socket offset overflow")?;
        let remote = *response
            .get(remote_index)
            .ok_or("short socket remote flag")?;
        let length = 28usize
            .checked_add(name_len)
            .ok_or("socket length overflow")?;
        if remote == 1 {
            length
                .checked_add(6)
                .ok_or_else(|| String::from("socket remote length overflow"))
        } else {
            Ok(length)
        }
    }

    fn socket_entry(response: &[u8], offset: usize) -> Result<SocketEntry, String> {
        let length = socket_entry_len(response, offset)?;
        let end = offset.checked_add(length).ok_or("socket end overflow")?;
        let record = response.get(offset..end).ok_or("short socket record")?;
        match IfaceResponse::decode(record).map_err(|_| String::from("cannot decode socket"))? {
            IfaceResponse::Socket(entry) => Ok(entry),
            other => Err(format!("unexpected socket record: {other:?}")),
        }
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
