// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `raven status`.

use std::ffi::OsString;
use std::fmt::Write as _;

use raven_output::{Exit, Format, Record, Value};

use super::BackendError;
use crate::Context;
use crate::commands::{Command, CommandMeta};

static STATUS_META: CommandMeta = CommandMeta {
    name: "status",
    summary: "show host health, services, load and memory",
    usage: "raven status [--json]",
    reserved: false,
};

pub(crate) static STATUS_COMMAND: StatusCommand = StatusCommand;

/// The `status` command.
pub(crate) struct StatusCommand;

impl Command for StatusCommand {
    fn meta(&self) -> &'static CommandMeta {
        &STATUS_META
    }

    fn run(&self, context: &mut Context<'_>, args: &[OsString]) -> Exit {
        if !args.is_empty() {
            return context.usage("status takes no arguments");
        }
        run_with_backend(context, &UnavailableBackend)
    }
}

/// One network link in the status object.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Link {
    /// Interface name.
    pub(crate) name: String,
    /// Link state.
    pub(crate) state: String,
}

/// The status data returned by a backend. A missing value is `None` and becomes JSON null.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Status {
    /// Client version.
    pub(crate) version: Option<String>,
    /// Target triple.
    pub(crate) target: Option<String>,
    /// Uptime in seconds.
    pub(crate) uptime_s: Option<u64>,
    /// Boot identifier.
    pub(crate) boot_id: Option<String>,
    /// Running services.
    pub(crate) services_running: Option<u64>,
    /// Ready services.
    pub(crate) services_ready: Option<u64>,
    /// Total services.
    pub(crate) services_total: Option<u64>,
    /// Load averages.
    pub(crate) load: Option<[f64; 3]>,
    /// Total memory in bytes.
    pub(crate) memory_total: Option<u64>,
    /// Free memory in bytes.
    pub(crate) memory_free: Option<u64>,
    /// Root mount name.
    pub(crate) root_mount: Option<String>,
    /// Root read-only state.
    pub(crate) root_read_only: Option<bool>,
    /// Network links.
    pub(crate) links: Vec<Link>,
    /// Stored log records.
    pub(crate) log_stored: Option<u64>,
    /// Dropped log records.
    pub(crate) log_dropped: Option<u64>,
    /// Whether every core service is running.
    pub(crate) core_ok: bool,
}

/// A status data source.
pub(crate) trait StatusBackend {
    /// Collect one status snapshot.
    fn status(&self) -> Result<Status, BackendError>;
}

/// The host placeholder: no target server, so report the error instead of fake data.
pub(crate) struct UnavailableBackend;

impl StatusBackend for UnavailableBackend {
    fn status(&self) -> Result<Status, BackendError> {
        Err(BackendError::unavailable("status"))
    }
}

pub(crate) fn run_with_backend(context: &mut Context<'_>, backend: &dyn StatusBackend) -> Exit {
    let status = match backend.status() {
        Ok(status) => status,
        Err(error) => {
            return context.fail_context(&error.code, &error.message, Some(("command", "status")));
        }
    };
    if context.format == Format::Json {
        let record = Record::new("status")
            .field("version", optional_string(&status.version))
            .field("target", optional_string(&status.target))
            .field("uptime_s", optional_u64(status.uptime_s))
            .field("boot_id", optional_string(&status.boot_id))
            .field(
                "services",
                Value::Object(vec![
                    ("running".into(), optional_u64(status.services_running)),
                    ("ready".into(), optional_u64(status.services_ready)),
                    ("total".into(), optional_u64(status.services_total)),
                ]),
            )
            .field("load", optional_load(status.load))
            .field("memory", optional_memory(&status))
            .field("root", optional_root(&status))
            .field(
                "links",
                Value::Array(
                    status
                        .links
                        .iter()
                        .map(|link| {
                            Value::Object(vec![
                                ("name".into(), Value::String(link.name.clone())),
                                ("state".into(), Value::String(link.state.clone())),
                            ])
                        })
                        .collect(),
                ),
            )
            .field("log", optional_log(&status));
        let result = context.emit_record(&record);
        if result != Exit::Success {
            return result;
        }
    } else if context.format == Format::Human {
        let mut text = String::new();
        let _ = writeln!(
            text,
            "version: {}",
            status.version.as_deref().unwrap_or("null")
        );
        let _ = writeln!(
            text,
            "services: {}/{}/{}",
            number(status.services_running),
            number(status.services_ready),
            number(status.services_total)
        );
        if let Some(load) = status.load {
            let _ = writeln!(text, "load: {:.2} {:.2} {:.2}", load[0], load[1], load[2]);
        }
        for link in &status.links {
            let _ = writeln!(text, "link: {} {}", link.name, link.state);
        }
        let result = context.write_stdout(&text);
        if result != Exit::Success {
            return result;
        }
    }
    if status.core_ok {
        Exit::Success
    } else {
        Exit::Failure
    }
}

fn optional_string(value: &Option<String>) -> Value {
    value
        .as_ref()
        .map_or(Value::Null, |value| Value::String(value.clone()))
}

fn optional_u64(value: Option<u64>) -> Value {
    value.map_or(Value::Null, Value::Unsigned)
}

fn optional_load(value: Option<[f64; 3]>) -> Value {
    value.map_or(Value::Null, |load| {
        Value::Array(load.into_iter().map(Value::Float).collect())
    })
}

fn optional_memory(status: &Status) -> Value {
    if status.memory_total.is_none() && status.memory_free.is_none() {
        return Value::Null;
    }
    Value::Object(vec![
        ("total".into(), optional_u64(status.memory_total)),
        ("free".into(), optional_u64(status.memory_free)),
    ])
}

fn optional_root(status: &Status) -> Value {
    if status.root_mount.is_none() && status.root_read_only.is_none() {
        return Value::Null;
    }
    Value::Object(vec![
        ("mount".into(), optional_string(&status.root_mount)),
        (
            "read_only".into(),
            status.root_read_only.map_or(Value::Null, Value::Bool),
        ),
    ])
}

fn optional_log(status: &Status) -> Value {
    if status.log_stored.is_none() && status.log_dropped.is_none() {
        return Value::Null;
    }
    Value::Object(vec![
        ("stored_records".into(), optional_u64(status.log_stored)),
        ("dropped_records".into(), optional_u64(status.log_dropped)),
    ])
}

fn number(value: Option<u64>) -> String {
    value.map_or_else(|| String::from("null"), |value| value.to_string())
}

#[cfg(test)]
#[path = "status_ut.rs"]
mod tests;
