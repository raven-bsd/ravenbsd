// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `ed` line editor.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::format;
    use alloc::string::{String, ToString};

    use ed::{Editor, Error, Io, parse_args};
    use raven_proto_fs::{O_CREAT, O_TRUNC, O_WRONLY};
    use raven_rt::entry;
    use raven_rt::fs;
    use raven_tool_core::Errno;
    use raven_tool_core::error::{EXIT_ERROR, EXIT_OK, EXIT_USAGE, usage};
    use raven_tool_core::path::with_file_scheme;

    entry!(main);

    struct FsIo;

    impl Io for FsIo {
        fn read(&mut self, path: &str) -> Result<String, Error> {
            let data =
                fs::read_to_vec(&with_file_scheme(&fs::resolve(path))).map_err(|_| Error::Io)?;
            Ok(String::from_utf8_lossy(&data).into_owned())
        }

        fn write(&mut self, path: &str, data: &str) -> Result<(), Error> {
            let resolved = with_file_scheme(&fs::resolve(path));
            let mut file =
                fs::open(&resolved, O_WRONLY | O_CREAT | O_TRUNC).map_err(|_| Error::Io)?;
            let bytes = data.as_bytes();
            let mut offset = 0usize;
            while offset < bytes.len() {
                let written = file
                    .write(bytes.get(offset..).unwrap_or(&[]))
                    .map_err(|_| Error::Io)?;
                if written == 0 {
                    return Err(Error::Io);
                }
                offset = offset.saturating_add(written);
            }
            file.close().map_err(|_| Error::Io)
        }
    }

    fn main(args: raven_rt::Args) -> i32 {
        let options = match parse_args(args.iter().skip(1).map(ToString::to_string)) {
            Ok(options) => options,
            Err(_) => {
                raven_rt::eprintln!("{}", usage("ed", "[-s] [file]"));
                return EXIT_USAGE;
            }
        };
        let mut editor = Editor::new();
        let mut io = FsIo;
        if let Some(path) = &options.file {
            let script = format!("e {path}\n");
            if let Err(error) = editor.execute(&script, &mut io) {
                raven_rt::eprintln!("ed: {}: {error:?}", path);
                return EXIT_ERROR;
            }
        }
        let mut script = String::new();
        let mut buffer = [0u8; 1024];
        loop {
            match raven_rt::debug_read(&mut buffer, 1_000_000_000) {
                Ok(0) => break,
                Ok(count) => {
                    let text = String::from_utf8_lossy(buffer.get(..count).unwrap_or(&[]));
                    script.push_str(&text);
                }
                // A console wake can arrive as an interrupt before the read deadline; it is
                // not end of input. K183: keep reading so the editor survives a blocked read.
                Err(Errno::EINTR) => continue,
                Err(_) => break,
            }
        }
        if let Err(error) = editor.execute(&script, &mut io) {
            let _ = write_stdout("?\n");
            raven_rt::eprintln!("ed: {error:?}");
            return EXIT_ERROR;
        }
        let _silent = options.silent;
        let output = editor.take_output();
        let _ = write_stdout(&output);
        EXIT_OK
    }

    fn write_stdout(text: &str) -> Result<(), Errno> {
        let bytes = text.as_bytes();
        let mut offset = 0usize;
        while offset < bytes.len() {
            let written = raven_rt::debug_write(bytes.get(offset..).unwrap_or(&[]))?;
            if written == 0 {
                return Err(Errno::EIO);
            }
            offset = offset.saturating_add(written);
        }
        Ok(())
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
