// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Ordered/unordered expectation matching and forbidden-line detection.

use crate::case::Case;

/// Matching outcome for one case.
pub(crate) struct MatchResult {
    pub matched: Vec<String>,
    pub unmatched: Vec<String>,
    pub forbidden_hits: Vec<String>,
}

/// Match every expectation against the captured serial lines.
///
/// An ordered expectation searches from the line after the previous ordered match. An
/// unordered expectation searches all lines not already consumed by an earlier expectation.
/// Each line can satisfy at most one expectation.
pub(crate) fn evaluate(case: &Case, lines: &[String]) -> MatchResult {
    let mut consumed = vec![false; lines.len()];
    let mut cursor = 0;
    let mut matched = Vec::new();
    let mut unmatched = Vec::new();

    for (expect, regex) in case.expects.iter().zip(case.expect_regexes()) {
        let found = if expect.unordered {
            lines
                .iter()
                .enumerate()
                .find(|(index, line)| !consumed[*index] && regex.is_match(line))
                .map(|(index, _)| index)
        } else {
            lines
                .iter()
                .enumerate()
                .skip(cursor)
                .find(|(index, line)| !consumed[*index] && regex.is_match(line))
                .map(|(index, _)| index)
        };
        match found {
            Some(index) => {
                consumed[index] = true;
                matched.push(expect.regex.clone());
                if !expect.unordered {
                    cursor = index + 1;
                }
            }
            None => unmatched.push(expect.regex.clone()),
        }
    }

    let mut forbidden_hits = Vec::new();
    for regex in case.forbid_regexes() {
        for line in lines {
            if regex.is_match(line) {
                forbidden_hits.push(line.clone());
            }
        }
    }

    MatchResult {
        matched,
        unmatched,
        forbidden_hits,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn case(expects: &str, forbids: &str) -> Case {
        let text = format!(
            "name = \"m\"\narch = \"x86_64\"\nprofile = \"base\"\ntimeout_secs = 5\n\
             expect_exit = \"success\"\n{expects}{forbids}"
        );
        Case::parse(&text).unwrap()
    }

    fn lines(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    #[test]
    fn ordered_expectations_must_appear_in_order() {
        let case = case(
            "[[expect]]\nregex = \"^first$\"\n[[expect]]\nregex = \"^second$\"\n",
            "",
        );
        let result = evaluate(&case, &lines(&["first", "second"]));
        assert_eq!(result.matched.len(), 2);
        assert!(result.unmatched.is_empty());
    }

    #[test]
    fn ordered_expectations_fail_when_reversed() {
        let case = case(
            "[[expect]]\nregex = \"^first$\"\n[[expect]]\nregex = \"^second$\"\n",
            "",
        );
        let result = evaluate(&case, &lines(&["second", "first"]));
        assert_eq!(result.matched, vec!["^first$"]);
        assert_eq!(result.unmatched, vec!["^second$"]);
    }

    #[test]
    fn ordered_matching_skips_unrelated_lines() {
        let case = case(
            "[[expect]]\nregex = \"^first$\"\n[[expect]]\nregex = \"^second$\"\n",
            "",
        );
        let result = evaluate(&case, &lines(&["noise", "first", "noise", "second"]));
        assert!(result.unmatched.is_empty());
    }

    #[test]
    fn unordered_expectations_match_any_order() {
        let case = case(
            "[[expect]]\nregex = \"^first$\"\nunordered = true\n\
             [[expect]]\nregex = \"^second$\"\nunordered = true\n",
            "",
        );
        let result = evaluate(&case, &lines(&["second", "noise", "first"]));
        assert_eq!(result.matched.len(), 2);
        assert!(result.unmatched.is_empty());
    }

    #[test]
    fn unordered_expectations_fail_when_missing() {
        let case = case(
            "[[expect]]\nregex = \"^first$\"\nunordered = true\n\
             [[expect]]\nregex = \"^second$\"\nunordered = true\n",
            "",
        );
        let result = evaluate(&case, &lines(&["first"]));
        assert_eq!(result.unmatched, vec!["^second$"]);
    }

    #[test]
    fn one_line_cannot_satisfy_two_expectations() {
        let case = case(
            "[[expect]]\nregex = \"same\"\n[[expect]]\nregex = \"same\"\n",
            "",
        );
        let result = evaluate(&case, &lines(&["same"]));
        assert_eq!(result.matched.len(), 1);
        assert_eq!(result.unmatched.len(), 1);
    }

    #[test]
    fn regexes_are_searched_not_anchored_implicitly() {
        let case = case("[[expect]]\nregex = \"kernel\"\n", "");
        let result = evaluate(&case, &lines(&["RavenBSD kernel 0.1"]));
        assert!(result.unmatched.is_empty());
    }

    #[test]
    fn forbid_hit_is_reported() {
        let case = case("", "[[forbid]]\nregex = \"(?i)panic\"\n");
        let result = evaluate(&case, &lines(&["PANIC: broken", "ok"]));
        assert_eq!(result.forbidden_hits, vec!["PANIC: broken"]);
    }

    #[test]
    fn every_matching_forbidden_line_is_reported() {
        let case = case("", "[[forbid]]\nregex = \"bad\"\n");
        let result = evaluate(&case, &lines(&["bad 1", "ok", "bad 2"]));
        assert_eq!(result.forbidden_hits.len(), 2);
    }

    #[test]
    fn mixed_ordered_and_unordered_expectations_work() {
        let case = case(
            "[[expect]]\nregex = \"^boot$\"\n\
             [[expect]]\nregex = \"^ready$\"\nunordered = true\n\
             [[expect]]\nregex = \"^done$\"\n",
            "",
        );
        let result = evaluate(&case, &lines(&["boot", "done", "ready"]));
        assert_eq!(result.matched.len(), 3);
        assert!(result.unmatched.is_empty());
    }
}
