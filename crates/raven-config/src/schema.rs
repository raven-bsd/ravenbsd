// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The known configuration schema and the type parsers of section 4.2.

use std::fs;
use std::os::unix::fs::MetadataExt;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use crate::ConfigError;
use crate::node::Node;

/// Parse an integer byte count or a string such as `"8 MiB"`.
pub fn parse_size(node: &Node) -> Result<u64, String> {
    match node {
        Node::Integer(value) if *value >= 0 => Ok(*value as u64),
        Node::String(value) => parse_size_text(value),
        _ => Err(String::from(
            "size must be integer bytes or a string like \"8 MiB\"",
        )),
    }
}

/// Parse a duration in milliseconds, from an integer or a string such as `"250ms"`.
pub fn parse_duration(node: &Node) -> Result<u64, String> {
    match node {
        Node::Integer(value) if *value >= 0 => Ok(*value as u64),
        Node::String(value) => parse_duration_text(value),
        _ => Err(String::from(
            "duration must be integer milliseconds or a string like \"250ms\"",
        )),
    }
}

fn parse_size_text(text: &str) -> Result<u64, String> {
    let (number, unit) = split_value(text).ok_or_else(|| format!("invalid size `{text}`"))?;
    let multiplier = match unit {
        "" | "B" => 1.0,
        "KiB" => 1024.0,
        "MiB" => 1024.0 * 1024.0,
        "GiB" => 1024.0 * 1024.0 * 1024.0,
        "TiB" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        _ => return Err(format!("unknown size unit `{unit}`")),
    };
    let value = number
        .parse::<f64>()
        .map_err(|_| format!("invalid size number `{number}`"))?;
    if !value.is_finite() || value < 0.0 {
        return Err(format!("invalid size `{text}`"));
    }
    let bytes = value * multiplier;
    if bytes > u64::MAX as f64 {
        return Err(format!("size `{text}` is too large"));
    }
    Ok(bytes as u64)
}

fn parse_duration_text(text: &str) -> Result<u64, String> {
    let (number, unit) = split_value(text).ok_or_else(|| format!("invalid duration `{text}`"))?;
    let multiplier = match unit {
        "ms" => 1.0,
        "s" => 1000.0,
        "m" => 60_000.0,
        "h" => 3_600_000.0,
        _ => return Err(format!("unknown duration unit `{unit}`")),
    };
    let value = number
        .parse::<f64>()
        .map_err(|_| format!("invalid duration number `{number}`"))?;
    if !value.is_finite() || value < 0.0 {
        return Err(format!("invalid duration `{text}`"));
    }
    let millis = value * multiplier;
    if millis > u64::MAX as f64 {
        return Err(format!("duration `{text}` is too large"));
    }
    Ok(millis as u64)
}

fn split_value(text: &str) -> Option<(&str, &str)> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let split = text
        .find(|character: char| !character.is_ascii_digit() && character != '.')
        .unwrap_or(text.len());
    let (number, unit) = text.split_at(split);
    let unit = unit.trim();
    if number.is_empty() {
        None
    } else {
        Some((number, unit))
    }
}

/// Validate one parsed file against the known schema.
pub fn validate(file: &Path, node: &Node) -> Result<(), ConfigError> {
    let table = node
        .as_table()
        .ok_or_else(|| ConfigError::file(file, "the configuration must be a table"))?;
    for (key, value) in table {
        match key.as_str() {
            "profile" => require_string(file, key, value, |value| !value.is_empty())?,
            "color" => require_string(file, key, value, |value| {
                matches!(value, "auto" | "always" | "never")
            })?,
            "log" => validate_log(file, value)?,
            "services" => validate_services(file, value)?,
            "status" => validate_status(file, value)?,
            "secrets" => validate_secrets(file, value)?,
            "network" => crate::network::validate_node(file, value)?,
            other => {
                return Err(ConfigError::file(file, "unknown configuration key").with_key(other));
            }
        }
    }
    Ok(())
}

