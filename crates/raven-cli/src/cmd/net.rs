// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `raven net`: read-only interface, route and status view over the `net:` scheme.

use std::ffi::OsString;
use std::fmt::Write as _;

use raven_output::{Exit, Format, Record, Value};

use super::BackendError;
use crate::Context;
use crate::commands::{Command, CommandMeta};

static NET_META: CommandMeta = CommandMeta {
    name: "net",
    summary: "show network interfaces and IPv4 routes",
    usage: "raven net <if|route|status> [--json]",
    reserved: false,
};

pub(crate) static NET_COMMAND: NetCommand = NetCommand;

/// The `net` command.
pub(crate) struct NetCommand;

impl Command for NetCommand {
    fn meta(&self) -> &'static CommandMeta {
        &NET_META
    }

    fn run(&self, context: &mut Context<'_>, args: &[OsString]) -> Exit {
        #[cfg(target_os = "ravenbsd")]
        let backend = SchemeBackend;
        #[cfg(not(target_os = "ravenbsd"))]
        let backend = UnavailableBackend;
        run_with_backend(context, &backend, args)
    }
}

/// One interface and its counters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Interface {
    /// Interface name.
    pub(crate) name: String,
    /// `up`, `no-carrier` or `down`.
    pub(crate) state: String,
    /// Dotted IPv4 address.
    pub(crate) address: Option<String>,
    /// Prefix length in bits.
    pub(crate) prefix: Option<u8>,
    /// MTU in bytes.
    pub(crate) mtu: u16,
    /// Colon-separated hardware address.
    pub(crate) mac: String,
    /// Packet and byte counters when the scheme provides them.
    pub(crate) counters: Option<Counters>,
}

/// Interface counters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Counters {
    /// Frames received.
    pub(crate) rx_packets: u64,
    /// Frames transmitted.
    pub(crate) tx_packets: u64,
    /// Bytes received.
    pub(crate) rx_bytes: u64,
    /// Bytes transmitted.
    pub(crate) tx_bytes: u64,
    /// Receive errors.
    pub(crate) rx_errors: u64,
    /// Transmit errors.
    pub(crate) tx_errors: u64,
    /// Received frames dropped.
    pub(crate) rx_drops: u64,
    /// Transmitted frames dropped.
    pub(crate) tx_drops: u64,
}

/// One IPv4 route.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Route {
    /// `default` or a dotted destination with its prefix.
    pub(crate) destination: String,
    /// Dotted next hop, absent for a directly connected route.
    pub(crate) gateway: Option<String>,
    /// Interface carrying the route.
    pub(crate) interface: String,
    /// `UP`, `GATEWAY` and `HOST` names.
    pub(crate) flags: Vec<String>,
}

/// The read-only data source.
pub(crate) trait NetBackend {
    /// Read the interface table.
    fn interfaces(&self) -> Result<Vec<Interface>, BackendError>;
    /// Read the route table.
    fn routes(&self) -> Result<Vec<Route>, BackendError>;
}

/// The host placeholder: no `net:` server in the host binary.
#[cfg(not(target_os = "ravenbsd"))]
pub(crate) struct UnavailableBackend;

#[cfg(not(target_os = "ravenbsd"))]
impl NetBackend for UnavailableBackend {
    fn interfaces(&self) -> Result<Vec<Interface>, BackendError> {
        Err(BackendError::unavailable("net"))
    }

    fn routes(&self) -> Result<Vec<Route>, BackendError> {
        Err(BackendError::unavailable("net"))
    }
}

pub(crate) fn run_with_backend(
    context: &mut Context<'_>,
    backend: &dyn NetBackend,
    args: &[OsString],
) -> Exit {
    let Some(command) = args.first() else {
        return context.usage("net needs an if, route, status or apply subcommand");
    };
    let rest = &args[1..];
    match command.to_string_lossy().as_ref() {
        "if" if rest.is_empty() => run_if(context, backend),
        "route" if rest.is_empty() => run_route(context, backend),
        "status" if rest.is_empty() => run_status(context, backend),
        "apply" => context.fail(
            "EUNAVAILABLE",
            "raven net apply is K198's writer; raven net is read-only",
        ),
        "if" | "route" | "status" => context.usage("this net subcommand takes no arguments"),
        _ => context.usage("net subcommand is if, route, status or apply"),
    }
}

