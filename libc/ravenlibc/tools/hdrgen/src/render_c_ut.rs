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

fn files() -> std::collections::BTreeMap<String, String> {
    let (tables, values) = tables();
    render_all(&tables, &values, "7813e9119eb5fe681b6750b8a033a0490bca0941").expect("render")
}

#[test]
fn renders_fourteen_headers() {
    let files = files();
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
    for (name, text) in files() {
        assert!(
            text.starts_with("/*\n * SPDX-License-Identifier: MIT"),
            "{name}"
        );
        assert!(
            text.contains("Copyright © 2005-2020 Rich Felker, et al."),
            "{name}"
        );
        assert!(text.contains("#ifndef "), "{name}");
        assert!(text.contains("#endif /* "), "{name}");
    }
}

#[test]
fn errno_header_has_the_values() {
    let text = &files()["bits/errno.h"];
    assert!(text.contains("#define EPERM 1\n"));
    assert!(text.contains("#define EWOULDBLOCK EAGAIN\n"));
    assert!(text.contains("#define ENOATTR ENODATA\n"));
    assert!(!text.contains("#define ENOATTR 61"));
}

#[test]
fn fcntl_header_keeps_octal_and_expressions() {
    let text = &files()["bits/abi_fcntl.h"];
    assert!(text.contains("#define O_CREAT 0100\n"));
    assert!(text.contains("#define O_ACCMODE (03|O_PATH)\n"));
    assert!(text.contains("#define O_LARGEFILE 0\n"));
    assert!(text.contains("#define AT_FDCWD (-100)\n"));
    assert!(text.contains("#define O_SEARCH O_PATH\n"));
}

#[test]
fn arch_entries_are_guarded() {
    let text = &files()["bits/abi_mman.h"];
    assert!(text.contains("#if defined(__x86_64__)\n"));
    assert!(text.contains("#define MAP_32BIT 0x40\n"));
}

#[test]
fn visibility_entries_are_guarded() {
    let text = &files()["bits/abi_fcntl.h"];
    assert!(text.contains("#if __BSD_VISIBLE\n"));
    assert!(text.contains("#define O_EXLOCK 0x10000000\n"));
    assert!(!text.contains("/* visible: bsd */\n/* visible: bsd */"));
}

#[test]
fn signal_header_has_the_frozen_stack_sizes() {
    let text = &files()["bits/signum.h"];
    assert!(text.contains("#define MINSIGSTKSZ 6144\n"));
    assert!(text.contains("#define SIGSTKSZ 16384\n"));
    assert!(text.contains("#define SIGRTMIN 35\n"));
    assert!(text.contains("#define NSIG _NSIG\n"));
}

#[test]
fn rendering_is_deterministic() {
    assert_eq!(files(), files());
}

#[test]
fn every_header_ends_with_a_newline() {
    for (name, text) in files() {
        assert!(text.ends_with('\n'), "{name}");
    }
}
