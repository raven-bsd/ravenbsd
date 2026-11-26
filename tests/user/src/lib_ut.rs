// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The U4 host matrix: tool argument layers plus the user database, against the canonical
//! fixtures and the real 0.1.0 image files.

use raven_userdb::{AddGroup, AddUser, Database, DbError};

const PASSWD: &str = include_str!("../../../tests/fixtures/user-management/passwd");
const GROUP: &str = include_str!("../../../tests/fixtures/user-management/group");
const SHADOW: &str = include_str!("../../../tests/fixtures/user-management/shadow");
const CAROL_PASSWD: &str = include_str!("../useradd-carol.passwd");
const CAROL_GROUP: &str = include_str!("../useradd-carol.group");
const CAROL_SHADOW: &str = include_str!("../useradd-carol.shadow");
const ALICE_SHADOW: &str = include_str!("../passwd-alice.shadow");
const STAFF_GROUP: &str = include_str!("../groupadd-staff.group");
const ETC_PASSWD: &str = include_str!("../../../etc/passwd");
const ETC_GROUP: &str = include_str!("../../../etc/group");
const ETC_SHADOW: &str = include_str!("../../../etc/shadow");
const TEST_HASH: &str = "$6$testsalt$k8uF1gMYhanpZJpvWku00HsKHnqnUBrMHUCgINekbAoti/YVItOH03uYEsLcpCUZlp7eTjSRYgjB92ov/XmXi.";

fn fixture() -> Database {
    Database::parse(PASSWD, GROUP, SHADOW).expect("canonical fixture")
}

fn useradd_request<'a>(options: &'a useradd::Options<'a>) -> AddUser<'a> {
    AddUser {
        name: options.name,
        uid: options.uid,
        group: options.group,
        groups: &options.groups,
        gecos: options.gecos.unwrap_or(""),
        home: options.home,
        shell: options.shell,
        system: false,
    }
}

fn groupadd_request<'a>(options: &'a groupadd::Options<'a>) -> AddGroup<'a> {
    AddGroup {
        name: options.name,
        gid: options.gid,
        system: options.system,
    }
}

#[test]
fn useradd_creates_the_passwd_group_and_locked_shadow_rows() {
    let mut database = fixture();
    let options = useradd::parse_args(["-c", "Carol Example", "carol"]).expect("parse");
    let outcome = database
        .add_user(&useradd_request(&options))
        .expect("add carol");
    assert_eq!(outcome.user.uid, 1002);
    assert_eq!(outcome.user.gid, 1002);
    assert_eq!(outcome.user.home, "/home/carol");
    assert_eq!(outcome.user.shell, "/bin/brush");
    assert!(outcome.created_group);
    assert_eq!(database.render_passwd(), CAROL_PASSWD);
    assert_eq!(database.render_group(), CAROL_GROUP);
    assert_eq!(database.render_shadow(), CAROL_SHADOW);
    assert!(
        database
            .shadow_by_name("carol")
            .expect("shadow")
            .is_locked()
    );
    assert_eq!(database.shadow_by_name("carol").expect("shadow").hash, "!");
}

#[test]
fn adduser_defaults_and_hash_update_round_trip() {
    let mut database = fixture();
    let name = adduser::parse_args(["dave"]).expect("parse").expect("name");
    assert_eq!(adduser::default_home(name), "/home/dave");
    assert_eq!(adduser::DEFAULT_SHELL, "/bin/brush");
    let home = adduser::default_home(name);
    let request = AddUser {
        name,
        uid: None,
        group: None,
        groups: &[],
        gecos: "Dave Example",
        home: Some(&home),
        shell: Some(adduser::DEFAULT_SHELL),
        system: false,
    };
    let outcome = database.add_user(&request).expect("add dave");
    assert_eq!(outcome.user.uid, 1002);
    passwd::validate_password("davepass", 0, outcome.user.uid).expect("password policy");
    let hash = raven_auth::hash_password("davepass", b"dave-salt").expect("hash");
    database.set_password("dave", &hash, 19_002).expect("set");
    assert!(raven_auth::verify_password("davepass", &hash));
    assert!(!raven_auth::verify_password("wrongpass", &hash));
    assert!(!database.render_passwd().contains("davepass"));
    assert!(!database.render_shadow().contains("davepass"));
    assert_eq!(
        database.shadow_by_name("dave").expect("dave").last_change,
        19_002
    );
}

