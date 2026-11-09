// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Parser for pjdfstest's TAP-like output.
//!
//! Direct output consists of `ok N` / `not ok N - description` lines, `1..M` plan lines and
//! `# SKIP` / `# TODO` directives. `prove` output adds `tests/<area>/<n>.t ..` markers and a
//! final `.. ok` or `.. Failed N/M subtests` summary; results are assigned to the test-file suite.

use crate::result::{Results, Status, TestResult};

/// Parse pjdfstest output. `target` is copied into the result metadata.
pub fn parse(input: &str, target: Option<&str>) -> Results {
    let mut results: Vec<TestResult> = Vec::new();
    let mut unparsed = 0usize;
    let mut suite = String::from("pjdfstest");
    let mut group_start = 0usize;

    for line in input.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((path, is_summary)) = prove_marker(line) {
            if is_summary {
                for result in &mut results[group_start..] {
                    result.suite.clone_from(&path);
                }
            } else {
                suite.clone_from(&path);
                group_start = results.len();
            }
            continue;
        }
        if is_plan(line) {
            continue;
        }
        if is_prove_footer(line) {
            continue;
        }
        if let Some(message) = line.strip_prefix("Bail out!") {
            results.push(TestResult {
                suite: suite.clone(),
                name: String::from("bail out"),
                status: Status::Broken,
                duration_ms: None,
                message: Some(message.trim().to_string()),
            });
            continue;
        }
        if let Some(parsed) = tap_line(line) {
            results.push(TestResult {
                suite: suite.clone(),
                name: parsed.name,
                status: parsed.status,
                duration_ms: None,
                message: parsed.message,
            });
            continue;
        }
        unparsed += 1;
    }

    let mut out = Results {
        tool: String::from("pjdfstest"),
        tool_version: None,
        target: target.map(str::to_string),
        unparsed,
        results,
    };
    out.normalize();
    out
}

struct TapResult {
    name: String,
    status: Status,
    message: Option<String>,
}

fn prove_marker(line: &str) -> Option<(String, bool)> {
    let (path, rest) = line.split_once(" ..")?;
    let path = path.trim();
    if path.is_empty() {
        return None;
    }
    let rest = rest.trim();
    if rest.is_empty() {
        return Some((path.to_string(), false));
    }
    if rest == "ok" || rest.starts_with("Failed ") {
        return Some((path.to_string(), true));
    }
    None
}

fn is_plan(line: &str) -> bool {
    let Some(rest) = line.strip_prefix("1..") else {
        return false;
    };
    rest.split_whitespace()
        .next()
        .is_some_and(|number| number.parse::<u64>().is_ok())
}

fn is_prove_footer(line: &str) -> bool {
    line.starts_with("Result:")
        || line.starts_with("Files=")
        || line.starts_with("Tests=")
        || line.starts_with("Duration:")
        || line == "All tests successful."
}

fn tap_line(line: &str) -> Option<TapResult> {
    let (passed, rest) = if let Some(rest) = line.strip_prefix("not ok") {
        (false, rest)
    } else {
        let rest = line.strip_prefix("ok")?;
        (true, rest)
    };

    let rest = rest.trim_start();
    let (number, after_number) = leading_number(rest);
    let after_number = after_number.trim_start();

    let (body, directive) = match after_number.find('#') {
        Some(index) => (
            after_number[..index].trim(),
            Some(after_number[index + 1..].trim()),
        ),
        None => (after_number, None),
    };

    let description = {
        let body = body.trim();
        let text = if let Some(rest) = body.strip_prefix("- ") {
            Some(rest.trim())
        } else {
            body.split_once(" - ")
                .map(|(_, description)| description.trim())
        };
        text.filter(|description| !description.is_empty())
    };

    let directive_kind = directive.map(|text| {
        text.split_whitespace()
            .next()
            .unwrap_or("")
            .to_ascii_lowercase()
    });
    let skipped = matches!(directive_kind.as_deref(), Some("skip" | "todo"));

    let name = description
        .map(str::to_string)
        .unwrap_or_else(|| match number {
            Some(number) => format!("test {number}"),
            None => String::from("test"),
        });

    let status = if skipped {
        Status::Skip
    } else if passed {
        Status::Pass
    } else {
        Status::Fail
    };

    let message = if skipped {
        directive.map(str::to_string)
    } else {
        description.map(str::to_string)
    };

    Some(TapResult {
        name,
        status,
        message,
    })
}

fn leading_number(text: &str) -> (Option<u64>, &str) {
    let end = text
        .bytes()
        .position(|byte| !byte.is_ascii_digit())
        .unwrap_or(text.len());
    if end == 0 {
        return (None, text);
    }
    (text[..end].parse().ok(), &text[end..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_lines() {
        let results = parse(
            "1..3\nok 1\nnot ok 2 - failed thing\nok 3 # SKIP no root\n",
            None,
        );
        assert_eq!(results.results.len(), 3);
        assert_eq!(find(&results.results, "test 1").status, Status::Pass);
        assert_eq!(find(&results.results, "failed thing").status, Status::Fail);
        assert_eq!(find(&results.results, "test 3").status, Status::Skip);
        assert_eq!(results.unparsed, 0);
    }

    fn find<'a>(results: &'a [TestResult], name: &str) -> &'a TestResult {
        results
            .iter()
            .find(|result| result.name == name)
            .unwrap_or_else(|| panic!("missing {name}"))
    }
}
