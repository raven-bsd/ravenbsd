// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the `at`/`batch` queue model.

use alloc::collections::BTreeMap;

use super::*;

#[derive(Default)]
struct MemSpool {
    files: BTreeMap<String, String>,
}

#[derive(Default)]
struct NoUnlinkSpool {
    files: BTreeMap<String, String>,
}

impl Spool for NoUnlinkSpool {
    fn names(&mut self) -> Result<Vec<String>, String> {
        Ok(self.files.keys().cloned().collect())
    }

    fn read(&mut self, id: &str) -> Result<String, String> {
        self.files
            .get(id)
            .cloned()
            .ok_or_else(|| format!("no such job `{id}`"))
    }

    fn write_atomic(&mut self, id: &str, text: &str) -> Result<(), String> {
        self.files.insert(id.to_string(), text.to_string());
        Ok(())
    }

    fn remove(&mut self, _id: &str) -> Result<(), String> {
        Err("unlink unsupported".to_string())
    }
}

impl Spool for MemSpool {
    fn names(&mut self) -> Result<Vec<String>, String> {
        Ok(self.files.keys().cloned().collect())
    }

    fn read(&mut self, id: &str) -> Result<String, String> {
        self.files
            .get(id)
            .cloned()
            .ok_or_else(|| format!("no such job `{id}`"))
    }

    fn write_atomic(&mut self, id: &str, text: &str) -> Result<(), String> {
        self.files.insert(id.to_string(), text.to_string());
        Ok(())
    }

    fn remove(&mut self, id: &str) -> Result<(), String> {
        self.files.remove(id);
        Ok(())
    }
}

const NOW: i64 = 1_794_994_200; // 2026-11-18 09:30:00 UTC

#[test]
fn civil_conversion_matches_known_epochs() {
    assert_eq!(
        civil_from_unix(0),
        Civil {
            year: 1970,
            month: 1,
            day: 1,
            hour: 0,
            minute: 0,
        }
    );
    let leap = civil_from_unix(1_709_164_800);
    assert_eq!((leap.year, leap.month, leap.day), (2024, 2, 29));
    assert_eq!(unix_from_civil(1970, 1, 1, 0, 0), 0);
    assert_eq!(unix_from_civil(2024, 2, 29, 0, 0), 1_709_164_800);
    assert_eq!(unix_from_civil(2026, 11, 18, 9, 30), NOW);
}

#[test]
fn parses_now_and_offsets() {
    assert_eq!(parse_time(NOW, "now").expect("now"), NOW);
    assert_eq!(
        parse_time(NOW, "now + 5 minutes").expect("minutes"),
        NOW + 300
    );
    assert_eq!(parse_time(NOW, "+ 2 hours").expect("hours"), NOW + 7200);
    assert_eq!(parse_time(NOW, "now + 1 day").expect("day"), NOW + 86_400);
    assert!(parse_time(NOW, "now + 5 weeks").is_err());
    assert!(parse_time(NOW, "now - 5 minutes").is_err());
}

#[test]
fn parses_clock_as_the_next_occurrence() {
    assert_eq!(parse_time(NOW, "10:00").expect("later today"), NOW + 1800);
    let next = parse_time(NOW, "09:30").expect("already passed");
    assert_eq!(next, unix_from_civil(2026, 11, 19, 9, 30));
    let midnight = parse_time(NOW, "00:00").expect("next midnight");
    assert_eq!(midnight, unix_from_civil(2026, 11, 19, 0, 0));
    assert!(parse_time(NOW, "24:00").is_err());
    assert!(parse_time(NOW, "12:60").is_err());
}

#[test]
fn parses_iso_date_forms() {
    assert_eq!(parse_time(NOW, "2026-11-18 09:30").expect("space"), NOW);
    assert_eq!(parse_time(NOW, "2026-11-18T10:15").expect("T"), NOW + 2700);
    assert_eq!(
        parse_time(NOW, "2026-12-01 00:00").expect("future"),
        unix_from_civil(2026, 12, 1, 0, 0)
    );
    assert!(parse_time(NOW, "2026-11-17 09:30").is_err());
    assert!(parse_time(NOW, "2026-02-30 09:30").is_err());
    assert!(parse_time(NOW, "noon").is_err());
}

#[test]
fn commands_must_be_single_simple_lines() {
    assert_eq!(parse_command(" echo hi ").expect("simple"), "echo hi");
    assert!(parse_command("").is_err());
    assert!(parse_command("echo one\necho two").is_err());
    assert!(parse_command("echo hi > file").is_err());
    assert!(parse_command("echo hi | cat").is_err());
    assert!(parse_command("echo \"hi\"").is_err());
}

