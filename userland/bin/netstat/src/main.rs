// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `netstat` utility.

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

    use raven_abi::types::Handle;
    use raven_netstat::{Options, state_text, transport_text};
    use raven_proto_net::{IfName, IfaceRequest, IfaceResponse};
    use raven_rt::entry;
    use raven_scheme::Client;

    entry!(main);

    const READ_WRITE: u32 = 3;
    const CONTROL_RESPONSE: usize = 4096;

    fn main(args: raven_rt::Args) -> i32 {
        let values: Vec<&str> = args.iter().skip(1).collect();
        let options = match raven_netstat::parse(&values) {
            Ok(options) => options,
            Err(error) => {
                raven_rt::eprintln!("netstat: {}", error.message());
                return 1;
            }
        };
        match run(&options) {
            Ok(()) => 0,
            Err(error) => {
                raven_rt::eprintln!("netstat: {error}");
                1
            }
        }
    }

    fn run(options: &Options) -> Result<(), String> {
        let client = Client::new();
        let control = client
            .open("net:", READ_WRITE)
            .map_err(|error| error.description().to_string())?;
        let result = (|| {
            if options.sockets {
                sockets(&client, control)?;
            }
            if options.counters {
                counters(&client, control)?;
            }
            Ok(())
        })();
        let _ = client.close(control);
        result
    }

    fn sockets(client: &Client, control: Handle) -> Result<(), String> {
        let response = request(client, control, IfaceRequest::SocketList)?;
        let count = count(&response)?;
        let mut offset = 8usize;
        for _ in 0..count {
            let length = socket_entry_len(&response, offset)?;
            let record = response
                .get(offset..offset + length)
                .ok_or("short socket record")?;
            let decoded = IfaceResponse::decode(record)
                .map_err(|_| String::from("socket record decode failed"))?;
            let IfaceResponse::Socket(entry) = decoded else {
                return Err(String::from("unexpected socket record"));
            };
            print_socket(&entry);
            offset = offset.checked_add(length).ok_or("socket offset overflow")?;
        }
        if offset != response.len() {
            return Err(String::from("trailing socket response bytes"));
        }
        Ok(())
    }

    fn counters(client: &Client, control: Handle) -> Result<(), String> {
        let response = request(client, control, IfaceRequest::List)?;
        let count = count(&response)?;
        let mut offset = 8usize;
        for _ in 0..count {
            let length = iface_len(&response, offset)?;
            let record = response
                .get(offset..offset + length)
                .ok_or("short interface record")?;
            let decoded = IfaceResponse::decode(record)
                .map_err(|_| String::from("interface record decode failed"))?;
            let IfaceResponse::Iface(info) = decoded else {
                return Err(String::from("unexpected interface record"));
            };
            let name = IfName::new(info.name.as_str()).ok_or("invalid interface name")?;
            let response = request(client, control, IfaceRequest::Counters { name })?;
            match IfaceResponse::decode(&response) {
                Ok(IfaceResponse::Counters(values)) => print_counters(info.name.as_str(), &values),
                Ok(IfaceResponse::Err(error)) => raven_rt::println!(
                    "netstat: {} counters unavailable ({})",
                    info.name.as_str(),
                    error.description()
                ),
                _ => return Err(String::from("unexpected counter response")),
            }
            offset = offset
                .checked_add(length)
                .ok_or("interface offset overflow")?;
        }
        if offset != response.len() {
            return Err(String::from("trailing interface response bytes"));
        }
        Ok(())
    }

    fn request(client: &Client, control: Handle, request: IfaceRequest) -> Result<Vec<u8>, String> {
        let mut encoded = [0u8; 64];
        let length = request
            .encode(&mut encoded)
            .map_err(|_| String::from("netstat request encode failed"))?;
        let mut response = [0u8; CONTROL_RESPONSE];
        let read = client
            .call(control, &encoded[..length], &mut response)
            .map_err(|error| error.description().to_string())?;
        Ok(response.get(..read).ok_or("short response")?.to_vec())
    }

    fn count(response: &[u8]) -> Result<u16, String> {
        match IfaceResponse::decode(response.get(..8).ok_or("short count")?)
            .map_err(|_| String::from("count decode failed"))?
        {
            IfaceResponse::Count { count } => Ok(count),
            _ => Err(String::from("list did not return a count")),
        }
    }

    fn iface_len(response: &[u8], offset: usize) -> Result<usize, String> {
        let name_len = usize::from(*response.get(offset + 6).ok_or("short interface name")?);
        21usize
            .checked_add(name_len)
            .ok_or_else(|| String::from("interface length overflow"))
    }

    fn socket_entry_len(response: &[u8], offset: usize) -> Result<usize, String> {
        let name_len = usize::from(*response.get(offset + 12).ok_or("short socket name")?);
        let remote = *response
            .get(offset + 19 + name_len)
            .ok_or("short socket remote flag")?;
        let base = 28usize
            .checked_add(name_len)
            .ok_or("socket length overflow")?;
        let extra = if remote == 1 { 6 } else { 0 };
        base.checked_add(extra)
            .ok_or_else(|| String::from("socket length overflow"))
    }

    fn print_socket(entry: &raven_proto_net::SocketEntry) {
        let local = endpoint_text(entry.local, entry.local_port);
        let remote = match entry.remote {
            Some(address) => endpoint_text(address, entry.remote_port),
            None => String::from("-"),
        };
        raven_rt::println!(
            "netstat: {} {} {} remote={} iface={} rx={} tx={}",
            transport_text(entry.transport),
            state_text(entry.state),
            local,
            remote,
            entry.iface.as_str(),
            entry.rx_queue,
            entry.tx_queue
        );
    }

    fn print_counters(iface: &str, counters: &raven_proto_net::CounterInfo) {
        raven_rt::println!(
            "netstat: {} rx_packets={} tx_packets={} rx_bytes={} tx_bytes={} \
             rx_errors={} tx_errors={} rx_drops={} tx_drops={}",
            iface,
            counters.rx_packets,
            counters.tx_packets,
            counters.rx_bytes,
            counters.tx_bytes,
            counters.rx_errors,
            counters.tx_errors,
            counters.rx_drops,
            counters.tx_drops
        );
    }

    fn endpoint_text(address: [u8; 4], port: u16) -> String {
        format!(
            "{}.{}.{}.{}:{}",
            address[0], address[1], address[2], address[3], port
        )
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
