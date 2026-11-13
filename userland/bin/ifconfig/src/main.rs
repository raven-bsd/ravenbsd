// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `ifconfig` utility.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::format;
    use alloc::string::String;
    use alloc::string::ToString;

    use raven_ifconfig::{Command, flags_text, netmask, parse};
    use raven_proto_net::{IfaceRequest, IfaceResponse};
    use raven_rt::entry;
    use raven_scheme::Client;

    entry!(main);

    const READ_WRITE: u32 = 3;

    fn main(args: raven_rt::Args) -> i32 {
        match run(args) {
            Ok(()) => 0,
            Err(error) => {
                raven_rt::eprintln!("ifconfig: {error}");
                1
            }
        }
    }

    fn run(args: raven_rt::Args) -> Result<(), String> {
        let values: alloc::vec::Vec<&str> = args.iter().skip(1).collect();
        let command = parse(&values).map_err(|error| String::from(error.message()))?;
        let client = Client::new();
        let control = client
            .open("net:", READ_WRITE)
            .map_err(|error| error.description().to_string())?;
        let result = match command {
            Command::List => list(&client, control),
            Command::Get(name) => get(&client, control, name),
            Command::SetAddress(name, address, prefix) => {
                set_address(&client, control, name, address, prefix)
            }
            Command::SetState(name, up) => set_state(&client, control, name, up),
        };
        let _ = client.close(control);
        result
    }

    fn list(client: &Client, control: raven_abi::types::Handle) -> Result<(), String> {
        let request = IfaceRequest::List;
        let mut encoded = [0u8; 32];
        let length = request
            .encode(&mut encoded)
            .map_err(|_| String::from("list encode failed"))?;
        let mut response = [0u8; 512];
        let read = client
            .call(control, &encoded[..length], &mut response)
            .map_err(|error| error.description().to_string())?;
        let bytes = response.get(..read).ok_or("short response")?;
        let count = IfaceResponse::decode(bytes.get(..8).ok_or("short count")?)
            .map_err(|_| String::from("count decode failed"))?;
        let IfaceResponse::Count { count } = count else {
            return Err(String::from("list did not return a count"));
        };
        let mut offset = 8usize;
        let mut index = 0u16;
        while index < count {
            let name_len = usize::from(*bytes.get(offset + 6).ok_or("short iface")?);
            let length = 21usize
                .checked_add(name_len)
                .ok_or("iface length overflow")?;
            let record = bytes
                .get(offset..offset + length)
                .ok_or("short iface record")?;
            let response =
                IfaceResponse::decode(record).map_err(|_| String::from("iface decode failed"))?;
            let IfaceResponse::Iface(info) = response else {
                return Err(String::from("list record is not an interface"));
            };
            print_iface(&info);
            offset = offset.checked_add(length).ok_or("offset overflow")?;
            index = index.saturating_add(1);
        }
        Ok(())
    }

    fn get(client: &Client, control: raven_abi::types::Handle, name: &str) -> Result<(), String> {
        let Some(name) = raven_proto_net::IfName::new(name) else {
            return Err(String::from("invalid interface name"));
        };
        let response = exchange(client, control, IfaceRequest::Get { name })?;
        let response = IfaceResponse::decode(&response.0[..response.1])
            .map_err(|_| String::from("iface response decode failed"))?;
        match response {
            IfaceResponse::Iface(info) => {
                print_iface(&info);
                Ok(())
            }
            IfaceResponse::Err(error) => Err(error.description().to_string()),
            _ => Err(String::from("unexpected response")),
        }
    }

    fn set_address(
        client: &Client,
        control: raven_abi::types::Handle,
        name: &str,
        address: [u8; 4],
        prefix: u8,
    ) -> Result<(), String> {
        let Some(name) = raven_proto_net::IfName::new(name) else {
            return Err(String::from("invalid interface name"));
        };
        let response = exchange(
            client,
            control,
            IfaceRequest::SetAddress {
                name,
                addr: address,
                prefix,
            },
        )?;
        expect_ok(&response.0[..response.1])?;
        raven_rt::println!(
            "ifconfig: {} address {}/{}",
            name.as_str(),
            ipv4_text(address),
            prefix
        );
        Ok(())
    }

    fn set_state(
        client: &Client,
        control: raven_abi::types::Handle,
        name: &str,
        up: bool,
    ) -> Result<(), String> {
        let Some(name) = raven_proto_net::IfName::new(name) else {
            return Err(String::from("invalid interface name"));
        };
        let response = exchange(client, control, IfaceRequest::SetState { name, up })?;
        expect_ok(&response.0[..response.1])?;
        raven_rt::println!(
            "ifconfig: {} {}",
            name.as_str(),
            if up { "up" } else { "down" }
        );
        Ok(())
    }

    fn exchange(
        client: &Client,
        control: raven_abi::types::Handle,
        request: IfaceRequest,
    ) -> Result<([u8; 512], usize), String> {
        let mut encoded = [0u8; 64];
        let length = request
            .encode(&mut encoded)
            .map_err(|_| String::from("request encode failed"))?;
        let mut response = [0u8; 512];
        let read = client
            .call(control, &encoded[..length], &mut response)
            .map_err(|error| error.description().to_string())?;
        if read == 0 || read > response.len() {
            return Err(String::from("short response"));
        }
        Ok((response, read))
    }

    fn expect_ok(response: &[u8]) -> Result<(), String> {
        let response =
            IfaceResponse::decode(response).map_err(|_| String::from("response decode failed"))?;
        match response {
            IfaceResponse::Ok => Ok(()),
            IfaceResponse::Err(error) => Err(error.description().to_string()),
            _ => Err(String::from("unexpected response")),
        }
    }

    fn print_iface(info: &raven_proto_net::IfaceInfo) {
        let flags = flags_text(info.flags);
        let mask = netmask(info.prefix);
        raven_rt::println!(
            "ifconfig: {} flags={} mtu {} inet {} netmask {}",
            info.name.as_str(),
            flags,
            info.mtu,
            ipv4_text(info.addr),
            ipv4_text(mask)
        );
    }

    fn ipv4_text(address: [u8; 4]) -> String {
        format!(
            "{}.{}.{}.{}",
            address[0], address[1], address[2], address[3]
        )
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
