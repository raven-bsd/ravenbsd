// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the parsed database, lookups, validation and tool edits.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use super::*;

const PASSWD: &str = include_str!("../../../tests/fixtures/user-management/passwd");
const GROUP: &str = include_str!("../../../tests/fixtures/user-management/group");
const SHADOW: &str = include_str!("../../../tests/fixtures/user-management/shadow");
const EXPECTED: &str = include_str!("../../../tests/fixtures/user-management/expected.tsv");
const CAROL_PASSWD: &str = include_str!("../../../tests/user/useradd-carol.passwd");
const CAROL_GROUP: &str = include_str!("../../../tests/user/useradd-carol.group");
const CAROL_SHADOW: &str = include_str!("../../../tests/user/useradd-carol.shadow");
const ALICE_SHADOW: &str = include_str!("../../../tests/user/passwd-alice.shadow");
const STAFF_GROUP: &str = include_str!("../../../tests/user/groupadd-staff.group");
const TEST_HASH: &str = "$6$testsalt$k8uF1gMYhanpZJpvWku00HsKHnqnUBrMHUCgINekbAoti/YVItOH03uYEsLcpCUZlp7eTjSRYgjB92ov/XmXi.";

fn fixture() -> Database {
    Database::parse(PASSWD, GROUP, SHADOW).expect("fixture")
}

fn carol() -> AddUser<'static> {
    AddUser {
        name: "carol",
        uid: None,
        group: None,
        groups: &[],
        gecos: "Carol Example",
        home: None,
        shell: None,
        system: false,
    }
}

#[test]
fn expected_lookups_match_the_canonical_fixture() {
    let database = fixture();
    for line in EXPECTED.lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split_whitespace().collect();
        match fields.first().copied() {
            Some("pw") => {
                let user = database
                    .user(fields.get(1).copied().unwrap_or(""))
                    .expect("user");
                assert_eq!(
                    user.uid.to_string(),
                    fields.get(2).copied().unwrap_or("").to_string()
                );
                assert_eq!(
                    user.gid.to_string(),
                    fields.get(3).copied().unwrap_or("").to_string()
                );
                assert_eq!(user.home, fields.get(4).copied().unwrap_or(""));
                assert_eq!(user.shell, fields.get(5).copied().unwrap_or(""));
            }
            Some("gr") => {
                let group = database
                    .group_by_name(fields.get(1).copied().unwrap_or(""))
                    .expect("group");
                assert_eq!(
                    group.gid.to_string(),
                    fields.get(2).copied().unwrap_or("").to_string()
                );
            }
            other => panic!("unexpected expected.tsv kind: {other:?}"),
        }
    }
}

#[test]
fn useradd_carol_produces_the_expected_files() {
    let mut database = fixture();
    let outcome = database.add_user(&carol()).expect("add carol");
    assert_eq!(outcome.user.uid, 1002);
    assert_eq!(outcome.user.gid, 1002);
    assert_eq!(outcome.user.home, "/home/carol");
    assert_eq!(outcome.user.shell, DEFAULT_SHELL);
    assert!(outcome.created_group);
    assert_eq!(database.render_passwd(), CAROL_PASSWD);
    assert_eq!(database.render_group(), CAROL_GROUP);
    assert_eq!(database.render_shadow(), CAROL_SHADOW);
}

#[test]
fn groupadd_staff_gets_the_next_regular_gid() {
    let mut database = fixture();
    let group = database
        .add_group(&AddGroup {
            name: "staff",
            gid: None,
            system: false,
        })
        .expect("add staff");
    assert_eq!(group.gid, 1002);
    assert_eq!(database.render_group(), STAFF_GROUP);
}

#[test]
fn passwd_alice_replaces_the_hash_and_last_change() {
    let mut database = fixture();
    database
        .set_password("alice", TEST_HASH, 19_001)
        .expect("set password");
    assert_eq!(database.render_shadow(), ALICE_SHADOW);
    assert_eq!(
        database.shadow_by_name("alice").expect("alice").hash,
        TEST_HASH
    );
}

#[test]
fn a_user_without_a_shadow_record_gets_one() {
    let mut database = fixture();
    assert!(database.shadow_by_name("nobody").is_none());
    database
        .set_password("nobody", TEST_HASH, 19_001)
        .expect("set password");
    assert!(database.shadow_by_name("nobody").is_some());
    database.validate().expect("still valid");
}