fn validate_log(file: &Path, node: &Node) -> Result<(), ConfigError> {
    let table = node
        .as_table()
        .ok_or_else(|| ConfigError::file(file, "`log` must be a table").with_key("log"))?;
    for (key, value) in table {
        match key.as_str() {
            "level" => require_string(file, "log.level", value, |value| {
                matches!(value, "error" | "warn" | "info" | "debug")
            })?,
            "ring_bytes" => {
                let _ = parse_size(value).map_err(|message| {
                    ConfigError::file(file, message).with_key("log.ring_bytes")
                })?;
            }
            "retention" => {
                let _ = parse_duration(value).map_err(|message| {
                    ConfigError::file(file, message).with_key("log.retention")
                })?;
            }
            other => {
                return Err(ConfigError::file(file, "unknown configuration key")
                    .with_key(format!("log.{other}")));
            }
        }
    }
    Ok(())
}

fn validate_services(file: &Path, node: &Node) -> Result<(), ConfigError> {
    let table = node.as_table().ok_or_else(|| {
        ConfigError::file(file, "`services` must be a table").with_key("services")
    })?;
    for (key, value) in table {
        match key.as_str() {
            "enable_file" => {
                let path = value.as_str().ok_or_else(|| {
                    ConfigError::file(file, "`services.enable_file` must be a path")
                        .with_key("services.enable_file")
                })?;
                validate_path(path).map_err(|message| {
                    ConfigError::file(file, message).with_key("services.enable_file")
                })?;
            }
            other => {
                return Err(ConfigError::file(file, "unknown configuration key")
                    .with_key(format!("services.{other}")));
            }
        }
    }
    Ok(())
}

fn validate_status(file: &Path, node: &Node) -> Result<(), ConfigError> {
    let table = node
        .as_table()
        .ok_or_else(|| ConfigError::file(file, "`status` must be a table").with_key("status"))?;
    for (key, value) in table {
        match key.as_str() {
            "core_services" => {
                let names = value.as_array().ok_or_else(|| {
                    ConfigError::file(file, "`status.core_services` must be an array")
                        .with_key("status.core_services")
                })?;
                for name in names {
                    if name.as_str().is_none_or(str::is_empty) {
                        return Err(ConfigError::file(
                            file,
                            "`status.core_services` entries must be non-empty strings",
                        )
                        .with_key("status.core_services"));
                    }
                }
            }
            other => {
                return Err(ConfigError::file(file, "unknown configuration key")
                    .with_key(format!("status.{other}")));
            }
        }
    }
    Ok(())
}

fn validate_secrets(file: &Path, node: &Node) -> Result<(), ConfigError> {
    let table = node
        .as_table()
        .ok_or_else(|| ConfigError::file(file, "`secrets` must be a table").with_key("secrets"))?;
    for (name, value) in table {
        let path = value.as_str().ok_or_else(|| {
            ConfigError::file(file, "secret value must be a path")
                .with_key(format!("secrets.{name}"))
        })?;
        validate_path(path).map_err(|message| {
            ConfigError::file(file, message).with_key(format!("secrets.{name}"))
        })?;
        check_secret_mode(Path::new(path)).map_err(|message| {
            ConfigError::file(file, message).with_key(format!("secrets.{name}"))
        })?;
    }
    Ok(())
}

fn require_string(
    file: &Path,
    key: &str,
    node: &Node,
    valid: impl Fn(&str) -> bool,
) -> Result<(), ConfigError> {
    let Some(value) = node.as_str() else {
        return Err(ConfigError::file(file, "value must be a string").with_key(key));
    };
    if !valid(value) {
        return Err(ConfigError::file(file, "value is not allowed").with_key(key));
    }
    Ok(())
}

/// Check that a path is absolute and cannot escape its root.
pub fn validate_path(path: &str) -> Result<(), String> {
    if !path.starts_with('/') {
        return Err(format!("path `{path}` is not absolute"));
    }
    if path.contains('\0') {
        return Err(String::from("path contains a NUL byte"));
    }
    if path.split('/').any(|part| part == "..") {
        return Err(format!("path `{path}` contains `..`"));
    }
    Ok(())
}

/// Check a secret file's mode: no world permission and group permission only for gid 0.
pub fn check_secret_mode(path: &Path) -> Result<(), String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("cannot stat secret file `{}`: {error}", path.display()))?;
    let mode = metadata.permissions().mode() & 0o777;
    if mode & 0o007 != 0 {
        return Err(format!(
            "secret file `{}` is world-readable (mode {mode:04o})",
            path.display()
        ));
    }
    if mode & 0o070 != 0 && metadata.gid() != 0 {
        return Err(format!(
            "secret file `{}` is group-readable outside gid 0 (mode {mode:04o})",
            path.display()
        ));
    }
    Ok(())
}
