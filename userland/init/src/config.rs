// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `/etc/svcd.conf` test-mode configuration.
//!
//! The file is a small `key = value` list; unknown keys are reported and ignored so that a newer
//! profile does not stop an older svcd. Parsing never panics and never allocates more than the
//! input plus one small warning per line.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// Test-mode options for `svcd`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Config {
    /// Exit as soon as this service terminates, using its exit code.
    pub exit_when_exits: Option<String>,
    /// Print `svcd: settled` and exit once every service is up or has given up.
    pub exit_when_settled: bool,
}

/// Parse `/etc/svcd.conf` into a [`Config`] and a list of human-readable warnings.
///
/// Unknown keys, malformed lines and bad boolean values are warnings, not errors: the file is a
/// test aid and must not make a normal boot fail. Duplicate keys use the last value.
pub fn parse(text: &str) -> (Config, Vec<String>) {
    let mut config = Config::default();
    let mut warnings = Vec::new();
    for (index, raw_line) in text.lines().enumerate() {
        let line = index + 1;
        let line_text = raw_line.split('#').next().unwrap_or("").trim();
        if line_text.is_empty() {
            continue;
        }
        let Some((key, value)) = line_text.split_once('=') else {
            warnings.push(alloc::format!("line {line}: missing `=`"));
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        if key.is_empty() || value.is_empty() {
            warnings.push(alloc::format!("line {line}: missing key or value"));
            continue;
        }
        let value = unquote(value).unwrap_or_else(|| value.to_string());
        match key {
            "exit_when_exits" => {
                if raven_proto_svc::valid_name(&value) {
                    config.exit_when_exits = Some(value);
                } else {
                    warnings.push(alloc::format!(
                        "line {line}: invalid service name `{value}`"
                    ));
                }
            }
            "exit_when_settled" => match value.as_str() {
                "true" => config.exit_when_settled = true,
                "false" => config.exit_when_settled = false,
                _ => warnings.push(alloc::format!(
                    "line {line}: exit_when_settled must be true or false"
                )),
            },
            other => warnings.push(alloc::format!("unknown key {other}")),
        }
    }
    (config, warnings)
}

fn unquote(value: &str) -> Option<String> {
    let inner = value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))?;
    if inner.contains('"') {
        return None;
    }
    Some(inner.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_file_uses_defaults() {
        let (config, warnings) = parse("");
        assert_eq!(config, Config::default());
        assert!(warnings.is_empty());
    }

    #[test]
    fn parses_both_keys() {
        let (config, warnings) = parse("exit_when_exits = shell\nexit_when_settled = true\n");
        assert_eq!(config.exit_when_exits.as_deref(), Some("shell"));
        assert!(config.exit_when_settled);
        assert!(warnings.is_empty());
    }

    #[test]
    fn quoted_values_are_accepted() {
        let (config, warnings) =
            parse("exit_when_exits = \"rsh\"\nexit_when_settled = \"false\"\n");
        assert_eq!(config.exit_when_exits.as_deref(), Some("rsh"));
        assert!(!config.exit_when_settled);
        assert!(warnings.is_empty());
    }

    #[test]
    fn comments_and_blank_lines_are_ignored() {
        let (config, warnings) = parse("# comment\n\n  exit_when_settled = true  # trailing\n");
        assert!(config.exit_when_settled);
        assert!(warnings.is_empty());
    }

    #[test]
    fn unknown_keys_are_warned_and_ignored() {
        let (config, warnings) = parse("colour = blue\nexit_when_settled = true\n");
        assert!(config.exit_when_settled);
        assert_eq!(warnings, ["unknown key colour"]);
    }

    #[test]
    fn bad_booleans_are_warnings() {
        let (config, warnings) = parse("exit_when_settled = yes\n");
        assert!(!config.exit_when_settled);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("true or false"));
    }

    #[test]
    fn invalid_service_names_are_warnings() {
        let (config, warnings) = parse("exit_when_exits = Not A Name\n");
        assert!(config.exit_when_exits.is_none());
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn malformed_lines_are_warnings() {
        let (_, warnings) = parse("this is not a setting\n= value\nkey =\n");
        assert_eq!(warnings.len(), 3);
    }

    #[test]
    fn duplicate_keys_use_the_last_value() {
        let (config, _) = parse("exit_when_settled = true\nexit_when_settled = false\n");
        assert!(!config.exit_when_settled);
    }

    #[test]
    fn unmatched_quotes_fall_back_to_the_literal() {
        let (config, warnings) = parse("exit_when_exits = \"shell\n");
        assert!(config.exit_when_exits.is_none());
        assert_eq!(warnings.len(), 1);
    }
}
