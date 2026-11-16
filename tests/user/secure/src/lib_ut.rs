// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! U3e adversarial host matrix for the `su` and `passwd` transition decisions.
//!
//! The transitions are modelled with the same pure functions the tools call. Every negative case
//! asserts that the caller credential snapshot is unchanged as well as the expected error. The
//! real setuid path, console prompts and saved-id drop are covered by the `user-secure` qtest.

use std::collections::BTreeMap;
use std::sync::{Arc, Barrier, Mutex};
use std::thread;

use raven_abi::errno::Errno;
use raven_userdb::store::{DbIo, Paths, Transaction, load};
use raven_userdb::{Credential, Database, DbError, PasswdEntry};

const PASSWD: &str = include_str!("../../../fixtures/user-management/passwd");
const GROUP: &str = include_str!("../../../fixtures/user-management/group");
const SHADOW: &str = include_str!("../../../fixtures/user-management/shadow");

fn fixture() -> Database {
    Database::parse(PASSWD, GROUP, SHADOW).expect("canonical fixture")
}

fn alice_hash(database: &Database) -> String {
    database
        .shadow_by_name("alice")
        .expect("alice shadow")
        .hash
        .clone()
}

fn install_alice_hash(database: &mut Database, password: &str) -> String {
    let hash = raven_auth::hash_password(password, b"u3e-salt").expect("hash");
    database
        .set_password("alice", &hash, 19_100)
        .expect("set alice");
    hash
}

/// The credential plan a successful `su` installs. All three user ids and all three group ids
/// become the target ids; no saved root id survives.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Transition {
    uid: u32,
    euid: u32,
    suid: u32,
    gid: u32,
    egid: u32,
    sgid: u32,
    groups: Vec<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum SuFailure {
    NoSuchUser,
    NoShadow,
    Locked,
    BadPassword,
    Groups,
}