fn run_if(context: &mut Context<'_>, backend: &dyn NetBackend) -> Exit {
    let interfaces = match backend.interfaces() {
        Ok(interfaces) => interfaces,
        Err(error) => return backend_error(context, error),
    };
    let routes = match backend.routes() {
        Ok(routes) => routes,
        Err(error) => return backend_error(context, error),
    };
    if context.format == Format::Json {
        for interface in &interfaces {
            let gateway = gateway_for(&routes, &interface.name);
            let record = Record::new("net.if")
                .string("name", &interface.name)
                .string("state", &interface.state)
                .field("address", optional_string(&interface.address))
                .field(
                    "prefix",
                    interface
                        .prefix
                        .map_or(Value::Null, |prefix| Value::Unsigned(u64::from(prefix))),
                )
                .field("gateway", optional_string(&gateway))
                .u64("mtu", u64::from(interface.mtu))
                .string("mac", &interface.mac)
                .field("counters", counters_value(interface.counters.as_ref()));
            let result = context.emit_record(&record);
            if result != Exit::Success {
                return result;
            }
        }
        Exit::Success
    } else if context.format == Format::Human {
        let mut text = String::new();
        let _ = writeln!(
            text,
            "{:<8} {:<10} {:<20} {:<12} {:>10} {:>10}",
            "NAME", "STATE", "ADDRESS", "GATEWAY", "RX_PACKETS", "TX_PACKETS"
        );
        for interface in &interfaces {
            let address = match (&interface.address, interface.prefix) {
                (Some(address), Some(prefix)) => format!("{address}/{prefix}"),
                (Some(address), None) => address.clone(),
                _ => String::from("-"),
            };
            let gateway =
                gateway_for(&routes, &interface.name).unwrap_or_else(|| String::from("-"));
            let (rx, tx) = interface.counters.map_or_else(
                || (String::from("-"), String::from("-")),
                |counters| {
                    (
                        counters.rx_packets.to_string(),
                        counters.tx_packets.to_string(),
                    )
                },
            );
            let _ = writeln!(
                text,
                "{:<8} {:<10} {:<20} {:<12} {:>10} {:>10}",
                interface.name, interface.state, address, gateway, rx, tx
            );
        }
        context.write_stdout(&text)
    } else {
        Exit::Success
    }
}

fn run_route(context: &mut Context<'_>, backend: &dyn NetBackend) -> Exit {
    let routes = match backend.routes() {
        Ok(routes) => routes,
        Err(error) => return backend_error(context, error),
    };
    if context.format == Format::Json {
        for route in &routes {
            let record = Record::new("net.route")
                .string("destination", &route.destination)
                .field("gateway", optional_string(&route.gateway))
                .string("interface", &route.interface)
                .array(
                    "flags",
                    route.flags.iter().cloned().map(Value::String).collect(),
                );
            let result = context.emit_record(&record);
            if result != Exit::Success {
                return result;
            }
        }
        Exit::Success
    } else if context.format == Format::Human {
        let mut text = String::new();
        let _ = writeln!(
            text,
            "{:<14} {:<12} {:<10} FLAGS",
            "DESTINATION", "GATEWAY", "INTERFACE"
        );
        for route in &routes {
            let gateway = route.gateway.as_deref().unwrap_or("-");
            let _ = writeln!(
                text,
                "{:<14} {:<12} {:<10} {}",
                route.destination,
                gateway,
                route.interface,
                route.flags.join(",")
            );
        }
        context.write_stdout(&text)
    } else {
        Exit::Success
    }
}

