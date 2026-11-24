// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Target admin backends for `raven status`, `raven doctor` and `raven svc`.
//!
//! The target reads the existing scheme servers directly (audit rule: no `/proc` and no fake
//! data): service state from `svc:`, the system identity from `sys:`, and the configuration from
//! `/etc/raven`. The command formatting keeps the `raven-output` contract, so the JSON records
//! have the same `schema`/`kind` envelope as the host-tested command modules in `raven-cli`.
//!
//! `Scheme` is the small transport seam; `RtScheme` is the RavenBSD runtime implementation and
//! the unit tests in `admin_ut.rs` use an in-memory fake.

#![allow(dead_code)] // helpers are used by the target dispatch or by the host tests

use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};

use raven_config::users::{Accounts, UserRecord, check as check_users};
use raven_config::{ConfigLoader, set_enabled};
use raven_output::{Record, Value};
use raven_proto_svc::{Op, State, Status};

/// The scheme transport the admin backends need.
pub(crate) trait Scheme {
    /// Read one scheme file as text.
    fn read(&self, path: &str) -> Result<String, String>;
    /// List one scheme directory.
    fn list(&self, path: &str) -> Result<Vec<String>, String>;
    /// Send one `svc:` control byte through `CALL`.
    fn call(&self, path: &str, op: u8) -> Result<(), String>;
}

/// One service row read from `svc:/<name>`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Service {
    /// Service name.
    pub(crate) name: String,
    /// Wire state spelling.
    pub(crate) state: String,
    /// Whether the service is running (its readiness signal).
    pub(crate) ready: bool,
    /// Restarts in the current window.
    pub(crate) restarts: u32,
}

/// One doctor check.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Check {
    /// Stable identifier.
    pub(crate) id: String,
    /// `ok`, `warn` or `fail`.
    pub(crate) status: &'static str,
    /// Human detail.
    pub(crate) detail: String,
}

#[derive(Default)]
struct Globals {
    json: bool,
    quiet: bool,
}

/// Dispatch a target admin command. `None` means "not mine": the caller runs the full CLI.
#[cfg(target_os = "ravenbsd")]
pub(crate) fn dispatch(args: &[OsString], out: &mut dyn Write, err: &mut dyn Write) -> Option<i32> {
    let (globals, command, rest) = parse(args)?;
    let scheme = RtScheme;
    let code = match command.as_str() {
        "status" => run_status(&globals, &scheme, &rest, out, err),
        "doctor" => run_doctor(&globals, &scheme, &rest, out, err),
        "svc" => run_svc(&globals, &scheme, &rest, out, err),
        "users" => run_users(&globals, &rest, out, err),
        _ => return None,
    };
    Some(code)
}

fn parse(args: &[OsString]) -> Option<(Globals, String, Vec<String>)> {
    let mut globals = Globals::default();
    let mut index = 0;
    while index < args.len() {
        let argument = args[index].to_string_lossy();
        match argument.as_ref() {
            "--json" => globals.json = true,
            "--quiet" => globals.quiet = true,
            "--verbose" | "--yes" | "--dry-run" => {}
            "--color" => index += 1,
            "-h" | "--help" | "-V" | "--version" => return None,
            other if other.starts_with("--color=") => {}
            other if other.starts_with('-') => return None,
            other => {
                let command = other.to_string();
                let rest = args[index + 1..]
                    .iter()
                    .map(|value| value.to_string_lossy().into_owned())
                    .collect();
                return matches!(command.as_str(), "status" | "doctor" | "svc" | "users")
                    .then_some((globals, command, rest));
            }
        }
        index += 1;
    }
    None
}