#[test]
fn duplicate_names_and_ids_are_rejected() {
    let mut database = fixture();
    let alice = AddUser {
        name: "alice",
        ..carol()
    };
    assert!(matches!(
        database.add_user(&alice),
        Err(DbError::Duplicate { .. })
    ));
    let duplicate_uid = AddUser {
        name: "dave",
        uid: Some(1000),
        ..carol()
    };
    assert!(matches!(
        database.add_user(&duplicate_uid),
        Err(DbError::Duplicate { .. })
    ));
    assert!(matches!(
        database.add_group(&AddGroup {
            name: "wheel",
            gid: None,
            system: false
        }),
        Err(DbError::Duplicate { .. })
    ));
    assert!(matches!(
        database.add_group(&AddGroup {
            name: "staff",
            gid: Some(0),
            system: false
        }),
        Err(DbError::Duplicate { .. })
    ));
}

#[test]
fn unknown_and_too_many_groups_are_rejected() {
    let mut database = fixture();
    let unknown = AddUser {
        name: "dave",
        groups: &["missing"],
        ..carol()
    };
    assert_eq!(database.add_user(&unknown), Err(DbError::UnknownGroup));
    let names = [
        "g01", "g02", "g03", "g04", "g05", "g06", "g07", "g08", "g09", "g10", "g11", "g12", "g13",
        "g14", "g15", "g16", "g17",
    ];
    let many = AddUser {
        name: "dave",
        groups: &names,
        ..carol()
    };
    assert_eq!(database.add_user(&many), Err(DbError::TooManyGroups));
}

#[test]
fn explicit_ids_and_groups_are_honoured() {
    let mut database = fixture();
    let dave = AddUser {
        name: "dave",
        uid: Some(2000),
        group: Some("users"),
        ..carol()
    };
    let outcome = database.add_user(&dave).expect("add dave");
    assert_eq!(outcome.user.uid, 2000);
    assert_eq!(outcome.user.gid, 100);
    assert!(!outcome.created_group);
    assert!(database.group_by_name("dave").is_none());
}

#[test]
fn a_same_name_group_with_a_different_gid_is_rejected() {
    let mut database = fixture();
    database
        .add_group(&AddGroup {
            name: "dave",
            gid: Some(3000),
            system: false,
        })
        .expect("group");
    let dave = AddUser {
        name: "dave",
        uid: Some(2000),
        group: None,
        ..carol()
    };
    assert!(matches!(
        database.add_user(&dave),
        Err(DbError::Duplicate {
            file: DbFile::Group,
            ..
        })
    ));
}

#[test]
fn supplementary_memberships_are_added() {
    let mut database = fixture();
    let dave = AddUser {
        name: "dave",
        groups: &["users", "wheel"],
        ..carol()
    };
    database.add_user(&dave).expect("add dave");
    assert!(
        database
            .group_by_name("users")
            .expect("users")
            .contains("dave")
    );
    assert!(
        database
            .group_by_name("wheel")
            .expect("wheel")
            .contains("dave")
    );
    let user = database.user("dave").expect("dave");
    assert_eq!(
        database.groups_for_user(user).expect("groups"),
        alloc::vec![1002, 0, 100]
    );
}

#[test]
fn sixteen_supplementary_groups_are_allowed_and_seventeen_are_not() {
    let mut database = fixture();
    for index in 0..16u32 {
        let name = alloc::format!("g{index:02}");
        database
            .add_group(&AddGroup {
                name: &name,
                gid: Some(2000 + index),
                system: false,
            })
            .expect("group");
        database.add_member(&name, "alice").expect("member");
    }
    {
        let alice = database.user("alice").expect("alice");
        assert_eq!(database.groups_for_user(alice).expect("sixteen").len(), 17);
    }
    let seventeenth = "g16".to_string();
    database
        .add_group(&AddGroup {
            name: &seventeenth,
            gid: Some(3000),
            system: false,
        })
        .expect("group");
    database.add_member(&seventeenth, "alice").expect("member");
    let alice = database.user("alice").expect("alice");
    assert_eq!(database.groups_for_user(alice), Err(DbError::TooManyGroups));
}

#[test]
fn lock_and_unlock_are_idempotent_and_safe() {
    let mut database = fixture();
    database.lock_password("alice").expect("lock");
    assert!(database.shadow_by_name("alice").expect("alice").is_locked());
    database.lock_password("alice").expect("lock again");
    database.unlock_password("alice").expect("unlock");
    assert_eq!(
        database.shadow_by_name("alice").expect("alice").hash,
        "$6$r4ven$S6VNu6h5bJJ4N0puQ63budZes8.I7oATcnLzSQBmkTcTvZ.4QTdkbKttZybkfYqCmRZvvzbfDzxeMs3FalGIQ."
    );
    assert!(database.unlock_password("bob").is_err());
    assert!(database.lock_password("missing").is_err());
}

