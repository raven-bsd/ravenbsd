// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `rc.conf`-style service overrides.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::manifest::{Manifest, ParseError, ParseErrorKind, Restart};
use crate::parse::{MAX_TEXT_BYTES, error, split_key_value, string_value, strip_comment};

/// One override action applied to a service.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Override {
    /// Force the service enabled or disabled.
    Enable(bool),
    /// Replace the restart policy.
    Restart(Restart),
}

/// Parsed overrides keyed by the service name with `-` replaced by `_`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Overrides {
    /// Entries in source order.
    pub entries: Vec<(String, Override)>,
}

/// An override names a service that is not present in the manifest list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnknownService {
    /// The normalized service key from the override.
    pub service: String,
}

/// Parse the `/etc/rc.conf`-style override file.
///
/// Only `NAME_enable="YES"|"NO"` and `NAME_restart="never"|"on-failure"|"always"` lines are
/// accepted. The service part of the key uses `_` where the manifest name uses `-`.
pub fn parse_overrides(text: &str) -> Result<Overrides, ParseError> {
    if text.len() > MAX_TEXT_BYTES {
        return Err(error(1, ParseErrorKind::TooLarge));
    }
    let mut entries = Vec::new();
    let mut seen = Vec::new();
    for (index, raw_line) in text.lines().enumerate() {
        let line = index + 1;
        let trimmed = strip_comment(raw_line).trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with('[') {
            return Err(error(line, ParseErrorKind::Syntax));
        }
        let Some((key, value)) = split_key_value(trimmed) else {
            return Err(error(line, ParseErrorKind::Syntax));
        };
        if seen.iter().any(|seen_key| seen_key == key) {
            return Err(error(line, ParseErrorKind::DuplicateKey(key.to_string())));
        }
        let (service, action) = if let Some(service) = key.strip_suffix("_enable") {
            if !is_override_service(service) {
                return Err(error(line, ParseErrorKind::InvalidValue("enable")));
            }
            let value = string_value(value, line, "enable")?;
            let enable = match value.as_str() {
                "YES" => true,
                "NO" => false,
                _ => return Err(error(line, ParseErrorKind::InvalidValue("enable"))),
            };
            (service, Override::Enable(enable))
        } else if let Some(service) = key.strip_suffix("_restart") {
            if !is_override_service(service) {
                return Err(error(line, ParseErrorKind::InvalidValue("restart")));
            }
            let value = string_value(value, line, "restart")?;
            let restart = match value.as_str() {
                "never" => Restart::Never,
                "on-failure" => Restart::OnFailure,
                "always" => Restart::Always,
                _ => return Err(error(line, ParseErrorKind::InvalidValue("restart"))),
            };
            (service, Override::Restart(restart))
        } else {
            return Err(error(line, ParseErrorKind::UnknownKey(key.to_string())));
        };
        seen.push(key.to_string());
        entries.push((service.to_string(), action));
    }
    Ok(Overrides { entries })
}

