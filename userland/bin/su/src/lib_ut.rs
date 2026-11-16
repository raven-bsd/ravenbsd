// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for `su` argument and environment handling.

use super::*;

fn user(name: &str, home: &str, shell: &str) -> PasswdEntry {
    PasswdEntry {
        name: name.to_string(),
        passwd: "x".to_string(),
        uid: 1000,
        gid: 1000,
        gecos: String::new(),
        home: home.to_string(),
        shell: shell.to_string(),
    }
}

#[test]
fn the_default_target_is_root() {
    let options = parse_args([]).expect("parse");
    assert_eq!(options.user, DEFAULT_USER);
    assert!(!options.login);
    assert_eq!(options.command, None);
}

#[test]
fn login_and_command_are_parsed() {
    let options = parse_args(["-", "-c", "id", "alice"]).expect("parse");
    assert!(options.login);
    assert_eq!(options.command, Some("id"));
    assert_eq!(options.user, "alice");
    assert_eq!(
        parse_args(["-cid", "alice"]).expect("attached").command,
        Some("id")
    );
}

#[test]
fn operand_and_option_errors_are_reported() {
    assert_eq!(
        parse_args(["-c"]).expect_err("value"),
        UsageError::MissingValue
    );
    assert_eq!(
        parse_args(["-x"]).expect_err("unknown"),
        UsageError::UnknownOption('x')
    );
    assert_eq!(
        parse_args(["a", "b"]).expect_err("extra"),
        UsageError::ExtraOperand
    );
    assert_eq!(parse_args(["--", "-"]).expect("literal").user, "-");
}

#[test]
fn the_environment_uses_the_account_record() {
    let alice = user("alice", "/home/alice", "/bin/brush");
    let login_environment = environment(&alice, true);
    assert!(login_environment.contains(&"HOME=/home/alice".to_string()));
    assert!(login_environment.contains(&"SHELL=/bin/brush".to_string()));
    assert!(login_environment.contains(&"USER=alice".to_string()));
    assert!(login_environment.contains(&"LOGNAME=alice".to_string()));
    assert!(login_environment.contains(&"PATH=/bin:/usr/bin:/sbin:/usr/sbin".to_string()));
    assert!(login_environment.contains(&"PWD=/home/alice".to_string()));
    let non_login = environment(&alice, false);
    assert!(non_login.contains(&"PWD=/".to_string()));
}

#[test]
fn an_empty_shell_uses_the_default() {
    let nobody = user("nobody", "", "");
    assert_eq!(shell_path(&nobody), DEFAULT_SHELL);
    let session = environment(&nobody, true);
    assert!(session.contains(&"HOME=/".to_string()));
    assert!(session.contains(&"SHELL=/bin/brush".to_string()));
}

#[test]
fn commands_are_passed_with_c() {
    let options = parse_args(["-c", "echo hello", "root"]).expect("parse");
    assert_eq!(shell_arguments(&options), vec!["-c", "echo hello"]);
    let interactive = parse_args([]).expect("interactive");
    assert!(shell_arguments(&interactive).is_empty());
}