#[test]
fn account_ranges_and_allocation() {
    assert!(valid_id(1000, false));
    assert!(valid_id(59_999, false));
    assert!(!valid_id(999, false));
    assert!(valid_id(1, true));
    assert!(valid_id(999, true));
    assert!(!valid_id(0, true));
    assert!(!valid_id(1000, true));

    let mut database = fixture();
    assert_eq!(database.next_regular_uid().expect("uid"), 1002);
    assert_eq!(database.next_regular_gid().expect("gid"), 1002);
    assert_eq!(database.next_system_gid().expect("system gid"), 101);
    let high = AddUser {
        name: "high",
        uid: Some(MAX_REGULAR_ID),
        ..carol()
    };
    database.add_user(&high).expect("high");
    assert_eq!(database.next_regular_uid(), Err(DbError::NoFreeId));
}

#[test]
fn malformed_names_and_paths_are_rejected() {
    let mut database = fixture();
    for name in ["", "-dave", "dave:x", "dave name"] {
        let request = AddUser { name, ..carol() };
        assert!(database.add_user(&request).is_err(), "{name}");
    }
    let traversal = AddUser {
        name: "dave",
        home: Some("/home/../etc"),
        ..carol()
    };
    assert!(database.add_user(&traversal).is_err());
    let relative = AddUser {
        name: "dave",
        home: Some("home/dave"),
        ..carol()
    };
    assert!(database.add_user(&relative).is_err());
}

#[test]
fn credentials_include_primary_and_supplementary_groups() {
    let mut database = fixture();
    database.add_member("users", "alice").expect("member");
    let alice = database.user("alice").expect("alice");
    let credential = Credential::for_user(&database, alice).expect("credential");
    assert_eq!(credential.uid, 1000);
    assert_eq!(credential.euid, 1000);
    assert_eq!(credential.gid, 1000);
    assert_eq!(credential.egid, 1000);
    assert_eq!(credential.groups, alloc::vec![1000, 100]);
}

#[test]
fn lookup_parsing_does_not_require_shadow() {
    let database = Database::parse_lookup(PASSWD, GROUP).expect("lookup");
    assert_eq!(database.user("root").expect("root").uid, 0);
    assert!(database.shadow().is_empty());
}

#[test]
fn cross_file_validation_rejects_unknown_references() {
    let bad_passwd = "root:x:0:4000:root:/root:/bin/brush\n";
    assert!(Database::parse(bad_passwd, GROUP, SHADOW).is_err());
    let bad_group = "wheel:x:0:missing\nusers:x:100:\n";
    assert!(Database::parse(PASSWD, bad_group, SHADOW).is_err());
    let orphan_shadow =
        "root:$6$salt$digest:0:0:99999:7:0:0:0\nmissing:$6$salt$digest:0:0:99999:7:0:0:0\n";
    assert!(Database::parse(PASSWD, GROUP, orphan_shadow).is_err());
}

#[test]
fn duplicate_rows_are_rejected() {
    let duplicate = format!("{PASSWD}root:x:0:0:root:/root:/bin/brush\n");
    assert!(Database::parse(&duplicate, GROUP, SHADOW).is_err());
    let duplicate_group = format!("{GROUP}wheel:x:0:root\n");
    assert!(Database::parse(PASSWD, &duplicate_group, SHADOW).is_err());
    let duplicate_shadow = format!("{SHADOW}root:!:0:0:0:0:0:0:0\n");
    assert!(Database::parse(PASSWD, GROUP, &duplicate_shadow).is_err());
}

#[test]
fn unicode_is_not_silently_accepted() {
    let line = "álice:x:1000:1000::/home/alice:/bin/brush";
    assert!(PasswdEntry::parse(line, 1).is_err());
}

#[test]
fn a_user_name_is_a_string_not_a_u32() {
    let database = fixture();
    assert!(database.user("1000").is_none());
    assert_eq!(database.user_by_uid(1000).expect("alice").name, "alice");
}

#[test]
fn render_always_ends_with_one_newline() {
    let database = fixture();
    for text in [
        database.render_passwd(),
        database.render_group(),
        database.render_shadow(),
    ] {
        assert!(text.ends_with('\n'));
        assert!(!text.ends_with("\n\n"));
    }
}

#[test]
fn an_empty_database_is_valid() {
    let database = Database::parse("", "", "").expect("empty");
    assert_eq!(database.render_passwd(), String::new());
    assert_eq!(database.next_regular_uid().expect("uid"), 1000);
}
