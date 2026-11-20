// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The strict `[network]` configuration section and its atomic drop-in writer.
//!
//! `raven config` validates the same section through [`crate::schema`]; `raven net apply`
//! (K197) calls [`write_drop_in`] to publish a validated `/etc/raven/conf.d/network.toml`.
//! The netd service re-reads that file at boot, so the writer is the only authoring path.

use std::fs;
use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::ConfigError;
use crate::node::Node;

/// The drop-in path, relative to the configuration root.
pub const NETWORK_DROP_IN: &str = "etc/raven/conf.d/network.toml";

/// Longest accepted hostname, matching `hostname` and `sysd`.
pub const HOSTNAME_MAX: usize = 64;

/// How the network interface gets its IPv4 address.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetworkMode {
    /// Keep the image's DHCP-first behaviour.
    Dhcp,
    /// Use the explicit address, prefix, gateway and DNS list.
    Static,
}

/// A static IPv4 configuration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StaticNetwork {
    /// Interface address.
    pub address: Ipv4Addr,
    /// Prefix length, `0..=32` (a static address must use `1..=32` in practice).
    pub prefix: u8,
    /// Default gateway; zero is never accepted.
    pub gateway: Option<Ipv4Addr>,
    /// DNS servers kept available to resolvd.
    pub dns: Vec<Ipv4Addr>,
}

/// A validated `[network]` section.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetworkConfig {
    /// Configured hostname, applied through `sysd` at boot.
    pub hostname: Option<String>,
    /// DHCP or static addressing.
    pub mode: NetworkMode,
    /// Static parameters; required when `mode` is [`NetworkMode::Static`].
    pub static_network: Option<StaticNetwork>,
}

impl NetworkConfig {
    /// A DHCP configuration with an optional hostname.
    #[must_use]
    pub fn dhcp(hostname: Option<String>) -> Self {
        Self {
            hostname,
            mode: NetworkMode::Dhcp,
            static_network: None,
        }
    }

    /// A static configuration.
    #[must_use]
    pub fn statically(
        hostname: Option<String>,
        address: Ipv4Addr,
        prefix: u8,
        gateway: Option<Ipv4Addr>,
        dns: Vec<Ipv4Addr>,
    ) -> Self {
        Self {
            hostname,
            mode: NetworkMode::Static,
            static_network: Some(StaticNetwork {
                address,
                prefix,
                gateway,
                dns,
            }),
        }
    }

    /// Validate the cross-field rules that a single node cannot check.
    pub fn validate(&self) -> Result<(), String> {
        if let Some(hostname) = &self.hostname {
            validate_hostname(hostname)?;
        }
        match (self.mode, &self.static_network) {
            (NetworkMode::Static, None) => Err(String::from(
                "`network.mode` is `static` but `network.static.address` is missing",
            )),
            (NetworkMode::Static, Some(static_network)) => validate_static(static_network),
            (NetworkMode::Dhcp, Some(static_network)) => validate_static(static_network),
            (NetworkMode::Dhcp, None) => Ok(()),
        }
    }

    /// Convert the section to a configuration node.
    #[must_use]
    pub fn to_node(&self) -> Node {
        let mut network = std::collections::BTreeMap::new();
        if let Some(hostname) = &self.hostname {
            network.insert("hostname".to_string(), Node::String(hostname.clone()));
        }
        network.insert(
            "mode".to_string(),
            Node::String(
                match self.mode {
                    NetworkMode::Dhcp => "dhcp",
                    NetworkMode::Static => "static",
                }
                .to_string(),
            ),
        );
        if let Some(static_network) = &self.static_network {
            let mut table = std::collections::BTreeMap::new();
            table.insert(
                "address".to_string(),
                Node::String(static_network.address.to_string()),
            );
            table.insert(
                "prefix".to_string(),
                Node::Integer(i64::from(static_network.prefix)),
            );
            if let Some(gateway) = static_network.gateway {
                table.insert("gateway".to_string(), Node::String(gateway.to_string()));
            }
            if !static_network.dns.is_empty() {
                table.insert(
                    "dns".to_string(),
                    Node::Array(
                        static_network
                            .dns
                            .iter()
                            .map(|server| Node::String(server.to_string()))
                            .collect(),
                    ),
                );
            }
            network.insert("static".to_string(), Node::Table(table));
        }
        Node::Table(network)
    }