fn run_status(context: &mut Context<'_>, backend: &dyn NetBackend) -> Exit {
    let interfaces = match backend.interfaces() {
        Ok(interfaces) => interfaces,
        Err(error) => return backend_error(context, error),
    };
    let routes = match backend.routes() {
        Ok(routes) => routes,
        Err(error) => return backend_error(context, error),
    };
    let up = interfaces
        .iter()
        .filter(|interface| interface.state == "up")
        .count();
    let default_gateway = routes
        .iter()
        .find(|route| route.destination == "default")
        .and_then(|route| route.gateway.clone());
    let ready = interfaces.iter().any(|interface| {
        interface.state == "up"
            && interface
                .address
                .as_deref()
                .is_some_and(|address| address != "127.0.0.1")
    });
    if context.format == Format::Json {
        let record = Record::new("net.status")
            .u64("interfaces", interfaces.len() as u64)
            .u64("up", up as u64)
            .field("default_gateway", optional_string(&default_gateway))
            .bool("ready", ready);
        context.emit_record(&record)
    } else if context.format == Format::Human {
        let mut text = String::new();
        let _ = writeln!(text, "interfaces: {} (up: {})", interfaces.len(), up);
        let _ = writeln!(
            text,
            "default gateway: {}",
            default_gateway.as_deref().unwrap_or("-")
        );
        let _ = writeln!(text, "ready: {}", if ready { "yes" } else { "no" });
        context.write_stdout(&text)
    } else {
        Exit::Success
    }
}

fn gateway_for(routes: &[Route], interface: &str) -> Option<String> {
    routes
        .iter()
        .find(|route| route.destination == "default" && route.interface == interface)
        .and_then(|route| route.gateway.clone())
}

fn optional_string(value: &Option<String>) -> Value {
    value
        .as_ref()
        .map_or(Value::Null, |value| Value::String(value.clone()))
}

fn counters_value(counters: Option<&Counters>) -> Value {
    let Some(counters) = counters else {
        return Value::Null;
    };
    Value::Object(vec![
        ("rx_packets".into(), Value::Unsigned(counters.rx_packets)),
        ("tx_packets".into(), Value::Unsigned(counters.tx_packets)),
        ("rx_bytes".into(), Value::Unsigned(counters.rx_bytes)),
        ("tx_bytes".into(), Value::Unsigned(counters.tx_bytes)),
        ("rx_errors".into(), Value::Unsigned(counters.rx_errors)),
        ("tx_errors".into(), Value::Unsigned(counters.tx_errors)),
        ("rx_drops".into(), Value::Unsigned(counters.rx_drops)),
        ("tx_drops".into(), Value::Unsigned(counters.tx_drops)),
    ])
}

fn backend_error(context: &mut Context<'_>, error: BackendError) -> Exit {
    context.fail_context(&error.code, &error.message, Some(("command", "net")))
}

#[cfg(target_os = "ravenbsd")]
mod target {
    //! The target backend: `net:` control CALLs through the scheme client.

    use raven_proto_fs::{WIRE_OPEN_READ, WIRE_OPEN_WRITE};
    use raven_proto_net::{
        CounterInfo, IF_LINK_UP, IF_LOOPBACK, IF_UP, IfName, IfaceInfo, IfaceRequest,
        IfaceResponse, ROUTE_GATEWAY, ROUTE_HOST, ROUTE_UP, RouteInfo,
    };
    use raven_scheme::Client;

    use super::{BackendError, Counters, Interface, NetBackend, Route};

    const CONTROL_RESPONSE: usize = 4096;

    pub(crate) struct SchemeBackend;

    impl NetBackend for SchemeBackend {
        fn interfaces(&self) -> Result<Vec<Interface>, BackendError> {
            let client = Client::new();
            let control = client
                .open("net:", u32::from(WIRE_OPEN_READ | WIRE_OPEN_WRITE))
                .map_err(|error| net_error("open net:", error.description()))?;
            let result = read_interfaces(&client, control);
            let _ = client.close(control);
            result
        }

