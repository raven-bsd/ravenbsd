// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `raven-integration-inventory`: print the inventory, or check the design document is current.
//!
//! ```text
//! raven-integration-inventory [--root <dir>]              # print the table and the counts
//! raven-integration-inventory --check [--doc <path>]      # fail when the document is stale
//! ```

#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use raven_integration_inventory::{
    BLOCK_BEGIN, BLOCK_END, check_document, inventory, render_block, write_document,
};

const DEFAULT_DOC: &str = "docs/design/integration-inventory.md";

const USAGE: &str = "\
usage:
  raven-integration-inventory [--root <dir>]
  raven-integration-inventory --write [--root <dir>] [--doc <path>]
  raven-integration-inventory --check [--root <dir>] [--doc <path>]

`--write` regenerates the `integration-inventory:begin/end` block in the document.
`--check` regenerates it in memory and fails (exit 1) when the file differs.";

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("raven-integration-inventory: {error:#}");
            ExitCode::from(2)
        }
    }
}

fn run() -> anyhow::Result<u8> {
    let mut root = PathBuf::from(".");
    let mut document = PathBuf::from(DEFAULT_DOC);
    let mut check = false;
    let mut write = false;
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--root" => {
                index += 1;
                root = PathBuf::from(value(&args, index, "--root")?);
            }
            "--doc" => {
                index += 1;
                document = PathBuf::from(value(&args, index, "--doc")?);
            }
            "--check" => check = true,
            "--write" => write = true,
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(0);
            }
            other => anyhow::bail!("unexpected argument `{other}`\n{USAGE}"),
        }
        index += 1;
    }
    let root = absolute(&root);
    let document = absolute(&document);

    if check {
        return match check_document(&root, &document)? {
            None => {
                println!("integration inventory: up to date");
                Ok(0)
            }
            Some(path) => {
                eprintln!(
                    "integration inventory: {path} is stale; regenerate the block with\n  \
                     cargo run -p raven-integration-inventory -- --write"
                );
                Ok(1)
            }
        };
    }
    if write {
        write_document(&root, &document)?;
        println!("integration inventory: wrote {}", document.display());
        return Ok(0);
    }

    let inventory = inventory(&root)?;
    println!("{BLOCK_BEGIN}{}{BLOCK_END}", render_block(&inventory));
    Ok(0)
}

fn absolute(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|current| current.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    }
}

fn value<'a>(args: &'a [String], index: usize, flag: &str) -> anyhow::Result<&'a str> {
    args.get(index)
        .map(String::as_str)
        .ok_or_else(|| anyhow::anyhow!("{flag} needs a value"))
}
