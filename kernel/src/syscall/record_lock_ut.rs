// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the record-lock table (range resolution, conflicts, splitting).

use super::*;

fn lock(kind: i16, start: i64, len: i64, whence: i16) -> RecordLock {
    RecordLock {
        l_type: kind,
        l_whence: whence,
        l_start: start,
        l_len: len,
        l_pid: 0,
    }
}

fn entry(owner: u64, kind: i16, start: u64, end: u64) -> Entry {
    Entry {
        owner,
        server: 7,
        ino: 11,
        kind,
        range: Range { start, end },
    }
}

#[test]
fn resolves_set_end_and_open_ended_ranges() {
    assert_eq!(
        resolve_range(&lock(WRLCK, 10, 5, SEEK_SET), 100).expect("set"),
        Range { start: 10, end: 14 }
    );
    assert_eq!(
        resolve_range(&lock(RDLCK, 0, 0, SEEK_SET), 100).expect("to eof"),
        Range {
            start: 0,
            end: u64::MAX
        }
    );
    assert_eq!(
        resolve_range(&lock(WRLCK, -5, 5, SEEK_END), 100).expect("end"),
        Range { start: 95, end: 99 }
    );
    assert!(resolve_range(&lock(WRLCK, 0, 1, 1), 100).is_err());
    assert!(resolve_range(&lock(WRLCK, -1, 1, SEEK_SET), 100).is_err());
    assert!(resolve_range(&lock(WRLCK, 0, -1, SEEK_SET), 100).is_err());
}

#[test]
fn read_locks_are_compatible_and_write_locks_are_not() {
    let mut table = Table::new();
    table.set(entry(1, RDLCK, 0, 9));
    assert!(
        table
            .conflict(2, 7, 11, RDLCK, Range { start: 5, end: 6 })
            .is_none()
    );
    assert!(
        table
            .conflict(2, 7, 11, WRLCK, Range { start: 5, end: 6 })
            .is_some()
    );
    table.set(entry(3, WRLCK, 20, 29));
    assert!(
        table
            .conflict(4, 7, 11, RDLCK, Range { start: 25, end: 25 })
            .is_some()
    );
}

#[test]
fn same_owner_and_other_files_do_not_conflict() {
    let mut table = Table::new();
    table.set(entry(1, WRLCK, 0, 9));
    assert!(
        table
            .conflict(1, 7, 11, WRLCK, Range { start: 0, end: 9 })
            .is_none()
    );
    assert!(
        table
            .conflict(2, 8, 11, WRLCK, Range { start: 0, end: 9 })
            .is_none()
    );
    assert!(
        table
            .conflict(2, 7, 12, WRLCK, Range { start: 0, end: 9 })
            .is_none()
    );
}

#[test]
fn conflicting_owner_is_the_lowest_start() {
    let mut table = Table::new();
    table.set(entry(5, WRLCK, 50, 59));
    table.set(entry(3, RDLCK, 10, 19));
    let found = table
        .conflict(1, 7, 11, WRLCK, Range { start: 0, end: 99 })
        .expect("conflict");
    assert_eq!(found.owner, 3);
    assert_eq!(found.range, Range { start: 10, end: 19 });
}

#[test]
fn set_replaces_the_owner_range() {
    let mut table = Table::new();
    table.set(entry(1, RDLCK, 0, 99));
    table.set(entry(1, WRLCK, 40, 59));
    let ranges = table
        .entries
        .iter()
        .map(|entry| (entry.kind, entry.range))
        .collect::<Vec<_>>();
    assert_eq!(
        ranges,
        vec![
            (RDLCK, Range { start: 0, end: 39 }),
            (WRLCK, Range { start: 40, end: 59 }),
            (RDLCK, Range { start: 60, end: 99 }),
        ]
    );
}

#[test]
fn unlock_splits_and_removes() {
    let mut table = Table::new();
    table.set(entry(1, WRLCK, 0, 99));
    table.unlock(1, 7, 11, Range { start: 40, end: 59 });
    let ranges = table
        .entries
        .iter()
        .map(|entry| entry.range)
        .collect::<Vec<_>>();
    assert_eq!(
        ranges,
        vec![Range { start: 0, end: 39 }, Range { start: 60, end: 99 },]
    );
    table.unlock(1, 7, 11, Range { start: 0, end: 99 });
    assert!(table.entries.is_empty());
}

#[test]
fn close_and_exit_release_by_key() {
    let mut table = Table::new();
    table.set(entry(1, WRLCK, 0, 9));
    table.set(Entry {
        owner: 1,
        server: 7,
        ino: 12,
        kind: WRLCK,
        range: Range { start: 0, end: 9 },
    });
    table.set(entry(2, WRLCK, 0, 9));
    assert!(table.has_owner(1));
    table.remove_file(1, 7, 11);
    assert!(table.has_owner(1));
    table.remove_process(1);
    assert!(!table.has_owner(1));
    assert!(table.has_owner(2));
}
