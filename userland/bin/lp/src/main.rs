// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `lp` utility.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
mod program {
    extern crate alloc;

    use alloc::vec::Vec;

    use lp::{
        DestinationKind, Document, NO_DESTINATION, Request, SubmitError, USAGE, copy_order,
        destination_kind, input_names, parse, printed_message, resolve_destination,
    };
    use raven_proto_fs::{O_TRUNC, O_WRONLY};
    use raven_rt::entry;
    use raven_rt::fs;
    use raven_tool_core::Errno;
    use raven_tool_core::error::{EXIT_ERROR, EXIT_OK, EXIT_USAGE, line, usage};
    use raven_tool_core::path::with_file_scheme;

    entry!(main);

    fn main(args: raven_rt::Args) -> i32 {
        let options = match parse(args.iter().skip(1)) {
            Ok(options) => options,
            Err(error) => {
                raven_rt::eprintln!("{}", error.message());
                raven_rt::eprintln!("{}", usage("lp", USAGE));
                return EXIT_USAGE;
            }
        };
        let destination = match resolve_destination(
            options.destination.as_deref(),
            args.env_get("LPDEST"),
            args.env_get("PRINTER"),
        ) {
            Some(destination) => destination,
            None => {
                raven_rt::eprintln!("{NO_DESTINATION}");
                return EXIT_ERROR;
            }
        };
        if destination_kind(&destination) == DestinationKind::Named {
            raven_rt::eprintln!("{}", SubmitError::NoService.message(&destination));
            return EXIT_ERROR;
        }
        let target = with_file_scheme(&fs::resolve(&destination));
        let mut sink = match fs::open(&target, O_WRONLY | O_TRUNC) {
            Ok(file) => file,
            Err(error) => {
                raven_rt::eprintln!(
                    "{}",
                    SubmitError::Device(error.description()).message(&destination)
                );
                return EXIT_ERROR;
            }
        };
        let mut documents = Vec::new();
        for name in input_names(&options.files) {
            match read_document(name) {
                Ok(data) => documents.push(Document {
                    name: name.into(),
                    data,
                }),
                Err(error) => {
                    raven_rt::eprintln!("{}", line("lp", document_label(name), error));
                    return EXIT_ERROR;
                }
            }
        }
        let request = Request {
            options,
            destination,
            documents,
        };
        let mut status = EXIT_OK;
        for chunk in copy_order(&request) {
            if let Err(error) = write_all(&mut sink, chunk) {
                raven_rt::eprintln!("{}", line("lp", &request.destination, error));
                status = EXIT_ERROR;
                break;
            }
        }
        if let Err(error) = sink.close()
            && status == EXIT_OK
        {
            raven_rt::eprintln!("{}", line("lp", &request.destination, error));
            status = EXIT_ERROR;
        }
        if status == EXIT_OK && request.options.write_terminal && !request.options.silent {
            raven_rt::eprintln!("{}", printed_message(&request.destination));
        }
        status
    }

    fn read_document(name: &str) -> Result<Vec<u8>, Errno> {
        if name == "-" {
            read_stdin()
        } else {
            fs::read_to_vec(&with_file_scheme(&fs::resolve(name)))
        }
    }

    fn document_label(name: &str) -> &str {
        if name == "-" { "standard input" } else { name }
    }

    fn read_stdin() -> Result<Vec<u8>, Errno> {
        let mut data = Vec::new();
        let mut buffer = [0u8; 4096];
        loop {
            let read = raven_rt::debug_read(&mut buffer, 1_000_000_000)?;
            if read == 0 {
                return Ok(data);
            }
            data.extend_from_slice(buffer.get(..read).unwrap_or(&[]));
        }
    }

    fn write_all(file: &mut fs::File, data: &[u8]) -> Result<(), Errno> {
        let mut offset = 0usize;
        while offset < data.len() {
            let written = file.write(data.get(offset..).unwrap_or(&[]))?;
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
