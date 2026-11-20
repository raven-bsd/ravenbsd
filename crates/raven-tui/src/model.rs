// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Data-driven module registry, network form model and atomic drop-in writer.
//!
//! The network schema is the K198 contract:
//! `[network] hostname/mode` plus `[network.static] address/prefix/gateway/dns`. K198 owns the
//! backend apply; this module validates the form and writes the dedicated drop-in.

use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

/// The dedicated network drop-in, relative to the configuration root.
pub const NETWORK_FILE: &str = "etc/raven/conf.d/network.toml";
/// Allowed network modes.
pub const MODE_CHOICES: &[&str] = &["dhcp", "static"];

/// The kind of value a form field holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldKind {
    /// Free text.
    Text,
    /// One of a fixed choice list.
    Choice(&'static [&'static str]),
    /// Read-only information.
    Info,
}

/// One form field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    /// Stable key used by the writer and tests.
    pub key: &'static str,
    /// Display label.
    pub label: &'static str,
    /// One-line help.
    pub help: &'static str,
    /// Field kind.
    pub kind: FieldKind,
    /// Value loaded from the current drop-in (or a default).
    pub current: String,
    /// Edit value; empty means "use current".
    pub value: String,
    /// Validation error for this field.
    pub error: Option<String>,
}

impl Field {
    fn new(
        key: &'static str,
        label: &'static str,
        help: &'static str,
        kind: FieldKind,
        current: String,
    ) -> Self {
        Self {
            key,
            label,
            help,
            kind,
            current,
            value: String::new(),
            error: None,
        }
    }

    /// The value the form would submit: the edit value when non-empty, otherwise the current
    /// value.
    #[must_use]
    pub fn effective(&self) -> String {
        if self.value.is_empty() {
            self.current.clone()
        } else {
            self.value.clone()
        }
    }

    /// Display value (the effective value).
    #[must_use]
    pub fn display(&self) -> String {
        self.effective()
    }
}

/// One module in the left pane.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Module {
    /// Stable identifier.
    pub id: &'static str,
    /// Left-pane title.
    pub title: &'static str,
    /// One-line summary.
    pub summary: &'static str,
    /// Form fields.
    pub fields: Vec<Field>,
    /// Whether the Save action writes the network drop-in.
    pub writable: bool,
}

/// Current network values read from the drop-in (or defaults).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetworkCurrent {
    /// Hostname.
    pub hostname: String,
    /// `dhcp` or `static`.
    pub mode: String,
    /// Static address.
    pub address: String,
    /// Static prefix length.
    pub prefix: String,
    /// Static gateway.
    pub gateway: String,
    /// DNS servers.
    pub dns: Vec<String>,
}

impl Default for NetworkCurrent {
    fn default() -> Self {
        Self {
            hostname: String::from("raven"),
            mode: String::from("dhcp"),
            address: String::new(),
            prefix: String::from("24"),
            gateway: String::new(),
            dns: Vec::new(),
        }
    }
}

/// A validated network plan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetworkPlan {
    /// Hostname.
    pub hostname: String,
    /// `dhcp` or `static`.
    pub mode: String,
    /// Static address (empty for DHCP).
    pub address: String,
    /// Static prefix length (0 for DHCP).
    pub prefix: u8,
    /// Static gateway (empty for DHCP).
    pub gateway: String,
    /// DNS servers (empty for DHCP unless supplied).
    pub dns: Vec<String>,
}

impl NetworkPlan {
    /// Render the K198 drop-in.
    #[must_use]
    pub fn to_toml(&self) -> String {
        let mut text = String::from("# Managed by raven config (K197).\n[network]\n");
        text.push_str(&format!("hostname = \"{}\"\n", toml_escape(&self.hostname)));
        text.push_str(&format!("mode = \"{}\"\n", toml_escape(&self.mode)));
        if self.mode == "static" {
            text.push_str("[network.static]\n");
            text.push_str(&format!("address = \"{}\"\n", toml_escape(&self.address)));
            text.push_str(&format!("prefix = {}\n", self.prefix));
            text.push_str(&format!("gateway = \"{}\"\n", toml_escape(&self.gateway)));
            text.push_str("dns = [");
            for (index, server) in self.dns.iter().enumerate() {
                if index > 0 {
                    text.push_str(", ");
                }
                text.push_str(&format!("\"{}\"", toml_escape(server)));
            }
            text.push_str("]\n");
        }
        text
    }
}

/// One field-level validation error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidationError {
    /// Field key.
    pub key: &'static str,
    /// Human message.
    pub message: String,
}

