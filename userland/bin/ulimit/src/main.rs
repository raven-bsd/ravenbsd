// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `ulimit` utility.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
// The target reads and writes the kernel's rlimit slots through raw PROC_GETRLIMIT/PROC_SETRLIMIT
// numbers; the two unsafe blocks are scalar syscall wrappers with SAFETY comments.
#![cfg_attr(not(target_os = "none"), forbid(unsafe_code))]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::string::String;
    use alloc::vec::Vec;

    use raven_abi::errno::Errno;
    use raven_rt::entry;
    use raven_tool_core::error::{EXIT_ERROR, EXIT_OK, EXIT_USAGE, usage};
    use ulimit::{Request, Resource, format_value, kernel_index, label, parse_args, to_kernel};

    /// `proc_getrlimit(resource, out[2])`; see `kernel/src/syscall/process_ctl.rs`.
    const PROC_GETRLIMIT: usize = 0x4E;
    /// `proc_setrlimit(resource, cur, max)`.
    const PROC_SETRLIMIT: usize = 0x4F;
    /// The resources reported by `-a`.
    const ALL: [Resource; 7] = [
        Resource::Core,
        Resource::Data,
        Resource::File,
        Resource::Stack,
        Resource::Cpu,
        Resource::OpenFiles,
        Resource::Address,
    ];

    entry!(main);

    fn main(args: raven_rt::Args) -> i32 {
        let arguments: Vec<&str> = args.iter().skip(1).collect();
        if arguments.first() == Some(&"--help") {
            raven_rt::println!("{}", usage("ulimit", "[-HS] [-a] [-cdfnstv] [limit]"));
            return EXIT_OK;
        }
        let request = match parse_args(&arguments) {
            Ok(request) => request,
            Err(_) => {
                raven_rt::eprintln!("{}", usage("ulimit", "[-HS] [-a] [-cdfnstv] [limit]"));
                return EXIT_USAGE;
            }
        };
        if request.resource == Resource::All {
            if request.value.is_some() {
                raven_rt::eprintln!("{}", usage("ulimit", "[-HS] [-a] [-cdfnstv] [limit]"));
                return EXIT_USAGE;
            }
            let mut output = String::new();
            for resource in ALL {
                match get(resource) {
                    Ok((soft, _)) => {
                        output.push_str(label(resource));
                        output.push(' ');
                        output.push_str(&format_value(resource, soft));
                        output.push('\n');
                    }
                    Err(_) => return EXIT_ERROR,
                }
            }
            raven_rt::print!("{output}");
            return EXIT_OK;
        }
        let Some(index) = kernel_index(request.resource) else {
            return EXIT_USAGE;
        };
        match request.value {
            None => match get(request.resource) {
                Ok((soft, hard)) => {
                    let value = if request.hard && !request.soft {
                        hard
                    } else {
                        soft
                    };
                    raven_rt::println!("{}", format_value(request.resource, value));
                    EXIT_OK
                }
                Err(_) => EXIT_ERROR,
            },
            Some(value) => {
                let bytes = to_kernel(request.resource, value);
                let current = match get(request.resource) {
                    Ok(pair) => pair,
                    Err(_) => return EXIT_ERROR,
                };
                let (soft, hard) = limits(request, current, bytes);
                match set(index, soft, hard) {
                    Ok(()) => EXIT_OK,
                    Err(error) => {
                        raven_rt::eprintln!(
                            "ulimit: {}: {}",
                            label(request.resource),
                            error.description()
                        );
                        EXIT_ERROR
                    }
                }
            }
        }
    }

    fn limits(request: Request, current: (u64, u64), bytes: u64) -> (u64, u64) {
        if request.hard && request.soft {
            (bytes, bytes)
        } else if request.hard {
            (current.0, bytes)
        } else {
            (bytes, current.1)
        }
    }

    fn get(resource: Resource) -> Result<(u64, u64), Errno> {
        let index = kernel_index(resource).ok_or(Errno::EINVAL)?;
        let mut out = [0u64; 2];
        // SAFETY: PROC_GETRLIMIT writes two u64 values to the pointer; the array is valid for
        // exactly two elements and the resource index is bounded by the kernel table.
        let result =
            unsafe { raven_rt::sys::syscall2(PROC_GETRLIMIT, index, out.as_mut_ptr() as usize) };
        Errno::from_ret(result)?;
        Ok((out[0], out[1]))
    }

    fn set(index: usize, soft: u64, hard: u64) -> Result<(), Errno> {
        // SAFETY: PROC_SETRLIMIT takes three scalar arguments (resource, cur, max) and no
        // pointers; the kernel validates cur <= max and the resource index.
        let result =
            unsafe { raven_rt::sys::syscall3(PROC_SETRLIMIT, index, soft as usize, hard as usize) };
        Errno::from_ret(result).map(|_| ())
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
