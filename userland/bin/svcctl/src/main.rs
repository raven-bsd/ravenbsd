// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `svcctl` utility.
//!
//! `svcctl list` lists every service, `svcctl status <name>` prints one service and
//! `svcctl start|stop|restart <name>` sends a control request to the `svc:` scheme. Every command
//! prints `<name> <state> <restarts>`; failures print `svcctl: <name>: <message>` and exit 1.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

extern crate alloc;

#[cfg(any(target_os = "none", test))]
use alloc::string::{String, ToString};

/// One parsed command line.
#[cfg(any(target_os = "none", test))]
#[derive(Clone, Debug, PartialEq, Eq)]
enum Command {
    /// List all services.
    List,
    /// Print one service.
    Status(String),
    /// Start a service.
    Start(String),
    /// Stop supervision of a service.
    Stop(String),
    /// Restart a service.
    Restart(String),
}

/// Parse `argv` (including `argv[0]`).
#[cfg(any(target_os = "none", test))]
fn parse(argv: &[&str]) -> Result<Command, String> {
    let arguments = argv.get(1..).unwrap_or(&[]);
    match arguments {
        ["list"] => Ok(Command::List),
        ["status", name] => Ok(Command::Status(String::from(*name))),
        ["start", name] => Ok(Command::Start(String::from(*name))),
        ["stop", name] => Ok(Command::Stop(String::from(*name))),
        ["restart", name] => Ok(Command::Restart(String::from(*name))),
        _ => Err(
            "usage: svcctl list | status <name> | start <name> | stop <name> | restart <name>"
                .to_string(),
        ),
    }
}

#[cfg(target_os = "none")]
mod program {
    use alloc::format;
    use alloc::string::{String, ToString};
    use alloc::vec::Vec;

    use raven_proto_fs::DirEntryIter;
    use raven_proto_svc::{Op, Status};
    use raven_rt::entry;
    use raven_scheme::Client;

    use crate::{Command, parse};

    entry!(main);

    fn main(args: raven_rt::Args) -> i32 {
        let argv: Vec<&str> = args.iter().collect();
        match run(&argv) {
            Ok(()) => 0,
            Err(message) => {
                raven_rt::eprintln!("svcctl: {}", message);
                1
            }
        }
    }

    fn run(argv: &[&str]) -> Result<(), String> {
        let command = parse(argv)?;
        let client = Client::new();
        match command {
            Command::List => {
                for name in service_names(&client)? {
                    raven_rt::println!("{}", status_line(&client, &name)?);
                }
                Ok(())
            }
            Command::Status(name) => {
                raven_rt::println!("{}", status_line(&client, &name)?);
                Ok(())
            }
            Command::Start(name) => control(&client, &name, Op::Start),
            Command::Stop(name) => control(&client, &name, Op::Stop),
            Command::Restart(name) => control(&client, &name, Op::Restart),
        }
    }

    fn service_names(client: &Client) -> Result<Vec<String>, String> {
        let handle = client
            .open("svc:", 1)
            .map_err(|error| error.description().to_string())?;
        let mut data = Vec::new();
        let mut buffer = [0u8; 4096];
        loop {
            let read = client
                .read(handle, 0, &mut buffer)
                .map_err(|error| error.description().to_string())?;
            if read == 0 {
                break;
            }
            let chunk = buffer.get(..read).ok_or_else(|| "short read".to_string())?;
            data.extend_from_slice(chunk);
        }
        let _ = client.close(handle);
        let mut names = Vec::new();
        for entry in DirEntryIter::new(&data) {
            let entry = entry.map_err(|error| format!("bad directory entry: {error}"))?;
            let name = core::str::from_utf8(entry.name)
                .map_err(|_| "bad directory entry name".to_string())?;
            names.push(name.to_string());
        }
        Ok(names)
    }

    fn status_line(client: &Client, name: &str) -> Result<String, String> {
        let path = format!("svc:/{name}");
        let handle = client
            .open(&path, 1)
            .map_err(|error| format!("{name}: {}", error.description()))?;
        let mut buffer = [0u8; 128];
        let read = client
            .read(handle, 0, &mut buffer)
            .map_err(|error| format!("{name}: {}", error.description()))?;
        let _ = client.close(handle);
        let line = buffer
            .get(..read)
            .ok_or_else(|| format!("{name}: short read"))?;
        let status = Status::decode(line).map_err(|_| format!("{name}: bad status line"))?;
        Ok(format!(
            "{} {} {}",
            status.name,
            status.state.as_str(),
            status.restarts
        ))
    }

    fn control(client: &Client, name: &str, operation: Op) -> Result<(), String> {
        let path = format!("svc:/{name}");
        let handle = client
            .open(&path, 1)
            .map_err(|error| format!("{name}: {}", error.description()))?;
        let mut response = [0u8; 1];
        let result = client.call(handle, &[operation.as_u8()], &mut response);
        let _ = client.close(handle);
        result.map_err(|error| format!("{name}: {}", error.description()))?;
        // Give the main loop one tick to act on the queued request before reading the state.
        let _ = raven_rt::nanosleep(50_000_000);
        raven_rt::println!("{}", status_line(client, name)?);
        Ok(())
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_services() {
        assert_eq!(parse(&["svcctl", "list"]), Ok(Command::List));
    }

    #[test]
    fn parses_service_commands() {
        assert_eq!(
            parse(&["svcctl", "status", "logd"]),
            Ok(Command::Status("logd".to_string()))
        );
        assert_eq!(
            parse(&["svcctl", "start", "logd"]),
            Ok(Command::Start("logd".to_string()))
        );
        assert_eq!(
            parse(&["svcctl", "stop", "logd"]),
            Ok(Command::Stop("logd".to_string()))
        );
        assert_eq!(
            parse(&["svcctl", "restart", "logd"]),
            Ok(Command::Restart("logd".to_string()))
        );
    }

    #[test]
    fn rejects_bad_argument_counts() {
        assert!(parse(&[]).is_err());
        assert!(parse(&["svcctl"]).is_err());
        assert!(parse(&["svcctl", "status"]).is_err());
        assert!(parse(&["svcctl", "status", "a", "b"]).is_err());
        assert!(parse(&["svcctl", "frobnicate"]).is_err());
    }

    #[test]
    fn usage_is_documented_in_the_error() {
        let error = parse(&["svcctl", "nope"]).unwrap_err();
        assert!(error.contains("svcctl list"));
        assert!(error.contains("restart <name>"));
    }
}