#[test]
fn submits_lists_and_removes_jobs() {
    let mut spool = MemSpool::default();
    let first = submit(&mut spool, NOW, "now + 5 minutes", "echo first", false).expect("first");
    assert_eq!(first.id, format!("{}.1", NOW + 300));
    assert_eq!(first.due, NOW + 300);
    let second = submit(&mut spool, NOW, "now + 1 minutes", "echo second", true).expect("second");
    assert_eq!(second.id, format!("{}.2", NOW + 60));
    let jobs = list(&mut spool).expect("list");
    assert_eq!(jobs.len(), 2);
    assert_eq!(jobs[0].command, "echo second");
    remove(&mut spool, &first.id).expect("remove");
    assert!(remove(&mut spool, &first.id).is_err());
    assert_eq!(list(&mut spool).expect("list").len(), 1);
}

#[test]
fn invalid_submission_does_not_touch_the_spool() {
    let mut spool = MemSpool::default();
    submit(&mut spool, NOW, "now", "echo kept", false).expect("submit");
    assert!(submit(&mut spool, NOW, "not-a-time", "echo bad", false).is_err());
    let empty = submit(&mut spool, NOW, "now", "   ", false);
    assert!(empty.is_err());
    let jobs = list(&mut spool).expect("list");
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].command, "echo kept");
}

#[test]
fn due_queues_separate_regular_and_batch_jobs() {
    let jobs = vec![
        Job {
            id: "1.1".to_string(),
            due: NOW - 1,
            batch: false,
            command: "echo regular".to_string(),
        },
        Job {
            id: "2.1".to_string(),
            due: NOW,
            batch: true,
            command: "echo batch".to_string(),
        },
        Job {
            id: "3.1".to_string(),
            due: NOW + 10,
            batch: false,
            command: "echo future".to_string(),
        },
    ];
    let (regular, batch) = due_queues(&jobs, NOW);
    assert_eq!(regular.len(), 1);
    assert_eq!(batch.len(), 1);
    assert_eq!(batch[0].command, "echo batch");
}

#[test]
fn at_arguments_are_parsed() {
    assert_eq!(
        parse_at_args(&["-l".to_string()]).expect("list"),
        AtAction::List
    );
    assert_eq!(
        parse_at_args(&["-r".to_string(), "1.1".to_string()]).expect("remove"),
        AtAction::Remove(vec!["1.1".to_string()])
    );
    assert_eq!(
        parse_at_args(&[
            "now + 1 minutes".to_string(),
            "echo".to_string(),
            "hi".to_string()
        ])
        .expect("schedule"),
        AtAction::Schedule {
            when: "now + 1 minutes".to_string(),
            command: Some("echo hi".to_string()),
            file: None,
        }
    );
    assert!(parse_at_args(&[]).is_err());
    assert!(parse_at_args(&["-l".to_string(), "-r".to_string()]).is_err());
    assert!(parse_at_args(&["-r".to_string()]).is_err());
    assert!(
        parse_at_args(&[
            "-f".to_string(),
            "script".to_string(),
            "now".to_string(),
            "echo".to_string()
        ])
        .is_err()
    );
    assert_eq!(
        parse_at_args(&[
            "now".to_string(),
            "+".to_string(),
            "5".to_string(),
            "minutes".to_string(),
            "echo".to_string(),
            "hi".to_string()
        ])
        .expect("unquoted relative"),
        AtAction::Schedule {
            when: "now + 5 minutes".to_string(),
            command: Some("echo hi".to_string()),
            file: None,
        }
    );
    assert_eq!(
        parse_at_args(&[
            "+".to_string(),
            "2".to_string(),
            "hours".to_string(),
            "echo".to_string(),
            "hi".to_string()
        ])
        .expect("unquoted shorthand"),
        AtAction::Schedule {
            when: "+ 2 hours".to_string(),
            command: Some("echo hi".to_string()),
            file: None,
        }
    );
    assert_eq!(
        parse_at_args(&[
            "2030-01-01".to_string(),
            "09:30".to_string(),
            "echo".to_string(),
            "dated".to_string()
        ])
        .expect("unquoted date"),
        AtAction::Schedule {
            when: "2030-01-01 09:30".to_string(),
            command: Some("echo dated".to_string()),
            file: None,
        }
    );
}

#[test]
fn unlink_failure_retires_the_job() {
    let mut spool = NoUnlinkSpool::default();
    let job = submit(&mut spool, NOW, "now", "echo retired", false).expect("submit");
    remove(&mut spool, &job.id).expect("retire fallback");
    assert!(list(&mut spool).expect("list").is_empty());
    assert!(spool.names().expect("names").contains(&job.id));
}

#[test]
fn batch_arguments_are_parsed() {
    assert_eq!(parse_batch_args(&[]).expect("stdin"), None);
    assert_eq!(
        parse_batch_args(&["echo".to_string(), "hi".to_string()]).expect("words"),
        Some("echo hi".to_string())
    );
    assert!(parse_batch_args(&["-q".to_string()]).is_err());
}
