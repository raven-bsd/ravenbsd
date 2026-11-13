// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! T7: hygiene, licence notices, identity macros and bits/ closure.

use std::path::{Path, PathBuf};

use raven_libc_hdrcheck::clang;
use raven_libc_hdrcheck::repo_root;

const FORBIDDEN_IDENTITY: &[&str] = &[
    "__linux__",
    "__GLIBC__",
    "__GNU_LIBRARY__",
    "__FreeBSD__",
    "__NetBSD__",
    "__OpenBSD__",
    "__sun",
];

const GENERATED_COPIES: &[&str] = &[
    "bits/errno.h",
    "bits/signum.h",
    "bits/sysconf.h",
    "bits/abi_fcntl.h",
    "bits/abi_limits.h",
    "bits/abi_misc.h",
    "bits/abi_mman.h",
    "bits/abi_netinet.h",
    "bits/abi_poll.h",
    "bits/abi_pthread.h",
    "bits/abi_resource.h",
    "bits/abi_socket.h",
    "bits/abi_stat.h",
    "bits/abi_time.h",
    "bits/syscall.h",
];

fn headers() -> Vec<PathBuf> {
    let root = repo_root().join("libc/ravenlibc");
    let mut files = Vec::new();
    for directory in [
        root.join("include"),
        root.join("arch/x86_64/include"),
        root.join("arch/aarch64/include"),
        root.join("arch/riscv64/include"),
    ] {
        collect(&directory, &mut files);
    }
    files.sort();
    files
}

fn collect(directory: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "h") {
            files.push(path);
        }
    }
}

fn install_relative(path: &Path) -> String {
    raven_libc_hdrcheck::install_name(path)
}

#[test]
fn every_header_has_spdx_and_a_guard() {
    let mut failures = 0;
    for path in headers() {
        let text = std::fs::read_to_string(&path).expect("read header");
        let name = install_relative(&path);
        let first_five: Vec<&str> = text.lines().take(5).collect();
        if !first_five
            .iter()
            .any(|line| line.contains("SPDX-License-Identifier:"))
        {
            failures += 1;
            eprintln!("{}: no SPDX line in the first five lines", path.display());
        }
        if text.contains("#pragma once") {
            failures += 1;
            eprintln!("{}: #pragma once", path.display());
        }
        if !name.starts_with("bits/") {
            let has_guard = text
                .lines()
                .take(80)
                .any(|line| line.trim_start().starts_with("#ifndef"));
            if !has_guard {
                failures += 1;
                eprintln!("{}: missing include guard", path.display());
            }
        }
        if text.contains('\r') {
            failures += 1;
            eprintln!("{}: CRLF", path.display());
        }
        for (line_no, line) in text.lines().enumerate() {
            if line.ends_with(' ') || line.ends_with('\t') {
                failures += 1;
                eprintln!("{}:{}: trailing whitespace", path.display(), line_no + 1);
                break;
            }
        }
        for macro_name in FORBIDDEN_IDENTITY {
            if text.contains(macro_name) {
                failures += 1;
                eprintln!("{}: forbidden identity macro {macro_name}", path.display());
            }
        }
        if std::fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.file_type().is_symlink())
        {
            failures += 1;
            eprintln!("{}: symlink", path.display());
        }
    }
    assert_eq!(failures, 0, "T7 hygiene failures: {failures}");
}

#[test]
fn musl_notice_is_present() {
    let mut failures = 0;
    for path in headers() {
        let text = std::fs::read_to_string(&path).expect("read header");
        if !text.contains("RavenBSD: derived from musl:") {
            continue;
        }
        let generated = GENERATED_COPIES.contains(&install_relative(&path).as_str());
        let copyright = if generated {
            text.contains("Copyright © 2005-2020 Rich Felker, et al.")
                || text.contains("Copyright (c) 2005-2020 Rich Felker, et al.")
        } else {
            text.contains("Copyright (c) 2005-2020 Rich Felker, et al.")
        };
        if !copyright {
            failures += 1;
            eprintln!("{}: missing Rich Felker copyright line", path.display());
        }
        if !text.contains("Permission is hereby granted") {
            failures += 1;
            eprintln!("{}: missing MIT permission text", path.display());
        }
    }
    assert_eq!(failures, 0, "T7 licence failures: {failures}");
}

#[test]
fn every_include_resolves_on_every_arch() {
    let toolchain = clang::locate_toolchain().expect("clang");
    let root = repo_root().join("libc/ravenlibc");
    let manifest_rows = raven_libc_hdrcheck::manifest::load().expect("manifest");
    let mut failures = 0;
    for arch in ["x86_64", "aarch64", "riscv64"] {
        let shared = root.join("include");
        let per_arch = root.join("arch").join(arch).join("include");
        for path in headers() {
            let name = install_relative(&path);
            let pending = raven_libc_hdrcheck::manifest::find(&manifest_rows, &name)
                .is_some_and(|row| row.status == raven_libc_hdrcheck::manifest::Status::Pending);
            if pending {
                println!(
                    "PENDING include closure: {name} (owner {})",
                    raven_libc_hdrcheck::manifest::find(&manifest_rows, &name)
                        .map_or("?", |row| row.owner.as_str())
                );
                continue;
            }
            let text = std::fs::read_to_string(&path).expect("read");
            for line in text.lines() {
                let trimmed = line.trim_start();
                let Some(rest) = trimmed.strip_prefix("#include <") else {
                    continue;
                };
                let Some(end) = rest.find('>') else {
                    continue;
                };
                let include = &rest[..end];
                let resolved = per_arch.join(include).is_file()
                    || shared.join(include).is_file()
                    || toolchain.resource_include.join(include).is_file();
                if !resolved {
                    failures += 1;
                    eprintln!(
                        "{} ({arch}): unresolved #include <{}>",
                        path.display(),
                        include
                    );
                }
            }
        }
    }
    assert_eq!(failures, 0, "T7 include closure failures: {failures}");
}

#[test]
fn no_file_lives_in_both_trees() {
    let root = repo_root().join("libc/ravenlibc");
    let mut shared = std::collections::BTreeSet::new();
    for path in headers() {
        let text = path.to_string_lossy();
        if text.contains("/include/") && !text.contains("/arch/") {
            shared.insert(install_relative(&path));
        }
    }
    for arch in ["x86_64", "aarch64", "riscv64"] {
        let directory = root.join("arch").join(arch).join("include");
        let mut files = Vec::new();
        collect(&directory, &mut files);
        for path in files {
            let name = install_relative(&path);
            assert!(
                !shared.contains(&name),
                "{name} exists in both include/ and arch/{arch}/include/"
            );
        }
    }
}
