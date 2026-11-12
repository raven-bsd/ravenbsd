// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `ls` utility.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::string::String;
    use alloc::vec;
    use alloc::vec::Vec;

    use raven_proto_fs::DT_DIR;
    use raven_rt::entry;
    use raven_rt::fs;
    use raven_tool_core::error::{EXIT_ERROR, EXIT_OK, EXIT_USAGE, line, usage};
    use raven_tool_core::path::{basename, join, sort_names, visible_names, with_file_scheme};

    entry!(main);

    fn main(args: raven_rt::Args) -> i32 {
        let parsed = match raven_ls::parse(args.iter().skip(1)) {
            Ok(parsed) => parsed,
            Err(_) => {
                raven_rt::eprintln!("{}", usage("ls", "[-a] [-l] [path ...]"));
                return EXIT_USAGE;
            }
        };
        let operands = if parsed.operands.is_empty() {
            vec!["."]
        } else {
            parsed.operands
        };
        let mut status = EXIT_OK;
        for path in operands {
            if let Err(error) = list(path, parsed.flags.a, parsed.flags.l) {
                raven_rt::eprintln!("{}", line("ls", path, error));
                status = EXIT_ERROR;
            }
        }
        status
    }

    fn list(path: &str, all: bool, long: bool) -> Result<(), raven_tool_core::Errno> {
        let resolved = fs::resolve(path);
        let full = with_file_scheme(&resolved);
        let stat = fs::stat(&full)?;
        if stat.mode & raven_proto_fs::S_IFMT != raven_proto_fs::S_IFDIR {
            print_entry(basename(path), false, long, stat.mode, stat.size);
            return Ok(());
        }
        let entries = fs::read_dir(&full)?;
        let mut names: Vec<String> = entries.iter().map(|entry| entry.name.clone()).collect();
        names = visible_names(names, all);
        sort_names(&mut names);
        let by_name: Vec<&raven_rt::fs::DirEntry> = entries.iter().collect();
        for name in names {
            let Some(entry) = by_name.iter().find(|entry| entry.name == name) else {
                continue;
            };
            if long {
                let child = join(&resolved, &name)?;
                let child_stat = fs::stat(&with_file_scheme(&child))?;
                print_entry(&name, false, true, child_stat.mode, child_stat.size);
            } else {
                print_entry(&name, entry.kind == DT_DIR, false, 0, 0);
            }
        }
        Ok(())
    }

    fn print_entry(name: &str, is_dir: bool, long: bool, mode: u32, size: u64) {
        if long {
            raven_rt::println!("{}", raven_ls::format_long(name, mode, size));
        } else {
            raven_rt::println!("{}", raven_ls::format_short(name, is_dir));
        }
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