#[test]
fn duplicate_user_and_group_requests_are_rejected() {
    let mut database = fixture();
    let before = database.render_passwd();
    let alice = useradd::parse_args(["alice"]).expect("parse");
    assert!(matches!(
        database.add_user(&useradd_request(&alice)),
        Err(DbError::Duplicate { .. })
    ));
    let explicit = useradd::parse_args(["-u", "1000", "dave"]).expect("parse");
    assert!(matches!(
        database.add_user(&useradd_request(&explicit)),
        Err(DbError::Duplicate { .. })
    ));
    let wheel = groupadd::parse_args(["wheel"]).expect("parse");
    assert!(matches!(
        database.add_group(&groupadd_request(&wheel)),
        Err(DbError::Duplicate { .. })
    ));
    let gid = groupadd::parse_args(["-g", "100", "staff"]).expect("parse");
    assert!(matches!(
        database.add_group(&groupadd_request(&gid)),
        Err(DbError::Duplicate { .. })
    ));
    assert_eq!(database.render_passwd(), before);
}

#[test]
fn passwd_updates_locks_and_unlocks_the_hash() {
    let mut database = fixture();
    let options = passwd::parse_args(["alice"]).expect("parse");
    assert_eq!(options.action, passwd::Action::Change);
    assert_eq!(options.user, Some("alice"));
    assert_eq!(
        passwd::validate_password("short", 0, 1000).expect_err("short"),
        passwd::PasswordError::TooShort
    );
    assert!(passwd::validate_password("x", 0, 0).is_ok());
    let hash = raven_auth::hash_password("new-password", b"testsalt").expect("hash");
    assert!(hash.starts_with("$6$"));
    database
        .set_password("alice", &hash, 19_001)
        .expect("set password");
    assert!(database.render_shadow().contains(&hash));
    assert!(database.render_shadow().contains(":19001:"));
    assert!(raven_auth::verify_password("new-password", &hash));
    assert!(!raven_auth::verify_password("wrong-password", &hash));
    assert_eq!(passwd::days_since_epoch(86_400), 1);

    let mut fixed_vector = fixture();
    fixed_vector
        .set_password("alice", TEST_HASH, 19_001)
        .expect("fixed vector");
    assert_eq!(fixed_vector.render_shadow(), ALICE_SHADOW);

    let lock = passwd::parse_args(["-l", "alice"]).expect("lock parse");
    assert_eq!(lock.action, passwd::Action::Lock);
    database.lock_password("alice").expect("lock");
    let locked = database
        .shadow_by_name("alice")
        .expect("alice")
        .hash
        .clone();
    assert!(locked.starts_with('!'));
    assert!(database.shadow_by_name("alice").expect("alice").is_locked());
    let unlock = passwd::parse_args(["-u", "alice"]).expect("unlock parse");
    assert_eq!(unlock.action, passwd::Action::Unlock);
    database.unlock_password("alice").expect("unlock");
    assert_eq!(database.shadow_by_name("alice").expect("alice").hash, hash);
    assert_eq!(
        database
            .set_password("missing", &hash, 0)
            .expect_err("missing"),
        DbError::UnknownUser
    );
}

