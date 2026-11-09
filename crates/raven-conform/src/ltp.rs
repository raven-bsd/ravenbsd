// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Parser for LTP logs.
//!
//! Two formats are recognized:
//!
//! * `ltp-pan` result lines such as
//!   `tag=open01 stime=... dur=1 exit=exited stat=0 core=no cu=1 cs=0`;
//! * plain test output tags `TPASS:`, `TFAIL:`, `TCONF:`, `TBROK:` (and the informational
//!   `TWARN:`/`TINFO:` lines, which are ignored).

use crate::result::{Results, Status, TestResult};

/// Parse LTP output. `target` is copied into the result metadata.
pub fn parse(input: &str, target: Option<&str>) -> Results {
    let mut results = Vec::new();
    let mut unparsed = 0usize;

    for (index, line) in input.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((name, status, duration_ms, message)) = pan_line(line) {
            results.push(TestResult {
                suite: String::from("ltp"),
                name,
                status,
                duration_ms,
                message,
            });
            continue;
        }
        match tag_line(line, index + 1) {
            Some(Some((name, status, message))) => results.push(TestResult {
                suite: String::from("ltp"),
                name,
                status,
                duration_ms: None,
                message,
            }),
            Some(None) => {}
            None => unparsed += 1,
        }
    }

    let mut out = Results {
        tool: String::from("ltp"),
        tool_version: None,
        target: target.map(str::to_string),
        unparsed,
        results,
    };
    out.normalize();
    out
}

fn pan_line(line: &str) -> Option<(String, Status, Option<u64>, Option<String>)> {
    let mut tag = None;
    let mut duration = None;
    let mut exit = None;
    let mut stat = None;

    for token in line.split_whitespace() {
        if let Some(value) = token.strip_prefix("tag=") {
            tag = Some(value.to_string());
        } else if let Some(value) = token.strip_prefix("dur=") {
            duration = Some(value.parse::<u64>().ok()?);
        } else if let Some(value) = token.strip_prefix("exit=") {
            exit = Some(value.to_string());
        } else if let Some(value) = token.strip_prefix("stat=") {
            stat = Some(value.parse::<u32>().ok()?);
        }
    }

    let tag = tag?;
    let exit = exit?;
    let stat = stat?;
    let status = match (exit.as_str(), stat) {
        ("exited", 0) => Status::Pass,
        ("exited", 32) => Status::Skip,
        ("exited", 1) => Status::Fail,
        ("exited", 2) => Status::Broken,
        _ => Status::Broken,
    };
    let message = Some(format!("exit={exit} stat={stat}"));
    Some((
        tag,
        status,
        duration.map(|seconds| seconds.saturating_mul(1000)),
        message,
    ))
}

fn tag_line(line: &str, line_number: usize) -> Option<Option<(String, Status, Option<String>)>> {
    for (tag, status) in [
        ("TPASS", Status::Pass),
        ("TFAIL", Status::Fail),
        ("TCONF", Status::Skip),
        ("TBROK", Status::Broken),
    ] {
        if let Some(index) = line.find(&format!("{tag}:")) {
            let name = line[..index].trim();
            let message = line[index + tag.len() + 1..].trim();
            let name = if name.is_empty() {
                format!("line {line_number}")
            } else {
                name.to_string()
            };
            let message = if message.is_empty() {
                None
            } else {
                Some(message.to_string())
            };
            return Some(Some((name, status, message)));
        }
    }

    for tag in ["TWARN", "TINFO"] {
        if line.contains(&format!("{tag}:")) {
            return Some(None);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pan_and_tags() {
        let input = "\
tag=open01 stime=1 dur=2 exit=exited stat=0 core=no cu=1 cs=0
tag=open02 stime=1 dur=0 exit=exited stat=1 core=no cu=1 cs=0
foo 1 TPASS: worked
foo 2 TFAIL: broke
bar 1 TCONF: skipped
baz 1 TBROK: broken
";
        let results = parse(input, None);
        assert_eq!(results.results.len(), 6);
        assert_eq!(find(&results.results, "open01").status, Status::Pass);
        assert_eq!(find(&results.results, "open02").status, Status::Fail);
        assert_eq!(find(&results.results, "foo 1").status, Status::Pass);
        assert_eq!(find(&results.results, "foo 2").status, Status::Fail);
        assert_eq!(find(&results.results, "bar 1").status, Status::Skip);
        assert_eq!(find(&results.results, "baz 1").status, Status::Broken);
    }

    fn find<'a>(results: &'a [TestResult], name: &str) -> &'a TestResult {
        results
            .iter()
            .find(|result| result.name == name)
            .unwrap_or_else(|| panic!("missing {name}"))
    }
}
