// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Command-line front-end.
//!
//! `pfctl` is split like the rest of the Raven userland: [`run`] is pure and
//! host-testable, `main.rs` only wires the file loader and the process exit.
//! Until the pf ABI lands (K273) every mutating action is refused and `-n`
//! (dry run) is required; `-nv -f pf.conf` is the supported invocation. Exit
//! codes follow the host tool convention: 0 success, 1 any parse/usage error.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::ast::{HostAddr, Statement, TableDef, TableEntry};
use crate::parser::{FileLoader, ParseOptions, parse_with};
use crate::print::{print_config, print_statement};
use crate::table::parse_table_file;

/// Usage text.
pub const USAGE: &str = "\
usage: pfctl [-n] [-v] [-f file] [-D macro=value] [-t table [-T command [-f file]]]

  -n   dry run (parse only; required: this build has no pf ABI yet)
  -v   print the normalized configuration
  -f   configuration file
  -D   define a macro (name=value, repeatable)
  -t   table name for -T
  -T   table command: add, delete, replace, test (show/flush need the pf ABI)
";

/// The result of one CLI invocation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outcome {
    /// Process exit code.
    pub exit_code: i32,
    /// Text for stdout (already newline-terminated).
    pub stdout: String,
    /// Text for stderr (already newline-terminated).
    pub stderr: String,
}

/// A table operation selected with `-T`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableOp {
    /// Add entries.
    Add,
    /// Delete entries.
    Delete,
    /// Replace entries.
    Replace,
    /// Test membership.
    Test,
    /// Show the table (needs the pf ABI).
    Show,
    /// Flush the table (needs the pf ABI).
    Flush,
}

#[derive(Clone, Debug, Default)]
struct Options {
    no_action: bool,
    verbose: bool,
    file: Option<String>,
    macros: Vec<(String, String)>,
    table: Option<String>,
    table_op: Option<TableOp>,
    help: bool,
}

fn parse_args(args: &[String]) -> Result<Options, String> {
    let mut options = Options::default();
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if arg == "--help" {
            options.help = true;
            index += 1;
            continue;
        }
        let Some(cluster) = arg.strip_prefix('-') else {
            return Err(format!("unexpected argument `{arg}'"));
        };
        if cluster.is_empty() {
            return Err(String::from("invalid option `-'"));
        }
        let chars: Vec<char> = cluster.chars().collect();
        let mut at = 0;
        while at < chars.len() {
            let letter = chars[at];
            match letter {
                'n' => options.no_action = true,
                'v' => options.verbose = true,
                'h' => options.help = true,
                'f' | 'D' | 't' | 'T' => {
                    let value = if at + 1 < chars.len() {
                        chars[at + 1..].iter().collect::<String>()
                    } else {
                        index += 1;
                        args.get(index)
                            .cloned()
                            .ok_or_else(|| format!("option -{letter} needs a value"))?
                    };
                    match letter {
                        'f' => options.file = Some(value),
                        'D' => {
                            let Some((name, value)) = value.split_once('=') else {
                                return Err(format!(
                                    "macro definition `{value}' is not name=value"
                                ));
                            };
                            options.macros.push((name.to_string(), value.to_string()));
                        }
                        't' => options.table = Some(value),
                        'T' => options.table_op = Some(parse_table_op(&value)?),
                        _ => unreachable!("option letters are matched above"),
                    }
                    at = chars.len();
                    continue;
                }
                other => return Err(format!("unknown option `-{other}'")),
            }
            at += 1;
        }
        index += 1;
    }
    Ok(options)
}

fn parse_table_op(value: &str) -> Result<TableOp, String> {
    match value {
        "add" => Ok(TableOp::Add),
        "delete" => Ok(TableOp::Delete),
        "replace" => Ok(TableOp::Replace),
        "test" => Ok(TableOp::Test),
        "show" => Ok(TableOp::Show),
        "flush" => Ok(TableOp::Flush),
        other => Err(format!(
            "unknown table command `{other}' (add, delete, replace, test, show, flush)"
        )),
    }
}

fn fail(message: impl Into<String>) -> Outcome {
    Outcome {
        exit_code: 1,
        stdout: String::new(),
        stderr: format!("pfctl: {}\n", message.into()),
    }
}

fn fail_with_usage(message: impl Into<String>) -> Outcome {
    Outcome {
        exit_code: 1,
        stdout: String::new(),
        stderr: format!("pfctl: {}\n{USAGE}", message.into()),
    }
}

/// Run one CLI invocation.
pub fn run(args: &[String], loader: &dyn FileLoader) -> Outcome {
    let options = match parse_args(args) {
        Ok(options) => options,
        Err(message) => return fail_with_usage(message),
    };
    if options.help {
        return Outcome {
            exit_code: 0,
            stdout: String::from(USAGE),
            stderr: String::new(),
        };
    }
    if let Some(op) = options.table_op {
        return run_table(&options, op, loader);
    }
    let Some(file) = &options.file else {
        return fail_with_usage("no action specified");
    };
    if !options.no_action {
        return fail(
            "loading a ruleset requires the pf ABI (K273); re-run with `-n' for a dry run",
        );
    }
    let text = match loader.read(file) {
        Ok(text) => text,
        Err(error) => return fail(format!("{file}: {error}")),
    };
    let config = match parse_with(
        &text,
        ParseOptions {
            source: Some(file),
            loader: Some(loader),
            macros: options.macros.clone(),
        },
    ) {
        Ok(config) => config,
        Err(error) => return fail(error.display()),
    };
    let stdout = if options.verbose {
        print_config(&config)
    } else {
        String::new()
    };
    Outcome {
        exit_code: 0,
        stdout,
        stderr: String::new(),
    }
}

fn run_table(options: &Options, op: TableOp, loader: &dyn FileLoader) -> Outcome {
    let Some(table) = &options.table else {
        return fail_with_usage("`-T' needs `-t <table>'");
    };
    match op {
        TableOp::Show | TableOp::Flush => {
            return fail(format!(
                "table {} requires the pf ABI (K273)",
                match op {
                    TableOp::Show => "show",
                    _ => "flush",
                }
            ));
        }
        TableOp::Add | TableOp::Delete | TableOp::Replace | TableOp::Test => {}
    }
    if !options.no_action {
        return fail("table changes require the pf ABI (K273); re-run with `-n'");
    }
    let Some(file) = &options.file else {
        return fail_with_usage("the table command needs `-f <file>'");
    };
    let text = match loader.read(file) {
        Ok(text) => text,
        Err(error) => return fail(format!("{file}: {error}")),
    };
    let parsed = match parse_table_file(&text) {
        Ok(parsed) => parsed,
        Err(error) => return fail(format!("{file}:{}", error.display())),
    };
    let stdout = if options.verbose {
        let entries: Vec<TableEntry> = parsed
            .entries
            .iter()
            .map(|entry| TableEntry::Address {
                negative: entry.negative,
                address: HostAddr::Literal(entry.address.clone()),
            })
            .collect();
        let definition = TableDef {
            name: table.clone(),
            flags: Vec::new(),
            entries,
        };
        let mut printed = print_statement(&Statement::Table(definition));
        printed.push('\n');
        printed
    } else {
        String::new()
    };
    Outcome {
        exit_code: 0,
        stdout,
        stderr: String::new(),
    }
}

#[cfg(test)]
#[path = "cli_ut.rs"]
mod tests;
