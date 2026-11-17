// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `logger` utility.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::format;
    use alloc::string::{String, ToString};

    use raven_proto_fs::O_WRONLY;
    use raven_rt::entry;

    entry!(main);

    fn scheme_path(path: &str) -> String {
        match path.find(':') {
            Some(index) if !path[..index].contains('/') => path.to_string(),
            _ if path.starts_with('/') => format!("file:{path}"),
            _ => format!("file:/{path}"),
        }
    }

    fn tag(text: Option<&str>) -> String {
        let Some(text) = text else {
            return "logger".to_string();
        };
        let mut result = String::new();
        for character in text.chars() {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-') {
                result.push(character);
            } else {
                result.push('_');
            }
        }
        if result.is_empty() {
            "logger".to_string()
        } else {
            result
        }
    }

    fn message(options: &raven_logger::Options) -> Result<String, raven_abi::errno::Errno> {
        if let Some(path) = &options.file {
            let data = raven_rt::fs::read_to_vec(&scheme_path(path))?;
            return Ok(String::from_utf8_lossy(&data).into_owned());
        }
        Ok(options.message.join(" "))
    }

    fn main(args: raven_rt::Args) -> i32 {
        let options = match raven_logger::parse_args(args.iter().skip(1)) {
            Ok(options) => options,
            Err(_) => {
                raven_rt::eprintln!("usage: logger {}", raven_logger::USAGE);
                return 2;
            }
        };
        let message = match message(&options) {
            Ok(message) => message,
            Err(error) => {
                raven_rt::eprintln!("logger: {}", error.description());
                return 1;
            }
        };
        let name = tag(options.tag.as_deref());
        let path = format!("log:{name}");
        let payload = raven_logger::payload(options.priority, &message);
        let mut file = match raven_rt::fs::open(&path, O_WRONLY) {
            Ok(file) => file,
            Err(error) => {
                raven_rt::eprintln!("logger: {path}: {}", error.description());
                return 1;
            }
        };
        if let Err(error) = file.write(payload.as_bytes()) {
            raven_rt::eprintln!("logger: {path}: {}", error.description());
            let _ = file.close();
            return 1;
        }
        if let Err(error) = file.close() {
            raven_rt::eprintln!("logger: {path}: {}", error.description());
            return 1;
        }
        0
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
