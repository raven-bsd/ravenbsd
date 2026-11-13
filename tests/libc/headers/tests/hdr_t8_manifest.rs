// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! T8: manifest ownership, absence rules and byte-identical C36 copies.

use raven_libc_hdrcheck::clang;
use raven_libc_hdrcheck::{manifest, repo_root};

const GENERATED_COPIES: &[(&str, &str)] = &[
    (
        "libc/ravenlibc/include/bits/errno.h",
        "libc/ravenlibc/abi/generated/include/bits/errno.h",
    ),
    (
        "libc/ravenlibc/include/bits/signum.h",
        "libc/ravenlibc/abi/generated/include/bits/signum.h",
    ),
    (
        "libc/ravenlibc/include/bits/sysconf.h",
        "libc/ravenlibc/abi/generated/include/bits/sysconf.h",
    ),
    (
        "libc/ravenlibc/include/bits/abi_fcntl.h",
        "libc/ravenlibc/abi/generated/include/bits/abi_fcntl.h",
    ),
    (
        "libc/ravenlibc/include/bits/abi_limits.h",
        "libc/ravenlibc/abi/generated/include/bits/abi_limits.h",
    ),
    (
        "libc/ravenlibc/include/bits/abi_misc.h",
        "libc/ravenlibc/abi/generated/include/bits/abi_misc.h",
    ),
    (
        "libc/ravenlibc/include/bits/abi_mman.h",
        "libc/ravenlibc/abi/generated/include/bits/abi_mman.h",
    ),
    (
        "libc/ravenlibc/include/bits/abi_netinet.h",
        "libc/ravenlibc/abi/generated/include/bits/abi_netinet.h",
    ),
    (
        "libc/ravenlibc/include/bits/abi_poll.h",
        "libc/ravenlibc/abi/generated/include/bits/abi_poll.h",
    ),
    (
        "libc/ravenlibc/include/bits/abi_pthread.h",
        "libc/ravenlibc/abi/generated/include/bits/abi_pthread.h",
    ),
    (
        "libc/ravenlibc/include/bits/abi_resource.h",
        "libc/ravenlibc/abi/generated/include/bits/abi_resource.h",
    ),
    (
        "libc/ravenlibc/include/bits/abi_socket.h",
        "libc/ravenlibc/abi/generated/include/bits/abi_socket.h",
    ),
    (
        "libc/ravenlibc/include/bits/abi_stat.h",
        "libc/ravenlibc/abi/generated/include/bits/abi_stat.h",
    ),
    (
        "libc/ravenlibc/include/bits/abi_time.h",
        "libc/ravenlibc/abi/generated/include/bits/abi_time.h",
    ),
    (
        "libc/ravenlibc/arch/x86_64/include/bits/syscall.h",
        "libc/ravenlibc/abi/generated/include/arch/x86_64/bits/syscall.h",
    ),
    (
        "libc/ravenlibc/arch/aarch64/include/bits/syscall.h",
        "libc/ravenlibc/abi/generated/include/arch/aarch64/bits/syscall.h",
    ),
    (
        "libc/ravenlibc/arch/riscv64/include/bits/syscall.h",
        "libc/ravenlibc/abi/generated/include/arch/riscv64/bits/syscall.h",
    ),
];

const VALID_OWNERS: &[&str] = &["C37", "C38", "C39", "C40", "C41", "C45"];

#[test]
fn every_installed_header_is_in_the_manifest() {
    let rows = manifest::load().expect("manifest");
    assert!(rows.len() >= 190, "manifest has {} rows", rows.len());
    let disk = manifest::disk_headers().expect("disk headers");
    for name in &disk {
        assert!(
            manifest::find(&rows, name).is_some(),
            "{name} exists on disk but has no manifest row"
        );
    }
}

#[test]
fn owned_and_absent_rows_match_the_tree() {
    let rows = manifest::load().expect("manifest");
    for row in &rows {
        assert!(
            VALID_OWNERS.contains(&row.owner.as_str()),
            "{} has owner {}",
            row.path,
            row.owner
        );
        match row.status {
            manifest::Status::Owned => assert!(
                manifest::installed(&row.path, None),
                "owned header {} does not exist",
                row.path
            ),
            manifest::Status::Absent => assert!(
                !manifest::installed(&row.path, None),
                "absent header {} exists",
                row.path
            ),
            manifest::Status::Pending => {}
        }
    }
}

#[test]
fn pending_rows_follow_the_required_owner_policy() {
    let rows = manifest::load().expect("manifest");
    let required = manifest::required_owners();
    let complete = manifest::require_complete();
    let mut failures = 0;
    for row in rows
        .iter()
        .filter(|row| row.status == manifest::Status::Pending)
    {
        if complete || required.iter().any(|owner| owner == &row.owner) {
            failures += 1;
            eprintln!("PENDING {} (owner {})", row.path, row.owner);
        }
    }
    assert_eq!(failures, 0, "{failures} pending rows are required");
}

#[test]
fn c36_copies_are_byte_identical() {
    let root = repo_root();
    for (target_path, source_path) in GENERATED_COPIES {
        let target = std::fs::read(root.join(target_path))
            .unwrap_or_else(|error| panic!("cannot read {target_path}: {error}"));
        let source = std::fs::read(root.join(source_path))
            .unwrap_or_else(|error| panic!("cannot read {source_path}: {error}"));
        assert_eq!(
            target, source,
            "{target_path} differs from the C36 output {source_path}"
        );
    }
    println!(
        "T8: {} C36 copies are byte-identical",
        GENERATED_COPIES.len()
    );
}

#[test]
fn no_extra_headers_are_unowned() {
    let rows = manifest::load().expect("manifest");
    for name in manifest::disk_headers().expect("disk") {
        let row = manifest::find(&rows, &name).expect("row");
        assert_ne!(
            row.status,
            manifest::Status::Absent,
            "{name} is absent but on disk"
        );
    }
}

#[test]
fn toolchain_probe_runs_for_the_manifest_test() {
    // T8 itself does not compile; this keeps the locate failure explicit when
    // someone runs only this test on a machine without clang.
    let toolchain = clang::locate_toolchain().expect("clang");
    assert!(!toolchain.version.is_empty());
}