        fn routes(&self) -> Result<Vec<Route>, BackendError> {
            let client = Client::new();
            let control = client
                .open("net:", u32::from(WIRE_OPEN_READ | WIRE_OPEN_WRITE))
                .map_err(|error| net_error("open net:", error.description()))?;
            let result = read_routes(&client, control);
            let _ = client.close(control);
            result
        }
    }

    fn read_interfaces(
        client: &Client,
        control: raven_abi::types::Handle,
    ) -> Result<Vec<Interface>, BackendError> {
        let mut encoded = [0u8; 32];
        let length = IfaceRequest::List
            .encode(&mut encoded)
            .map_err(|error| net_error("encode iface list:", error))?;
        let mut response = [0u8; CONTROL_RESPONSE];
        let read = client
            .call(control, &encoded[..length], &mut response)
            .map_err(|error| net_error("iface list:", error.description()))?;
        let count = decode_count(&response[..read])?;
        let mut offset = 8usize;
        let mut interfaces = Vec::new();
        for _ in 0..count {
            let name_len = usize::from(
                *response
                    .get(offset + 6)
                    .ok_or_else(|| malformed("short interface record"))?,
            );
            let length = 21usize
                .checked_add(name_len)
                .ok_or_else(|| malformed("interface record length overflow"))?;
            let end = offset
                .checked_add(length)
                .ok_or_else(|| malformed("interface record end overflow"))?;
            let record = response
                .get(offset..end)
                .ok_or_else(|| malformed("short interface record"))?;
            let info = match IfaceResponse::decode(record)
                .map_err(|error| malformed(&format!("interface decode: {error}")))?
            {
                IfaceResponse::Iface(info) => info,
                IfaceResponse::Err(error) => {
                    return Err(net_error("interface list:", error.description()));
                }
                _ => return Err(malformed("interface list record is not an interface")),
            };
            let counters = read_counters(client, control, info.name)?;
            interfaces.push(interface_from(info, counters));
            offset = end;
        }
        if offset != read {
            return Err(malformed("trailing interface response bytes"));
        }
        Ok(interfaces)
    }

    fn read_counters(
        client: &Client,
        control: raven_abi::types::Handle,
        name: IfName,
    ) -> Result<Option<CounterInfo>, BackendError> {
        let mut encoded = [0u8; 32];
        let length = IfaceRequest::Counters { name }
            .encode(&mut encoded)
            .map_err(|error| net_error("encode counters:", error))?;
        let mut response = [0u8; 128];
        let read = client
            .call(control, &encoded[..length], &mut response)
            .map_err(|error| net_error("counters:", error.description()))?;
        match IfaceResponse::decode(&response[..read])
            .map_err(|error| malformed(&format!("counters decode: {error}")))?
        {
            IfaceResponse::Counters(counters) => Ok(Some(counters)),
            IfaceResponse::Err(error) => Err(net_error("counters:", error.description())),
            _ => Err(malformed("counters response is not a counters record")),
        }
    }

    fn read_routes(
        client: &Client,
        control: raven_abi::types::Handle,
    ) -> Result<Vec<Route>, BackendError> {
        let mut encoded = [0u8; 32];
        let length = IfaceRequest::RouteList
            .encode(&mut encoded)
            .map_err(|error| net_error("encode route list:", error))?;
        let mut response = [0u8; CONTROL_RESPONSE];
        let read = client
            .call(control, &encoded[..length], &mut response)
            .map_err(|error| net_error("route list:", error.description()))?;
        let count = decode_count(&response[..read])?;
        let mut offset = 8usize;
        let mut routes = Vec::new();
        for _ in 0..count {
            let flags = *response
                .get(offset + 11)
                .ok_or_else(|| malformed("short route flags"))?;
            let name_len = usize::from(
                *response
                    .get(offset + 12)
                    .ok_or_else(|| malformed("short route name"))?,
            );
            let base = 13usize
                .checked_add(name_len)
                .ok_or_else(|| malformed("route length overflow"))?;
            let gateway_len = if flags & ROUTE_GATEWAY != 0 { 4 } else { 0 };
            let length = base
                .checked_add(gateway_len)
                .ok_or_else(|| malformed("route length overflow"))?;
            let end = offset
                .checked_add(length)
                .ok_or_else(|| malformed("route end overflow"))?;
            let record = response
                .get(offset..end)
                .ok_or_else(|| malformed("short route record"))?;
            let info = match IfaceResponse::decode(record)
                .map_err(|error| malformed(&format!("route decode: {error}")))?
            {
                IfaceResponse::RouteInfo(info) => info,
                IfaceResponse::Err(error) => {
                    return Err(net_error("route list:", error.description()));
                }
                _ => return Err(malformed("route list record is not a route")),
            };
            routes.push(route_from(info));
            offset = end;
        }
        if offset != read {
            return Err(malformed("trailing route response bytes"));
        }
        Ok(routes)
    }

