// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `route` utility.

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
    use raven_proto_net::{IfName, IfaceRequest, IfaceResponse, ROUTE_GATEWAY, Route};
    use raven_route::Command;
    use raven_rt::entry;
    use raven_scheme::Client;

    entry!(main);

    const READ_WRITE: u32 = 3;
    const CONTROL_RESPONSE: usize = 4096;

    fn main(args: raven_rt::Args) -> i32 {
        let values: Vec<&str> = args.iter().skip(1).collect();
        let command = match raven_route::parse(&values) {
            Ok(command) => command,
            Err(error) => {
                raven_rt::eprintln!("route: {}", error.message());
                return 1;
            }
        };
        match run(command) {
            Ok(()) => 0,
            Err(error) => {
                raven_rt::eprintln!("route: {error}");
                1
            }
        }
    }

    fn run(command: Command<'_>) -> Result<(), String> {
        let client = Client::new();
        let control = client
            .open("net:", READ_WRITE)
            .map_err(|error| error.description().to_string())?;
        let result = match command {
            Command::List => list(&client, control),
            Command::Add(spec) => {
                let name = IfName::new(spec.iface).ok_or("invalid interface name")?;
                let response = request(
                    &client,
                    control,
                    IfaceRequest::RouteAddOn {
                        name,
                        route: Route {
                            dest: spec.dest,
                            prefix: spec.prefix,
                            gateway: spec.gateway,
                        },
                    },
                )?;
                expect_ok(&response)?;
                print_change("add", spec.dest, spec.prefix, spec.gateway, spec.iface);
                Ok(())
            }
            Command::Delete {
                dest,
                prefix,
                iface,
            } => {
                let name = IfName::new(iface).ok_or("invalid interface name")?;
                let response = request(
                    &client,
                    control,
                    IfaceRequest::RouteDelOn { name, dest, prefix },
                )?;
                expect_ok(&response)?;
                print_change("delete", dest, prefix, None, iface);
                Ok(())
            }
        };
        let _ = client.close(control);
        result
    }

    fn list(client: &Client, control: Handle) -> Result<(), String> {
        let response = request(client, control, IfaceRequest::RouteList)?;
        let count = count(&response)?;
        let mut offset = 8usize;
        for _ in 0..count {
            let length = route_info_len(&response, offset)?;
            let record = response
                .get(offset..offset + length)
                .ok_or("short route record")?;
            let decoded = IfaceResponse::decode(record)
                .map_err(|_| String::from("route record decode failed"))?;
            let IfaceResponse::RouteInfo(route) = decoded else {
                return Err(String::from("unexpected route record"));
            };
            print_route(&route);
            offset = offset.checked_add(length).ok_or("route offset overflow")?;
        }
        if offset != response.len() {
            return Err(String::from("trailing route response bytes"));
        }
        Ok(())
    }

    fn request(client: &Client, control: Handle, request: IfaceRequest) -> Result<Vec<u8>, String> {
        let mut encoded = [0u8; 128];
        let length = request
            .encode(&mut encoded)
            .map_err(|_| String::from("route request encode failed"))?;
        let mut response = [0u8; CONTROL_RESPONSE];
        let read = client
            .call(control, &encoded[..length], &mut response)
            .map_err(|error| error.description().to_string())?;
        Ok(response.get(..read).ok_or("short response")?.to_vec())
    }

    fn expect_ok(response: &[u8]) -> Result<(), String> {
        match IfaceResponse::decode(response) {
            Ok(IfaceResponse::Ok) => Ok(()),
            Ok(IfaceResponse::Err(error)) => Err(error.description().to_string()),
            _ => Err(String::from("unexpected route response")),
        }
    }

    fn count(response: &[u8]) -> Result<u16, String> {
        match IfaceResponse::decode(response.get(..8).ok_or("short route count")?)
            .map_err(|_| String::from("route count decode failed"))?
        {
            IfaceResponse::Count { count } => Ok(count),
            _ => Err(String::from("route list did not return a count")),
        }
    }

    fn route_info_len(response: &[u8], offset: usize) -> Result<usize, String> {
        let flags = *response.get(offset + 11).ok_or("short route flags")?;
        let name_len = usize::from(*response.get(offset + 12).ok_or("short route name")?);
        let base = 13usize
            .checked_add(name_len)
            .ok_or("route length overflow")?;
        let extra = if flags & ROUTE_GATEWAY != 0 { 4 } else { 0 };
        base.checked_add(extra)
            .ok_or_else(|| String::from("route length overflow"))
    }

    fn print_route(route: &raven_proto_net::RouteInfo) {
        let dest = destination_text(route.dest, route.prefix);
        let flags = raven_route::flags_text(route.flags);
        match route.gateway {
            Some(gateway) => raven_rt::println!(
                "route: {dest} via {} dev {} flags={flags}",
                ipv4_text(gateway),
                route.iface.as_str()
            ),
            None => raven_rt::println!("route: {dest} dev {} flags={flags}", route.iface.as_str()),
        }
    }

    fn print_change(
        action: &str,
        dest: [u8; 4],
        prefix: u8,
        gateway: Option<[u8; 4]>,
        iface: &str,
    ) {
        let dest = destination_text(dest, prefix);
        match gateway {
            Some(gateway) => raven_rt::println!(
                "route: {action} {dest} via {} dev {iface}",
                ipv4_text(gateway)
            ),
            None => raven_rt::println!("route: {action} {dest} dev {iface}"),
        }
    }

    fn destination_text(dest: [u8; 4], prefix: u8) -> String {
        if prefix == 0 {
            String::from("default")
        } else {
            format!("{}/{prefix}", ipv4_text(dest))
        }
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
