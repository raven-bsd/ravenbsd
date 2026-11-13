// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the nsswitch parser, dispatcher and line scanner.

use super::*;

struct FakeFiles {
    data: alloc::collections::BTreeMap<&'static str, Result<Vec<u8>, Errno>>,
}

impl FakeFiles {
    fn new() -> Self {
        Self {
            data: alloc::collections::BTreeMap::new(),
        }
    }

    fn put(&mut self, path: &'static str, bytes: &[u8]) {
        self.data.insert(path, Ok(bytes.to_vec()));
    }
}

impl FileSource for FakeFiles {
    fn read_file(&self, path: &str, _max: usize) -> Result<Vec<u8>, Errno> {
        self.data
            .get(path)
            .cloned()
            .unwrap_or(Err(raven_libc_abi::errno::ENOENT))
    }
}

fn entry(source: Source, on_notfound_return: bool) -> SourceEntry {
    SourceEntry {
        source,
        on_notfound_return,
    }
}

#[test]
fn host_defaults_are_files_then_dns() {
    let list = Nsswitch::defaults().sources(Database::Hosts);
    let entries: Vec<_> = list.iter().collect();
    assert_eq!(
        entries,
        vec![entry(Source::Files, false), entry(Source::Dns, false)]
    );
}

#[test]
fn other_databases_default_to_files_only() {
    for db in [
        Database::Passwd,
        Database::Group,
        Database::Services,
        Database::Protocols,
        Database::Networks,
    ] {
        let list = Nsswitch::defaults().sources(db);
        assert_eq!(list.len(), 1);
        assert_eq!(list.get(0), Some(entry(Source::Files, false)));
    }
}

#[test]
fn an_empty_file_is_an_empty_configuration() {
    let nss = Nsswitch::parse(b"");
    for db in [
        Database::Hosts,
        Database::Passwd,
        Database::Group,
        Database::Services,
        Database::Protocols,
        Database::Networks,
    ] {
        assert!(nss.sources(db).is_empty());
    }
}

#[test]
fn a_hosts_line_parses() {
    let nss = Nsswitch::parse(b"hosts: files dns\n");
    let list = nss.sources(Database::Hosts);
    assert_eq!(
        list.iter().collect::<Vec<_>>(),
        vec![entry(Source::Files, false), entry(Source::Dns, false)]
    );
}

#[test]
fn every_known_database_parses() {
    let text = b"hosts: dns\npasswd: files\ngroup: files\nservices: files\nprotocols: files\nnetworks: files\n";
    let nss = Nsswitch::parse(text);
    assert_eq!(
        nss.sources(Database::Hosts).get(0),
        Some(entry(Source::Dns, false))
    );
    for db in [
        Database::Passwd,
        Database::Group,
        Database::Services,
        Database::Protocols,
        Database::Networks,
    ] {
        assert_eq!(nss.sources(db).get(0), Some(entry(Source::Files, false)));
    }
}

#[test]
fn unknown_databases_are_ignored() {
    let nss = Nsswitch::parse(b"unknown: files\nhosts: files\n");
    assert_eq!(nss.sources(Database::Hosts).len(), 1);
}

#[test]
fn uppercase_database_names_are_not_recognised() {
    let nss = Nsswitch::parse(b"HOSTS: files\n");
    assert!(nss.sources(Database::Hosts).is_empty());
}

#[test]
fn unknown_sources_are_skipped() {
    let nss = Nsswitch::parse(b"hosts: files ldap nis resolve\n");
    let entries: Vec<_> = nss.sources(Database::Hosts).iter().collect();
    assert_eq!(entries.len(), 4);
    assert_eq!(entries[0].source, Source::Files);
    assert_eq!(entries[1].source, Source::Skipped);
    assert_eq!(entries[2].source, Source::Skipped);
    assert_eq!(entries[3].source, Source::Skipped);
}

#[test]
fn the_notfound_return_action_is_attached_to_the_previous_source() {
    let nss = Nsswitch::parse(b"hosts: files [NOTFOUND=return] dns\n");
    let entries: Vec<_> = nss.sources(Database::Hosts).iter().collect();
    assert_eq!(entries[0], entry(Source::Files, true));
    assert_eq!(entries[1], entry(Source::Dns, false));
}

#[test]
fn other_actions_are_ignored() {
    let nss = Nsswitch::parse(b"hosts: files [UNAVAIL=continue] dns\n");
    let entries: Vec<_> = nss.sources(Database::Hosts).iter().collect();
    assert_eq!(
        entries,
        vec![entry(Source::Files, false), entry(Source::Dns, false)]
    );
}

