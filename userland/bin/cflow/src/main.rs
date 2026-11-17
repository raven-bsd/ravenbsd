// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `cflow`: print the direct C call graph, forward or reversed.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(not(target_os = "none"))]
#[derive(Debug, PartialEq, Eq)]
struct Options {
    output: Option<String>,
    reverse: bool,
    files: Vec<String>,
}

#[cfg(not(target_os = "none"))]
struct FunctionInfo {
    name: String,
    line: u32,
    calls: Vec<String>,
}

#[cfg(not(target_os = "none"))]
fn parse_args<I: Iterator<Item = String>>(mut args: I) -> Result<Options, String> {
    let mut output = None;
    let mut reverse = false;
    let mut files = Vec::new();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-o" => {
                output = Some(args.next().ok_or_else(|| "-o needs a file".to_string())?);
            }
            "-r" => reverse = true,
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
        reverse,
        files,
    })
}

#[cfg(not(target_os = "none"))]
fn main() {
    std::process::exit(run());
}

#[cfg(not(target_os = "none"))]
fn run() -> i32 {
    use std::io::Write as _;

    use raven_cparse::analyze;

    let options = match parse_args(std::env::args().skip(1)) {
        Ok(options) => options,
        Err(error) => {
            eprintln!("cflow: {error}");
            eprintln!("usage: cflow [-r] [-o file] file...");
            return 2;
        }
    };

    let mut functions: Vec<FunctionInfo> = Vec::new();
    for file in &options.files {
        let source = match std::fs::read_to_string(file) {
            Ok(source) => source,
            Err(error) => {
                eprintln!("cflow: {file}: {error}");
                return 1;
            }
        };
        for function in analyze(&source).functions {
            if let Some(existing) = functions
                .iter_mut()
                .find(|candidate| candidate.name == function.name)
            {
                for call in function.calls {
                    if !existing.calls.contains(&call) {
                        existing.calls.push(call);
                    }
                }
            } else {
                functions.push(FunctionInfo {
                    name: function.name,
                    line: function.line,
                    calls: function.calls,
                });
            }
        }
    }

    let mut edges = functions
        .iter()
        .map(|function| (function.name.clone(), function.calls.clone()))
        .collect::<Vec<_>>();
    if options.reverse {
        let mut inverted = functions
            .iter()
            .map(|function| (function.name.clone(), Vec::new()))
            .collect::<Vec<_>>();
        for (caller, callees) in &edges {
            for callee in callees {
                if let Some(entry) = inverted.iter_mut().find(|(name, _)| name == callee)
                    && !entry.1.contains(caller)
                {
                    entry.1.push(caller.clone());
                }
            }
        }
        edges = inverted;
    }

    let callers = edges
        .iter()
        .flat_map(|(_, callees)| callees.iter().cloned())
        .collect::<Vec<_>>();
    let mut printed = Vec::new();
    let mut text = String::new();
    for function in &functions {
        if !callers.contains(&function.name) {
            emit(
                &function.name,
                0,
                &functions,
                &edges,
                &mut Vec::new(),
                &mut printed,
                &mut text,
            );
        }
    }
    for function in &functions {
        if !printed.contains(&function.name) {
            emit(
                &function.name,
                0,
                &functions,
                &edges,
                &mut Vec::new(),
                &mut printed,
                &mut text,
            );
        }
    }

    match options.output {
        Some(path) if path != "-" => {
            if let Err(error) = std::fs::write(&path, text) {
                eprintln!("cflow: {path}: {error}");
                return 1;
            }
        }
        _ => {
            if let Err(error) = std::io::stdout().write_all(text.as_bytes()) {
                eprintln!("cflow: stdout: {error}");
                return 1;
            }
        }
    }
    0
}

#[cfg(not(target_os = "none"))]
fn emit(
    name: &str,
    depth: usize,
    functions: &[FunctionInfo],
    edges: &[(String, Vec<String>)],
    visiting: &mut Vec<String>,
    printed: &mut Vec<String>,
    text: &mut String,
) {
    use std::fmt::Write as _;

    let Some(function) = functions.iter().find(|candidate| candidate.name == name) else {
        return;
    };
    let _ = writeln!(text, "{}{}: {}", "    ".repeat(depth), name, function.line);
    if !printed.iter().any(|candidate| candidate == name) {
        printed.push(name.to_string());
    }
    visiting.push(name.to_string());
    if let Some((_, callees)) = edges.iter().find(|(caller, _)| caller == name) {
        for callee in callees {
            if visiting.iter().any(|candidate| candidate == callee) {
                if let Some(cycle) = functions.iter().find(|candidate| candidate.name == *callee) {
                    let _ = writeln!(
                        text,
                        "{}{}: {} (cycle)",
                        "    ".repeat(depth + 1),
                        callee,
                        cycle.line
                    );
                }
            } else {
                emit(callee, depth + 1, functions, edges, visiting, printed, text);
            }
        }
    }
    visiting.pop();
}

#[cfg(target_os = "none")]
mod program {
    use raven_rt::entry;

    entry!(main);

    fn main(_args: raven_rt::Args) -> i32 {
        raven_rt::eprintln!("cflow: target build is host-only");
        1
    }
}

#[cfg(test)]
#[path = "main_ut.rs"]
mod tests;
