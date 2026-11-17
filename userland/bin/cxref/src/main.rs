// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `cxref`: print a C cross-reference table.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(not(target_os = "none"))]
#[derive(Debug, PartialEq, Eq)]
struct Options {
    output: Option<String>,
    files: Vec<String>,
}

#[cfg(not(target_os = "none"))]
fn parse_args<I: Iterator<Item = String>>(mut args: I) -> Result<Options, String> {
    let mut output = None;
    let mut files = Vec::new();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-o" => {
                output = Some(args.next().ok_or_else(|| "-o needs a file".to_string())?);
            }
            "-x" => {}
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
    Ok(Options { output, files })
}

#[cfg(not(target_os = "none"))]
fn main() {
    std::process::exit(run());
}

#[cfg(not(target_os = "none"))]
fn run() -> i32 {
    use std::io::Write as _;

    use raven_cparse::{SymbolKind, analyze};

    struct Entry {
        name: String,
        kind: SymbolKind,
        file: String,
        line: u32,
        references: Vec<(String, u32)>,
    }

    let options = match parse_args(std::env::args().skip(1)) {
        Ok(options) => options,
        Err(error) => {
            eprintln!("cxref: {error}");
            eprintln!("usage: cxref [-x] [-o file] file...");
            return 2;
        }
    };

    let mut entries = Vec::new();
    let mut references: Vec<(String, String, u32)> = Vec::new();
    for file in &options.files {
        let source = match std::fs::read_to_string(file) {
            Ok(source) => source,
            Err(error) => {
                eprintln!("cxref: {file}: {error}");
                return 1;
            }
        };
        let analysis = analyze(&source);
        for symbol in analysis.symbols {
            entries.push(Entry {
                name: symbol.name,
                kind: symbol.kind,
                file: file.clone(),
                line: symbol.line,
                references: Vec::new(),
            });
        }
        for reference in analysis.references {
            references.push((reference.name, file.clone(), reference.line));
        }
    }
    for entry in &mut entries {
        entry.references = references
            .iter()
            .filter(|(name, file, line)| {
                *name == entry.name && !(*file == entry.file && *line == entry.line)
            })
            .map(|(_, file, line)| (file.clone(), *line))
            .collect();
    }
    entries.sort_by(|left, right| {
        (&left.name, left.kind.as_str(), &left.file, left.line).cmp(&(
            &right.name,
            right.kind.as_str(),
            &right.file,
            right.line,
        ))
    });

    let mut text = String::new();
    for entry in &entries {
        let references = if entry.references.is_empty() {
            "-".to_string()
        } else {
            entry
                .references
                .iter()
                .map(|(file, line)| format!("{file}:{line}"))
                .collect::<Vec<_>>()
                .join(",")
        };
        text.push_str(&format!(
            "{}\t{}\t{}:{}\t{}\n",
            entry.name,
            entry.kind.as_str(),
            entry.file,
            entry.line,
            references
        ));
    }

    match options.output {
        Some(path) if path != "-" => {
            if let Err(error) = std::fs::write(&path, text) {
                eprintln!("cxref: {path}: {error}");
                return 1;
            }
        }
        _ => {
            if let Err(error) = std::io::stdout().write_all(text.as_bytes()) {
                eprintln!("cxref: stdout: {error}");
                return 1;
            }
        }
    }
    0
}

#[cfg(target_os = "none")]
mod program {
    use raven_rt::entry;

    entry!(main);

    fn main(_args: raven_rt::Args) -> i32 {
        raven_rt::eprintln!("cxref: target build is host-only");
        1
    }
}

#[cfg(test)]
#[path = "main_ut.rs"]
mod tests;