#[test]
fn comments_and_blank_lines_are_ignored() {
    let nss = Nsswitch::parse(b"# comment\n\nhosts: files # trailing\n");
    assert_eq!(nss.sources(Database::Hosts).len(), 1);
}

#[test]
fn crlf_lines_are_accepted() {
    let nss = Nsswitch::parse(b"hosts: files\r\ndns\r\n");
    // The second line has no database name, so the whole file is malformed.
    assert_eq!(nss, Nsswitch::defaults());
}

#[test]
fn an_overlong_line_is_skipped_whole() {
    let mut text = alloc::vec![b'#'; MAX_LINE + 1];
    text.extend_from_slice(b"\nhosts: files\n");
    let nss = Nsswitch::parse(&text);
    assert_eq!(
        nss.sources(Database::Hosts).get(0),
        Some(entry(Source::Files, false))
    );
}

#[test]
fn a_malformed_line_gives_the_defaults() {
    let nss = Nsswitch::parse(b"hosts files dns");
    assert_eq!(nss, Nsswitch::defaults());
}

#[test]
fn a_source_list_is_capped_at_eight() {
    let nss = Nsswitch::parse(b"hosts: files dns files dns files dns files dns files dns\n");
    assert_eq!(nss.sources(Database::Hosts).len(), 8);
}

#[test]
fn load_of_a_missing_file_gives_defaults() {
    let files = FakeFiles::new();
    assert_eq!(load(&files), Nsswitch::defaults());
}

#[test]
fn load_reads_the_file_each_time() {
    let mut files = FakeFiles::new();
    files.put("/etc/nsswitch.conf", b"hosts: dns\n");
    let nss = load(&files);
    assert_eq!(
        nss.sources(Database::Hosts).get(0),
        Some(entry(Source::Dns, false))
    );
}

#[test]
fn load_of_a_malformed_file_gives_defaults() {
    let mut files = FakeFiles::new();
    files.put("/etc/nsswitch.conf", b"not a database line");
    assert_eq!(load(&files), Nsswitch::defaults());
}

#[test]
fn dispatch_returns_the_first_success() {
    let list = Nsswitch::defaults().sources(Database::Hosts);
    let mut calls = Vec::new();
    let status = dispatch(&list, |source| {
        calls.push(source);
        Status::Success
    });
    assert_eq!(status, Status::Success);
    assert_eq!(calls, vec![Source::Files]);
}

#[test]
fn dispatch_continues_after_notfound() {
    let list = Nsswitch::defaults().sources(Database::Hosts);
    let mut calls = Vec::new();
    let status = dispatch(&list, |source| {
        calls.push(source);
        if source == Source::Files {
            Status::NotFound
        } else {
            Status::Success
        }
    });
    assert_eq!(status, Status::Success);
    assert_eq!(calls, vec![Source::Files, Source::Dns]);
}

#[test]
fn dispatch_stops_on_notfound_return() {
    let nss = Nsswitch::parse(b"hosts: files [NOTFOUND=return] dns\n");
    let list = nss.sources(Database::Hosts);
    let mut calls = Vec::new();
    let status = dispatch(&list, |source| {
        calls.push(source);
        Status::NotFound
    });
    assert_eq!(status, Status::NotFound);
    assert_eq!(calls, vec![Source::Files]);
}

#[test]
fn dispatch_continues_after_unavail_and_tryagain() {
    let list = Nsswitch::defaults().sources(Database::Hosts);
    let mut calls = Vec::new();
    let status = dispatch(&list, |source| {
        calls.push(source);
        if source == Source::Files {
            Status::Unavail
        } else {
            Status::TryAgain
        }
    });
    assert_eq!(status, Status::Unavail);
    assert_eq!(calls, vec![Source::Files, Source::Dns]);
}

#[test]
fn dispatch_never_calls_a_skipped_source() {
    let nss = Nsswitch::parse(b"hosts: ldap files\n");
    let list = nss.sources(Database::Hosts);
    let mut calls = Vec::new();
    let status = dispatch(&list, |source| {
        calls.push(source);
        Status::Success
    });
    assert_eq!(status, Status::Success);
    assert_eq!(calls, vec![Source::Files]);
}

#[test]
fn dispatch_of_an_empty_list_is_unavail() {
    let list = SourceList::new();
    assert_eq!(dispatch(&list, |_| Status::Success), Status::Unavail);
}

#[test]
fn the_scanner_returns_lines_without_the_newline() {
    let mut scanner = ColonScanner::new(b"a:b\nc:d\n", 128);
    assert_eq!(scanner.next_line(), Some(b"a:b".as_slice()));
    assert_eq!(scanner.next_line(), Some(b"c:d".as_slice()));
    assert_eq!(scanner.next_line(), None);
}

