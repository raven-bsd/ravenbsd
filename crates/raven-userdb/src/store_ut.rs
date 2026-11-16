// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the locked transaction and its failure paths.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use super::*;
use crate::db::AddGroup;

const PASSWD: &str = include_str!("../../../tests/fixtures/user-management/passwd");
const GROUP: &str = include_str!("../../../tests/fixtures/user-management/group");
const SHADOW: &str = include_str!("../../../tests/fixtures/user-management/shadow");
const STAFF_GROUP: &str = include_str!("../../../tests/user/groupadd-staff.group");

#[derive(Default)]
struct Store {
    files: BTreeMap<String, Vec<u8>>,
    modes: BTreeMap<String, u32>,
    fail_rename_to: Option<String>,
    pauses: usize,
}

#[derive(Clone, Default)]
struct MemIo {
    store: Rc<RefCell<Store>>,
}

impl MemIo {
    fn with_fixture() -> Self {
        let io = Self::default();
        {
            let mut store = io.store.borrow_mut();
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

    fn file(&self, path: &str) -> Option<String> {
        self.store
            .borrow()
            .files
            .get(path)
            .map(|data| String::from_utf8(data.clone()).expect("utf8"))
    }

    fn has(&self, path: &str) -> bool {
        self.store.borrow().files.contains_key(path)
    }

    fn fail_rename(&self, path: &str) {
        self.store.borrow_mut().fail_rename_to = Some(path.to_string());
    }

    fn pauses(&self) -> usize {
        self.store.borrow().pauses
    }
}

impl DbIo for MemIo {
    fn read(&mut self, path: &str) -> Result<Vec<u8>, DbError> {
        self.store
            .borrow()
            .files
            .get(path)
            .cloned()
            .ok_or(DbError::Io(Errno::ENOENT))
    }

    fn create_exclusive(&mut self, path: &str) -> Result<(), DbError> {
        let mut store = self.store.borrow_mut();
        if store.files.contains_key(path) {
            return Err(DbError::Busy);
        }
        store.files.insert(path.to_string(), Vec::new());
        Ok(())
    }

    fn remove(&mut self, path: &str) -> Result<(), DbError> {
        self.store.borrow_mut().files.remove(path);
        Ok(())
    }

    fn write_new(&mut self, path: &str, data: &[u8], mode: u32) -> Result<(), DbError> {
        let mut store = self.store.borrow_mut();
        store.files.insert(path.to_string(), data.to_vec());
        store.modes.insert(path.to_string(), mode);
        Ok(())
    }

    fn chmod(&mut self, path: &str, mode: u32) -> Result<(), DbError> {
        self.store.borrow_mut().modes.insert(path.to_string(), mode);
        Ok(())
    }

    fn sync(&mut self) -> Result<(), DbError> {
        Ok(())
    }

    fn rename(&mut self, from: &str, to: &str) -> Result<(), DbError> {
        let mut store = self.store.borrow_mut();
        if store.fail_rename_to.as_deref() == Some(to) {
            return Err(DbError::Io(Errno::EIO));
        }
        let data = store.files.remove(from).ok_or(DbError::Io(Errno::ENOENT))?;
        store.files.insert(to.to_string(), data);
        Ok(())
    }

    fn pause(&mut self) {
        self.store.borrow_mut().pauses += 1;
    }
}

#[test]
fn a_commit_writes_all_three_files_and_releases_the_lock() {
    let io = MemIo::with_fixture();
    let mut transaction = Transaction::begin(io.clone(), Paths::system()).expect("begin");
    transaction
        .database_mut()
        .add_group(&AddGroup {
            name: "staff",
            gid: None,
            system: false,
        })
        .expect("add group");
    transaction.commit().expect("commit");
    assert_eq!(io.file("/etc/group").as_deref(), Some(STAFF_GROUP));
    assert_eq!(io.file("/etc/passwd").as_deref(), Some(PASSWD));
    assert_eq!(io.file("/etc/shadow").as_deref(), Some(SHADOW));
    assert!(!io.has("/etc/.pwd.lock"));
    assert!(!io.has("/etc/passwd.tmp"));
    assert!(!io.has("/etc/group.tmp"));
    assert!(!io.has("/etc/shadow.tmp"));
    let modes = &io.store.borrow().modes;
    assert_eq!(modes.get("/etc/passwd.tmp"), Some(&0o644));
    assert_eq!(modes.get("/etc/group.tmp"), Some(&0o644));
    assert_eq!(modes.get("/etc/shadow.tmp"), Some(&0o600));
}

#[test]
fn an_existing_lock_is_busy_after_the_bounded_wait() {
    let io = MemIo::with_fixture();
    io.store
        .borrow_mut()
        .files
        .insert("/etc/.pwd.lock".to_string(), Vec::new());
    assert_eq!(
        Transaction::begin(io.clone(), Paths::system()).err(),
        Some(DbError::Busy)
    );
    assert_eq!(io.pauses(), LOCK_ATTEMPTS);
}

#[test]
fn abort_releases_the_lock_without_writing() {
    let io = MemIo::with_fixture();
    let transaction = Transaction::begin(io.clone(), Paths::system()).expect("begin");
    transaction.abort();
    assert!(!io.has("/etc/.pwd.lock"));
    assert_eq!(io.file("/etc/group").as_deref(), Some(GROUP));
}

#[test]
fn a_parse_error_releases_the_lock() {
    let io = MemIo::with_fixture();
    io.store
        .borrow_mut()
        .files
        .insert("/etc/passwd".to_string(), b"broken\n".to_vec());
    assert!(Transaction::begin(io.clone(), Paths::system()).is_err());
    assert!(!io.has("/etc/.pwd.lock"));
}

#[test]
fn a_shadow_rename_failure_restores_passwd_and_group() {
    let io = MemIo::with_fixture();
    io.fail_rename("/etc/shadow");
    let mut transaction = Transaction::begin(io.clone(), Paths::system()).expect("begin");
    transaction
        .database_mut()
        .add_group(&AddGroup {
            name: "staff",
            gid: None,
            system: false,
        })
        .expect("add group");
    let error = transaction.commit().expect_err("commit fails");
    assert_eq!(error, DbError::Io(Errno::EIO));
    assert_eq!(io.file("/etc/passwd").as_deref(), Some(PASSWD));
    assert_eq!(io.file("/etc/group").as_deref(), Some(GROUP));
    assert_eq!(io.file("/etc/shadow").as_deref(), Some(SHADOW));
    assert!(!io.has("/etc/.pwd.lock"));
    assert!(!io.has("/etc/passwd.tmp"));
    assert!(!io.has("/etc/group.tmp"));
    assert!(!io.has("/etc/shadow.tmp"));
}

#[test]
fn a_validation_error_does_not_touch_the_files() {
    let io = MemIo::with_fixture();
    let mut transaction = Transaction::begin(io.clone(), Paths::system()).expect("begin");
    transaction
        .database_mut()
        .add_group(&AddGroup {
            name: "wheel",
            gid: None,
            system: false,
        })
        .expect_err("duplicate");
    transaction.commit().expect("unchanged commit");
    assert_eq!(io.file("/etc/group").as_deref(), Some(GROUP));
}

#[test]
fn lookup_loading_reads_only_passwd_and_group() {
    let io = MemIo::with_fixture();
    io.store.borrow_mut().files.remove("/etc/shadow");
    let database = load_lookup(&mut io.clone(), &Paths::system()).expect("lookup");
    assert_eq!(database.user("alice").expect("alice").uid, 1000);
    assert!(load(&mut io.clone(), &Paths::system()).is_err());
}

#[test]
fn non_utf8_content_is_rejected() {
    let io = MemIo::with_fixture();
    io.store
        .borrow_mut()
        .files
        .insert("/etc/passwd".to_string(), vec![0xFF, 0xFE]);
    assert_eq!(
        load_lookup(&mut io.clone(), &Paths::system()).expect_err("utf8"),
        DbError::Io(Errno::EILSEQ)
    );
}
