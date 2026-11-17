// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `ar` utility.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::string::{String, ToString};
    use alloc::vec::Vec;

    use ar::{Member, Operation, Options, parse_archive, parse_args, write_archive};
    use raven_proto_fs::{O_CREAT, O_TRUNC, O_WRONLY};
    use raven_rt::entry;
    use raven_rt::fs;
    use raven_tool_core::Errno;
    use raven_tool_core::error::{EXIT_ERROR, EXIT_OK, EXIT_USAGE, line, usage};
    use raven_tool_core::path::with_file_scheme;

    entry!(main);

    fn main(args: raven_rt::Args) -> i32 {
        let options = match parse_args(args.iter().skip(1)) {
            Ok(options) => options,
            Err(_) => {
                raven_rt::eprintln!("{}", usage("ar", "[r|t|x|s][vc] archive [members ...]"));
                return EXIT_USAGE;
            }
        };
        match run(&options) {
            Ok(()) => EXIT_OK,
            Err((path, error)) => {
                raven_rt::eprintln!("{}", line("ar", &path, error));
                EXIT_ERROR
            }
        }
    }

    fn run(options: &Options) -> Result<(), (String, Errno)> {
        match options.operation {
            Operation::Replace => replace(options),
            Operation::List => list(options),
            Operation::Extract => extract(options),
            Operation::Symbols => rewrite_symbols(options),
        }
    }

    fn replace(options: &Options) -> Result<(), (String, Errno)> {
        let mut members = read_archive(options.archive, true)?;
        for path in &options.operands {
            let member = read_member(path)?;
            if let Some(existing) = members.iter_mut().find(|item| item.name == member.name) {
                *existing = member;
            } else {
                members.push(member);
            }
        }
        write_file(options.archive, &write_archive(&members, options.symbols))
    }

    fn list(options: &Options) -> Result<(), (String, Errno)> {
        let members = read_archive(options.archive, false)?;
        for member in members {
            if !selected(&member.name, &options.operands) {
                continue;
            }
            if options.verbose {
                raven_rt::println!("{:o} {} {}", member.mode, member.data.len(), member.name);
            } else {
                raven_rt::println!("{}", member.name);
            }
        }
        Ok(())
    }

    fn extract(options: &Options) -> Result<(), (String, Errno)> {
        let members = read_archive(options.archive, false)?;
        for member in members {
            if !selected(&member.name, &options.operands) {
                continue;
            }
            let name = if safe_name(&member.name) {
                member.name.clone()
            } else {
                return Err(("archive".to_string(), Errno::EINVAL));
            };
            write_file(&name, &member.data)?;
            let path = with_file_scheme(&fs::resolve(&name));
            fs::chmod(&path, member.mode & 0o7777).map_err(|error| (name.clone(), error))?;
            if options.verbose {
                raven_rt::println!("{name}");
            }
        }
        Ok(())
    }

    fn rewrite_symbols(options: &Options) -> Result<(), (String, Errno)> {
        let members = read_archive(options.archive, false)?;
        write_file(options.archive, &write_archive(&members, true))
    }

    fn read_archive(path: &str, allow_missing: bool) -> Result<Vec<Member>, (String, Errno)> {
        let resolved = with_file_scheme(&fs::resolve(path));
        match fs::read_to_vec(&resolved) {
            Ok(bytes) => parse_archive(&bytes).map_err(|_| (path.to_string(), Errno::EINVAL)),
            Err(Errno::ENOENT) if allow_missing => Ok(Vec::new()),
            Err(error) => Err((path.to_string(), error)),
        }
    }

    fn read_member(path: &str) -> Result<Member, (String, Errno)> {
        let name = path
            .rsplit('/')
            .next()
            .filter(|name| !name.is_empty())
            .ok_or((path.to_string(), Errno::EINVAL))?;
        let resolved = fs::resolve(path);
        let stat =
            fs::stat(&with_file_scheme(&resolved)).map_err(|error| (path.to_string(), error))?;
        let data = fs::read_to_vec(&with_file_scheme(&resolved))
            .map_err(|error| (path.to_string(), error))?;
        Ok(Member {
            name: name.to_string(),
            mtime: stat.mtime,
            uid: stat.uid,
            gid: stat.gid,
            mode: stat.mode & 0o7777,
            data,
        })
    }

    fn write_file(path: &str, data: &[u8]) -> Result<(), (String, Errno)> {
        let resolved = with_file_scheme(&fs::resolve(path));
        let mut file = fs::open(&resolved, O_WRONLY | O_CREAT | O_TRUNC)
            .map_err(|error| (path.to_string(), error))?;
        let mut offset = 0usize;
        while offset < data.len() {
            let written = file
                .write(data.get(offset..).unwrap_or(&[]))
                .map_err(|error| (path.to_string(), error))?;
            if written == 0 {
                return Err((path.to_string(), Errno::EIO));
            }
            offset = offset.saturating_add(written);
        }
        file.close().map_err(|error| (path.to_string(), error))
    }

    fn selected(name: &str, filters: &[&str]) -> bool {
        filters.is_empty() || filters.contains(&name)
    }

    fn safe_name(name: &str) -> bool {
        !name.is_empty()
            && name != "."
            && name != ".."
            && !name.contains('/')
            && !name.contains('\\')
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