fn is_override_service(service: &str) -> bool {
    !service.is_empty()
        && service.len() <= 32
        && service
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

/// Apply parsed overrides to the manifest list.
///
/// Manifest names are normalized by replacing `-` with `_` before matching. Every override must
/// name at least one manifest; otherwise [`UnknownService`] is returned.
pub fn apply_overrides(
    services: &mut [Manifest],
    overrides: &Overrides,
) -> Result<(), UnknownService> {
    for (service, action) in &overrides.entries {
        let mut found = false;
        for manifest in services.iter_mut() {
            if manifest.name.replace('-', "_") == *service {
                found = true;
                match action {
                    Override::Enable(enable) => manifest.enable = *enable,
                    Override::Restart(restart) => manifest.restart = *restart,
                }
            }
        }
        if !found {
            return Err(UnknownService {
                service: service.clone(),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;
    use alloc::vec;

    fn manifest(name: &str) -> Manifest {
        Manifest {
            name: name.to_string(),
            exec: "/x".to_string(),
            ..Manifest::default()
        }
    }

    fn kind(text: &str) -> ParseErrorKind {
        parse_overrides(text)
            .expect_err("overrides should fail")
            .kind
    }

    #[test]
    fn parses_enable_and_restart() {
        let overrides =
            parse_overrides("# comment\nvfsd_enable = \"NO\"\nvfsd_restart = \"always\"\n")
                .unwrap();
        assert_eq!(
            overrides.entries,
            vec![
                ("vfsd".to_string(), Override::Enable(false)),
                ("vfsd".to_string(), Override::Restart(Restart::Always)),
            ]
        );
    }

    #[test]
    fn applies_overrides() {
        let overrides =
            parse_overrides("vfsd_enable = \"NO\"\nvfsd_restart = \"never\"\n").unwrap();
        let mut services = vec![manifest("vfsd")];
        apply_overrides(&mut services, &overrides).unwrap();
        assert!(!services[0].enable);
        assert_eq!(services[0].restart, Restart::Never);
    }

    #[test]
    fn hyphen_keys_map_to_manifest_names() {
        let overrides = parse_overrides("raven_init_enable = \"NO\"").unwrap();
        let mut services = vec![manifest("raven-init")];
        apply_overrides(&mut services, &overrides).unwrap();
        assert!(!services[0].enable);
    }

    #[test]
    fn unknown_service_is_reported() {
        let overrides = parse_overrides("missing_enable = \"NO\"").unwrap();
        let mut services = vec![manifest("vfsd")];
        let error = apply_overrides(&mut services, &overrides).unwrap_err();
        assert_eq!(error.service, "missing");
    }

    #[test]
    fn invalid_enable_value() {
        assert!(matches!(
            kind("vfsd_enable = \"MAYBE\""),
            ParseErrorKind::InvalidValue("enable")
        ));
    }

    #[test]
    fn invalid_restart_value() {
        assert!(matches!(
            kind("vfsd_restart = \"sometimes\""),
            ParseErrorKind::InvalidValue("restart")
        ));
    }

    #[test]
    fn unknown_override_key() {
        assert!(matches!(
            kind("vfsd_exec = \"/x\""),
            ParseErrorKind::UnknownKey(key) if key == "vfsd_exec"
        ));
    }

    #[test]
    fn duplicate_override_key() {
        assert!(matches!(
            kind("vfsd_enable = \"YES\"\nvfsd_enable = \"NO\""),
            ParseErrorKind::DuplicateKey(key) if key == "vfsd_enable"
        ));
    }

    #[test]
    fn invalid_service_key() {
        assert!(matches!(
            kind("Bad-Name_enable = \"YES\""),
            ParseErrorKind::InvalidValue("enable")
        ));
        assert!(matches!(
            kind("_enable = \"YES\""),
            ParseErrorKind::InvalidValue("enable")
        ));
    }

    #[test]
    fn comments_and_empty_input() {
        assert!(parse_overrides("").unwrap().entries.is_empty());
        assert!(
            parse_overrides("# only a comment\n\n")
                .unwrap()
                .entries
                .is_empty()
        );
        assert_eq!(
            parse_overrides("vfsd_enable = \"YES\" # comment")
                .unwrap()
                .entries
                .len(),
            1
        );
    }

    #[test]
    fn syntax_errors() {
        assert!(matches!(kind("vfsd_enable"), ParseErrorKind::Syntax));
        assert!(matches!(kind("[env]\nA = \"b\""), ParseErrorKind::Syntax));
        assert!(matches!(
            kind("vfsd_enable = YES"),
            ParseErrorKind::InvalidValue("enable")
        ));
    }

    #[test]
    fn text_limit() {
        let text = format!("#{}", "x".repeat(MAX_TEXT_BYTES));
        let error = parse_overrides(&text).unwrap_err();
        assert_eq!(error.line, 1);
        assert!(matches!(error.kind, ParseErrorKind::TooLarge));
    }
}