    /// Render the section as the drop-in TOML text.
    #[must_use]
    pub fn render(&self) -> String {
        let mut text = String::from("[network]\n");
        if let Some(hostname) = &self.hostname {
            text.push_str(&format!("hostname = \"{hostname}\"\n"));
        }
        match self.mode {
            NetworkMode::Dhcp => text.push_str("mode = \"dhcp\"\n"),
            NetworkMode::Static => text.push_str("mode = \"static\"\n"),
        }
        if let Some(static_network) = &self.static_network {
            text.push_str("\n[network.static]\n");
            text.push_str(&format!("address = \"{}\"\n", static_network.address));
            text.push_str(&format!("prefix = {}\n", static_network.prefix));
            if let Some(gateway) = static_network.gateway {
                text.push_str(&format!("gateway = \"{gateway}\"\n"));
            }
            if !static_network.dns.is_empty() {
                let servers: Vec<String> = static_network
                    .dns
                    .iter()
                    .map(|server| format!("\"{server}\""))
                    .collect();
                text.push_str(&format!("dns = [{}]\n", servers.join(", ")));
            }
        }
        text
    }
}

/// Parse a `[network]` node into [`NetworkConfig`].
pub fn from_node(node: &Node) -> Result<NetworkConfig, String> {
    let table = node
        .as_table()
        .ok_or_else(|| String::from("`network` must be a table"))?;
    let mut hostname = None;
    let mut mode = None;
    let mut static_network = None;
    for (key, value) in table {
        match key.as_str() {
            "hostname" => {
                let name = value
                    .as_str()
                    .ok_or_else(|| String::from("`network.hostname` must be a string"))?;
                validate_hostname(name)?;
                hostname = Some(name.to_string());
            }
            "mode" => {
                let value = value
                    .as_str()
                    .ok_or_else(|| String::from("`network.mode` must be a string"))?;
                mode = Some(match value {
                    "dhcp" => NetworkMode::Dhcp,
                    "static" => NetworkMode::Static,
                    _ => {
                        return Err(String::from("`network.mode` must be `dhcp` or `static`"));
                    }
                });
            }
            "static" => static_network = Some(parse_static(value)?),
            other => {
                return Err(format!("unknown configuration key `network.{other}`"));
            }
        }
    }
    let mode = mode.ok_or_else(|| String::from("`network.mode` is required"))?;
    let config = NetworkConfig {
        hostname,
        mode,
        static_network,
    };
    config.validate()?;
    Ok(config)
}

/// Validate one `[network]` node and attach its file to any error.
pub fn validate_node(file: &Path, node: &Node) -> Result<(), ConfigError> {
    from_node(node)
        .map(|_| ())
        .map_err(|message| ConfigError::file(file, message).with_key("network"))
}

/// Atomically write the validated drop-in below `root`, creating `conf.d`.
pub fn write_drop_in(root: &Path, config: &NetworkConfig) -> Result<PathBuf, ConfigError> {
    config
        .validate()
        .map_err(|message| ConfigError::file(NETWORK_DROP_IN, message))?;
    let path = root.join(NETWORK_DROP_IN);
    let directory = path
        .parent()
        .ok_or_else(|| ConfigError::file(&path, "drop-in path has no parent"))?;
    fs::create_dir_all(directory).map_err(|error| {
        ConfigError::file(
            directory,
            format!("cannot create drop-in directory: {error}"),
        )
    })?;
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let temporary = directory.join(format!(".network.toml.tmp.{}.{unique}", std::process::id()));
    fs::write(&temporary, config.render())
        .map_err(|error| ConfigError::file(&temporary, format!("cannot write: {error}")))?;
    fs::rename(&temporary, &path).map_err(|error| {
        let _ = fs::remove_file(&temporary);
        ConfigError::file(&path, format!("cannot replace: {error}"))
    })?;
    Ok(path)
}

