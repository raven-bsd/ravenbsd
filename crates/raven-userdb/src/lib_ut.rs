// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Cross-module smoke tests for the public database surface.

use super::*;

const PASSWD: &str = include_str!("../../../tests/fixtures/user-management/passwd");
const GROUP: &str = include_str!("../../../tests/fixtures/user-management/group");
const SHADOW: &str = include_str!("../../../tests/fixtures/user-management/shadow");

#[test]
fn the_canonical_fixture_parses_and_renders_byte_for_byte() {
    let database = Database::parse(PASSWD, GROUP, SHADOW).expect("fixture");
    assert_eq!(database.render_passwd(), PASSWD);
    assert_eq!(database.render_group(), GROUP);
    assert_eq!(database.render_shadow(), SHADOW);
}

#[test]
fn the_root_record_is_the_release_vector() {
    let database = Database::parse(PASSWD, GROUP, SHADOW).expect("fixture");
    let root = database.shadow_by_name("root").expect("root shadow");
    assert_eq!(root.hash, raven_auth::DEFAULT_ROOT_HASH);
}

#[test]
fn errors_have_a_stable_description() {
    assert_eq!(DbError::UnknownUser.to_string(), "unknown user");
    assert_eq!(DbError::Busy.to_string(), "account database is locked");
    assert_eq!(
        DbError::FieldCount {
            file: DbFile::Passwd,
            line: 7,
            expected: 7,
            found: 5
        }
        .to_string(),
        "passwd: line 7: expected 7 fields, got 5"
    );
}
