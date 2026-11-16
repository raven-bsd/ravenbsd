// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `find` utility.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::string::String;
    use alloc::vec::Vec;

    use raven_find::{Kind, Options, Tree, parse, walk};
    use raven_rt::entry;
    use raven_rt::fs;
    use raven_tool_core::Errno;
    use raven_tool_core::error::{EXIT_ERROR, EXIT_OK, EXIT_USAGE, usage};
    use raven_tool_core::path::with_file_scheme;

    struct FsTree;

    impl Tree for FsTree {
        fn kind(&mut self, path: &str) -> Result<Kind, Errno> {
            let resolved = fs::resolve(path);
            if fs::readlink(&resolved).is_ok() {
                return Ok(Kind::Symlink);
            }
            let stat = fs::stat(&with_file_scheme(&resolved))?;
            Ok(Kind::from_mode(stat.mode))
        }

        fn mode(&mut self, path: &str) -> Result<u32, Errno> {
            fs::stat(&with_file_scheme(&fs::resolve(path))).map(|stat| stat.mode)
        }

        fn children(&mut self, path: &str) -> Result<Vec<String>, Errno> {
            let entries = fs::read_dir(&with_file_scheme(&fs::resolve(path)))?;
            Ok(entries
                .into_iter()
                .map(|entry| entry.name)
                .filter(|name| name != "." && name != "..")
                .collect())
        }
    }

    entry!(main);

    fn main(args: raven_rt::Args) -> i32 {
        let options = match parse(args.iter().skip(1)) {
            Ok(options) => options,
            Err(_) => {
                raven_rt::eprintln!(
                    "{}",
                    usage(
                        "find",
                        "[path ...] [-name pattern] [-type f|d|l] [-perm mode] [-print]"
                    )
                );
                return EXIT_USAGE;
            }
        };
        let mut out = Vec::new();
        if let Err(error) = walk(&mut FsTree, &Options { ..options }, &mut out) {
            raven_rt::eprintln!("find: {}", error.description());
            return EXIT_ERROR;
        }
        for path in out {
            raven_rt::println!("{path}");
        }
        EXIT_OK
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
