// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Strict reader for `/etc/raven/conf.d/network.toml` in the target process.
//!
//! The host-side `raven-config` crate is the authoring and validation path; this module is the
//! `no_std` boot-time re-validation so a hand-edited or corrupt drop-in never reaches the stack.
//! It parses the small published subset (one `[network]` table and one `[network.static]` table)
//! and rejects unknown keys, duplicate keys and out-of-range values with a line number.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// Longest accepted hostname, matching `hostname` and `sysd`.
pub const HOSTNAME_MAX: usize = 64;

/// The drop-in path, relative to the configuration root.
pub const NETWORK_DROP_IN: &str = "/etc/raven/conf.d/network.toml";

/// How the interface gets its IPv4 address.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetworkMode {
    /// Keep the image's DHCP-first behaviour.
    Dhcp,
    /// Use the explicit static parameters.
    Static,
}

/// Validated static parameters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StaticConfig {
    /// Interface address.
    pub addr: [u8; 4],
    /// Prefix length.
    pub prefix: u8,
    /// Default gateway, never zero.
    pub gateway: Option<[u8; 4]>,
    /// DNS servers kept available to resolvd.
    pub dns: Vec<[u8; 4]>,
}

/// A validated network drop-in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetworkConfig {
    /// Configured hostname.
    pub hostname: Option<String>,
    /// Addressing mode.
    pub mode: NetworkMode,
    /// Static parameters when the mode is static.
    pub static_config: Option<StaticConfig>,
}

impl NetworkConfig {
    /// The static parameters to hand to the NIC, if this is a static configuration.
    #[must_use]
    pub fn static_config(&self) -> Option<StaticConfig> {
        self.static_config.clone()
    }
}

/// Why the boot-time load did not produce a configuration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoadError {
    /// The file service or initfs is not registered yet; retry the boot.
    NotReady,
    /// The file was readable but invalid.
    Invalid(String),
}