    fn decode_count(response: &[u8]) -> Result<u16, BackendError> {
        let header = response
            .get(..8)
            .ok_or_else(|| malformed("short net response"))?;
        match IfaceResponse::decode(header)
            .map_err(|error| malformed(&format!("count decode: {error}")))?
        {
            IfaceResponse::Count { count } => Ok(count),
            IfaceResponse::Err(error) => Err(net_error("net list:", error.description())),
            _ => Err(malformed("net list did not return a count")),
        }
    }

    fn interface_from(info: IfaceInfo, counters: Option<CounterInfo>) -> Interface {
        Interface {
            name: info.name.as_str().to_string(),
            state: state_text(info.flags).to_string(),
            address: Some(ipv4_text(info.addr)),
            prefix: Some(info.prefix),
            mtu: info.mtu,
            mac: mac_text(info.mac),
            counters: counters.map(|counters| Counters {
                rx_packets: counters.rx_packets,
                tx_packets: counters.tx_packets,
                rx_bytes: counters.rx_bytes,
                tx_bytes: counters.tx_bytes,
                rx_errors: counters.rx_errors,
                tx_errors: counters.tx_errors,
                rx_drops: counters.rx_drops,
                tx_drops: counters.tx_drops,
            }),
        }
    }

    fn route_from(info: RouteInfo) -> Route {
        Route {
            destination: if info.prefix == 0 {
                String::from("default")
            } else {
                format!("{}/{}", ipv4_text(info.dest), info.prefix)
            },
            gateway: info.gateway.map(ipv4_text),
            interface: info.iface.as_str().to_string(),
            flags: route_flags(info.flags),
        }
    }

    fn state_text(flags: u8) -> &'static str {
        if flags & IF_LOOPBACK != 0 {
            "up"
        } else if flags & IF_UP == 0 {
            "down"
        } else if flags & IF_LINK_UP == 0 {
            "no-carrier"
        } else {
            "up"
        }
    }

    fn route_flags(flags: u8) -> Vec<String> {
        let mut names = Vec::new();
        if flags & ROUTE_UP != 0 {
            names.push(String::from("UP"));
        }
        if flags & ROUTE_GATEWAY != 0 {
            names.push(String::from("GATEWAY"));
        }
        if flags & ROUTE_HOST != 0 {
            names.push(String::from("HOST"));
        }
        names
    }

    fn ipv4_text(address: [u8; 4]) -> String {
        format!(
            "{}.{}.{}.{}",
            address[0], address[1], address[2], address[3]
        )
    }

    fn mac_text(mac: [u8; 6]) -> String {
        format!(
            "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
        )
    }

    fn net_error(context: &str, message: impl std::fmt::Display) -> BackendError {
        BackendError::new("ENET", &format!("{context} {message}"))
    }

    fn malformed(message: &str) -> BackendError {
        BackendError::new("EPROTO", message)
    }
}

#[cfg(target_os = "ravenbsd")]
use target::SchemeBackend;

#[cfg(test)]
#[path = "net_ut.rs"]
mod tests;