#[test]
fn the_scanner_accepts_a_last_line_without_a_newline() {
    let mut scanner = ColonScanner::new(b"a:b", 128);
    assert_eq!(scanner.next_line(), Some(b"a:b".as_slice()));
    assert_eq!(scanner.next_line(), None);
}

#[test]
fn the_scanner_skips_an_overlong_line_whole() {
    let mut data = alloc::vec![b'x'; 129];
    data.push(b'\n');
    data.extend_from_slice(b"ok\n");
    let mut scanner = ColonScanner::new(&data, 128);
    assert_eq!(scanner.next_line(), Some(b"ok".as_slice()));
}

#[test]
fn the_scanner_trims_a_trailing_cr() {
    let mut scanner = ColonScanner::new(b"a:b\r\n", 128);
    assert_eq!(scanner.next_line(), Some(b"a:b".as_slice()));
}

#[test]
fn the_scanner_accepts_empty_lines() {
    let mut scanner = ColonScanner::new(b"\n\nx\n", 128);
    assert_eq!(scanner.next_line(), Some(b"".as_slice()));
    assert_eq!(scanner.next_line(), Some(b"".as_slice()));
    assert_eq!(scanner.next_line(), Some(b"x".as_slice()));
}

#[test]
fn split_fields_returns_every_colon_field() {
    let mut out = [b"".as_slice(); 8];
    assert_eq!(ColonScanner::split_fields(b"a:b:c", &mut out), 3);
    assert_eq!(
        out[..3],
        [b"a".as_slice(), b"b".as_slice(), b"c".as_slice()]
    );
}

#[test]
fn split_fields_keeps_empty_fields() {
    let mut out = [b"".as_slice(); 8];
    assert_eq!(ColonScanner::split_fields(b":a::b:", &mut out), 5);
    assert_eq!(out[1], b"a".as_slice());
    assert_eq!(out[2], b"".as_slice());
    assert_eq!(out[4], b"".as_slice());
}

#[test]
fn split_fields_truncates_to_the_output_size() {
    let mut out = [b"".as_slice(); 2];
    assert_eq!(ColonScanner::split_fields(b"a:b:c", &mut out), 2);
}

#[test]
fn a_source_list_reports_its_entries() {
    let mut list = SourceList::new();
    list.push(entry(Source::Files, false));
    list.push(entry(Source::Dns, true));
    assert_eq!(list.len(), 2);
    assert_eq!(list.get(0), Some(entry(Source::Files, false)));
    assert_eq!(list.get(1), Some(entry(Source::Dns, true)));
    assert_eq!(list.get(2), None);
    assert!(!list.is_empty());
}

#[test]
fn an_empty_source_list_is_empty() {
    let list = SourceList::new();
    assert!(list.is_empty());
    assert_eq!(list.iter().count(), 0);
}

#[test]
fn the_default_configuration_is_stable() {
    assert_eq!(Nsswitch::default(), Nsswitch::defaults());
}

#[test]
fn a_notfound_action_without_a_previous_source_is_ignored() {
    let nss = Nsswitch::parse(b"hosts: [NOTFOUND=return] files\n");
    let entries: Vec<_> = nss.sources(Database::Hosts).iter().collect();
    assert_eq!(entries, vec![entry(Source::Files, false)]);
}

#[test]
fn a_hosts_dns_files_order_parses() {
    let nss = Nsswitch::parse(b"hosts: dns files\n");
    let entries: Vec<_> = nss.sources(Database::Hosts).iter().collect();
    assert_eq!(
        entries,
        vec![entry(Source::Dns, false), entry(Source::Files, false)]
    );
}

#[test]
fn the_scanner_skips_a_long_last_line() {
    let data = alloc::vec![b'x'; 129];
    let mut scanner = ColonScanner::new(&data, 128);
    assert_eq!(scanner.next_line(), None);
}

#[test]
fn split_fields_of_an_empty_line_is_one_empty_field() {
    let mut out = [b"".as_slice(); 4];
    assert_eq!(ColonScanner::split_fields(b"", &mut out), 1);
    assert_eq!(out[0], b"".as_slice());
}

#[test]
fn a_trailing_cr_on_a_long_line_still_counts_toward_the_limit() {
    let mut data = alloc::vec![b'x'; 128];
    data.push(b'\r');
    data.push(b'\n');
    let mut scanner = ColonScanner::new(&data, 128);
    assert_eq!(scanner.next_line(), None);
}