fn run_status(
    globals: &Globals,
    scheme: &dyn Scheme,
    args: &[String],
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> i32 {
    if !args.is_empty() {
        return error(
            globals,
            "EUSAGE",
            "status takes no arguments",
            "status",
            err,
        );
    }
    let services = match read_services(scheme) {
        Ok(services) => services,
        Err(message) => return error(globals, "EUNAVAILABLE", &message, "status", err),
    };
    let core = core_services().unwrap_or_default();
    let core_ok = !core.is_empty()
        && core.iter().all(|name| {
            services
                .iter()
                .any(|service| service.name == *name && service.ready)
        });
    let running = services.iter().filter(|service| service.ready).count() as u64;
    let total = services.len() as u64;
    let version = scheme
        .read("sys:version")
        .ok()
        .map(|text| text.trim().to_string());
    let uptime = scheme
        .read("sys:uptime")
        .ok()
        .and_then(|text| text.trim().parse::<u64>().ok());
    if globals.quiet {
        return i32::from(!core_ok);
    }
    if globals.json {
        let mut line = String::new();
        let record = Record::new("status")
            .field("version", optional_string(&version))
            .string("target", raven_cli::TARGET)
            .field("uptime_s", uptime.map_or(Value::Null, Value::Unsigned))
            .null("boot_id")
            .object(
                "services",
                vec![
                    ("running".into(), Value::Unsigned(running)),
                    ("ready".into(), Value::Unsigned(running)),
                    ("total".into(), Value::Unsigned(total)),
                ],
            )
            .null("load")
            .null("memory")
            .null("root")
            .array("links", Vec::new())
            .null("log");
        if record.write_json(&mut line).is_err() {
            return 1;
        }
        let _ = writeln!(out, "{line}");
    } else {
        let _ = writeln!(out, "version: {}", version.as_deref().unwrap_or("null"));
        let _ = writeln!(out, "services: {running}/{running}/{total}");
    }
    if core_ok { 0 } else { 1 }
}

fn run_doctor(
    globals: &Globals,
    scheme: &dyn Scheme,
    args: &[String],
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> i32 {
    let mut strict = false;
    for argument in args {
        match argument.as_str() {
            "--strict" if !strict => strict = true,
            other => {
                return error(
                    globals,
                    "EUSAGE",
                    &format!("unknown doctor argument `{other}`"),
                    "doctor",
                    err,
                );
            }
        }
    }
    let checks = doctor_checks(scheme);
    let mut failed = false;
    for check in &checks {
        if check.status == "fail" || (strict && check.status == "warn") {
            failed = true;
        }
        if globals.quiet {
            continue;
        }
        if globals.json {
            let mut line = String::new();
            let record = Record::new("check")
                .string("id", &check.id)
                .string("status", check.status)
                .string("detail", &check.detail);
            if record.write_json(&mut line).is_ok() {
                let _ = writeln!(out, "{line}");
            }
        } else {
            let _ = writeln!(out, "{}: {} {}", check.id, check.status, check.detail);
        }
    }
    if failed { 1 } else { 0 }
}

fn run_svc(
    globals: &Globals,
    scheme: &dyn Scheme,
    args: &[String],
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> i32 {
    let Some(command) = args.first() else {
        return error(globals, "EUSAGE", "svc needs a subcommand", "svc", err);
    };
    let rest = &args[1..];
    match command.as_str() {
        "list" if rest.is_empty() => match read_services(scheme) {
            Ok(services) => {
                for service in &services {
                    emit_service(globals, service, false, out);
                }
                0
            }
            Err(message) => error(globals, "EUNAVAILABLE", &message, "svc", err),
        },
        "status" | "start" | "stop" | "restart" => {
            let [name] = rest else {
                return error(
                    globals,
                    "EUSAGE",
                    "this svc subcommand needs exactly one service name",
                    "svc",
                    err,
                );
            };
            let result = match command.as_str() {
                "status" => read_service(scheme, name),
                "start" => control(scheme, name, Op::Start),
                "stop" => control(scheme, name, Op::Stop),
                "restart" => control(scheme, name, Op::Restart),
                _ => unreachable!(),
            };
            match result {
                Ok(service) => {
                    emit_service(globals, &service, command != "status", out);
                    0
                }
                Err(message) => error(globals, "EUNAVAILABLE", &message, "svc", err),
            }
        }
        "enable" | "disable" => {
            let [name] = rest else {
                return error(
                    globals,
                    "EUSAGE",
                    "svc enable/disable needs exactly one service name",
                    "svc",
                    err,
                );
            };
            let enabled = command == "enable";
            match set_enabled(Path::new("/"), name, enabled) {
                Ok(changed) => {
                    let service = Service {
                        name: name.clone(),
                        state: if enabled { "enabled" } else { "disabled" }.into(),
                        ready: enabled,
                        restarts: 0,
                    };
                    emit_service(globals, &service, changed, out);
                    0
                }
                Err(cause) => error(globals, "ECONFIG", &cause.to_string(), "svc", err),
            }
        }
        "logs" => error(
            globals,
            "EUNAVAILABLE",
            "svc logs is the raven log command, which arrives with K76",
            "svc",
            err,
        ),
        _ => error(
            globals,
            "EUSAGE",
            "svc subcommand is list, status, start, stop, restart, enable, disable or logs",
            "svc",
            err,
        ),
    }
}

fn emit_service(globals: &Globals, service: &Service, changed: bool, out: &mut dyn Write) {
    if globals.quiet {
        return;
    }
    if globals.json {
        let mut line = String::new();
        let record = Record::new("service")
            .string("name", &service.name)
            .string("state", &service.state)
            .bool("ready", service.ready)
            .null("pid")
            .bool("changed", changed);
        if record.write_json(&mut line).is_ok() {
            let _ = writeln!(out, "{line}");
        }
    } else {
        let suffix = if changed { " changed" } else { "" };
        let _ = writeln!(
            out,
            "{} {} {}{suffix}",
            service.name,
            service.state,
            if service.ready { "ready" } else { "not-ready" }
        );
    }
}

fn run_users(globals: &Globals, args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    let root = std::env::var_os("RAVEN_CONFIG_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"));
    run_users_at(&root, globals, args, out, err)
}

fn run_users_at(
    root: &Path,
    globals: &Globals,
    args: &[String],
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> i32 {
    let Some(command) = args.first().map(String::as_str) else {
        return error(
            globals,
            "EUSAGE",
            "users needs list, show or check",
            "users",
            err,
        );
    };
    match command {
        "list" if args.len() == 1 => {
            let accounts = match Accounts::load(root) {
                Ok(accounts) => accounts,
                Err(failure) => {
                    return error(globals, "EUSERS", &failure.to_string(), "users", err);
                }
            };
            if globals.quiet {
                return 0;
            }
            for user in &accounts.users {
                if globals.json {
                    let mut line = String::new();
                    if user_record(&accounts, user).write_json(&mut line).is_ok() {
                        let _ = writeln!(out, "{line}");
                    }
                } else {
                    let _ = writeln!(
                        out,
                        "{}\t{}\t{}\t{}\t{}",
                        user.name, user.uid, user.gid, user.home, user.shell
                    );
                }
            }
            0
        }
        "show" if args.len() == 2 => {
            let accounts = match Accounts::load(root) {
                Ok(accounts) => accounts,
                Err(failure) => {
                    return error(globals, "EUSERS", &failure.to_string(), "users", err);
                }
            };
            let Some(user) = accounts.user(&args[1]) else {
                return error(
                    globals,
                    "ENOENT",
                    &format!("user `{}` does not exist", args[1]),
                    "users",
                    err,
                );
            };
            if globals.quiet {
                return 0;
            }
            if globals.json {
                let mut line = String::new();
                if user_record(&accounts, user).write_json(&mut line).is_ok() {
                    let _ = writeln!(out, "{line}");
                }
            } else {
                let _ = writeln!(
                    out,
                    "{}: uid={} gid={} home={} shell={} autologin={} password_set={}",
                    user.name,
                    user.uid,
                    user.gid,
                    user.home,
                    user.shell,
                    accounts.is_autologin(&user.name),
                    accounts
                        .shadow(&user.name)
                        .is_some_and(raven_config::users::ShadowRecord::has_hash)
                );
            }
            0
        }
        "check" if args.len() == 1 => {
            let report = match check_users(root) {
                Ok(report) => report,
                Err(failure) => {
                    return error(globals, "EUSERS", &failure.to_string(), "users", err);
                }
            };
            if globals.quiet {
                return i32::from(!report.ok());
            }
            if globals.json {
                let mut line = String::new();
                let record = Record::new("check")
                    .string("id", "users")
                    .string("status", if report.ok() { "ok" } else { "fail" })
                    .u64("users", report.users as u64)
                    .u64("groups", report.groups as u64)
                    .array(
                        "errors",
                        report
                            .errors
                            .iter()
                            .map(|error| Value::String(error.clone()))
                            .collect(),
                    );
                if record.write_json(&mut line).is_ok() {
                    let _ = writeln!(out, "{line}");
                }
            } else if report.ok() {
                let _ = writeln!(
                    out,
                    "users: ok ({} users, {} groups)",
                    report.users, report.groups
                );
            } else {
                for error in &report.errors {
                    let _ = writeln!(err, "users: {error}");
                }
            }
            i32::from(!report.ok())
        }
        _ => error(
            globals,
            "EUSAGE",
            "usage: raven users list|show <name>|check",
            "users",
            err,
        ),
    }
}

fn user_record(accounts: &Accounts, user: &UserRecord) -> Record {
    Record::new("user")
        .string("name", &user.name)
        .u64("uid", u64::from(user.uid))
        .u64("gid", u64::from(user.gid))
        .string("home", &user.home)
        .string("shell", &user.shell)
        .bool("autologin", accounts.is_autologin(&user.name))
        .bool(
            "password_set",
            accounts
                .shadow(&user.name)
                .is_some_and(raven_config::users::ShadowRecord::has_hash),
        )
}

fn error(globals: &Globals, code: &str, message: &str, command: &str, err: &mut dyn Write) -> i32 {
    if globals.json {
        let mut line = String::new();
        let record = Record::new("error")
            .string("code", code)
            .string("message", message)
            .object(
                "context",
                vec![("command".into(), Value::String(command.into()))],
            );
        if record.write_json(&mut line).is_ok() {
            let _ = writeln!(err, "{line}");
        }
    } else {
        let _ = writeln!(err, "raven: {message}");
    }
    1
}

fn optional_string(value: &Option<String>) -> Value {
    value
        .as_ref()
        .map_or(Value::Null, |value| Value::String(value.clone()))
}

fn doctor_checks(scheme: &dyn Scheme) -> Vec<Check> {
    let mut checks = Vec::new();
    match read_services(scheme) {
        Ok(services) => {
            let core = core_services().unwrap_or_default();
            let missing: Vec<&str> = core
                .iter()
                .filter(|name| {
                    !services
                        .iter()
                        .any(|service| service.name == **name && service.ready)
                })
                .map(String::as_str)
                .collect();
            let (status, detail) = if missing.is_empty() {
                ("ok", format!("{} service(s) ready", services.len()))
            } else {
                ("fail", format!("not ready: {}", missing.join(", ")))
            };
            checks.push(Check {
                id: "services.ready".into(),
                status,
                detail,
            });
        }
        Err(message) => checks.push(Check {
            id: "services.ready".into(),
            status: "fail",
            detail: message,
        }),
    }
    match ConfigLoader::new("/").load() {
        Ok(config) => checks.push(Check {
            id: "config.parse".into(),
            status: "ok",
            detail: format!("{} file(s)", config.source_count()),
        }),
        Err(error) => checks.push(Check {
            id: "config.parse".into(),
            status: "fail",
            detail: error.to_string(),
        }),
    }
    match scheme.list("/") {
        Ok(entries) if !entries.is_empty() => checks.push(Check {
            id: "root.mount".into(),
            status: "ok",
            detail: format!("{} entries", entries.len()),
        }),
        Ok(_) => checks.push(Check {
            id: "root.mount".into(),
            status: "fail",
            detail: "root is empty".into(),
        }),
        Err(message) => checks.push(Check {
            id: "root.mount".into(),
            status: "fail",
            detail: message,
        }),
    }
    checks
}

fn read_services(scheme: &dyn Scheme) -> Result<Vec<Service>, String> {
    let mut names = scheme.list("svc:")?;
    names.sort();
    let mut services = Vec::with_capacity(names.len());
    for name in names {
        services.push(read_service(scheme, &name)?);
    }
    Ok(services)
}

fn read_service(scheme: &dyn Scheme, name: &str) -> Result<Service, String> {
    let path = format!("svc:/{name}");
    let text = scheme.read(&path)?;
    let status = Status::decode(text.as_bytes()).map_err(|error| format!("{path}: {error:?}"))?;
    Ok(Service {
        name: status.name.to_string(),
        state: status.state.as_str().to_string(),
        ready: status.state == State::Running,
        restarts: status.restarts,
    })
}

fn control(scheme: &dyn Scheme, name: &str, operation: Op) -> Result<Service, String> {
    let path = format!("svc:/{name}");
    scheme.call(&path, operation.as_u8())?;
    if operation == Op::Start {
        // START is complete only when svcd reports the readiness state.
        for _ in 0..200 {
            let service = read_service(scheme, name)?;
            if service.ready {
                return Ok(service);
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        return Err(format!("{name}: timed out waiting for the service"));
    }
    read_service(scheme, name)
}

fn core_services() -> Result<Vec<String>, String> {
    let config = ConfigLoader::new("/")
        .load()
        .map_err(|error| error.to_string())?;
    let node = config
        .get("status.core_services")
        .ok_or_else(|| "status.core_services is not configured".to_string())?;
    let items = node
        .as_array()
        .ok_or_else(|| "status.core_services is not an array".to_string())?;
    items
        .iter()
        .map(|item| {
            item.as_str()
                .map(str::to_string)
                .ok_or_else(|| "status.core_services has a non-string entry".to_string())
        })
        .collect()
}

/// The RavenBSD runtime transport.
#[cfg(target_os = "ravenbsd")]
struct RtScheme;

/// The kernel `OPEN` needs a scheme; bare absolute paths are the `file:` scheme.
#[cfg(target_os = "ravenbsd")]
fn wire_path(path: &str) -> String {
    match path.find(':') {
        Some(colon) if !path[..colon].contains('/') => path.to_string(),
        _ => format!("file:{path}"),
    }
}

#[cfg(target_os = "ravenbsd")]
impl RtScheme {
    fn read_all(&self, path: &str, flags: u32) -> Result<Vec<u8>, String> {
        let client = raven_scheme::Client::new();
        let handle = client
            .open(path, flags)
            .map_err(|error| format!("{path}: {error:?}"))?;
        let mut data = Vec::new();
        let mut buffer = [0u8; 4096];
        loop {
            let read = client
                .read(handle, 0, &mut buffer)
                .map_err(|error| format!("{path}: {error:?}"))?;
            if read == 0 {
                break;
            }
            data.extend_from_slice(buffer.get(..read).unwrap_or_default());
        }
        let _ = client.close(handle);
        Ok(data)
    }
}

#[cfg(target_os = "ravenbsd")]
impl Scheme for RtScheme {
    fn read(&self, path: &str) -> Result<String, String> {
        let wire = wire_path(path);
        let flags = u32::from(raven_proto_fs::WIRE_OPEN_READ);
        let bytes = self.read_all(&wire, flags)?;
        String::from_utf8(bytes).map_err(|_| format!("{wire}: not UTF-8"))
    }

    fn list(&self, path: &str) -> Result<Vec<String>, String> {
        let wire = wire_path(path);
        let client = raven_scheme::Client::new();
        let flags = u32::from(raven_proto_fs::WIRE_OPEN_READ);
        let handle = client
            .open(&wire, flags)
            .map_err(|error| format!("{wire}: {error:?}"))?;
        let mut names = Vec::new();
        let mut buffer = [0u8; 4096];
        loop {
            let read = client
                .read(handle, 0, &mut buffer)
                .map_err(|error| format!("{wire}: {error:?}"))?;
            if read == 0 {
                break;
            }
            let chunk = buffer.get(..read).unwrap_or_default();
            // Directory entries are packed per read; decode each chunk on its own.
            for entry in raven_proto_fs::DirEntryIter::new(chunk) {
                let entry = entry.map_err(|error| format!("{wire}: {error:?}"))?;
                let name = std::str::from_utf8(entry.name)
                    .map_err(|_| format!("{wire}: a directory entry is not UTF-8"))?;
                names.push(name.to_string());
            }
        }
        let _ = client.close(handle);
        Ok(names)
    }

    fn call(&self, path: &str, op: u8) -> Result<(), String> {
        let client = raven_scheme::Client::new();
        let handle = client
            .open(path, u32::from(raven_proto_fs::WIRE_OPEN_READ))
            .map_err(|error| format!("{path}: {error:?}"))?;
        let mut response = [0u8; 1];
        let result = client.call(handle, &[op], &mut response);
        let _ = client.close(handle);
        result
            .map(|_| ())
            .map_err(|error| format!("{path}: {error:?}"))
    }
}

#[cfg(test)]
#[path = "admin_ut.rs"]
mod tests;
