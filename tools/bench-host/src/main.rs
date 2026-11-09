// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-only benchmark harness for the DR-10 container decision.

mod affinity;
mod forkexec;
mod measure;
mod microvm;
mod ns;
mod podman;
mod ptrace_bench;
mod report;
mod result;
mod setup;
mod stats;
mod syscall;

use std::env;
use std::process;

use anyhow::{Result, bail};

#[derive(Debug, PartialEq, Eq)]
enum Command {
    Setup,
    SyscallRtt,
    ForkExec,
    NsStart,
    PtraceSyscall,
    PodmanStart,
    MicrovmBoot,
    Report,
    Help,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("bench-host: {error:#}");
        process::exit(1);
    }
}

fn run() -> Result<()> {
    match parse_command(env::args().skip(1))? {
        Command::Setup => setup::run_setup(),
        Command::SyscallRtt => syscall::run(),
        Command::ForkExec => forkexec::run(),
        Command::NsStart => ns::run(),
        Command::PtraceSyscall => ptrace_bench::run(),
        Command::PodmanStart => podman::run(),
        Command::MicrovmBoot => microvm::run(),
        Command::Report => report::run(),
        Command::Help => {
            print_usage();
            Ok(())
        }
    }
}

fn parse_command(mut args: impl Iterator<Item = String>) -> Result<Command> {
    let Some(argument) = args.next() else {
        bail!(
            "usage: bench-host <setup|syscall-rtt|fork-exec|ns-start|ptrace-syscall|podman-start|microvm-boot|report>"
        );
    };
    if args.next().is_some() {
        bail!("unexpected extra arguments after `{argument}`");
    }
    Ok(match argument.as_str() {
        "setup" => Command::Setup,
        "syscall-rtt" => Command::SyscallRtt,
        "fork-exec" => Command::ForkExec,
        "ns-start" => Command::NsStart,
        "ptrace-syscall" => Command::PtraceSyscall,
        "podman-start" => Command::PodmanStart,
        "microvm-boot" => Command::MicrovmBoot,
        "report" => Command::Report,
        "--help" | "-h" | "help" => Command::Help,
        other => bail!("unknown command `{other}`; run `bench-host --help`"),
    })
}

fn print_usage() {
    println!(
        "usage: bench-host <command>\n\
         \x20 setup            build static musl helper binaries\n\
         \x20 syscall-rtt      raw getpid/clock_gettime/read round trips\n\
         \x20 fork-exec        fork + execve(/bin/true) + waitpid\n\
         \x20 ns-start         clone/exec with user,pid,mnt,net namespaces\n\
         \x20 ptrace-syscall   PTRACE_SYSCALL forwarding proxy\n\
         \x20 podman-start     local-image container start and idle memory\n\
         \x20 microvm-boot     QEMU microvm boot to READY, if measurable\n\
         \x20 report           collect results/*.json into results/summary.md"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Command> {
        parse_command(args.iter().map(|value| value.to_string()))
    }

    #[test]
    fn parses_every_command() {
        assert_eq!(parse(&["setup"]).unwrap(), Command::Setup);
        assert_eq!(parse(&["syscall-rtt"]).unwrap(), Command::SyscallRtt);
        assert_eq!(parse(&["fork-exec"]).unwrap(), Command::ForkExec);
        assert_eq!(parse(&["ns-start"]).unwrap(), Command::NsStart);
        assert_eq!(parse(&["ptrace-syscall"]).unwrap(), Command::PtraceSyscall);
        assert_eq!(parse(&["podman-start"]).unwrap(), Command::PodmanStart);
        assert_eq!(parse(&["microvm-boot"]).unwrap(), Command::MicrovmBoot);
        assert_eq!(parse(&["report"]).unwrap(), Command::Report);
    }

    #[test]
    fn rejects_unknown_and_extra_arguments() {
        assert!(parse(&["unknown"]).is_err());
        assert!(parse(&[]).is_err());
        assert!(parse(&["report", "extra"]).is_err());
    }
}