/// Build the module registry for the current values.
#[must_use]
pub fn modules(current: &NetworkCurrent) -> Vec<Module> {
    let mode = if MODE_CHOICES.contains(&current.mode.as_str()) {
        current.mode.clone()
    } else {
        String::from("dhcp")
    };
    let network = Module {
        id: "network",
        title: "Network",
        summary: "Hostname, DHCP or static IPv4 address",
        fields: vec![
            Field::new(
                "hostname",
                "Hostname",
                "Name applied at boot; K198/netd owns the apply",
                FieldKind::Text,
                current.hostname.clone(),
            ),
            Field::new(
                "mode",
                "Mode",
                "DHCP or a static address (left/right toggles)",
                FieldKind::Choice(MODE_CHOICES),
                mode,
            ),
            Field::new(
                "address",
                "Address",
                "Static IPv4 address (ignored for DHCP)",
                FieldKind::Text,
                current.address.clone(),
            ),
            Field::new(
                "prefix",
                "Prefix",
                "Static prefix length 1-32 (ignored for DHCP)",
                FieldKind::Text,
                current.prefix.clone(),
            ),
            Field::new(
                "gateway",
                "Gateway",
                "Static IPv4 gateway (ignored for DHCP)",
                FieldKind::Text,
                current.gateway.clone(),
            ),
            Field::new(
                "dns",
                "DNS",
                "Comma-separated IPv4 servers; K198 resolvd consumes it",
                FieldKind::Text,
                current.dns.join(","),
            ),
        ],
        writable: true,
    };
    vec![
        network,
        Module {
            id: "services",
            title: "Services",
            summary: "Service enablement (read-only; K198/K76 own apply)",
            fields: vec![
                Field::new(
                    "core",
                    "Core services",
                    "Effective status core list",
                    FieldKind::Info,
                    String::from("logd, consd, vfsd"),
                ),
                Field::new(
                    "enable_file",
                    "Enable file",
                    "Service enablement drop-in",
                    FieldKind::Info,
                    String::from("/etc/raven/services.enable"),
                ),
            ],
            writable: false,
        },
        Module {
            id: "logging",
            title: "Logging",
            summary: "Log level and destination (read-only; K76 owns apply)",
            fields: vec![
                Field::new(
                    "level",
                    "Level",
                    "Default log level",
                    FieldKind::Info,
                    String::from("info"),
                ),
                Field::new(
                    "destination",
                    "Destination",
                    "Console/file selection",
                    FieldKind::Info,
                    String::from("console"),
                ),
            ],
            writable: false,
        },
        Module {
            id: "theme",
            title: "Theme",
            summary: "Colour policy and prompt (read-only)",
            fields: vec![
                Field::new(
                    "color",
                    "Colour",
                    "auto, always or never",
                    FieldKind::Info,
                    String::from("auto"),
                ),
                Field::new(
                    "prompt",
                    "Prompt",
                    "Shell prompt",
                    FieldKind::Info,
                    String::from("raven#"),
                ),
            ],
            writable: false,
        },
        Module {
            id: "about",
            title: "About",
            summary: "raven config version and layout",
            fields: vec![
                Field::new(
                    "version",
                    "Version",
                    "Client version",
                    FieldKind::Info,
                    String::from(env!("CARGO_PKG_VERSION")),
                ),
                Field::new(
                    "layout",
                    "Layout",
                    "Two-pane searchable module list and form",
                    FieldKind::Info,
                    String::from("Crush-style"),
                ),
            ],
            writable: false,
        },
    ]
}

/// Path of the network drop-in for `root`.
#[must_use]
pub fn network_path(root: &Path) -> PathBuf {
    root.join(NETWORK_FILE)
}

/// Read the current network drop-in. A malformed or unreadable file returns the defaults plus a
/// warning; it never prevents the TUI from opening.
#[must_use]
pub fn load_network(root: &Path) -> (NetworkCurrent, Option<String>) {
    let path = network_path(root);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return (NetworkCurrent::default(), None);
        }
        Err(error) => {
            return (
                NetworkCurrent::default(),
                Some(format!("cannot read {}: {error}", path.display())),
            );
        }
    };
    match parse_current(&text) {
        Ok(current) => (current, None),
        Err(error) => (
            NetworkCurrent::default(),
            Some(format!("{}: {error}", path.display())),
        ),
    }
}

fn parse_current(text: &str) -> Result<NetworkCurrent, String> {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Section {
        None,
        Network,
        Static,
    }
    let mut current = NetworkCurrent::default();
    let mut section = Section::None;
    for (number, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            section = match line {
                "[network]" => Section::Network,
                "[network.static]" => Section::Static,
                _ => return Err(format!("line {}: unsupported section `{line}`", number + 1)),
            };
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            return Err(format!("line {}: expected `key = value`", number + 1));
        };
        let key = key.trim();
        let value = value.trim();
        match (section, key) {
            (Section::Network, "hostname") => current.hostname = parse_string(value)?,
            (Section::Network, "mode") => current.mode = parse_string(value)?,
            (Section::Static, "address") => current.address = parse_string(value)?,
            (Section::Static, "prefix") => {
                current.prefix = value.to_string();
            }
            (Section::Static, "gateway") => current.gateway = parse_string(value)?,
            (Section::Static, "dns") => current.dns = parse_array(value)?,
            _ => {}
        }
    }
    Ok(current)
}

