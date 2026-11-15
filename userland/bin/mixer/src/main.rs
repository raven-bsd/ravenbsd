// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! RavenBSD `mixer`: volume, mute and status on `audio:out0`.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::string::String;
    use alloc::vec::Vec;

    use raven_abi::errno::Errno;
    use raven_abi::types::Handle;
    use raven_proto_audio::{Call, StreamRequest, StreamResponse, StreamState};
    use raven_rt::entry;
    use raven_scheme::Client;

    use raven_mixer::{Command, mute_line, parse_args, status_line, volume_line};

    entry!(main);

    fn call(client: &Client, handle: Handle, call: Call) -> Result<StreamResponse, Errno> {
        let mut request = [0u8; 64];
        let length = StreamRequest::Call(call)
            .encode(&mut request)
            .map_err(|_| Errno::EINVAL)?;
        let mut response = [0u8; 64];
        let length = client.call(handle, &request[..length], &mut response)?;
        StreamResponse::decode(&response[..length]).map_err(|_| Errno::EPROTO)
    }

    fn expect_ok(client: &Client, handle: Handle, call: Call) -> Result<(), Errno> {
        match self::call(client, handle, call)? {
            StreamResponse::Ok => Ok(()),
            StreamResponse::Err(error) => Err(error),
            _ => Err(Errno::EPROTO),
        }
    }

    fn state_name(state: StreamState) -> &'static str {
        match state {
            StreamState::Stopped => "stopped",
            StreamState::Running => "running",
            StreamState::Draining => "draining",
            StreamState::Underrun => "underrun",
        }
    }

    fn run(command: Command) -> Result<(), Errno> {
        let client = Client::new();
        let handle = client.open("audio:out0", 3)?;
        match command {
            Command::Volume(percent) => {
                let gain = raven_mixer::percent_to_gain(percent);
                expect_ok(
                    &client,
                    handle,
                    Call::SetVolume {
                        left: gain,
                        right: gain,
                    },
                )?;
                raven_rt::println!("{}", volume_line(percent));
            }
            Command::Mute => {
                expect_ok(&client, handle, Call::SetVolume { left: 0, right: 0 })?;
                raven_rt::println!("{}", mute_line());
            }
            Command::Status => {
                let response = call(&client, handle, Call::GetStatus)?;
                match response {
                    StreamResponse::Status(status) => raven_rt::println!(
                        "{}",
                        status_line(
                            status.position_frames,
                            status.underruns,
                            state_name(status.state)
                        )
                    ),
                    StreamResponse::Err(error) => {
                        let _ = client.close(handle);
                        return Err(error);
                    }
                    _ => {
                        let _ = client.close(handle);
                        return Err(Errno::EPROTO);
                    }
                }
            }
        }
        let _ = client.close(handle);
        Ok(())
    }

    fn main(args: raven_rt::Args) -> i32 {
        let operands: Vec<String> = args
            .iter()
            .skip(1)
            .map(alloc::string::ToString::to_string)
            .collect();
        let command = match parse_args(&operands) {
            Ok(command) => command,
            Err(message) => {
                raven_rt::eprintln!("{message}");
                return 2;
            }
        };
        match run(command) {
            Ok(()) => 0,
            Err(error) => {
                raven_rt::eprintln!("mixer: {}", error.description());
                1
            }
        }
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