fn parse_static(node: &Node) -> Result<StaticNetwork, String> {
    let table = node
        .as_table()
        .ok_or_else(|| String::from("`network.static` must be a table"))?;
    let mut address = None;
    let mut prefix = None;
    let mut gateway = None;
    let mut dns = Vec::new();
    for (key, value) in table {
        match key.as_str() {
            "address" => address = Some(parse_address(value)?),
            "prefix" => {
                let value = value
                    .as_i64()
                    .ok_or_else(|| String::from("`network.static.prefix` must be an integer"))?;
                if !(0..=32).contains(&value) {
                    return Err(String::from(
                        "`network.static.prefix` must be between 0 and 32",
                    ));
                }
                prefix = Some(value as u8);
            }
            "gateway" => gateway = Some(parse_gateway(value)?),
            "dns" => dns = parse_dns(value)?,
            other => {
                return Err(format!(
                    "unknown configuration key `network.static.{other}`"
                ));
            }
        }
    }
    let address = address.ok_or_else(|| String::from("`network.static.address` is required"))?;
    let prefix = prefix.ok_or_else(|| String::from("`network.static.prefix` is required"))?;
    let static_network = StaticNetwork {
        address,
        prefix,
        gateway,
        dns,
    };
    validate_static(&static_network)?;
    Ok(static_network)
}

fn validate_static(static_network: &StaticNetwork) -> Result<(), String> {
    if static_network.prefix > 32 {
        return Err(String::from(
            "`network.static.prefix` must be between 0 and 32",
        ));
    }
    if static_network.address.is_unspecified() {
        return Err(String::from("`network.static.address` must not be 0.0.0.0"));
    }
    if let Some(gateway) = static_network.gateway
        && gateway.is_unspecified()
    {
        return Err(String::from("`network.static.gateway` must not be 0.0.0.0"));
    }
    if static_network.dns.iter().any(Ipv4Addr::is_unspecified) {
        return Err(String::from(
            "`network.static.dns` entries must not be 0.0.0.0",
        ));
    }
    Ok(())
}

fn parse_address(node: &Node) -> Result<Ipv4Addr, String> {
    let value = node
        .as_str()
        .ok_or_else(|| String::from("`network.static.address` must be a string"))?;
    value
        .parse()
        .map_err(|_| format!("`network.static.address` is not an IPv4 address: `{value}`"))
}

fn parse_gateway(node: &Node) -> Result<Ipv4Addr, String> {
    let value = node
        .as_str()
        .ok_or_else(|| String::from("`network.static.gateway` must be a string"))?;
    if value.trim().is_empty() {
        return Err(String::from(
            "`network.static.gateway` must not be empty or zero",
        ));
    }
    let address: Ipv4Addr = value
        .parse()
        .map_err(|_| format!("`network.static.gateway` is not an IPv4 address: `{value}`"))?;
    if address.is_unspecified() {
        return Err(String::from("`network.static.gateway` must not be 0.0.0.0"));
    }
    Ok(address)
}

fn parse_dns(node: &Node) -> Result<Vec<Ipv4Addr>, String> {
    let values = node
        .as_array()
        .ok_or_else(|| String::from("`network.static.dns` must be an array"))?;
    if values.is_empty() {
        return Err(String::from("`network.static.dns` must not be empty"));
    }
    let mut servers = Vec::with_capacity(values.len());
    for value in values {
        let text = value
            .as_str()
            .ok_or_else(|| String::from("`network.static.dns` entries must be strings"))?;
        let server: Ipv4Addr = text
            .parse()
            .map_err(|_| format!("`network.static.dns` entry is not an IPv4 address: `{text}`"))?;
        if server.is_unspecified() {
            return Err(String::from(
                "`network.static.dns` entries must not be 0.0.0.0",
            ));
        }
        servers.push(server);
    }
    Ok(servers)
}

/// A hostname is bounded, printable and free of TOML/path delimiters.
pub fn validate_hostname(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err(String::from("`network.hostname` must not be empty"));
    }
    if name.len() > HOSTNAME_MAX {
        return Err(format!(
            "`network.hostname` is longer than {HOSTNAME_MAX} bytes"
        ));
    }
    if name
        .bytes()
        .any(|byte| byte.is_ascii_control() || matches!(byte, b'/' | b'"' | b'\\'))
    {
        return Err(String::from(
            "`network.hostname` contains a control, slash, quote or backslash",
        ));
    }
    if name.bytes().any(|byte| byte.is_ascii_whitespace()) {
        return Err(String::from(
            "`network.hostname` must not contain whitespace",
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "network_ut.rs"]
mod tests;
