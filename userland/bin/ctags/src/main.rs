// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `ctags`: emit a tags file for C functions, globals, macros, structs and enums.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(not(target_os = "none"))]
#[derive(Debug, PartialEq, Eq)]
struct Options {
    output: String,
    xref: bool,
    files: Vec<String>,
}

#[cfg(not(target_os = "none"))]
fn parse_args<I: Iterator<Item = String>>(mut args: I) -> Result<Options, String> {
    let mut output = "tags".to_string();
    let mut xref = false;
    let mut files = Vec::new();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-f" => {
                output = args.next().ok_or_else(|| "-f needs a file".to_string())?;
            }
            "-x" => xref = true,
            "--" => {
                files.extend(args);
                break;
            }
            value if value.starts_with('-') => {
                return Err(format!("unsupported option {value}"));
            }
            value => files.push(value.to_string()),
        }
    }
    if files.is_empty() {
        return Err("missing file operand".to_string());
    }
    Ok(Options {
        output,
        xref,
        files,
    })
}

#[cfg(not(target_os = "none"))]
fn main() {
    std::process::exit(run());
}

#[cfg(not(target_os = "none"))]
fn run() -> i32 {
    use std::io::Write;

    use raven_cparse::{SymbolKind, analyze};

    struct Tag {
        name: String,
        file: String,
        line: u32,
        kind: SymbolKind,
        text: String,
    }

    let options = match parse_args(std::env::args().skip(1)) {
        Ok(options) => options,
        Err(error) => {
            eprintln!("ctags: {error}");
            eprintln!("usage: ctags [-x] [-f file] file...");
            return 2;
        }
    };

    let mut tags = Vec::new();
    for file in &options.files {
        let source = match std::fs::read_to_string(file) {
            Ok(source) => source,
            Err(error) => {
                eprintln!("ctags: {file}: {error}");
                return 1;
            }
        };
        let lines = source.lines().collect::<Vec<_>>();
        for symbol in analyze(&source).symbols {
            let text = lines
                .get(symbol.line.saturating_sub(1) as usize)
                .map(|line| line.trim().to_string())
                .unwrap_or_default();
            tags.push(Tag {
                name: symbol.name,
                file: file.clone(),
                line: symbol.line,
                kind: symbol.kind,
                text,
            });
        }
    }
    tags.sort_by(|left, right| {
        (&left.name, &left.file, left.line, left.kind.as_str()).cmp(&(
            &right.name,
            &right.file,
            right.line,
            right.kind.as_str(),
        ))
    });

    let mut text = String::new();
    for tag in &tags {
        if options.xref {
            text.push_str(&format!(
                "{}\t{}\t{}\t{}\n",
                tag.name, tag.line, tag.file, tag.text
            ));
        } else {
            text.push_str(&format!("{}\t{}\t{}\n", tag.name, tag.file, tag.line));
        }
    }
    if options.xref || options.output == "-" {
        if let Err(error) = std::io::stdout().write_all(text.as_bytes()) {
            eprintln!("ctags: stdout: {error}");
            return 1;
        }
    } else if let Err(error) = std::fs::write(&options.output, text) {
        eprintln!("ctags: {}: {error}", options.output);
        return 1;
    }
    0
}

#[cfg(target_os = "none")]
mod program {
    use raven_rt::entry;

    entry!(main);

    fn main(_args: raven_rt::Args) -> i32 {
        raven_rt::eprintln!("ctags: target build is host-only");
        1
    }
}

#[cfg(test)]
#[path = "main_ut.rs"]
mod tests;
