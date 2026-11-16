// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for `id` argument and line formatting.

use super::*;

const PASSWD: &str = include_str!("../../../../tests/fixtures/user-management/passwd");
const GROUP: &str = include_str!("../../../../tests/fixtures/user-management/group");

fn database() -> Database {
    Database::parse_lookup(PASSWD, GROUP).expect("fixture")
}

fn alice() -> Credential {
    let database = database();
    let user = database.user("alice").expect("alice");
    Credential::for_user(&database, user).expect("credential")
}

#[test]
fn selectors_parse_singly_and_combined() {
    assert_eq!(parse_args(["-u"]).expect("u").selection, Selection::User);
    assert_eq!(parse_args(["-g"]).expect("g").selection, Selection::Group);
    assert_eq!(parse_args(["-G"]).expect("G").selection, Selection::Groups);
    assert!(parse_args(["-un"]).expect("un").names);
    assert!(!parse_args([]).expect("default").names);
}

#[test]
fn selector_errors_are_reported() {
    assert_eq!(
        parse_args(["-u", "-g"]).expect_err("conflict"),
        UsageError::Conflict
    );
    assert_eq!(
        parse_args(["-n"]).expect_err("names"),
        UsageError::NamesWithoutSelection
    );
    assert_eq!(
        parse_args(["-z"]).expect_err("unknown"),
        UsageError::UnknownOption('z')
    );
    assert_eq!(
        parse_args(["a", "b"]).expect_err("extra"),
        UsageError::ExtraOperand
    );
}

#[test]
fn numbers_and_names_are_rendered() {
    let database = database();
    let credential = alice();
    assert_eq!(
        format(&database, &credential, &parse_args([]).expect("all")),
        "uid=1000(alice) gid=1000(alice) groups=1000(alice)"
    );
    assert_eq!(
        format(&database, &credential, &parse_args(["-u"]).expect("u")),
        "1000"
    );
    assert_eq!(
        format(&database, &credential, &parse_args(["-un"]).expect("un")),
        "alice"
    );
    assert_eq!(
        format(&database, &credential, &parse_args(["-gn"]).expect("gn")),
        "alice"
    );
    assert_eq!(
        format(&database, &credential, &parse_args(["-G"]).expect("G")),
        "1000"
    );
}

#[test]
fn supplementary_groups_follow_the_primary_group() {
    let mut database = database();
    database.add_member("users", "alice").expect("member");
    let credential = alice_for(&database);
    assert_eq!(
        format(&database, &credential, &parse_args(["-G"]).expect("G")),
        "1000,100"
    );
    assert_eq!(
        format(&database, &credential, &parse_args(["-Gn"]).expect("Gn")),
        "alice,users"
    );
    assert_eq!(
        format(&database, &credential, &parse_args([]).expect("all")),
        "uid=1000(alice) gid=1000(alice) groups=1000(alice),100(users)"
    );
}

fn alice_for(database: &Database) -> Credential {
    Credential::for_user(database, database.user("alice").expect("alice")).expect("credential")
}

#[test]
fn unknown_ids_stay_numeric() {
    let database = database();
    let credential = Credential {
        uid: 4242,
        euid: 4242,
        gid: 4343,
        egid: 4343,
        groups: alloc::vec![4343],
    };
    assert_eq!(
        format(&database, &credential, &parse_args([]).expect("all")),
        "uid=4242 gid=4343 groups=4343"
    );
    assert_eq!(
        format(&database, &credential, &parse_args(["-un"]).expect("un")),
        "4242"
    );
}

#[test]
fn an_operand_resolves_by_name_and_number() {
    let database = database();
    let by_name = credential_for(&database, Some("alice")).expect("name");
    let by_number = credential_for(&database, Some("1000")).expect("number");
    assert_eq!(by_name, by_number);
    assert_eq!(
        credential_for(&database, Some("missing")).expect_err("missing"),
        DbError::UnknownUser
    );
}

#[test]
fn process_credential_puts_the_primary_group_first() {
    let ids = ProcessIds {
        uid: 1000,
        euid: 0,
        gid: 1001,
        egid: 0,
        groups: alloc::vec![100, 1001, 0],
    };
    let credential = process_credential(&ids);
    assert_eq!(credential.uid, 1000);
    assert_eq!(credential.euid, 0);
    assert_eq!(credential.gid, 1001);
    assert_eq!(credential.egid, 0);
    assert_eq!(credential.groups, alloc::vec![1001, 100, 0]);
}

#[test]
fn the_current_identity_uses_the_root_record() {
    let database = database();
    let credential = credential_for(&database, None).expect("root");
    assert_eq!(credential.uid, 0);
    assert_eq!(credential.gid, 0);
    assert_eq!(credential.groups, alloc::vec![0]);
}
