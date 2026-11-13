// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use std::path::Path;

use super::*;

#[test]
fn every_command_is_recognised() {
    for (name, _) in COMMANDS {
        assert!(is_libc_command(name), "{name}");
    }
}

#[test]
fn unknown_command_is_rejected() {
    assert!(!is_libc_command("libc-nope"));
    assert!(run(Path::new("."), "libc-nope", &[]).is_err());
}

#[test]
fn oracle_dispatch_requires_a_subcommand() {
    let error = run(Path::new("."), "libc-oracle", &[]).expect_err("usage");
    assert!(error.to_string().contains("needs a subcommand"));
}

#[test]
fn headers_stub_exits_with_c37_message() {
    let error = run(Path::new("."), "libc-headers", &[]).expect_err("stub");
    assert!(
        error
            .to_string()
            .contains("not implemented by C34 (ticket C37)")
    );
}

#[test]
fn pack_stub_exits_with_c60_message() {
    let error = run(Path::new("."), "libc-pack", &[]).expect_err("stub");
    assert!(
        error
            .to_string()
            .contains("not implemented by C34 (ticket C60)")
    );
}

#[test]
fn size_stub_exits_with_c69_message() {
    let error = run(Path::new("."), "libc-size", &[]).expect_err("stub");
    assert!(
        error
            .to_string()
            .contains("not implemented by C34 (ticket C69)")
    );
}

#[test]
fn world_stub_exits_with_c72_message() {
    let error = run(Path::new("."), "libc-world", &[]).expect_err("stub");
    assert!(
        error
            .to_string()
            .contains("not implemented by C34 (ticket C72)")
    );
}

#[test]
fn lint_unknown_check_is_an_error() {
    let error = run(Path::new("."), "libc-lint", &["nope".to_string()]).expect_err("unknown");
    assert!(error.to_string().contains("unknown check"));
}
