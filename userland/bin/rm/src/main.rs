// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `rm` utility.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
mod program {
    use raven_proto_fs::{S_IFDIR, S_IFMT};
    use raven_rm::Action;
    use raven_rt::entry;
    use raven_rt::fs;
    use raven_tool_core::Errno;
    use raven_tool_core::error::{EXIT_ERROR, EXIT_OK, EXIT_USAGE, line, usage};
    use raven_tool_core::path::{join, with_file_scheme};

    entry!(main);

    fn main(args: raven_rt::Args) -> i32 {
        let parsed = match raven_rm::parse(args.iter().skip(1)) {
            Ok(parsed) => parsed,
            Err(_) => {
                raven_rt::eprintln!("{}", usage("rm", "[-r] [-f] path ..."));
                return EXIT_USAGE;
            }
        };
        if parsed.operands.is_empty() {
            raven_rt::eprintln!("{}", usage("rm", "[-r] [-f] path ..."));
            return EXIT_USAGE;
        }
        let mut status = EXIT_OK;
        for path in parsed.operands {
            if let Err(error) = remove(path, parsed.flags.r, parsed.flags.f) {
                raven_rt::eprintln!("{}", line("rm", path, error));
                status = EXIT_ERROR;
            }
        }
        status
    }

    fn remove(path: &str, recursive: bool, force: bool) -> Result<(), Errno> {
        let resolved = fs::resolve(path);
        let stat = match fs::stat(&with_file_scheme(&resolved)) {
            Ok(stat) => stat,
            Err(Errno::ENOENT) => return raven_rm::missing(force).map(|_| ()),
            Err(error) => return Err(error),
        };
        let action = raven_rm::action(stat.mode & S_IFMT == S_IFDIR, recursive)?;
        match action {
            Action::Unlink => fs::unlink(&resolved),
            Action::RemoveDir => {
                for entry in fs::read_dir(&with_file_scheme(&resolved))? {
                    let child = join(&resolved, &entry.name)?;
                    remove(&child, true, force)?;
                }
                fs::rmdir(&resolved)
            }
            Action::Ignore => Ok(()),
        }
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
