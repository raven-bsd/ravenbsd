// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `xargs` utility.

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

    use raven_initfs::Initfs;
    use raven_rt::entry;
    use raven_rt::fs;
    use raven_rt::spawn::{RtSys, SpawnError, SpawnOptions};
    use raven_tool_core::error::{EXIT_ERROR, usage};
    use raven_xargs::{ChildStatus, RunError, Runner, XargsError, parse_args, run};

    use super::exit_code;

    entry!(main);

    struct TargetRunner<'a> {
        initfs: Initfs<'a>,
    }

    impl Runner for TargetRunner<'_> {
        fn trace(&mut self, command: &[String]) {
            raven_rt::eprintln!("{}", command.join(" "));
        }

        fn run(&mut self, command: &[String]) -> Result<ChildStatus, RunError> {
            let Some(path) = self.resolve(&command[0]) else {
                return Err(RunError::NotFound);
            };
            let arguments: Vec<&str> = command.iter().map(String::as_str).collect();
            let environment = ["PATH=/bin:/sbin", "PWD=/"];
            let spawn_options = SpawnOptions {
                args: &arguments,
                env: &environment,
            };
            match raven_rt::spawn::spawn(&self.initfs, &path, &spawn_options) {
                Ok(process) => {
                    let mut code = 0i32;
                    let mut sys = RtSys;
                    let waited = sys.proc_wait(process, &mut code);
                    let _ = raven_rt::handle_close(process);
                    match waited {
                        Ok(()) => Ok(ChildStatus::Code(code)),
                        Err(_) => Err(RunError::CannotInvoke),
                    }
                }
                Err(SpawnError::NotFound) => Err(RunError::NotFound),
                Err(_) => Err(RunError::CannotInvoke),
            }
        }
    }

    impl TargetRunner<'_> {
        fn resolve(&self, name: &str) -> Option<String> {
            if name.contains('/') {
                return self.initfs.get(name).map(|_| name.to_string());
            }
            for directory in ["/bin", "/sbin"] {
                let path = format!("{directory}/{name}");
                if self.initfs.get(&path).is_some() {
                    return Some(path);
                }
            }
            None
        }
    }

    fn main(args: raven_rt::Args) -> i32 {
        let arguments: Vec<String> = args.iter().skip(1).map(|value| value.to_string()).collect();
        let options = match parse_args(&arguments) {
            Ok(options) => options,
            Err(error) => return fail(error),
        };
        let input = match read_console() {
            Ok(text) => text,
            Err(error) => {
                raven_rt::eprintln!("xargs: tty:console: {}", error.description());
                return EXIT_ERROR;
            }
        };
        let image = match raven_rt::map_initfs() {
            Ok(image) => image,
            Err(error) => {
                raven_rt::eprintln!("xargs: initfs: {}", error.description());
                return EXIT_ERROR;
            }
        };
        let initfs = match Initfs::parse(image) {
            Ok(initfs) => initfs,
            Err(error) => {
                raven_rt::eprintln!("xargs: initfs: {error}");
                return EXIT_ERROR;
            }
        };
        let mut runner = TargetRunner { initfs };
        match run(&input, &options, &mut runner) {
            Ok(outcome) => outcome.exit_code,
            Err(error) => fail(error),
        }
    }

    fn read_console() -> Result<String, raven_tool_core::Errno> {
        let mut file = fs::open("tty:console", 0)?;
        let mut data = Vec::new();
        let mut buffer = [0u8; 1024];
        loop {
            let count = file.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            data.extend_from_slice(buffer.get(..count).unwrap_or(&[]));
        }
        Ok(String::from_utf8_lossy(&data).into_owned())
    }

    fn fail(error: XargsError) -> i32 {
        let code = exit_code(&error);
        if error == XargsError::Usage {
            raven_rt::eprintln!(
                "{}",
                usage(
                    "xargs",
                    "[-0rtx] [-n max] [-I repl] [-s size] [command [argument ...]]"
                )
            );
        } else {
            raven_rt::eprintln!("xargs: {error}");
        }
        code
    }
}

#[cfg(not(target_os = "none"))]
use std::io::Read as _;

#[cfg(not(target_os = "none"))]
struct HostRunner;

#[cfg(not(target_os = "none"))]
impl raven_xargs::Runner for HostRunner {
    fn trace(&mut self, command: &[String]) {
        eprintln!("{}", command.join(" "));
    }

    fn run(
        &mut self,
        command: &[String],
    ) -> Result<raven_xargs::ChildStatus, raven_xargs::RunError> {
        use std::os::unix::process::ExitStatusExt as _;

        let mut process = std::process::Command::new(&command[0]);
        process
            .args(&command[1..])
            .stdin(std::process::Stdio::null());
        match process.status() {
            Ok(status) => match status.code() {
                Some(code) => Ok(raven_xargs::ChildStatus::Code(code)),
                None => Ok(raven_xargs::ChildStatus::Signal(
                    status.signal().unwrap_or(0),
                )),
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Err(raven_xargs::RunError::NotFound)
            }
            Err(_) => Err(raven_xargs::RunError::CannotInvoke),
        }
    }
}

#[cfg(not(target_os = "none"))]
fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(host_run(&arguments));
}

#[cfg(not(target_os = "none"))]
fn host_run(arguments: &[String]) -> i32 {
    use raven_xargs::{parse_args, run};

    let options = match parse_args(arguments) {
        Ok(options) => options,
        Err(error) => return host_fail(error),
    };
    let mut data = Vec::new();
    if let Err(error) = std::io::stdin().read_to_end(&mut data) {
        eprintln!("xargs: standard input: {error}");
        return 1;
    }
    let input = String::from_utf8_lossy(&data).into_owned();
    let mut runner = HostRunner;
    match run(&input, &options, &mut runner) {
        Ok(outcome) => outcome.exit_code,
        Err(error) => host_fail(error),
    }
}

#[cfg(not(target_os = "none"))]
fn host_fail(error: raven_xargs::XargsError) -> i32 {
    use raven_xargs::XargsError;

    let code = exit_code(&error);
    if error == XargsError::Usage {
        eprintln!("usage: xargs [-0rtx] [-n max] [-I repl] [-s size] [command [argument ...]]");
    } else {
        eprintln!("xargs: {error}");
    }
    code
}

fn exit_code(error: &raven_xargs::XargsError) -> i32 {
    use raven_xargs::XargsError;

    match error {
        XargsError::Usage => 2,
        XargsError::CommandNotFound => 127,
        XargsError::CommandNotInvokable => 126,
        XargsError::UnmatchedQuote | XargsError::TrailingBackslash | XargsError::SizeExceeded => 1,
    }
}
