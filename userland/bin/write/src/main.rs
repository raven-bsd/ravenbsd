// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `write` utility.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::format;
    use alloc::string::{String, ToString};

    use raven_rt::entry;
    use raven_userdb::rt::RtIo;
    use raven_userdb::store::{Paths, load_lookup};
    use write::{Directory, Error, Terminal, deliver, parse_args};

    entry!(main);

    struct DbDirectory {
        known: bool,
    }

    impl Directory for DbDirectory {
        fn known(&self, _user: &str) -> bool {
            self.known
        }
    }

    struct ConsoleTerminal {
        known: bool,
    }

    impl Terminal for ConsoleTerminal {
        fn logged_in(&mut self, _user: &str, _tty: Option<&str>) -> bool {
            self.known
        }

        fn mesg_enabled(&mut self, _user: &str, _tty: Option<&str>) -> bool {
            true
        }

        fn send(&mut self, user: &str, _tty: Option<&str>, message: &str) -> bool {
            let header = format!("Message from {user}\r\n");
            write_stdout(&header) && write_stdout(message)
        }
    }

    fn main(args: raven_rt::Args) -> i32 {
        let options = match parse_args(args.iter().skip(1).map(ToString::to_string)) {
            Ok(options) => options,
            Err(_) => {
                raven_rt::eprintln!("usage: write user [tty]");
                return 2;
            }
        };
        let mut io = RtIo;
        let database = match load_lookup(&mut io, &Paths::system()) {
            Ok(database) => database,
            Err(error) => {
                raven_rt::eprintln!("write: {error}");
                return 1;
            }
        };
        let known = database.user(&options.user).is_some();
        let mut message = String::new();
        let mut buffer = [0u8; 512];
        loop {
            match raven_rt::debug_read(&mut buffer, 1_000_000_000) {
                Ok(0) | Err(_) => break,
                Ok(count) => {
                    let text = String::from_utf8_lossy(buffer.get(..count).unwrap_or(&[]));
                    message.push_str(&text);
                }
            }
        }
        let directory = DbDirectory { known };
        let mut terminal = ConsoleTerminal { known };
        match deliver(&options, &message, &directory, &mut terminal) {
            Ok(_) => 0,
            Err(error) => {
                raven_rt::eprintln!("write: {}", text(error));
                1
            }
        }
    }

    fn write_stdout(text: &str) -> bool {
        let bytes = text.as_bytes();
        let mut offset = 0usize;
        while offset < bytes.len() {
            match raven_rt::debug_write(bytes.get(offset..).unwrap_or(&[])) {
                Ok(0) | Err(_) => return false,
                Ok(count) => offset = offset.saturating_add(count),
            }
        }
        true
    }

    fn text(error: Error) -> &'static str {
        match error {
            Error::UnknownUser => "no such user",
            Error::NotLoggedIn => "user is not logged in",
            Error::MesgOff => "user has messages disabled",
            Error::Send => "terminal write failed",
        }
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