impl LoadError {
    /// Human-readable detail.
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::NotReady => String::from("file service is not ready"),
            Self::Invalid(message) => message.clone(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Section {
    None,
    Network,
    Static,
}

/// Parse one drop-in text.
pub fn parse(text: &str) -> Result<NetworkConfig, String> {
    let mut section = Section::None;
    let mut hostname = None;
    let mut mode = None;
    let mut address = None;
    let mut prefix = None;
    let mut gateway = None;
    let mut dns: Option<Vec<[u8; 4]>> = None;
    let mut seen_static_section = false;

    for (index, raw_line) in text.lines().enumerate() {
        let line_no = index + 1;
        let text = strip_comment(raw_line).trim();
        if text.is_empty() {
            continue;
        }
        if let Some(header) = text.strip_prefix('[') {
            let header = header
                .strip_suffix(']')
                .ok_or_else(|| format!("line {line_no}: unterminated section header"))?;
            section = match header.trim() {
                "network" => Section::Network,
                "network.static" => {
                    if seen_static_section {
                        return Err(format!(
                            "line {line_no}: duplicate section `[network.static]`"
                        ));
                    }
                    seen_static_section = true;
                    Section::Static
                }
                other => return Err(format!("line {line_no}: unknown section `[{other}]`")),
            };
            continue;
        }
        let (key, value) = text
            .split_once('=')
            .ok_or_else(|| format!("line {line_no}: missing `=`"))?;
        let key = key.trim();
        let value = value.trim();
        if key.is_empty() || value.is_empty() {
            return Err(format!("line {line_no}: missing key or value"));
        }
        match section {
            Section::Network => match key {
                "hostname" => {
                    if hostname.is_some() {
                        return Err(format!("line {line_no}: duplicate key `network.hostname`"));
                    }
                    let name = parse_string(value, line_no)?;
                    validate_hostname(&name)
                        .map_err(|message| format!("line {line_no}: {message}"))?;
                    hostname = Some(name);
                }
                "mode" => {
                    if mode.is_some() {
                        return Err(format!("line {line_no}: duplicate key `network.mode`"));
                    }
                    mode = Some(match parse_string(value, line_no)?.as_str() {
                        "dhcp" => NetworkMode::Dhcp,
                        "static" => NetworkMode::Static,
                        _ => {
                            return Err(format!(
                                "line {line_no}: `network.mode` must be `dhcp` or `static`"
                            ));
                        }
                    });
                }
                other => {
                    return Err(format!("line {line_no}: unknown key `network.{other}`"));
                }
            },
            Section::Static => match key {
                "address" => {
                    if address.is_some() {
                        return Err(format!(
                            "line {line_no}: duplicate key `network.static.address`"
                        ));
                    }
                    address = Some(parse_ipv4(&parse_string(value, line_no)?, line_no)?);
                }
                "prefix" => {
                    if prefix.is_some() {
                        return Err(format!(
                            "line {line_no}: duplicate key `network.static.prefix`"
                        ));
                    }
                    prefix = Some(parse_prefix(value, line_no)?);
                }
                "gateway" => {
                    if gateway.is_some() {
                        return Err(format!(
                            "line {line_no}: duplicate key `network.static.gateway`"
                        ));
                    }
                    let parsed = parse_ipv4(&parse_string(value, line_no)?, line_no)?;
                    if parsed == [0; 4] {
                        return Err(format!(
                            "line {line_no}: `network.static.gateway` must not be 0.0.0.0"
                        ));
                    }
                    gateway = Some(parsed);
                }
                "dns" => {
                    if dns.is_some() {
                        return Err(format!(
                            "line {line_no}: duplicate key `network.static.dns`"
                        ));
                    }
                    dns = Some(parse_dns(value, line_no)?);
                }
                other => {
                    return Err(format!(
                        "line {line_no}: unknown key `network.static.{other}`"
                    ));
                }
            },
            Section::None => {
                return Err(format!("line {line_no}: key `{key}` is outside a section"));
            }
        }
    }

    let mode = mode.ok_or_else(|| String::from("`network.mode` is required"))?;
    let static_config = match mode {
        NetworkMode::Static => {
            let address = address.ok_or_else(|| {
                String::from("`network.mode` is `static` but `network.static.address` is missing")
            })?;
            let prefix = prefix.ok_or_else(|| {
                String::from("`network.mode` is `static` but `network.static.prefix` is missing")
            })?;
            Some(StaticConfig {
                addr: address,
                prefix,
                gateway,
                dns: dns.unwrap_or_default(),
            })
        }
        // A static table in DHCP mode is parsed and validated but ignored, so the same file
        // can switch modes without a rewrite.
        NetworkMode::Dhcp => None,
    };
    Ok(NetworkConfig {
        hostname,
        mode,
        static_config,
    })
}

/// Read the drop-in from the live file service or the initfs fallback.
#[cfg(target_os = "none")]
pub fn load() -> Result<Option<NetworkConfig>, LoadError> {
    let paths = [
        alloc::format!("file:{NETWORK_DROP_IN}"),
        alloc::format!("initfs:{NETWORK_DROP_IN}"),
    ];
    let mut io_error = None;
    for path in &paths {
        match raven_rt::fs::read_to_vec(path) {
            Ok(data) => {
                let text = core::str::from_utf8(&data)
                    .map_err(|_| LoadError::Invalid(alloc::format!("{path}: file is not UTF-8")))?;
                return parse(text).map(Some).map_err(LoadError::Invalid);
            }
            Err(raven_abi::errno::Errno::ENOENT) => continue,
            Err(error) => {
                io_error = Some(alloc::format!("{path}: {}", error.description()));
            }
        }
    }
    if let Some(message) = io_error {
        return Err(LoadError::Invalid(message));
    }
    // Both paths returned ENOENT. If either namespace is mounted, the file is genuinely
    // absent; otherwise the file service is still starting and the caller should retry.
    if raven_rt::fs::stat("/").is_ok() || raven_rt::fs::stat("initfs:/").is_ok() {
        Ok(None)
    } else {
        Err(LoadError::NotReady)
    }
}

/// Validate a hostname with the same rules as `raven-config`.
pub fn validate_hostname(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err(String::from("`network.hostname` must not be empty"));
    }
    if name.len() > HOSTNAME_MAX {
        return Err(alloc::format!(
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

fn strip_comment(line: &str) -> &str {
    let mut in_string = false;
    let mut escaped = false;
    for (index, character) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match character {
            '\\' if in_string => escaped = true,
            '"' => in_string = !in_string,
            '#' if !in_string => return &line[..index],
            _ => {}
        }
    }
    line
}

fn parse_string(value: &str, line_no: usize) -> Result<String, String> {
    let value = value.trim();
    let Some(inner) = value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
    else {
        return Err(format!("line {line_no}: expected a quoted string"));
    };
    if inner
        .bytes()
        .any(|byte| byte.is_ascii_control() || matches!(byte, b'"' | b'\\'))
    {
        return Err(format!("line {line_no}: unsupported string character"));
    }
    Ok(inner.to_string())
}

fn parse_prefix(value: &str, line_no: usize) -> Result<u8, String> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(format!(
            "line {line_no}: `network.static.prefix` must be an integer"
        ));
    }
    let prefix: u16 = value
        .parse()
        .map_err(|_| format!("line {line_no}: `network.static.prefix` is too large"))?;
    if prefix > 32 {
        return Err(format!(
            "line {line_no}: `network.static.prefix` must be between 0 and 32"
        ));
    }
    Ok(prefix as u8)
}

fn parse_ipv4(value: &str, line_no: usize) -> Result<[u8; 4], String> {
    let mut octets = [0u8; 4];
    let mut parts = value.split('.');
    for octet in &mut octets {
        let part = parts
            .next()
            .ok_or_else(|| format!("line {line_no}: `{value}` is not an IPv4 address"))?;
        if part.is_empty() || part.len() > 3 || !part.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(format!("line {line_no}: `{value}` is not an IPv4 address"));
        }
        if part.len() > 1 && part.starts_with('0') {
            return Err(format!(
                "line {line_no}: `{value}` has a leading-zero octet"
            ));
        }
        let octet_value: u16 = part
            .parse()
            .map_err(|_| format!("line {line_no}: `{value}` is not an IPv4 address"))?;
        if octet_value > u16::from(u8::MAX) {
            return Err(format!("line {line_no}: `{value}` is not an IPv4 address"));
        }
        *octet = octet_value as u8;
    }
    if parts.next().is_some() {
        return Err(format!("line {line_no}: `{value}` is not an IPv4 address"));
    }
    Ok(octets)
}

fn parse_dns(value: &str, line_no: usize) -> Result<Vec<[u8; 4]>, String> {
    let inner = value
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .ok_or_else(|| format!("line {line_no}: `network.static.dns` must be an array"))?;
    let mut servers = Vec::new();
    for entry in inner.split(',') {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }
        let text = parse_string(entry, line_no)?;
        servers.push(parse_ipv4(&text, line_no)?);
    }
    if servers.is_empty() {
        return Err(format!(
            "line {line_no}: `network.static.dns` must not be empty"
        ));
    }
    Ok(servers)
}

#[cfg(test)]
#[path = "network_ut.rs"]
mod tests;
