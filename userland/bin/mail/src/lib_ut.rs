// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the bounded local mail core.

use super::*;
use std::sync::atomic::AtomicU64;

static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn temporary(name: &str) -> PathBuf {
    let sequence = TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "raven-mail-{}-{sequence}-{name}",
        std::process::id()
    ));
    let _ = fs::remove_file(&path);
    let _ = fs::remove_dir_all(&path);
    path
}

fn arguments(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

#[test]
fn an_empty_command_line_lists() {
    let invocation = parse_args("mail", &[]).expect("parses");
    assert_eq!(invocation.action, Action::List);
}

#[test]
fn a_subject_and_recipient_select_send() {
    let invocation = parse_args("mail", &arguments(&["-s", "Hi there", "alice"])).expect("parses");
    assert_eq!(invocation.action, Action::Send);
    assert_eq!(invocation.subject.as_deref(), Some("Hi there"));
    assert_eq!(invocation.recipients, vec!["alice".to_string()]);
}

#[test]
fn read_delete_and_exists_are_parsed() {
    assert_eq!(
        parse_args("mail", &arguments(&["-r", "2"])).unwrap().action,
        Action::Read(2)
    );
    assert_eq!(
        parse_args("mail", &arguments(&["-d", "1"])).unwrap().action,
        Action::Delete(1)
    );
    assert_eq!(
        parse_args("mail", &arguments(&["-e"])).unwrap().action,
        Action::Exists
    );
}

#[test]
fn unknown_options_are_usage_errors() {
    assert!(parse_args("mail", &arguments(&["-R"])).is_err());
    assert!(parse_args("mail", &arguments(&["-s"])).is_err());
}

#[test]
fn inconsistent_options_are_rejected() {
    assert!(parse_args("mail", &arguments(&["-s", "x"])).is_err());
    assert!(parse_args("mail", &arguments(&["-r", "1", "alice"])).is_err());
    assert!(parse_args("mail", &arguments(&["-H", "-s", "x"])).is_err());
}

#[test]
fn compose_and_parse_an_mbox_message() {
    let message = compose(
        "alice@localhost",
        &["bob".to_string()],
        Some("Hello"),
        "Thu, 01 Jan 1970 00:00:00 +0000",
        "From the body\nline two\n",
    );
    assert!(message.contains("Subject: Hello"));
    let parsed = parse_message(&message, false);
    assert_eq!(parsed.field("To"), Some("bob"));
    assert_eq!(parsed.body, "From the body\nline two\n");
}

#[test]
fn mbox_quoting_round_trips() {
    let path = temporary("mbox");
    let store = Store::Mbox(path.clone());
    let message = compose(
        "alice@localhost",
        &["bob".to_string()],
        Some("Quote"),
        "Thu, 01 Jan 1970 00:00:00 +0000",
        "From the body\n",
    );
    store.append(&message).expect("append");
    let messages = store.list().expect("list");
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].field("Subject"), Some("Quote"));
    assert_eq!(messages[0].body, "From the body\n");
    store.delete(0).expect("delete");
    assert!(store.list().expect("list").is_empty());
    let _ = fs::remove_file(path);
}

#[test]
fn maildir_append_list_and_delete() {
    let path = temporary("Maildir");
    fs::create_dir_all(&path).expect("maildir");
    let store = Store::Maildir(path.clone());
    let message = compose(
        "alice@localhost",
        &["bob".to_string()],
        Some("Maildir"),
        "Thu, 01 Jan 1970 00:00:00 +0000",
        "body\n",
    );
    store.append(&message).expect("append");
    let messages = store.list().expect("list");
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].field("Subject"), Some("Maildir"));
    store.delete(0).expect("delete");
    assert!(store.list().expect("list").is_empty());
    let _ = fs::remove_dir_all(path);
}

#[test]
fn split_mbox_keeps_separators() {
    let text = "From a Thu Jan  1 00:00:00 1970\nSubject: one\n\nbody\nFrom b Thu Jan  1 00:00:00 1970\nSubject: two\n\nbody\n";
    let messages = split_mbox(text);
    assert_eq!(messages.len(), 2);
    assert!(messages[0].starts_with("From a "));
    assert!(messages[1].starts_with("From b "));
}

#[test]
fn the_epoch_formats_as_rfc_2822() {
    assert_eq!(format_date(0), "Thu, 01 Jan 1970 00:00:00 +0000");
    assert_eq!(format_date(946_684_800), "Sat, 01 Jan 2000 00:00:00 +0000");
}

#[test]
fn an_existing_directory_is_maildir() {
    let path = temporary("open");
    fs::create_dir_all(&path).expect("dir");
    assert!(matches!(Store::open(&path), Store::Maildir(_)));
    let _ = fs::remove_dir_all(path);
}
