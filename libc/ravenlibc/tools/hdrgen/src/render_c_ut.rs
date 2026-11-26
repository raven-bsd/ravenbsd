// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the C header renderer.

use super::*;
use crate::validate;

fn tables() -> (Tables, validate::Values) {
    let abi = std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../.."))
        .join("libc/ravenlibc/abi");
    let tables = validate::load(&abi).expect("tables");
    let values = validate::validate(&tables).expect("values");
    (tables, values)
}

fn files() -> Option<std::collections::BTreeMap<String, String>> {
    let resources = resources_or_skip()?;
    let (tables, values) = tables();
    let lock = crate::provenance::read_lock(&resources).expect("lock");
    Some(
        render_all(
            &tables,
            &values,
            &crate::attribution::Attributions::resolve(&tables, &lock).expect("attribution"),
        )
        .expect("render"),
    )
}

fn resources_or_skip() -> Option<std::path::PathBuf> {
    if let Some(value) = std::env::var_os("RAVENBSD_RESOURCES") {
        let path = std::path::PathBuf::from(value);
        assert!(
            path.join("donors.lock").is_file(),
            "RAVENBSD_RESOURCES has no donors.lock: {}",
            path.display()
        );
        return Some(path);
    }
    let here = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    match crate::provenance::discover_resources(&here) {
        Some(path) => Some(path),
        None => {
            eprintln!(
                "SKIP: ravenbsd-resources/donors.lock is absent; \
                 set RAVENBSD_RESOURCES to a donor checkout to run this row"
            );
            None
        }
    }
}

#[test]
fn renders_fourteen_headers() {
    let Some(files) = files() else {
        return;
    };
    assert_eq!(files.len(), 14);
    for header in [
        "bits/errno.h",
        "bits/signum.h",
        "bits/sysconf.h",
        "bits/abi_fcntl.h",
        "bits/abi_mman.h",
        "bits/abi_stat.h",
        "bits/abi_socket.h",
        "bits/abi_netinet.h",
        "bits/abi_poll.h",
        "bits/abi_time.h",
        "bits/abi_resource.h",
        "bits/abi_limits.h",
        "bits/abi_pthread.h",
        "bits/abi_misc.h",
    ] {
        assert!(files.contains_key(header), "{header}");
    }
}

#[test]
fn headers_have_a_notice_and_guard() {
    let Some(files) = files() else {
        return;
    };
    let freebsd = [
        "bits/errno.h",
        "bits/abi_fcntl.h",
        "bits/abi_mman.h",
        "bits/abi_poll.h",
        "bits/abi_time.h",
        "bits/abi_resource.h",
        "bits/sysconf.h",
    ];
    for (name, text) in files {
        if freebsd.contains(&name.as_str()) {
            assert!(
                text.starts_with("/*\n * SPDX-License-Identifier: BSD-3-Clause"),
                "{name}"
            );
            assert!(text.contains("derived from freebsd:"), "{name}");
        } else {
            assert!(
                text.starts_with("/*\n * SPDX-License-Identifier: MIT"),
                "{name}"
            );
            assert!(
                text.contains("Copyright © 2005-2020 Rich Felker, et al."),
                "{name}"
            );
        }
        assert!(text.contains("#ifndef "), "{name}");
        assert!(text.contains("#endif /* "), "{name}");
    }
}

#[test]
fn errno_header_has_the_values() {
    let Some(files) = files() else {
        return;
    };
    let text = &files["bits/errno.h"];
    assert!(text.contains("#define EPERM 1\n"));
    assert!(text.contains("#define EWOULDBLOCK 35\n"));
    assert!(text.contains("#define ENOTSUP 45\n"));
    assert!(text.contains("#define ENOATTR 87\n"));
    assert!(!text.contains("#define ENOATTR ENODATA"));
    assert!(!text.contains("#define ENOATTR 61"));
}

#[test]
fn fcntl_header_keeps_octal_and_expressions() {
    let Some(files) = files() else {
        return;
    };
    let text = &files["bits/abi_fcntl.h"];
    assert!(text.contains("#define O_CREAT 0x200\n"));
    assert!(text.contains("#define O_ACCMODE 03\n"));
    assert!(text.contains("#define O_LARGEFILE 0\n"));
    assert!(text.contains("#define AT_FDCWD (-100)\n"));
    assert!(text.contains("#define O_SEARCH O_EXEC\n"));
}

#[test]
fn map_32bit_has_the_freebsd_value() {
    let Some(files) = files() else {
        return;
    };
    let text = &files["bits/abi_mman.h"];
    assert!(text.contains("#define MAP_32BIT 0x80000\n"));
    assert!(!text.contains("#if defined(__x86_64__)\n"));
}

#[test]
fn visibility_entries_are_guarded() {
    let Some(files) = files() else {
        return;
    };
    let text = &files["bits/abi_fcntl.h"];
    assert!(text.contains("#if __BSD_VISIBLE\n"));
    assert!(text.contains("#define O_EXLOCK 0x20\n"));
    assert!(!text.contains("/* visible: bsd */\n/* visible: bsd */"));
}

#[test]
fn signal_header_has_the_frozen_stack_sizes() {
    let Some(files) = files() else {
        return;
    };
    let text = &files["bits/signum.h"];
    assert!(text.contains("#define MINSIGSTKSZ 6144\n"));
    assert!(text.contains("#define SIGSTKSZ 16384\n"));
    assert!(text.contains("#define SIGRTMIN 35\n"));
    assert!(text.contains("#define NSIG _NSIG\n"));
}

#[test]
fn rendering_is_deterministic() {
    let Some(first) = files() else {
        return;
    };
    let Some(second) = files() else {
        return;
    };
    assert_eq!(first, second);
}

#[test]
fn every_header_ends_with_a_newline() {
    let Some(files) = files() else {
        return;
    };
    for (name, text) in files {
        assert!(text.ends_with('\n'), "{name}");
    }
}