/// Model the authentication and plan steps in `su`'s `run`. The caller is mutated only after
/// every check succeeds, so a failed attempt must leave it byte-for-byte unchanged.
fn attempt_su(
    database: &Database,
    caller: &mut Credential,
    target: &str,
    password: &str,
) -> Result<Transition, SuFailure> {
    let user = database.user(target).ok_or(SuFailure::NoSuchUser)?.clone();
    if su::needs_password(caller.uid) {
        let shadow = database
            .shadow_by_name(&user.name)
            .ok_or(SuFailure::NoShadow)?;
        if !su::verify_target_password(shadow, password) {
            return Err(if shadow.is_locked() {
                SuFailure::Locked
            } else {
                SuFailure::BadPassword
            });
        }
    }
    let groups = database
        .groups_for_user(&user)
        .map_err(|_| SuFailure::Groups)?;
    let supplementary = su::supplementary_groups(&groups, user.gid);
    let plan = Transition {
        uid: user.uid,
        euid: user.uid,
        suid: user.uid,
        gid: user.gid,
        egid: user.gid,
        sgid: user.gid,
        groups: supplementary,
    };
    caller.uid = plan.uid;
    caller.euid = plan.euid;
    caller.gid = plan.gid;
    caller.egid = plan.egid;
    caller.groups = plan.groups.clone();
    Ok(plan)
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum PasswdFailure {
    NoSuchUser,
    Denied,
    NoShadow,
    BadOldPassword,
    Policy(passwd::PasswordError),
    Hash,
    Db(DbError),
}

/// Model `passwd`'s old-password check and update. The database is mutated only after the caller,
/// target, old password and new-password policy all pass.
fn attempt_passwd(
    database: &mut Database,
    caller_uid: u32,
    target: &str,
    old_password: &str,
    new_password: &str,
) -> Result<(), PasswdFailure> {
    let target = database
        .user(target)
        .ok_or(PasswdFailure::NoSuchUser)?
        .clone();
    if !passwd::target_allowed(caller_uid, target.uid) {
        return Err(PasswdFailure::Denied);
    }
    if passwd::requires_old_password(caller_uid) {
        let shadow = database
            .shadow_by_name(&target.name)
            .ok_or(PasswdFailure::NoShadow)?;
        if !passwd::verify_old_password(&shadow.hash, old_password) {
            return Err(PasswdFailure::BadOldPassword);
        }
    }
    passwd::validate_password(new_password, caller_uid, target.uid)
        .map_err(PasswdFailure::Policy)?;
    let hash =
        raven_auth::hash_password(new_password, b"u3e-new").map_err(|_| PasswdFailure::Hash)?;
    database
        .set_password(&target.name, &hash, 19_101)
        .map_err(PasswdFailure::Db)
}

fn caller_1000() -> Credential {
    Credential {
        uid: 1000,
        euid: 1000,
        gid: 1000,
        egid: 1000,
        groups: vec![0, 1000],
    }
}

#[test]
fn su_wrong_password_locked_missing_shadow_and_unknown_user_leave_the_caller_unchanged() {
    let mut database = fixture();
    install_alice_hash(&mut database, "correct-horse");
    let caller = caller_1000();
    let mut attempt = caller.clone();

    assert_eq!(
        attempt_su(&database, &mut attempt, "alice", "wrong"),
        Err(SuFailure::BadPassword)
    );
    assert_eq!(attempt, caller);

    assert_eq!(
        attempt_su(&database, &mut attempt, "ghost", "anything"),
        Err(SuFailure::NoSuchUser)
    );
    assert_eq!(attempt, caller);

    // `nobody` exists in passwd but deliberately has no shadow record.
    assert_eq!(
        attempt_su(&database, &mut attempt, "nobody", "anything"),
        Err(SuFailure::NoShadow)
    );
    assert_eq!(attempt, caller);

    // `bob` has the canonical locked `!` hash.
    assert_eq!(
        attempt_su(&database, &mut attempt, "bob", "anything"),
        Err(SuFailure::Locked)
    );
    assert_eq!(attempt, caller);

    // Locking a valid hash still never authenticates, even with the correct password.
    let mut locked = fixture();
    install_alice_hash(&mut locked, "correct-horse");
    locked.lock_password("alice").expect("lock alice");
    assert_eq!(
        attempt_su(&locked, &mut attempt, "alice", "correct-horse"),
        Err(SuFailure::Locked)
    );
    assert_eq!(attempt, caller);

    let plan = attempt_su(&database, &mut attempt, "alice", "correct-horse").expect("authenticate");
    assert_eq!(plan.uid, 1000);
    assert_eq!(plan.euid, 1000);
    assert_eq!(plan.suid, 1000);
    assert_eq!(plan.gid, 1000);
    assert_eq!(plan.egid, 1000);
    assert_eq!(plan.sgid, 1000);
    assert_eq!(plan.groups, Vec::<u32>::new());
    assert_eq!(attempt.uid, 1000);
    assert_eq!(attempt.euid, 1000);
    assert_eq!(attempt.gid, 1000);
    assert_eq!(attempt.egid, 1000);
    assert_eq!(attempt.groups, Vec::<u32>::new());
}

#[test]
fn su_root_bypasses_the_password_but_still_drops_every_saved_id() {
    let database = fixture();
    let mut root = Credential::root();
    let plan = attempt_su(&database, &mut root, "alice", "not-consulted").expect("root su");

    assert_eq!(plan.uid, 1000);
    assert_eq!(plan.suid, 1000);
    assert_eq!(plan.gid, 1000);
    assert_eq!(plan.sgid, 1000);
    assert_eq!(root.uid, 1000);
    assert_eq!(root.euid, 1000);
    assert_eq!(root.gid, 1000);
    assert_eq!(root.egid, 1000);
}

#[test]
fn su_environment_is_rebuilt_and_a_missing_shell_uses_the_fallback() {
    let database = fixture();
    let alice = database.user("alice").expect("alice");

    assert_eq!(
        su::environment(alice, false),
        vec![
            "HOME=/home/alice".to_string(),
            "SHELL=/bin/brush".to_string(),
            "USER=alice".to_string(),
            "LOGNAME=alice".to_string(),
            "PATH=/bin:/usr/bin:/sbin:/usr/sbin".to_string(),
            "PWD=/".to_string(),
        ]
    );
    assert_eq!(
        su::environment(alice, true),
        vec![
            "HOME=/home/alice".to_string(),
            "SHELL=/bin/brush".to_string(),
            "USER=alice".to_string(),
            "LOGNAME=alice".to_string(),
            "PATH=/bin:/usr/bin:/sbin:/usr/sbin".to_string(),
            "PWD=/home/alice".to_string(),
        ]
    );

    // An empty shell and home are the passwd representation of "not configured".
    let mut incomplete = PasswdEntry {
        name: "incomplete".to_string(),
        passwd: "x".to_string(),
        uid: 1003,
        gid: 1003,
        gecos: String::new(),
        home: String::new(),
        shell: String::new(),
    };
    assert_eq!(su::shell_path(&incomplete), su::DEFAULT_SHELL);
    let environment = su::environment(&incomplete, true);
    assert!(environment.contains(&format!("SHELL={}", su::DEFAULT_SHELL)));
    assert!(environment.contains(&"HOME=/".to_string()));
    assert!(environment.contains(&"PWD=/".to_string()));

    incomplete.shell = "/bin/brush".to_string();
    assert_eq!(su::shell_path(&incomplete), "/bin/brush");

    // The caller environment is not an input to the fixed list; only TERM is appended by main.
    let mut built = su::environment(alice, false);
    built.push("TERM=vt220".to_string());
    for hostile in [
        "PATH=/evil",
        "HOME=/evil",
        "SHELL=/evil",
        "USER=evil",
        "LOGNAME=evil",
        "LD_PRELOAD=/evil.so",
        "IFS=:",
    ] {
        assert!(
            !built.iter().any(|entry| entry == hostile),
            "caller entry leaked: {hostile}"
        );
    }
    for key in ["HOME", "SHELL", "USER", "LOGNAME", "PATH", "PWD", "TERM"] {
        assert_eq!(
            built
                .iter()
                .filter(|entry| entry.starts_with(&format!("{key}=")))
                .count(),
            1,
            "duplicate or missing {key}"
        );
    }
    assert_eq!(
        su::shell_arguments(&su::parse_args(["-c", "id"]).expect("parse")),
        vec!["-c", "id"]
    );
}

#[test]
fn passwd_old_password_mismatch_and_foreign_target_leave_the_database_unchanged() {
    let mut database = fixture();
    install_alice_hash(&mut database, "old-secret");
    let passwd_before = database.render_passwd();
    let group_before = database.render_group();
    let shadow_before = database.render_shadow();

    assert_eq!(
        attempt_passwd(&mut database, 1000, "alice", "wrong", "new-secret"),
        Err(PasswdFailure::BadOldPassword)
    );
    assert_eq!(database.render_passwd(), passwd_before);
    assert_eq!(database.render_group(), group_before);
    assert_eq!(database.render_shadow(), shadow_before);
    assert!(passwd::verify_old_password(
        &alice_hash(&database),
        "old-secret"
    ));

    // A non-root caller may not target another account, even with the right old password.
    assert_eq!(
        attempt_passwd(&mut database, 1000, "bob", "old-secret", "new-secret"),
        Err(PasswdFailure::Denied)
    );
    assert_eq!(
        attempt_passwd(&mut database, 1000, "ghost", "old-secret", "new-secret"),
        Err(PasswdFailure::NoSuchUser)
    );
    assert_eq!(database.render_shadow(), shadow_before);

    // A locked or missing hash never verifies.
    assert!(!passwd::verify_old_password("!", "old-secret"));
    assert!(!passwd::verify_old_password("", "old-secret"));
    assert!(!passwd::verify_old_password("*", "old-secret"));
    database.lock_password("alice").expect("lock alice");
    let locked_before = database.render_shadow();
    assert_eq!(
        attempt_passwd(&mut database, 1000, "alice", "old-secret", "new-secret"),
        Err(PasswdFailure::BadOldPassword)
    );
    assert_eq!(database.render_shadow(), locked_before);
    database.unlock_password("alice").expect("unlock alice");

    // The root minimum applies only to the root account; regular accounts need eight characters.
    assert_eq!(
        attempt_passwd(&mut database, 0, "alice", "", "short"),
        Err(PasswdFailure::Policy(passwd::PasswordError::TooShort))
    );
    assert!(passwd::validate_password("x", 0, 0).is_ok());
    assert!(passwd::validate_password("short", 0, 1000).is_err());
    assert!(passwd::validate_password("long-enough", 1000, 1000).is_ok());

    let before_success = database.render_shadow();
    assert_eq!(
        attempt_passwd(&mut database, 1000, "alice", "old-secret", "new-secret"),
        Ok(())
    );
    assert_ne!(database.render_shadow(), before_success);
    assert!(!passwd::verify_old_password(
        &alice_hash(&database),
        "old-secret"
    ));
    assert!(passwd::verify_old_password(
        &alice_hash(&database),
        "new-secret"
    ));
}

#[test]
fn su_group_list_is_derived_from_the_target_account_not_the_caller() {
    let mut database = fixture();
    let alice = database.user("alice").expect("alice");
    assert_eq!(database.groups_for_user(alice).expect("groups"), vec![1000]);
    assert_eq!(su::supplementary_groups(&[1000], 1000), Vec::<u32>::new());

    // The caller's unrelated wheel membership does not reach the target session: the plan comes
    // only from alice's own group records.
    let caller_groups = [0, 1000];
    database.add_member("users", "alice").expect("users");
    let alice = database.user("alice").expect("alice");
    let groups = database.groups_for_user(alice).expect("groups");
    assert_eq!(groups, vec![1000, 100]);
    let plan = su::supplementary_groups(&groups, alice.gid);
    assert_eq!(plan, vec![100]);
    assert!(!plan.iter().any(|gid| caller_groups.contains(gid)));

    database.add_member("wheel", "alice").expect("wheel");
    let alice = database.user("alice").expect("alice");
    let groups = database.groups_for_user(alice).expect("groups");
    assert_eq!(groups, vec![1000, 0, 100]);
    assert_eq!(su::supplementary_groups(&groups, alice.gid), vec![0, 100]);

    // The primary gid is removed and duplicates collapse, regardless of caller input.
    assert_eq!(
        su::supplementary_groups(&[1000, 1000, 100, 100, 0, 0], 1000),
        vec![100, 0]
    );
}

#[derive(Default)]
struct Store {
    files: BTreeMap<String, Vec<u8>>,
    modes: BTreeMap<String, u32>,
    pauses: usize,
}

/// A host `DbIo` that can be shared between threads. It models the exclusive lock file and the
/// same-directory rename steps of the real transaction.
#[derive(Clone, Default)]
struct SharedIo {
    store: Arc<Mutex<Store>>,
}

impl SharedIo {
    fn with_fixture() -> Self {
        let io = Self::default();
        {
            let mut store = io.store.lock().expect("store");
            store
                .files
                .insert("/etc/passwd".to_string(), PASSWD.as_bytes().to_vec());
            store
                .files
                .insert("/etc/group".to_string(), GROUP.as_bytes().to_vec());
            store
                .files
                .insert("/etc/shadow".to_string(), SHADOW.as_bytes().to_vec());
            store.modes.insert("/etc/passwd".to_string(), 0o644);
            store.modes.insert("/etc/group".to_string(), 0o644);
            store.modes.insert("/etc/shadow".to_string(), 0o600);
        }
        io
    }

    fn has_lock(&self) -> bool {
        self.store
            .lock()
            .expect("store")
            .files
            .contains_key("/etc/.pwd.lock")
    }
}

impl DbIo for SharedIo {
    fn read(&mut self, path: &str) -> Result<Vec<u8>, DbError> {
        self.store
            .lock()
            .expect("store")
            .files
            .get(path)
            .cloned()
            .ok_or(DbError::Io(Errno::ENOENT))
    }

    fn create_exclusive(&mut self, path: &str) -> Result<(), DbError> {
        let mut store = self.store.lock().expect("store");
        if store.files.contains_key(path) {
            return Err(DbError::Busy);
        }
        store.files.insert(path.to_string(), Vec::new());
        Ok(())
    }

    fn remove(&mut self, path: &str) -> Result<(), DbError> {
        self.store.lock().expect("store").files.remove(path);
        Ok(())
    }

    fn write_new(&mut self, path: &str, data: &[u8], mode: u32) -> Result<(), DbError> {
        let mut store = self.store.lock().expect("store");
        store.files.insert(path.to_string(), data.to_vec());
        store.modes.insert(path.to_string(), mode);
        Ok(())
    }

    fn chmod(&mut self, path: &str, mode: u32) -> Result<(), DbError> {
        self.store
            .lock()
            .expect("store")
            .modes
            .insert(path.to_string(), mode);
        Ok(())
    }

    fn sync(&mut self) -> Result<(), DbError> {
        Ok(())
    }

    fn rename(&mut self, from: &str, to: &str) -> Result<(), DbError> {
        let mut store = self.store.lock().expect("store");
        let data = store.files.remove(from).ok_or(DbError::Io(Errno::ENOENT))?;
        store.files.insert(to.to_string(), data);
        Ok(())
    }

    fn pause(&mut self) {
        self.store.lock().expect("store").pauses += 1;
        thread::yield_now();
    }
}

#[test]
fn concurrent_passwd_writers_serialize_and_do_not_lose_an_update() {
    let io = SharedIo::with_fixture();
    let first_hash = raven_auth::hash_password("writer-one", b"u3e-one").expect("hash one");
    let second_hash = raven_auth::hash_password("writer-two", b"u3e-two").expect("hash two");

    let first_started = Arc::new(Barrier::new(2));
    let second_started = Arc::new(Barrier::new(2));

    let first_io = io.clone();
    let first_barrier = Arc::clone(&first_started);
    let first_release = Arc::clone(&second_started);
    let first = thread::spawn(move || {
        let mut transaction = Transaction::begin(first_io, Paths::system()).expect("first begin");
        first_barrier.wait();
        first_release.wait();
        transaction
            .database_mut()
            .set_password("alice", &first_hash, 19_101)
            .expect("first set");
        transaction.commit().expect("first commit");
    });

    first_started.wait();
    let second_io = io.clone();
    let second_release = Arc::clone(&second_started);
    let second = thread::spawn(move || {
        let result = Transaction::begin(second_io, Paths::system()).map(|transaction| {
            transaction.abort();
        });
        second_release.wait();
        result
    });

    assert_eq!(second.join().expect("second thread"), Err(DbError::Busy));
    first.join().expect("first thread");
    assert!(!io.has_lock());
    assert!(
        io.store.lock().expect("store").pauses > 0,
        "the losing writer must have retried the lock"
    );

    // A retry after the winner commits observes the new hash and can apply the next update.
    let mut transaction = Transaction::begin(io.clone(), Paths::system()).expect("retry begin");
    transaction
        .database_mut()
        .set_password("alice", &second_hash, 19_102)
        .expect("second set");
    transaction.commit().expect("second commit");

    let mut reader = io.clone();
    let database = load(&mut reader, &Paths::system()).expect("load");
    let alice = database.shadow_by_name("alice").expect("alice");
    assert_eq!(alice.hash, second_hash);
    assert_eq!(alice.last_change, 19_102);
    assert!(!raven_auth::verify_password("writer-one", &alice.hash));
    assert!(raven_auth::verify_password("writer-two", &alice.hash));
    assert!(!io.has_lock());
}
