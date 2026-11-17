// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `talk` utility.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::string::{String, ToString};
    use alloc::vec::Vec;

    use raven_proto_fs::{Errno, O_RDWR};
    use raven_rt::entry;
    use raven_rt::fs;
    use raven_scheme::Client;
    use raven_tool_core::error::{EXIT_ERROR, EXIT_USAGE, usage};
    use talk::{Options, loopback_transcript, parse_args};

    entry!(main);

    const TTYD_VERSION: u16 = 1;
    const TTYD_OP_STATE: u16 = 0x23;
    const HUNG_BIT: u32 = 0x8000_0000;

    fn main(args: raven_rt::Args) -> i32 {
        let args: Vec<String> = args.iter().skip(1).map(ToString::to_string).collect();
        if args.len() == 1 && args.first().is_some_and(|arg| arg == "--loopback") {
            return loopback();
        }
        let options = match parse_args(args) {
            Ok(options) => options,
            Err(_) => {
                raven_rt::eprintln!("{}", usage("talk", "[-l] address [terminal]"));
                return EXIT_USAGE;
            }
        };
        diagnostic(&options)
    }

    fn loopback() -> i32 {
        match loopback_transcript() {
            Ok(lines) => {
                for line in lines {
                    raven_rt::println!("{line}");
                }
                0
            }
            Err(error) => {
                raven_rt::eprintln!("talk: loopback: {}", error.describe());
                EXIT_ERROR
            }
        }
    }

    fn diagnostic(options: &Options) -> i32 {
        if options.listen {
            raven_rt::eprintln!("talk: no peer transport for a local listener");
            return EXIT_ERROR;
        }
        let Some(address) = &options.address else {
            raven_rt::eprintln!("{}", usage("talk", "[-l] address [terminal]"));
            return EXIT_USAGE;
        };
        if let Some(host) = &address.host
            && host != "localhost"
        {
            raven_rt::eprintln!(
                "talk: network talk to {} is not implemented; only local tty-to-tty is supported",
                address.text()
            );
            return EXIT_ERROR;
        }
        match session_state() {
            Ok((foreground, session)) => raven_rt::eprintln!(
                "talk: local session {session} (fg {foreground}) has no peer terminal registry; {} is not reachable",
                address.user
            ),
            Err(error) => raven_rt::eprintln!(
                "talk: tty session unavailable: {}; {} is not reachable",
                error.description(),
                address.user
            ),
        }
        EXIT_ERROR
    }

    fn session_state() -> Result<(u32, u32), Errno> {
        let flags = fs::s11_flags(O_RDWR)?;
        let client = Client::new();
        let handle = client.open("ttyd:session", flags)?;
        let mut request = [0u8; 12];
        request[..2].copy_from_slice(&TTYD_VERSION.to_le_bytes());
        request[2..4].copy_from_slice(&TTYD_OP_STATE.to_le_bytes());
        let mut response = [0u8; 12];
        let result = client.call(handle, &request, &mut response);
        let _ = client.close(handle);
        result?;
        if u16::from_le_bytes([response[0], response[1]]) != TTYD_VERSION {
            return Err(Errno::EPROTO);
        }
        let foreground = u32::from_le_bytes([response[4], response[5], response[6], response[7]]);
        let session =
            u32::from_le_bytes([response[8], response[9], response[10], response[11]]) & !HUNG_BIT;
        Ok((foreground, session))
    }
}

#[cfg(not(target_os = "none"))]
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(host_run(&args));
}

#[cfg(not(target_os = "none"))]
fn host_run(args: &[String]) -> i32 {
    if args.len() == 1 && args.first().is_some_and(|arg| arg == "--loopback") {
        return match talk::loopback_transcript() {
            Ok(lines) => {
                for line in lines {
                    println!("{line}");
                }
                0
            }
            Err(error) => {
                eprintln!("talk: loopback: {}", error.describe());
                1
            }
        };
    }
    let options = match talk::parse_args(args.iter().cloned()) {
        Ok(options) => options,
        Err(_) => {
            eprintln!("usage: {}", talk::USAGE);
            return 2;
        }
    };
    if options.listen {
        eprintln!("talk: no peer transport for a local listener");
        return 1;
    }
    let Some(address) = &options.address else {
        eprintln!("usage: {}", talk::USAGE);
        return 2;
    };
    if let Some(host) = &address.host
        && host != "localhost"
    {
        eprintln!(
            "talk: network talk to {} is not implemented; only local tty-to-tty is supported",
            address.text()
        );
        return 1;
    }
    eprintln!(
        "talk: no logged-in terminal for {} (no terminal registry)",
        address.user
    );
    1
}
