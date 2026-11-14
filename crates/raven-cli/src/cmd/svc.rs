// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `raven svc` and the enable-file front end.

use std::ffi::OsString;
use std::fmt::Write as _;
use std::path::Path;

use raven_config::set_enabled;
use raven_output::{Exit, Format, Record, Value};

use super::BackendError;
use crate::Context;
use crate::commands::{Command, CommandMeta};

static SVC_META: CommandMeta = CommandMeta {
    name: "svc",
    summary: "list, inspect and control services",
    usage: "raven svc <list|status|start|stop|restart|enable|disable|logs> [name]",
    reserved: false,
};

pub(crate) static SVC_COMMAND: SvcCommand = SvcCommand;

/// The `svc` command.
pub(crate) struct SvcCommand;

impl Command for SvcCommand {
    fn meta(&self) -> &'static CommandMeta {
        &SVC_META
    }

    fn run(&self, context: &mut Context<'_>, args: &[OsString]) -> Exit {
        run_with_backend(context, Path::new("/"), &UnavailableBackend, args)
    }
}

/// One service record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Service {
    /// Service name.
    pub(crate) name: String,
    /// State spelling from the control protocol.
    pub(crate) state: String,
    /// Readiness.
    pub(crate) ready: bool,
    /// Process id when the backend knows it.
    pub(crate) pid: Option<i32>,
    /// Whether the action changed anything.
    pub(crate) changed: bool,
}

/// The service control data source.
pub(crate) trait SvcBackend {
    /// Every service.
    fn list(&self) -> Result<Vec<Service>, BackendError>;
    /// One service.
    fn status(&self, name: &str) -> Result<Service, BackendError>;
    /// Start one service.
    fn start(&self, name: &str) -> Result<Service, BackendError>;
    /// Stop one service.
    fn stop(&self, name: &str) -> Result<Service, BackendError>;
    /// Restart one service.
    fn restart(&self, name: &str) -> Result<Service, BackendError>;
}

/// The host placeholder: no svc: server, so report the error rather than fake state.
pub(crate) struct UnavailableBackend;

impl SvcBackend for UnavailableBackend {
    fn list(&self) -> Result<Vec<Service>, BackendError> {
        Err(BackendError::unavailable("svc"))
    }
    fn status(&self, _name: &str) -> Result<Service, BackendError> {
        Err(BackendError::unavailable("svc"))
    }
    fn start(&self, _name: &str) -> Result<Service, BackendError> {
        Err(BackendError::unavailable("svc"))
    }
    fn stop(&self, _name: &str) -> Result<Service, BackendError> {
        Err(BackendError::unavailable("svc"))
    }
    fn restart(&self, _name: &str) -> Result<Service, BackendError> {
        Err(BackendError::unavailable("svc"))
    }
}

pub(crate) fn run_with_backend(
    context: &mut Context<'_>,
    root: &Path,
    backend: &dyn SvcBackend,
    args: &[OsString],
) -> Exit {
    let Some(command) = args.first() else {
        return context.usage("svc needs a subcommand");
    };
    let command = command.to_string_lossy();
    let rest = &args[1..];
    match command.as_ref() {
        "list" if rest.is_empty() => {
            let services = match backend.list() {
                Ok(services) => services,
                Err(error) => return backend_error(context, error),
            };
            for service in services {
                let result = emit_service(context, &service);
                if result != Exit::Success {
                    return result;
                }
            }
            Exit::Success
        }
        "status" | "start" | "stop" | "restart" => {
            let Some(name) = one_name(rest) else {
                return context.usage("this svc subcommand needs exactly one service name");
            };
            let name = name.to_string_lossy();
            let result = match command.as_ref() {
                "status" => backend.status(&name),
                "start" => backend.start(&name),
                "stop" => backend.stop(&name),
                "restart" => backend.restart(&name),
                _ => unreachable!(),
            };
            match result {
                Ok(service) => emit_service(context, &service),
                Err(error) => backend_error(context, error),
            }
        }
        "enable" | "disable" => {
            let Some(name) = one_name(rest) else {
                return context.usage("svc enable/disable needs exactly one service name");
            };
            let name = name.to_string_lossy();
            let enabled = command == "enable";
            match set_enabled(root, &name, enabled) {
                Ok(changed) => {
                    let service = Service {
                        name: name.to_string(),
                        state: if enabled {
                            "enabled".to_string()
                        } else {
                            "disabled".to_string()
                        },
                        ready: enabled,
                        pid: None,
                        changed,
                    };
                    emit_service(context, &service)
                }
                Err(error) => context.fail("ECONFIG", &error.to_string()),
            }
        }
        "logs" => context.fail(
            "EUNAVAILABLE",
            "svc logs is the raven log command, which arrives with K76",
        ),
        _ => context
            .usage("svc subcommand is list, status, start, stop, restart, enable, disable or logs"),
    }
}

fn one_name(args: &[OsString]) -> Option<&OsString> {
    match args {
        [name] => Some(name),
        _ => None,
    }
}

fn backend_error(context: &mut Context<'_>, error: BackendError) -> Exit {
    context.fail_context(&error.code, &error.message, Some(("command", "svc")))
}

fn emit_service(context: &mut Context<'_>, service: &Service) -> Exit {
    if context.format == Format::Json {
        let record = Record::new("service")
            .string("name", &service.name)
            .string("state", &service.state)
            .bool("ready", service.ready)
            .field(
                "pid",
                service
                    .pid
                    .map_or(Value::Null, |pid| Value::Signed(i64::from(pid))),
            )
            .bool("changed", service.changed);
        context.emit_record(&record)
    } else if context.format == Format::Human {
        let mut text = String::new();
        let _ = writeln!(
            text,
            "{} {} {}{}",
            service.name,
            service.state,
            if service.ready { "ready" } else { "not-ready" },
            if service.changed { " changed" } else { "" }
        );
        context.write_stdout(&text)
    } else {
        Exit::Success
    }
}

#[cfg(test)]
#[path = "svc_ut.rs"]
mod tests;
