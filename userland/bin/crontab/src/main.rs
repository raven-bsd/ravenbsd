// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `crontab` command-line wrapper.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(not(target_os = "none"))]
fn main() {
    use std::io::Read;

    struct HostSpool;

    impl crontab::Spool for HostSpool {
        fn read(&mut self, path: &str) -> Result<String, String> {
            std::fs::read_to_string(path).map_err(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    "no crontab for this user".to_string()
                } else {
                    error.to_string()
                }
            })
        }

        fn write_atomic(&mut self, path: &str, text: &str) -> Result<(), String> {
            let temporary = format!("{path}.tmp.{}", std::process::id());
            std::fs::write(&temporary, text).map_err(|error| error.to_string())?;
            std::fs::rename(&temporary, path).map_err(|error| {
                let _ = std::fs::remove_file(&temporary);
                error.to_string()
            })
        }

        fn remove(&mut self, path: &str) -> Result<(), String> {
            std::fs::remove_file(path).map_err(|error| error.to_string())
        }

        fn exists(&mut self, path: &str) -> bool {
            std::path::Path::new(path).exists()
        }
    }

    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let action = match crontab::action(&args) {
        Ok(action) => action,
        Err(error) => {
            eprintln!("crontab: {error}");
            std::process::exit(2);
        }
    };
    let user = std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .unwrap_or_else(|_| "root".to_string());
    let path = crontab::spool_path(&user);
    let mut spool = HostSpool;
    let result = match action {
        crontab::Action::List => crontab::list(&mut spool, &path).map(|text| print!("{text}")),
        crontab::Action::Remove => crontab::remove(&mut spool, &path),
        crontab::Action::Install(file) => {
            if let Some(parent) = std::path::Path::new(&path).parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let text = if file == "-" {
                let mut text = String::new();
                match std::io::stdin().read_to_string(&mut text) {
                    Ok(_) => text,
                    Err(error) => {
                        eprintln!("crontab: stdin: {error}");
                        std::process::exit(1);
                    }
                }
            } else {
                match std::fs::read_to_string(&file) {
                    Ok(text) => text,
                    Err(error) => {
                        eprintln!("crontab: {file}: {error}");
                        std::process::exit(1);
                    }
                }
            };
            crontab::install(&mut spool, &path, &text)
        }
    };
    if let Err(error) = result {
        eprintln!("crontab: {error}");
        std::process::exit(1);
    }
}

#[cfg(target_os = "none")]
mod program {
    use alloc::format;
    use alloc::string::{String, ToString};
    use alloc::vec::Vec;

    use raven_abi::errno::Errno;
    use raven_rt::entry;
    use raven_rt::fs;

    entry!(main);

    struct TargetSpool;

    impl crontab::Spool for TargetSpool {
        fn read(&mut self, path: &str) -> Result<String, String> {
            let data = fs::read_to_vec(&scheme(path)).map_err(|error| {
                if error == Errno::ENOENT {
                    "no crontab for this user".to_string()
                } else {
                    error.description().to_string()
                }
            })?;
            String::from_utf8(data).map_err(|_| "crontab is not UTF-8".to_string())
        }

        fn write_atomic(&mut self, path: &str, text: &str) -> Result<(), String> {
            let temporary = format!("{path}.tmp");
            let mut file = fs::open(
                &scheme(&temporary),
                raven_proto_fs::O_WRONLY | raven_proto_fs::O_CREAT | raven_proto_fs::O_TRUNC,
            )
            .map_err(|error| error.description().to_string())?;
            file.write(text.as_bytes())
                .map_err(|error| error.description().to_string())?;
            file.close()
                .map_err(|error| error.description().to_string())?;
            fs::rename(&scheme(&temporary), &scheme(path))
                .map_err(|error| error.description().to_string())
        }

        fn remove(&mut self, path: &str) -> Result<(), String> {
            fs::unlink(&scheme(path)).map_err(|error| error.description().to_string())
        }

        fn exists(&mut self, path: &str) -> bool {
            fs::stat(&scheme(path)).is_ok()
        }
    }

    fn scheme(path: &str) -> String {
        if path.contains(':') {
            path.to_string()
        } else {
            format!("file:{}", fs::resolve(path))
        }
    }

    fn main(args: raven_rt::Args) -> i32 {
        let user = args
            .env_get("USER")
            .map(str::to_string)
            .or_else(|| {
                raven_rt::proc::credentials()
                    .ok()
                    .map(|credentials| credentials.euid.to_string())
            })
            .unwrap_or_else(|| "root".to_string());
        let args = args.iter().skip(1).map(str::to_string).collect::<Vec<_>>();
        let action = match crontab::action(&args) {
            Ok(action) => action,
            Err(error) => {
                raven_rt::eprintln!("crontab: {error}");
                return 2;
            }
        };
        let path = crontab::spool_path(&user);
        let mut spool = TargetSpool;
        let result = match action {
            crontab::Action::List => crontab::list(&mut spool, &path).map(|text| {
                raven_rt::print!("{text}");
            }),
            crontab::Action::Remove => crontab::remove(&mut spool, &path),
            crontab::Action::Install(file) => {
                if file == "-" {
                    Err("standard input is unsupported on target; pass a file".to_string())
                } else {
                    let directory = crontab::spool_dir();
                    let _ = fs::mkdir(&scheme(&directory), 0o700);
                    match fs::read_to_vec(&scheme(&file)) {
                        Ok(data) => match String::from_utf8(data) {
                            Ok(text) => crontab::install(&mut spool, &path, &text),
                            Err(_) => Err(format!("{file}: not UTF-8")),
                        },
                        Err(error) => Err(format!("{file}: {}", error.description())),
                    }
                }
            }
        };
        if let Err(error) = result {
            raven_rt::eprintln!("crontab: {error}");
            return 1;
        }
        0
    }
}
