// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Human, TAP, JSON and JUnit reports for a run.

use crate::{Outcome, Verdict};

/// Report format.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Format {
    /// Human-readable table.
    Human,
    /// TAP version 12-ish, one line per body.
    Tap,
    /// A hand-written JSON array (no extra dependency).
    Json,
    /// JUnit XML.
    Junit,
}

impl Format {
    /// Parse `--report`.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "tap" => Some(Self::Tap),
            "json" => Some(Self::Json),
            "junit" => Some(Self::Junit),
            "human" => Some(Self::Human),
            _ => None,
        }
    }
}

/// Render a report.
#[must_use]
pub fn render(outcomes: &[Outcome], format: Format) -> String {
    match format {
        Format::Human => human(outcomes),
        Format::Tap => tap(outcomes),
        Format::Json => json(outcomes),
        Format::Junit => junit(outcomes),
    }
}

fn human(outcomes: &[Outcome]) -> String {
    let mut text = String::new();
    for outcome in outcomes {
        text.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\t{}\n",
            outcome.case.suite,
            outcome.case.program,
            outcome.case.name,
            outcome.verdict.as_str(),
            outcome.reason,
            outcome.ticket
        ));
    }
    text
}

fn tap(outcomes: &[Outcome]) -> String {
    let mut text = format!("1..{}\n", outcomes.len());
    for (index, outcome) in outcomes.iter().enumerate() {
        let number = index + 1;
        let label = format!(
            "{}/{}/{}",
            outcome.case.suite, outcome.case.program, outcome.case.name
        );
        match outcome.verdict {
            Verdict::Pass => text.push_str(&format!("ok {number} - {label}\n")),
            Verdict::Skip => text.push_str(&format!(
                "ok {number} - {label} # SKIP {}\n",
                outcome.reason
            )),
            Verdict::Excluded | Verdict::OracleFail | Verdict::NotRun => text.push_str(&format!(
                "ok {number} - {label} # SKIP {} {}\n",
                outcome.verdict.as_str(),
                outcome.reason
            )),
            Verdict::Fail => text.push_str(&format!(
                "not ok {number} - {label} # FAIL {}\n",
                outcome.reason
            )),
        }
    }
    text
}

fn json(outcomes: &[Outcome]) -> String {
    let mut text = String::from("[\n");
    for (index, outcome) in outcomes.iter().enumerate() {
        text.push_str(&format!(
            "  {{\"suite\":\"{}\",\"program\":\"{}\",\"case\":\"{}\",\"verdict\":\"{}\",\"reason\":\"{}\",\"ticket\":\"{}\"}}",
            json_escape(&outcome.case.suite),
            json_escape(&outcome.case.program),
            json_escape(&outcome.case.name),
            outcome.verdict.as_str(),
            json_escape(&outcome.reason),
            json_escape(&outcome.ticket)
        ));
        if index + 1 != outcomes.len() {
            text.push(',');
        }
        text.push('\n');
    }
    text.push_str("]\n");
    text
}

fn junit(outcomes: &[Outcome]) -> String {
    let failures = outcomes
        .iter()
        .filter(|outcome| outcome.verdict == Verdict::Fail)
        .count();
    let skipped = outcomes
        .iter()
        .filter(|outcome| {
            matches!(
                outcome.verdict,
                Verdict::Skip | Verdict::Excluded | Verdict::OracleFail | Verdict::NotRun
            )
        })
        .count();
    let mut text = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<testsuite name=\"raven-atf-run\" tests=\"{}\" failures=\"{failures}\" skipped=\"{skipped}\">\n",
        outcomes.len()
    );
    for outcome in outcomes {
        let name = xml_escape(&format!(
            "{}/{}/{}",
            outcome.case.suite, outcome.case.program, outcome.case.name
        ));
        match outcome.verdict {
            Verdict::Pass => {
                text.push_str(&format!("  <testcase name=\"{name}\"/>\n"));
            }
            Verdict::Fail => {
                text.push_str(&format!(
                    "  <testcase name=\"{name}\"><failure message=\"{}\"/></testcase>\n",
                    xml_escape(&outcome.reason)
                ));
            }
            _ => {
                text.push_str(&format!(
                    "  <testcase name=\"{name}\"><skipped message=\"{} {}\"/></testcase>\n",
                    outcome.verdict.as_str(),
                    xml_escape(&outcome.reason)
                ));
            }
        }
    }
    text.push_str("</testsuite>\n");
    text
}

fn json_escape(text: &str) -> String {
    let mut escaped = String::new();
    for character in text.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            character if character.is_control() => {
                escaped.push_str(&format!("\\u{:04x}", character as u32));
            }
            character => escaped.push(character),
        }
    }
    escaped
}

fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
#[path = "report_ut.rs"]
mod tests;
