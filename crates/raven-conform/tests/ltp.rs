// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! LTP parser tests.

use raven_conform::Status;
use raven_conform::ltp::parse;

const FIXTURE: &str = include_str!("fixtures/ltp-sample.log");
const PAN: &str = include_str!("fixtures/ltp-pan.log");
const TAGS: &str = include_str!("fixtures/ltp-tags.log");

#[test]
fn parses_the_pan_fixture() {
    let results = parse(PAN, None);
    assert_eq!(results.results.len(), 4);
    assert_eq!(find(&results.results, "fs_acl01").status, Status::Pass);
    assert_eq!(find(&results.results, "fs_acl02").status, Status::Fail);
    assert_eq!(find(&results.results, "fs_acl03").status, Status::Skip);
    assert_eq!(find(&results.results, "fs_acl04").status, Status::Broken);
}

#[test]
fn parses_the_tag_fixture() {
    let results = parse(TAGS, None);
    assert_eq!(results.results.len(), 4);
    assert_eq!(find(&results.results, "open01 1").status, Status::Pass);
    assert_eq!(find(&results.results, "open01 2").status, Status::Fail);
    assert_eq!(find(&results.results, "open02 1").status, Status::Skip);
    assert_eq!(find(&results.results, "open03 1").status, Status::Broken);
    assert_eq!(results.unparsed, 0);
}

#[test]
fn parses_the_synthetic_fixture() {
    let results = parse(FIXTURE, Some("raven-test"));
    assert_eq!(results.tool, "ltp");
    assert_eq!(results.target.as_deref(), Some("raven-test"));
    assert_eq!(results.results.len(), 9);
    assert_eq!(results.unparsed, 0);

    let pan01 = find(&results.results, "open01");
    assert_eq!(pan01.status, Status::Pass);
    assert_eq!(pan01.duration_ms, Some(1000));

    assert_eq!(find(&results.results, "open02").status, Status::Fail);
    assert_eq!(find(&results.results, "open03").status, Status::Skip);
    assert_eq!(find(&results.results, "open04").status, Status::Broken);
    assert_eq!(find(&results.results, "open05").status, Status::Broken);
    assert_eq!(find(&results.results, "open01 1").status, Status::Pass);
    assert_eq!(find(&results.results, "open02 1").status, Status::Fail);
    assert_eq!(find(&results.results, "open03 1").status, Status::Skip);
    assert_eq!(find(&results.results, "open04 1").status, Status::Broken);
}

#[test]
fn pan_stat_mapping_covers_zero_one_two_and_thirty_two() {
    let results = parse(
        "tag=a stime=1 dur=0 exit=exited stat=0 core=no cu=0 cs=0\n\
         tag=b stime=1 dur=0 exit=exited stat=1 core=no cu=0 cs=0\n\
         tag=c stime=1 dur=0 exit=exited stat=2 core=no cu=0 cs=0\n\
         tag=d stime=1 dur=0 exit=exited stat=32 core=no cu=0 cs=0\n",
        None,
    );
    let statuses: Vec<Status> = results.results.iter().map(|result| result.status).collect();
    assert_eq!(
        statuses,
        vec![Status::Pass, Status::Fail, Status::Broken, Status::Skip]
    );
}

#[test]
fn signaled_pan_result_is_broken() {
    let results = parse(
        "tag=sig stime=1 dur=1 exit=signaled stat=9 core=yes cu=1 cs=0\n",
        None,
    );
    assert_eq!(results.results[0].status, Status::Broken);
    assert_eq!(results.results[0].duration_ms, Some(1000));
}

#[test]
fn plain_tags_map_to_statuses() {
    let results = parse(
        "one 1 TPASS: ok\ntwo 1 TFAIL: no\nthree 1 TCONF: skip\nfour 1 TBROK: broken\n",
        None,
    );
    assert_eq!(find(&results.results, "one 1").status, Status::Pass);
    assert_eq!(find(&results.results, "two 1").status, Status::Fail);
    assert_eq!(find(&results.results, "three 1").status, Status::Skip);
    assert_eq!(find(&results.results, "four 1").status, Status::Broken);
}

#[test]
fn tinfo_and_twarn_are_ignored_not_unparsed() {
    let results = parse("one TPASS: ok\none TINFO: note\none TWARN: warning\n", None);
    assert_eq!(results.results.len(), 1);
    assert_eq!(results.unparsed, 0);
}

#[test]
fn tag_without_tcid_uses_the_line_number() {
    let results = parse("TPASS: no prefix\n", None);
    assert_eq!(results.results[0].name, "line 1");
    assert_eq!(results.results[0].status, Status::Pass);
}

#[test]
fn duplicate_tag_names_get_a_suffix() {
    let results = parse("foo 1 TPASS: one\nfoo 1 TPASS: two\n", None);
    let names: Vec<&str> = results
        .results
        .iter()
        .map(|result| result.name.as_str())
        .collect();
    assert_eq!(names, vec!["foo 1", "foo 1#2"]);
}

#[test]
fn garbage_lines_are_counted() {
    let results = parse("not an LTP line\nTPASS: valid\n", None);
    assert_eq!(results.results.len(), 1);
    assert_eq!(results.unparsed, 1);
}

#[test]
fn malformed_pan_line_is_unparsed() {
    let results = parse("tag=missing stime=1 exit=exited\n", None);
    assert!(results.results.is_empty());
    assert_eq!(results.unparsed, 1);
}

#[test]
fn empty_input_is_valid() {
    let results = parse("", None);
    assert!(results.results.is_empty());
    assert_eq!(results.unparsed, 0);
}

fn find<'a>(results: &'a [raven_conform::TestResult], name: &str) -> &'a raven_conform::TestResult {
    results
        .iter()
        .find(|result| result.name == name)
        .unwrap_or_else(|| panic!("missing {name}"))
}