fn parse_string(value: &str) -> Result<String, String> {
    let Some(inner) = value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
    else {
        return Err(format!("expected a quoted string, got `{value}`"));
    };
    let mut output = String::with_capacity(inner.len());
    let mut escaped = false;
    for character in inner.chars() {
        if escaped {
            output.push(match character {
                '"' => '"',
                '\\' => '\\',
                'n' => '\n',
                other => other,
            });
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else {
            output.push(character);
        }
    }
    if escaped {
        return Err(String::from("dangling backslash in string"));
    }
    Ok(output)
}

fn parse_array(value: &str) -> Result<Vec<String>, String> {
    let Some(inner) = value
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
    else {
        return Err(format!("expected an array, got `{value}`"));
    };
    let mut values = Vec::new();
    for item in inner.split(',') {
        let item = item.trim();
        if item.is_empty() {
            continue;
        }
        values.push(parse_string(item)?);
    }
    Ok(values)
}

/// Validate the form fields and build a plan, or return every field error.
pub fn plan_from_fields(fields: &[Field]) -> Result<NetworkPlan, Vec<ValidationError>> {
    let mut errors = Vec::new();
    let field = |key: &'static str| fields.iter().find(|field| field.key == key);
    let hostname = field("hostname").map_or(String::new(), Field::effective);
    let hostname = hostname.trim();
    if !valid_hostname(hostname) {
        errors.push(ValidationError {
            key: "hostname",
            message: String::from(
                "hostname must be 1-253 characters of letters, digits, dots and hyphens",
            ),
        });
    }
    let mode = field("mode").map_or(String::from("dhcp"), Field::effective);
    let mode = mode.trim().to_ascii_lowercase();
    if !MODE_CHOICES.contains(&mode.as_str()) {
        errors.push(ValidationError {
            key: "mode",
            message: String::from("mode must be dhcp or static"),
        });
    }
    let mut address = String::new();
    let mut prefix = 0u8;
    let mut gateway = String::new();
    let mut dns = Vec::new();
    if mode == "static" {
        let address_value = field("address").map_or(String::new(), Field::effective);
        let address_value = address_value.trim();
        if !valid_ipv4(address_value) || address_value == "0.0.0.0" {
            errors.push(ValidationError {
                key: "address",
                message: String::from("static address must be a non-zero IPv4 address"),
            });
        } else {
            address = address_value.to_string();
        }
        let prefix_value = field("prefix").map_or(String::new(), Field::effective);
        match prefix_value.trim().parse::<u8>() {
            Ok(value) if (1..=32).contains(&value) => prefix = value,
            _ => errors.push(ValidationError {
                key: "prefix",
                message: String::from("prefix must be 1-32"),
            }),
        }
        let gateway_value = field("gateway").map_or(String::new(), Field::effective);
        let gateway_value = gateway_value.trim();
        if !valid_ipv4(gateway_value) || gateway_value == "0.0.0.0" {
            errors.push(ValidationError {
                key: "gateway",
                message: String::from("static gateway must be a non-zero IPv4 address"),
            });
        } else {
            gateway = gateway_value.to_string();
        }
        let dns_value = field("dns").map_or(String::new(), Field::effective);
        for server in dns_value
            .split([',', ' ', '\t'])
            .filter(|item| !item.is_empty())
        {
            if !valid_ipv4(server) {
                errors.push(ValidationError {
                    key: "dns",
                    message: format!("`{server}` is not an IPv4 address"),
                });
            } else {
                dns.push(server.to_string());
            }
        }
    }
    if errors.is_empty() {
        Ok(NetworkPlan {
            hostname: hostname.to_string(),
            mode,
            address,
            prefix,
            gateway,
            dns,
        })
    } else {
        Err(errors)
    }
}

/// Write the plan atomically to the dedicated drop-in. The temporary file is in the same
/// directory, synced, chmod 0644 and renamed; no other configuration file is touched.
pub fn write_atomic(root: &Path, plan: &NetworkPlan) -> io::Result<PathBuf> {
    let path = network_path(root);
    let Some(directory) = path.parent() else {
        return Err(io::Error::other("network path has no parent"));
    };
    fs::create_dir_all(directory)?;
    let temporary = directory.join(format!(".network.toml.{}.tmp", std::process::id()));
    let result = (|| -> io::Result<()> {
        let mut file = fs::File::create(&temporary)?;
        file.write_all(plan.to_toml().as_bytes())?;
        file.sync_all()?;
        drop(file);
        fs::set_permissions(&temporary, fs::Permissions::from_mode(0o644))?;
        fs::rename(&temporary, &path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map(|()| path)
}

fn valid_hostname(value: &str) -> bool {
    if value.is_empty() || value.len() > 253 {
        return false;
    }
    value.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '-')
    })
}

fn valid_ipv4(value: &str) -> bool {
    let parts: Vec<&str> = value.split('.').collect();
    parts.len() == 4
        && parts.iter().all(|part| {
            !part.is_empty()
                && part.len() <= 3
                && part.chars().all(|character| character.is_ascii_digit())
                && part.parse::<u8>().is_ok()
        })
}

fn toml_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
#[path = "model_ut.rs"]
mod tests;
