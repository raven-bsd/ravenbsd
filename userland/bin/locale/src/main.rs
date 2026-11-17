// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `locale` command-line wrapper.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(not(target_os = "none"))]
fn main() {
    std::process::exit(run());
}

#[cfg(not(target_os = "none"))]
fn run() -> i32 {
    use std::collections::BTreeMap;
    use std::os::unix::ffi::OsStrExt;

    use locale::{CATEGORIES, CHARMAPS, KEYWORDS, LOCALES, list, query, render, resolve};
    use raven_libc_locale::EnvSource;

    struct HostEnv {
        values: BTreeMap<Vec<u8>, Vec<u8>>,
    }

    impl HostEnv {
        fn capture() -> Self {
            let mut values = BTreeMap::new();
            for (name, value) in std::env::vars_os() {
                values.insert(
                    name.as_os_str().as_bytes().to_vec(),
                    value.as_os_str().as_bytes().to_vec(),
                );
            }
            Self { values }
        }
    }

    impl EnvSource for HostEnv {
        fn get(&self, name: &[u8]) -> Option<&[u8]> {
            self.values.get(name).map(Vec::as_slice)
        }
    }

    fn usage() -> i32 {
        eprintln!("usage: locale [-a | -m | -k] [category...]");
        2
    }

    let mut list_locales = false;
    let mut list_charmaps = false;
    let mut list_keywords = false;
    let mut names = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-a" => list_locales = true,
            "-m" => list_charmaps = true,
            "-k" => list_keywords = true,
            "--" => {
                names.extend(args);
                break;
            }
            value if value.starts_with('-') => {
                eprintln!("locale: unsupported option {value}");
                return 2;
            }
            value => names.push(value.to_string()),
        }
    }
    let modes = usize::from(list_locales) + usize::from(list_charmaps) + usize::from(list_keywords);
    if modes > 1 || (modes == 1 && !names.is_empty()) {
        return usage();
    }
    if list_locales {
        for value in list(&LOCALES) {
            println!("{value}");
        }
        return 0;
    }
    if list_charmaps {
        for value in list(&CHARMAPS) {
            println!("{value}");
        }
        return 0;
    }
    if list_keywords {
        for value in list(&KEYWORDS) {
            println!("{value}");
        }
        return 0;
    }

    let env = HostEnv::capture();
    if names.is_empty() {
        print!("{}", render(&env));
        return 0;
    }
    let mut state = resolve(&env);
    for name in &names {
        if !CATEGORIES
            .iter()
            .any(|category| category.name == name.as_str())
            && name != "LANG"
            && name != "LC_ALL"
        {
            eprintln!("locale: unknown locale category `{name}`");
            return 1;
        }
        match query(&mut state, &env, name) {
            Ok(line) => println!("{line}"),
            Err(error) => {
                eprintln!("locale: {error}");
                return 1;
            }
        }
    }
    0
}

#[cfg(target_os = "none")]
mod program {
    use raven_rt::entry;

    entry!(main);

    fn main(_args: raven_rt::Args) -> i32 {
        raven_rt::eprintln!("locale: target build is host-only");
        1
    }
}
