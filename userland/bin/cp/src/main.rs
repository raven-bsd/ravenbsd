// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `cp` utility.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
mod program {
    use raven_proto_fs::{O_CREAT, O_RDONLY, O_TRUNC, O_WRONLY, S_IFDIR, S_IFMT};
    use raven_rt::entry;
    use raven_rt::fs;
    use raven_tool_core::Errno;
    use raven_tool_core::error::{EXIT_ERROR, EXIT_OK, EXIT_USAGE, line, usage};
    use raven_tool_core::path::{join, with_file_scheme};

    entry!(main);

    fn main(args: raven_rt::Args) -> i32 {
        let parsed = match raven_cp::parse(args.iter().skip(1)) {
            Ok(parsed) => parsed,
            Err(_) => {
                raven_rt::eprintln!("{}", usage("cp", "[-r] source target"));
                return EXIT_USAGE;
            }
        };
        let Some((source, target)) = raven_cp::pair(&parsed.operands) else {
            raven_rt::eprintln!("{}", usage("cp", "[-r] source target"));
            return EXIT_USAGE;
        };
        let target_is_dir = fs::stat(&with_file_scheme(&fs::resolve(target)))
            .map(|stat| stat.mode & S_IFMT == S_IFDIR)
            .unwrap_or(false);
        let destination = raven_cp::destination(source, target, target_is_dir);
        match copy(source, &destination, parsed.flags.r) {
            Ok(()) => EXIT_OK,
            Err(error) => {
                raven_rt::eprintln!("{}", line("cp", source, error));
                EXIT_ERROR
            }
        }
    }

    fn copy(source: &str, target: &str, recursive: bool) -> Result<(), Errno> {
        let source = fs::resolve(source);
        let target = fs::resolve(target);
        let stat = fs::stat(&with_file_scheme(&source))?;
        if stat.mode & S_IFMT == S_IFDIR {
            if !recursive {
                return Err(Errno::EISDIR);
            }
            match fs::mkdir(&target, stat.mode & 0o7777) {
                Ok(()) | Err(Errno::EEXIST) => {}
                Err(error) => return Err(error),
            }
            for entry in fs::read_dir(&with_file_scheme(&source))? {
                let from = join(&source, &entry.name)?;
                let to = join(&target, &entry.name)?;
                copy(&from, &to, true)?;
            }
            let _ = fs::chmod(&target, stat.mode & 0o7777);
        } else {
            copy_file(&source, &target)?;
            let _ = fs::chmod(&target, stat.mode & 0o7777);
        }
        Ok(())
    }

    fn copy_file(source: &str, target: &str) -> Result<(), Errno> {
        let mut input = fs::open(&with_file_scheme(source), O_RDONLY)?;
        let mut output = fs::open(&with_file_scheme(target), O_WRONLY | O_CREAT | O_TRUNC)?;
        let mut buffer = [0u8; 4096];
        loop {
            let read = input.read(&mut buffer)?;
            if read == 0 {
                return output.close();
            }
            let mut written = 0usize;
            while written < read {
                let count = output.write(buffer.get(written..read).unwrap_or(&[]))?;
                if count == 0 {
                    return Err(Errno::EIO);
                }
                written = written.saturating_add(count);
            }
        }
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