#[test]
fn groupadd_allocates_regular_and_system_gids() {
    let mut database = fixture();
    let staff = groupadd::parse_args(["staff"]).expect("parse");
    let group = database
        .add_group(&groupadd_request(&staff))
        .expect("add staff");
    assert_eq!(group.gid, 1002);
    assert_eq!(database.render_group(), STAFF_GROUP);

    let service = groupadd::parse_args(["-r", "svc"]).expect("parse");
    let group = database
        .add_group(&groupadd_request(&service))
        .expect("add svc");
    assert_eq!(group.gid, 101);

    let explicit = groupadd::parse_args(["-g", "2000", "ops"]).expect("parse");
    let group = database
        .add_group(&groupadd_request(&explicit))
        .expect("add ops");
    assert_eq!(group.gid, 2000);
}

#[test]
fn su_environment_and_command_arguments_are_stable() {
    let database = fixture();
    let root = database.user("root").expect("root");
    let options = su::parse_args(["-c", "id", "root"]).expect("parse");
    assert_eq!(options.user, "root");
    assert_eq!(options.command, Some("id"));
    assert_eq!(su::shell_arguments(&options), vec!["-c", "id"]);
    let environment = su::environment(root, false);
    assert!(environment.contains(&"HOME=/root".to_string()));
    assert!(environment.contains(&"SHELL=/bin/brush".to_string()));
    assert!(environment.contains(&"USER=root".to_string()));
    assert!(environment.contains(&"LOGNAME=root".to_string()));
    assert!(environment.contains(&"PWD=/".to_string()));
    assert!(environment.contains(&"PATH=/bin:/usr/bin:/sbin:/usr/sbin".to_string()));
    assert_eq!(su::shell_path(root), "/bin/brush");

    let alice = database.user("alice").expect("alice");
    let login = su::parse_args(["-", "alice"]).expect("parse login");
    assert!(login.login);
    let environment = su::environment(alice, true);
    assert!(environment.contains(&"HOME=/home/alice".to_string()));
    assert!(environment.contains(&"PWD=/home/alice".to_string()));
    assert_eq!(su::parse_args([]).expect("default").user, su::DEFAULT_USER);
    assert!(su::parse_args(["-x"]).is_err());
    assert!(su::parse_args(["-c"]).is_err());
}

#[test]
fn id_renders_the_root_and_named_accounts() {
    let database = fixture();
    let root = id::credential_for(&database, None).expect("root");
    assert_eq!(
        id::format(&database, &root, &id::parse_args([]).expect("all")),
        "uid=0(root) gid=0(wheel) groups=0(wheel)"
    );
    assert_eq!(
        id::format(&database, &root, &id::parse_args(["-un"]).expect("name")),
        "root"
    );
    assert_eq!(
        id::format(&database, &root, &id::parse_args(["-G"]).expect("groups")),
        "0"
    );
    let alice = id::credential_for(&database, Some("alice")).expect("alice");
    assert_eq!(
        id::format(&database, &alice, &id::parse_args(["alice"]).expect("all")),
        "uid=1000(alice) gid=1000(alice) groups=1000(alice)"
    );
    assert_eq!(
        id::credential_for(&database, Some("missing")).expect_err("missing"),
        DbError::UnknownUser
    );
    assert_eq!(
        id::parse_args(["-u", "-g"]).expect_err("conflict"),
        id::UsageError::Conflict
    );
    assert_eq!(
        id::parse_args(["-n"]).expect_err("names"),
        id::UsageError::NamesWithoutSelection
    );
}

#[test]
fn id_shows_supplementary_groups() {
    let mut database = fixture();
    database.add_member("users", "alice").expect("member");
    let alice = id::credential_for(&database, Some("alice")).expect("alice");
    assert_eq!(
        id::format(&database, &alice, &id::parse_args(["-G"]).expect("groups")),
        "1000,100"
    );
    assert_eq!(
        id::format(&database, &alice, &id::parse_args(["-Gn"]).expect("names")),
        "alice,users"
    );
    assert_eq!(
        id::format(&database, &alice, &id::parse_args([]).expect("all")),
        "uid=1000(alice) gid=1000(alice) groups=1000(alice),100(users)"
    );
}

