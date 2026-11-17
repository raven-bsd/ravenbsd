// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! POSIX `ulimit` command surface.
//!
//! The RavenBSD ABI has no `getrlimit`/`setrlimit` syscall yet, so the utility parses the standard
//! option set and returns a typed unsupported result for every query or assignment. It never
//! fabricates a limit. The parser is host-tested so the surface is ready for the syscall.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::format;
use alloc::string::String;

/// Usage line.
pub const USAGE: &str = "ulimit [-HS] [-a] [-cdfnstv] [limit]";

/// One resource selector.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resource {
    /// All resources (`-a`).
    All,
    /// Core file size (`-c`).
    Core,
    /// Data segment size (`-d`).
    Data,
    /// File size (`-f`).
    File,
    /// Open file descriptors (`-n`).
    OpenFiles,
    /// Stack size (`-s`).
    Stack,
    /// CPU time (`-t`).
    Cpu,
    /// Address space (`-v`).
    Address,
}

/// Parsed request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Request {
    /// Hard limit (`-H`).
    pub hard: bool,
    /// Soft limit (`-S`).
    pub soft: bool,
    /// Resource selector.
    pub resource: Resource,
    /// New limit, or `None` to query.
    pub value: Option<u64>,
}

/// Argument errors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseError {
    /// Unknown option.
    UnknownOption,
    /// Extra operand after the limit.
    UnexpectedOperand,
    /// Limit is not `unlimited` or an integer.
    BadValue,
}

/// Parse `ulimit` arguments (without `argv[0]`).
pub fn parse_args(args: &[&str]) -> Result<Request, ParseError> {
    let mut hard = false;
    let mut soft = false;
    let mut resource = Resource::File;
    let mut value = None;
    for arg in args {
        match *arg {
            "-H" => hard = true,
            "-S" => soft = true,
            "-a" => resource = Resource::All,
            "-c" => resource = Resource::Core,
            "-d" => resource = Resource::Data,
            "-f" => resource = Resource::File,
            "-n" => resource = Resource::OpenFiles,
            "-s" => resource = Resource::Stack,
            "-t" => resource = Resource::Cpu,
            "-v" => resource = Resource::Address,
            other if other.starts_with('-') => return Err(ParseError::UnknownOption),
            other => {
                if value.is_some() {
                    return Err(ParseError::UnexpectedOperand);
                }
                value = Some(parse_value(other)?);
            }
        }
    }
    Ok(Request {
        hard,
        soft,
        resource,
        value,
    })
}

fn parse_value(value: &str) -> Result<u64, ParseError> {
    if value == "unlimited" {
        return Ok(u64::MAX);
    }
    value.parse::<u64>().map_err(|_| ParseError::BadValue)
}

/// Kernel resource index for a selector; `All` has no single index.
pub const fn kernel_index(resource: Resource) -> Option<usize> {
    match resource {
        Resource::All => None,
        Resource::Cpu => Some(0),
        Resource::File => Some(1),
        Resource::Data => Some(2),
        Resource::Stack => Some(3),
        Resource::Core => Some(4),
        Resource::OpenFiles => Some(7),
        Resource::Address => Some(9),
    }
}

/// POSIX display unit scale for a resource (bytes per reported unit).
pub const fn unit_scale(resource: Resource) -> u64 {
    match resource {
        Resource::Core | Resource::File => 512,
        Resource::Data | Resource::Stack | Resource::Address => 1024,
        Resource::Cpu | Resource::OpenFiles | Resource::All => 1,
    }
}

/// POSIX `ulimit -a` label for a resource.
pub const fn label(resource: Resource) -> &'static str {
    match resource {
        Resource::All => "all",
        Resource::Core => "core file size (blocks, -c)",
        Resource::Data => "data seg size (kbytes, -d)",
        Resource::File => "file size (blocks, -f)",
        Resource::OpenFiles => "open files (-n)",
        Resource::Stack => "stack size (kbytes, -s)",
        Resource::Cpu => "cpu time (seconds, -t)",
        Resource::Address => "virtual memory (kbytes, -v)",
    }
}

/// Format a kernel limit in POSIX display units.
pub fn format_value(resource: Resource, value: u64) -> String {
    if value == u64::MAX {
        String::from("unlimited")
    } else {
        format!("{}", value / unit_scale(resource).max(1))
    }
}

/// Kernel bytes for a parsed limit value.
pub fn to_kernel(resource: Resource, value: u64) -> u64 {
    value.saturating_mul(unit_scale(resource).max(1))
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
