// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `tar` utility (ustar subset).

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::format;
    use alloc::string::{String, ToString};
    use alloc::vec::Vec;

    use raven_proto_fs::{O_CREAT, O_TRUNC, O_WRONLY, S_IFDIR, S_IFMT, S_IFREG};
    use raven_rt::entry;
    use raven_rt::fs;
    use raven_tool_core::Errno;
    use raven_tool_core::error::{EXIT_ERROR, EXIT_OK, EXIT_USAGE, line, usage};
    use raven_tool_core::path::with_file_scheme;
    use tar::{Entry, Operation, Options, encode_archive, parse_archive, parse_args, safe_path};

    entry!(main);

    fn main(args: raven_rt::Args) -> i32 {
        let options = match parse_args(args.iter().skip(1).map(ToString::to_string)) {
            Ok(options) => options,
            Err(_) => {
                raven_rt::eprintln!("{}", usage("tar", "-c|-t|-x [-v] -f archive [paths ...]"));
                return EXIT_USAGE;
            }
        };
        match run(&options) {
            Ok(()) => EXIT_OK,
            Err((path, error)) => {
                raven_rt::eprintln!("{}", line("tar", &path, error));
                EXIT_ERROR
            }
        }
    }

    fn run(options: &Options) -> Result<(), (String, Errno)> {
        match options.operation {
            Operation::Create => create(options),
            Operation::List => list(options),
            Operation::Extract => extract(options),
        }
    }

    fn create(options: &Options) -> Result<(), (String, Errno)> {
        let mut entries = Vec::new();
        for path in &options.operands {
            add_path(&mut entries, path)?;
        }
        let archive =
            encode_archive(&entries).map_err(|_| (options.archive.clone(), Errno::EINVAL))?;
        write_file(&options.archive, &archive)
    }

    fn add_path(entries: &mut Vec<Entry>, path: &str) -> Result<(), (String, Errno)> {
        let resolved = fs::resolve(path);
        if let Ok(target) = fs::readlink(&resolved) {
            entries.push(Entry {
                name: path.to_string(),
                mode: 0o777,
                uid: 0,
                gid: 0,
                mtime: 0,
                kind: b'2',
                linkname: target,
                data: Vec::new(),
            });
            return Ok(());
        }
        let stat =
            fs::stat(&with_file_scheme(&resolved)).map_err(|error| (path.to_string(), error))?;
        if stat.mode & S_IFMT == S_IFDIR {
            let mut name = path.to_string();
            if !name.ends_with('/') {
                name.push('/');
            }
            entries.push(Entry {
                name,
                mode: stat.mode & 0o7777,
                uid: 0,
                gid: 0,
                mtime: stat.mtime,
                kind: b'5',
                linkname: String::new(),
                data: Vec::new(),
            });
            let mut children: Vec<String> = fs::read_dir(&with_file_scheme(&resolved))
                .map_err(|error| (path.to_string(), error))?
                .into_iter()
                .map(|entry| entry.name)
                .filter(|name| name != "." && name != "..")
                .collect();
            children.sort();
            for child in children {
                let child_path = format!("{}/{}", path.trim_end_matches('/'), child);
                add_path(entries, &child_path)?;
            }
            return Ok(());
        }
        if stat.mode & S_IFMT != S_IFREG {
            return Err((path.to_string(), Errno::EINVAL));
        }
        let data = fs::read_to_vec(&with_file_scheme(&resolved))
            .map_err(|error| (path.to_string(), error))?;
        entries.push(Entry {
            name: path.to_string(),
            mode: stat.mode & 0o7777,
            uid: 0,
            gid: 0,
            mtime: stat.mtime,
            kind: b'0',
            linkname: String::new(),
            data,
        });
        Ok(())
    }

    fn list(options: &Options) -> Result<(), (String, Errno)> {
        let entries = read_archive(&options.archive)?;
        for entry in entries {
            if !selected(&entry.name, &options.operands) {
                continue;
            }
            if options.verbose {
                raven_rt::println!("{:o} {} {}", entry.mode, entry.data.len(), entry.name);
            } else {
                raven_rt::println!("{}", entry.name);
            }
        }
        Ok(())
    }

    fn extract(options: &Options) -> Result<(), (String, Errno)> {
        let entries = read_archive(&options.archive)?;
        for entry in entries {
            if !selected(&entry.name, &options.operands) {
                continue;
            }
            if !safe_path(&entry.name) {
                return Err((entry.name, Errno::EINVAL));
            }
            match entry.kind {
                b'5' => {
                    let target = with_file_scheme(&fs::resolve(&entry.name));
                    match fs::mkdir(&target, entry.mode & 0o7777) {
                        Ok(()) | Err(Errno::EEXIST) => {}
                        Err(error) => return Err((entry.name, error)),
                    }
                    let _ = fs::chmod(&target, entry.mode & 0o7777);
                }
                b'2' => {
                    let target = with_file_scheme(&fs::resolve(&entry.name));
                    fs::symlink(&target, &entry.linkname)
                        .map_err(|error| (entry.name.clone(), error))?;
                }
                _ => {
                    write_file(&entry.name, &entry.data)?;
                    let target = with_file_scheme(&fs::resolve(&entry.name));
                    let _ = fs::chmod(&target, entry.mode & 0o7777);
                }
            }
            if options.verbose {
                raven_rt::println!("{}", entry.name);
            }
        }
        Ok(())
    }

    fn read_archive(path: &str) -> Result<Vec<Entry>, (String, Errno)> {
        let resolved = with_file_scheme(&fs::resolve(path));
        let bytes = fs::read_to_vec(&resolved).map_err(|error| (path.to_string(), error))?;
        parse_archive(&bytes).map_err(|_| (path.to_string(), Errno::EINVAL))
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

    fn selected(name: &str, filters: &[String]) -> bool {
        filters.is_empty()
            || filters.iter().any(|filter| {
                filter == name || filter.trim_end_matches('/') == name.trim_end_matches('/')
            })
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
