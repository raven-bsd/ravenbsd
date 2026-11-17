// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for `newgrp` argument, group and shell-plan handling.

use super::*;

fn user(name: &str, gid: u32, home: &str, shell: &str) -> PasswdEntry {
    PasswdEntry {
        name: name.to_string(),
        passwd: "x".to_string(),
        uid: 1000,
        gid,
        gecos: String::new(),
        home: home.to_string(),
        shell: shell.to_string(),
    }
}

fn group(name: &str, gid: u32, members: &[&str]) -> GroupEntry {
    GroupEntry {
        name: name.to_string(),
        passwd: "*".to_string(),
        gid,
        members: members.iter().map(|name| (*name).to_string()).collect(),
    }
}

#[test]
fn parses_no_operand_login_and_group() {
    assert_eq!(
        parse_args([]).expect("empty"),
        Options {
            login: false,
            group: None
        }
    );
    assert_eq!(
        parse_args(["-l"]).expect("login"),
        Options {
            login: true,
            group: None
        }
    );
    assert_eq!(
        parse_args(["-", "staff"]).expect("dash login"),
        Options {
            login: true,
            group: Some("staff")
        }
    );
    assert_eq!(parse_args(["50"]).expect("gid").group, Some("50"));
    assert_eq!(parse_args(["--", "-l"]).expect("literal").group, Some("-l"));
}

#[test]
fn rejects_unknown_options_and_extra_operands() {
    assert_eq!(
        parse_args(["-x"]).expect_err("unknown"),
        UsageError::UnknownOption('x')
    );
    assert_eq!(
        parse_args(["staff", "users"]).expect_err("extra"),
        UsageError::ExtraOperand
    );
}

#[test]
fn finds_groups_by_name_and_numeric_gid() {
    let groups = [group("staff", 50, &[]), group("users", 100, &["alice"])];
    assert_eq!(find_group(&groups, "staff").map(|g| g.gid), Some(50));
    assert_eq!(
        find_group(&groups, "100").map(|g| g.name.as_str()),
        Some("users")
    );
    assert!(find_group(&groups, "missing").is_none());
    assert!(find_group(&groups, "999").is_none());
}

#[test]
fn membership_allows_root_primary_and_explicit_members() {
    let alice = user("alice", 100, "/home/alice", "/bin/brush");
    let users = group("users", 100, &[]);
    let staff = group("staff", 50, &["alice"]);
    let wheel = group("wheel", 0, &[]);
    assert_eq!(membership(&users, &alice, false), Access::Allowed);
    assert_eq!(membership(&staff, &alice, false), Access::Allowed);
    assert_eq!(membership(&wheel, &alice, false), Access::Denied);
    assert_eq!(membership(&wheel, &alice, true), Access::Allowed);
}

#[test]
fn shell_plan_prefers_the_environment_shell() {
    let alice = user("alice", 100, "/home/alice", "/bin/brush");
    let plan = shell_plan(&alice, Some("/bin/sh"), false);
    assert_eq!(plan.shell, "/bin/sh");
    assert!(plan.args.is_empty());
    assert!(plan.env.is_empty());
    let plan = shell_plan(&alice, None, false);
    assert_eq!(plan.shell, "/bin/brush");
    let plan = shell_plan(&alice, None, true);
    assert_eq!(plan.args, vec!["-brush".to_string()]);
    assert!(plan.env.contains(&"HOME=/home/alice".to_string()));
    assert!(plan.env.contains(&"SHELL=/bin/brush".to_string()));
    assert!(plan.env.contains(&"USER=alice".to_string()));
    assert!(plan.env.contains(&"LOGNAME=alice".to_string()));
    assert!(
        plan.env
            .contains(&"PATH=/bin:/usr/bin:/sbin:/usr/sbin".to_string())
    );
    assert!(plan.env.contains(&"PWD=/home/alice".to_string()));
}

#[test]
fn shell_plan_falls_back_to_the_default() {
    let nobody = user("nobody", 65534, "", "");
    assert_eq!(shell_plan(&nobody, None, false).shell, DEFAULT_SHELL);
    assert_eq!(
        shell_plan(&nobody, Some(""), true).args,
        vec!["-brush".to_string()]
    );
}
