// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! One-shot `tty:` client used by the `consd` qtest.
//!
//! Prints a prompt, round-trips the mode operations, reads one line, prints it, then expects the
//! next read to be interrupted (`^C`). The qtest types the input; see
//! `tests/qtest/cases/consd.toml`.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::format;
    use alloc::string::{String, ToString};
    use alloc::vec;
    use alloc::vec::Vec;

    use raven_abi::errno::Errno;
    use raven_abi::types::Handle;
    use raven_proto_fs::{O_RDWR, S_IFCHR, S_IFMT};
    use raven_proto_tty::{
        CALL_LEN, Mode, Request, Response, Settings, decode_response, encode_request,
    };
    use raven_rt::entry;
    use raven_scheme::Client;

    entry!(main);

    /// Longest accepted line.
    const LINE: usize = 256;

    fn main(_args: raven_rt::Args) -> i32 {
        match run() {
            Ok(code) => code,
            Err(message) => {
                raven_rt::eprintln!("consd-probe: {}", message);
                1
            }
        }
    }

    fn run() -> Result<i32, String> {
        let client = Client::new();
        // `Client::open` takes the S11 access bits, not the Linux `O_*` values.
        let flags = raven_rt::fs::s11_flags(O_RDWR)
            .map_err(|error| format!("open flags: {}", error.description()))?;
        let handle = client
            .open("tty:console", flags)
            .map_err(|error| format!("open tty:console: {}", error.description()))?;
        let stat = client
            .fstat(handle)
            .map_err(|error| format!("fstat tty:console: {}", error.description()))?;
        if stat.mode & S_IFMT != S_IFCHR {
            return Err(format!(
                "tty:console is not a character device: {:o}",
                stat.mode
            ));
        }

        modes_ok(&client, handle)?;
        raven_rt::print!("consd: type something> ");

        let line = read_line(&client, handle)?;
        raven_rt::println!("consd: got {}", trim_newline(&line));

        let result = match client.read(handle, 0, &mut [0u8; LINE]) {
            Err(Errno::EINTR) => {
                raven_rt::println!("consd: interrupted");
                Ok(0)
            }
            Ok(0) => {
                raven_rt::println!("consd: eof");
                Ok(0)
            }
            Ok(count) => Err(format!("second read returned {count} bytes")),
            Err(error) => Err(format!("second read failed: {}", error.description())),
        };
        let _ = client.close(handle);
        result
    }

    /// `GET_MODE`, switch to raw and back, and check every answer.
    fn modes_ok(client: &Client, handle: Handle) -> Result<(), String> {
        let initial = request(client, handle, Request::GetMode)?;
        if initial != Settings::default() {
            return Err(format!("initial settings are {initial:?}"));
        }
        let raw = Settings {
            mode: Mode::Raw,
            echo: false,
        };
        if request(client, handle, Request::SetMode { settings: raw })? != raw {
            return Err("raw mode was not applied".to_string());
        }
        let canonical = Settings::default();
        if request(
            client,
            handle,
            Request::SetMode {
                settings: canonical,
            },
        )? != canonical
        {
            return Err("canonical mode was not restored".to_string());
        }
        raven_rt::println!("consd: modes ok");
        Ok(())
    }

    fn request(client: &Client, handle: Handle, request: Request) -> Result<Settings, String> {
        let mut body = [0u8; CALL_LEN];
        encode_request(request, &mut body);
        let mut response = vec![0u8; CALL_LEN];
        let written = client
            .call(handle, &body, &mut response)
            .map_err(|error| format!("tty call: {}", error.description()))?;
        if written != CALL_LEN {
            return Err(format!("tty call wrote {written} bytes"));
        }
        match decode_response(&response) {
            Ok(Response::Mode(settings)) => Ok(settings),
            Ok(other) => Err(format!("unexpected tty response {other:?}")),
            Err(error) => Err(error.describe().to_string()),
        }
    }

    fn read_line(client: &Client, handle: Handle) -> Result<Vec<u8>, String> {
        let mut line = Vec::with_capacity(LINE);
        let mut buffer = [0u8; LINE];
        while line.len() < LINE {
            match client.read(handle, 0, &mut buffer) {
                Ok(0) => break,
                Ok(count) => {
                    line.extend_from_slice(&buffer[..count]);
                    if line.contains(&b'\n') {
                        break;
                    }
                }
                Err(Errno::EINTR) => return Err("interrupted while reading the line".to_string()),
                Err(error) => return Err(format!("read: {}", error.description())),
            }
        }
        Ok(line)
    }

    fn trim_newline(line: &[u8]) -> &str {
        let end = line
            .iter()
            .position(|byte| *byte == b'\n' || *byte == b'\r')
            .unwrap_or(line.len());
        core::str::from_utf8(&line[..end]).unwrap_or("<non-utf8>")
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
