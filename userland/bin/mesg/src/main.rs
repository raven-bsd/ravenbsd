// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `mesg` utility.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
mod program {
    use raven_proto_fs::{OpenFlags, S_IFCHR, S_IFMT};
    use raven_proto_tty::{CALL_LEN, WriteRequest, decode_write_response, encode_write_request};
    use raven_rt::entry;
    use raven_scheme::Client;

    entry!(main);

    fn set_write(target: &str, allowed: bool) -> Result<bool, raven_abi::errno::Errno> {
        let client = Client::new();
        let handle = client.open_flags(target, OpenFlags::READ_WRITE)?;
        let mut request = [0u8; CALL_LEN];
        encode_write_request(WriteRequest::Set { allowed }, &mut request);
        let mut response = [0u8; CALL_LEN];
        let length = match client.call(handle, &request, &mut response) {
            Ok(length) => length,
            Err(error) => {
                let _ = client.close(handle);
                return Err(error);
            }
        };
        let _ = client.close(handle);
        if length < CALL_LEN {
            return Err(raven_abi::errno::Errno::EPROTO);
        }
        decode_write_response(&response[..length]).map_err(|_| raven_abi::errno::Errno::EPROTO)
    }

    fn main(args: raven_rt::Args) -> i32 {
        let options = match raven_mesg::parse_args(args.iter().skip(1)) {
            Ok(options) => options,
            Err(_) => {
                raven_rt::eprintln!("usage: mesg {}", raven_mesg::USAGE);
                return 2;
            }
        };
        let target = options.target.as_deref().unwrap_or("tty:console");
        let mode = match raven_rt::fs::stat(target) {
            Ok(stat) => stat.mode,
            Err(error) => {
                raven_rt::eprintln!("mesg: {target}: {}", error.description());
                return 1;
            }
        };
        if mode & S_IFMT != S_IFCHR {
            raven_rt::eprintln!("mesg: {target}: not a tty");
            return 1;
        }
        match options.action {
            raven_mesg::Action::Query => {
                if raven_mesg::allows(mode) {
                    raven_rt::println!("y");
                    0
                } else {
                    raven_rt::println!("n");
                    1
                }
            }
            raven_mesg::Action::Set(allowed) => match set_write(target, allowed) {
                Ok(_) => 0,
                Err(error) => {
                    raven_rt::eprintln!("mesg: {target}: {}", error.description());
                    1
                }
            },
        }
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
