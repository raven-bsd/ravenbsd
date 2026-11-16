// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `patch` utility.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::string::{String, ToString};
    use alloc::vec::Vec;

    use raven_patch::{FileSystem, PatchError, apply, parse_args};
    use raven_proto_fs::{O_CREAT, O_TRUNC, O_WRONLY};
    use raven_rt::entry;
    use raven_rt::fs;
    use raven_tool_core::error::{EXIT_ERROR, EXIT_OK, EXIT_USAGE, usage};
    use raven_tool_core::path::with_file_scheme;

    entry!(main);

    struct Files;

    impl FileSystem for Files {
        fn read(&mut self, path: &str) -> Option<String> {
            let resolved = with_file_scheme(&fs::resolve(path));
            fs::read_to_vec(&resolved)
                .ok()
                .map(|data| String::from_utf8_lossy(&data).into_owned())
        }

        fn write(&mut self, path: &str, contents: &str) -> bool {
            let resolved = with_file_scheme(&fs::resolve(path));
            let Ok(mut file) = fs::open(&resolved, O_WRONLY | O_CREAT | O_TRUNC) else {
                return false;
            };
            if write_all(&mut file, contents.as_bytes()).is_err() {
                return false;
            }
            file.close().is_ok()
        }

        fn remove(&mut self, path: &str) -> bool {
            fs::unlink(&with_file_scheme(&fs::resolve(path))).is_ok()
        }
    }

    fn main(args: raven_rt::Args) -> i32 {
        let arguments: Vec<String> = args.iter().skip(1).map(|value| value.to_string()).collect();
        let invocation = match parse_args(&arguments) {
            Ok(invocation) => invocation,
            Err(error) => return fail(error),
        };
        let mut patches = Vec::new();
        if let Some(input) = &invocation.input {
            match read_patch(input) {
                Some(text) => patches.push(text),
                None => {
                    raven_rt::eprintln!("patch: {input}: cannot read patch file");
                    return EXIT_ERROR;
                }
            }
        } else if invocation.patch_files.is_empty() {
            match read_console() {
                Ok(text) => patches.push(text),
                Err(error) => {
                    raven_rt::eprintln!("patch: tty:console: {}", error.description());
                    return EXIT_ERROR;
                }
            }
        } else {
            for path in &invocation.patch_files {
                match read_patch(path) {
                    Some(text) => patches.push(text),
                    None => {
                        raven_rt::eprintln!("patch: {path}: cannot read patch file");
                        return EXIT_ERROR;
                    }
                }
            }
        }
        let mut files = Files;
        for text in patches {
            if let Err(error) = apply(&text, &invocation.options, &mut files) {
                return fail(error);
            }
        }
        EXIT_OK
    }

    fn read_patch(path: &str) -> Option<String> {
        if path == "-" {
            read_console().ok()
        } else {
            Files.read(path)
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

    fn write_all(file: &mut fs::File, data: &[u8]) -> Result<(), raven_tool_core::Errno> {
        let mut offset = 0usize;
        while offset < data.len() {
            let written = file.write(data.get(offset..).unwrap_or(&[]))?;
            if written == 0 {
                return Err(raven_tool_core::Errno::EIO);
            }
            offset = offset.saturating_add(written);
        }
        Ok(())
    }

    fn fail(error: PatchError) -> i32 {
        if error == PatchError::Usage {
            raven_rt::eprintln!(
                "{}",
                usage("patch", "[-Ruf] [-p strip] [-i patchfile] [patchfile ...]")
            );
            return EXIT_USAGE;
        }
        raven_rt::eprintln!("patch: {error}");
        EXIT_ERROR
    }
}

#[cfg(not(target_os = "none"))]
use std::io::Read as _;

#[cfg(not(target_os = "none"))]
struct HostFiles;

#[cfg(not(target_os = "none"))]
impl raven_patch::FileSystem for HostFiles {
    fn read(&mut self, path: &str) -> Option<String> {
        std::fs::read_to_string(path).ok()
    }

    fn write(&mut self, path: &str, contents: &str) -> bool {
        std::fs::write(path, contents).is_ok()
    }

    fn remove(&mut self, path: &str) -> bool {
        std::fs::remove_file(path).is_ok()
    }
}

#[cfg(not(target_os = "none"))]
fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(host_run(&arguments));
}

#[cfg(not(target_os = "none"))]
fn host_run(arguments: &[String]) -> i32 {
    use raven_patch::{apply, parse_args};

    let invocation = match parse_args(arguments) {
        Ok(invocation) => invocation,
        Err(error) => return host_fail(error),
    };
    let mut patches = Vec::new();
    if let Some(input) = &invocation.input {
        match read_patch(input) {
            Some(text) => patches.push(text),
            None => {
                eprintln!("patch: {input}: cannot read patch file");
                return 1;
            }
        }
    } else if invocation.patch_files.is_empty() {
        match read_stdin() {
            Ok(text) => patches.push(text),
            Err(error) => {
                eprintln!("patch: standard input: {error}");
                return 1;
            }
        }
    } else {
        for path in &invocation.patch_files {
            match read_patch(path) {
                Some(text) => patches.push(text),
                None => {
                    eprintln!("patch: {path}: cannot read patch file");
                    return 1;
                }
            }
        }
    }
    let mut files = HostFiles;
    for text in patches {
        if let Err(error) = apply(&text, &invocation.options, &mut files) {
            return host_fail(error);
        }
    }
    0
}

#[cfg(not(target_os = "none"))]
fn read_patch(path: &str) -> Option<String> {
    if path == "-" {
        read_stdin().ok()
    } else {
        std::fs::read_to_string(path).ok()
    }
}

#[cfg(not(target_os = "none"))]
fn read_stdin() -> std::io::Result<String> {
    let mut text = String::new();
    std::io::stdin().read_to_string(&mut text)?;
    Ok(text)
}

#[cfg(not(target_os = "none"))]
fn host_fail(error: raven_patch::PatchError) -> i32 {
    use raven_patch::PatchError;

    if error == PatchError::Usage {
        eprintln!("usage: patch [-Ruf] [-p strip] [-i patchfile] [patchfile ...]");
        return 2;
    }
    eprintln!("patch: {error}");
    1
}
