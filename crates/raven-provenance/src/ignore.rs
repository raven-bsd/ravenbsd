// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `.provenance-ignore` matching.

use std::fs;
use std::path::Path;

use regex::Regex;

/// Load non-comment ignore patterns from `<root>/.provenance-ignore`.
pub(crate) fn load(root: &Path) -> Vec<String> {
    let Ok(text) = fs::read_to_string(root.join(".provenance-ignore")) else {
        return Vec::new();
    };
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| line.trim_start_matches('/').to_string())
        .collect()
}

/// Whether a repository-relative path matches any ignore pattern.
pub(crate) fn is_ignored(path: &str, patterns: &[String]) -> bool {
    patterns.iter().any(|pattern| matches(path, pattern))
}

fn matches(path: &str, pattern: &str) -> bool {
    let pattern = pattern.trim_end_matches('/');
    if pattern.contains('*') || pattern.contains('?') {
        return glob_regex(pattern).is_some_and(|regex| regex.is_match(path));
    }
    path == pattern
        || path
            .strip_prefix(pattern)
            .is_some_and(|rest| rest.starts_with('/'))
}

fn glob_regex(pattern: &str) -> Option<Regex> {
    let mut expression = String::from("^");
    for character in pattern.chars() {
        match character {
            '*' => expression.push_str(".*"),
            '?' => expression.push('.'),
            other => expression.push_str(&regex::escape(&other.to_string())),
        }
    }
    expression.push('$');
    Regex::new(&expression).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn patterns(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    #[test]
    fn exact_paths_match() {
        assert!(is_ignored("Cargo.lock", &patterns(&["Cargo.lock"])));
        assert!(!is_ignored("Cargo.toml", &patterns(&["Cargo.lock"])));
    }

    #[test]
    fn directory_prefixes_match() {
        assert!(is_ignored("docs/man/raven-foo.8", &patterns(&["docs/man"])));
        assert!(is_ignored("docs/man", &patterns(&["docs/man"])));
    }

    #[test]
    fn prefixes_respect_component_boundaries() {
        assert!(!is_ignored("docs/manpage.txt", &patterns(&["docs/man"])));
        assert!(!is_ignored("Cargo.lock.bak", &patterns(&["Cargo.lock"])));
    }

    #[test]
    fn glob_star_is_supported() {
        assert!(is_ignored("a/b/tmp123", &patterns(&["a/*/tmp*"])));
        assert!(is_ignored("notes.md", &patterns(&["*.md"])));
        assert!(!is_ignored("notes.txt", &patterns(&["*.md"])));
    }

    #[test]
    fn glob_question_mark_is_supported() {
        assert!(is_ignored("doc1.md", &patterns(&["doc?.md"])));
        assert!(!is_ignored("doc10.md", &patterns(&["doc?.md"])));
    }

    #[test]
    fn load_skips_comments_and_blank_lines() {
        let temp = crate::testutil::TempDir::new("ignore-load");
        temp.write(
            ".provenance-ignore",
            "# comment\n\nCargo.lock\n  docs/man  \n",
        );
        assert_eq!(load(temp.path()), vec!["Cargo.lock", "docs/man"]);
    }

    #[test]
    fn missing_ignore_file_means_no_patterns() {
        let temp = crate::testutil::TempDir::new("ignore-missing");
        assert!(load(temp.path()).is_empty());
    }
}