#[test]
fn whoami_resolves_known_and_unknown_uids() {
    let database = fixture();
    assert!(whoami::parse_args([]).is_ok());
    assert!(whoami::parse_args(["root"]).is_err());
    assert_eq!(whoami::name_for_uid(&database, 0), "root");
    assert_eq!(whoami::name_for_uid(&database, 1000), "alice");
    assert_eq!(whoami::name_for_uid(&database, 4242), "4242");
    assert!(whoami::known_uid(&database, 0));
    assert!(!whoami::known_uid(&database, 4242));
}

#[test]
fn canonical_fixture_is_structurally_sound() {
    let database = fixture();
    assert_eq!(database.passwd().len(), 4);
    assert_eq!(database.group().len(), 5);
    assert_eq!(database.shadow().len(), 3);
    assert!(database.shadow_by_name("nobody").is_none());
    for entry in database.shadow() {
        assert!(
            entry.hash.starts_with("$6$") || entry.is_locked(),
            "{}",
            entry.name
        );
    }
    assert!(!database.render_shadow().contains(":raven:"));
    assert!(database.render_shadow().contains("bob:!"));
}

#[test]
fn the_010_root_account_round_trips_canonically() {
    let image = Database::parse(ETC_PASSWD, ETC_GROUP, ETC_SHADOW).expect("0.1.0 image");
    assert_eq!(image.render_passwd(), ETC_PASSWD);
    assert_eq!(image.render_group(), ETC_GROUP);
    // Empty inactive/expire fields are the canonical "not set" and render as -1.
    assert_eq!(
        image.render_shadow(),
        ETC_SHADOW.replace(":7:::", ":7:-1:-1:0")
    );
    let root = image.user("root").expect("root");
    assert_eq!(root.uid, 0);
    assert_eq!(root.gid, 0);
    assert_eq!(root.home, "/root");
    // The 0.1.0 release sessions use rsh; U1's /bin/brush is the 0.2.0 target.
    assert_eq!(root.shell, "/bin/rsh");
    let shadow = image.shadow_by_name("root").expect("root shadow");
    // The image record is the canonical 16-character-salt hash, not the build-time
    // DEFAULT_ROOT_HASH (which uses a different fixed salt).
    let fields: Vec<&str> = shadow.hash.split('$').collect();
    assert_eq!(fields.get(2), Some(&"rounds=5000"));
    assert_eq!(fields.get(3).map(|salt| salt.len()), Some(16));
    let root_line = ETC_SHADOW.lines().next().expect("root shadow line");
    assert!(raven_auth::verify_root_shadow(root_line, "raven"));
    assert!(!raven_auth::verify_root_shadow(root_line, "wrong"));
}

#[test]
fn the_010_image_can_add_a_regular_account_without_changing_root() {
    let mut image = Database::parse(ETC_PASSWD, ETC_GROUP, ETC_SHADOW).expect("0.1.0 image");
    let root_before = image.user("root").expect("root").clone();
    let shadow_before = image.shadow_by_name("root").expect("root").clone();
    let options = useradd::parse_args(["alice"]).expect("parse");
    let outcome = image
        .add_user(&useradd_request(&options))
        .expect("add alice");
    // The base image already ships raven (uid/gid 1000), so the next regular id is 1001.
    assert_eq!(outcome.user.uid, 1001);
    assert_eq!(outcome.user.gid, 1001);
    assert!(outcome.created_group);
    assert_eq!(image.user("root").expect("root"), &root_before);
    assert_eq!(image.shadow_by_name("root").expect("root"), &shadow_before);
    assert!(image.render_passwd().starts_with(ETC_PASSWD));
}

#[test]
fn the_010_image_still_needs_the_u1_group_migration() {
    let mut image = Database::parse(ETC_PASSWD, ETC_GROUP, ETC_SHADOW).expect("0.1.0 image");
    let options = useradd::parse_args(["-g", "users", "bob"]).expect("parse");
    assert_eq!(
        image.add_user(&useradd_request(&options)),
        Err(DbError::UnknownGroup)
    );
}
