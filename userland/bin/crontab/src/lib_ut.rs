// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for crontab parsing and spool operations.

use alloc::collections::BTreeMap;

use super::*;

#[derive(Default)]
struct MemSpool {
    files: BTreeMap<String, String>,
}

impl Spool for MemSpool {
    fn read(&mut self, path: &str) -> Result<String, String> {
        self.files
            .get(path)
            .cloned()
            .ok_or_else(|| "no crontab for this user".to_string())
    }

    fn write_atomic(&mut self, path: &str, text: &str) -> Result<(), String> {
        self.files.insert(path.to_string(), text.to_string());
        Ok(())
    }

    fn remove(&mut self, path: &str) -> Result<(), String> {
        self.files.remove(path);
        Ok(())
    }

    fn exists(&mut self, path: &str) -> bool {
        self.files.contains_key(path)
    }
}

const SAMPLE: &str = "\
# minute hour day month weekday command
*/15 1-5 1,15 * 1-5 echo hello
0 0 * * 0 echo sunday
";

#[test]
fn parses_the_five_fields_and_command() {
    let entries = parse(SAMPLE).expect("parse");
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].minute, "*/15");
    assert_eq!(entries[0].command, "echo hello");
    assert_eq!(entries[1].weekday, "0");
}

#[test]
fn rejects_invalid_fields_and_at_reboot() {
    assert!(parse("* * * * echo missing\n").is_err());
    assert!(parse("60 * * * * echo bad\n").is_err());
    assert!(parse("* * * 13 * echo bad\n").is_err());
    assert!(parse("@reboot echo boot\n").is_err());
    assert!(parse("*/0 * * * * echo bad\n").is_err());
}

#[test]
fn field_matching_supports_lists_ranges_and_steps() {
    assert!(field_matches("*/15", 30, 0, 59));
    assert!(!field_matches("*/15", 31, 0, 59));
    assert!(field_matches("1,5,9", 5, 0, 59));
    assert!(field_matches("1-5", 3, 0, 59));
    assert!(!field_matches("1-5", 6, 0, 59));
    assert!(field_matches("1-9/2", 5, 0, 59));
}

#[test]
fn weekday_matches_sunday_as_zero_or_seven() {
    let entry = Entry {
        minute: "0".to_string(),
        hour: "0".to_string(),
        day: "*".to_string(),
        month: "*".to_string(),
        weekday: "7".to_string(),
        command: "echo sunday".to_string(),
    };
    assert!(entry_matches(&entry, 0, 0, 1, 1, 0));
    let entry = Entry {
        weekday: "0".to_string(),
        ..entry
    };
    assert!(entry_matches(&entry, 0, 0, 1, 1, 0));
}

#[test]
fn day_and_weekday_are_ored_when_both_restricted() {
    let entry = Entry {
        minute: "0".to_string(),
        hour: "0".to_string(),
        day: "15".to_string(),
        month: "*".to_string(),
        weekday: "1".to_string(),
        command: "echo either".to_string(),
    };
    assert!(entry_matches(&entry, 0, 0, 15, 6, 3));
    assert!(entry_matches(&entry, 0, 0, 14, 6, 1));
    assert!(!entry_matches(&entry, 0, 0, 14, 6, 3));
}

#[test]
fn civil_conversion_handles_the_epoch_and_leap_days() {
    assert_eq!(
        civil_from_unix(0),
        Civil {
            year: 1970,
            month: 1,
            day: 1,
            hour: 0,
            minute: 0,
            weekday: 4,
        }
    );
    let leap = civil_from_unix(1_709_164_800); // 2024-02-29 00:00 UTC
    assert_eq!(
        (leap.year, leap.month, leap.day, leap.weekday),
        (2024, 2, 29, 4)
    );
    let minute = civil_from_unix(1_700_000_060);
    assert_eq!(minute.minute, 14);
}

#[test]
fn options_are_parsed() {
    assert_eq!(action(&["-l".to_string()]).expect("list"), Action::List);
    assert_eq!(action(&["-r".to_string()]).expect("remove"), Action::Remove);
    assert_eq!(
        action(&["file".to_string()]).expect("install"),
        Action::Install("file".to_string())
    );
    assert_eq!(
        action(&[]).expect("stdin"),
        Action::Install("-".to_string())
    );
    assert!(action(&["-l".to_string(), "-r".to_string()]).is_err());
    assert!(action(&["-l".to_string(), "file".to_string()]).is_err());
}

#[test]
fn install_list_and_remove_use_the_spool() {
    let mut spool = MemSpool::default();
    let path = "/var/cron/root";
    install(&mut spool, path, SAMPLE).expect("install");
    assert_eq!(list(&mut spool, path).expect("list"), SAMPLE);
    remove(&mut spool, path).expect("remove");
    assert!(!spool.exists(path));
    assert!(list(&mut spool, path).is_err());
    assert!(remove(&mut spool, path).is_err());
}

#[test]
fn invalid_install_leaves_the_spool_untouched() {
    let mut spool = MemSpool::default();
    let path = "/var/cron/root";
    install(&mut spool, path, SAMPLE).expect("install");
    assert!(install(&mut spool, path, "@reboot echo bad\n").is_err());
    assert_eq!(list(&mut spool, path).expect("list"), SAMPLE);
}
