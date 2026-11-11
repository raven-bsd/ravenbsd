// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The service manifest type and its strict TOML-subset parser.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::parse::{
    MAX_TEXT_BYTES, array_value, bool_value, error, integer_value, is_env_key, is_key_text,
    is_scheme_name, is_service_name, scheme_name, split_key_value, string_value, strip_comment,
};

/// Restart policy for a service that exits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Restart {
    /// Never restart.
    Never,
    /// Restart only when the service exits with a non-zero status.
    #[default]
    OnFailure,
    /// Always restart.
    Always,
}

/// How `init` decides that a service is ready.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Readiness {
    /// Ready when the service's scheme appears.
    Scheme,
    /// Ready when the service exits successfully.
    Exit,
    /// No readiness wait.
    #[default]
    None,
}

/// One parsed service manifest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Manifest {
    /// Service name, `[a-z0-9_-]{1,32}` and unique within a graph.
    pub name: String,
    /// Absolute path of the executable.
    pub exec: String,
    /// Fixed arguments passed to the executable.
    pub args: Vec<String>,
    /// Service names or `scheme:<name>` references that must start first.
    pub needs: Vec<String>,
    /// Schemes this service registers, each as `scheme:<name>`.
    pub provides: Vec<String>,
    /// Scheme names visible to the service.
    pub namespace: Vec<String>,
    /// Free-form rights strings for now.
    pub rights: Vec<String>,
    /// Restart policy.
    pub restart: Restart,
    /// Delay before a restart, in milliseconds, `0..=60000`.
    pub restart_delay_ms: u32,
    /// Maximum restart attempts.
    pub max_restarts: u32,
    /// Readiness policy.
    pub readiness: Readiness,
    /// How long readiness may take, in milliseconds, `0..=60000`.
    pub ready_timeout_ms: u32,
    /// Whether the service is required for a successful boot.
    pub required: bool,
    /// Window in which restart attempts are counted, in milliseconds, `0..=3600000`.
    pub window_ms: u32,
    /// Capabilities transferred to the child before its first thread starts.
    pub grants: Vec<String>,
    /// Whether the service participates in the start plan.
    pub enable: bool,
    /// Environment entries from the optional `[env]` table.
    pub env: Vec<(String, String)>,
}

impl Default for Manifest {
    fn default() -> Self {
        Self {
            name: String::new(),
            exec: String::new(),
            args: Vec::new(),
            needs: Vec::new(),
            provides: Vec::new(),
            namespace: Vec::new(),
            rights: Vec::new(),
            restart: Restart::OnFailure,
            restart_delay_ms: 200,
            max_restarts: 5,
            readiness: Readiness::None,
            ready_timeout_ms: 5_000,
            required: true,
            window_ms: 60_000,
            grants: Vec::new(),
            enable: true,
            env: Vec::new(),
        }
    }
}

/// A parse error with its 1-based source line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    /// 1-based line number of the offending input.
    pub line: usize,
    /// What was wrong.
    pub kind: ParseErrorKind,
}

/// The reason a manifest or override text failed to parse.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParseErrorKind {
    /// Malformed syntax: missing `=`, bad quoting, trailing text or a bad table header.
    Syntax,
    /// A key that is not part of the documented subset.
    UnknownKey(String),
    /// A key that appears more than once.
    DuplicateKey(String),
    /// A required key is absent.
    MissingKey(&'static str),
    /// A value has the wrong type or an unsupported spelling for its key.
    InvalidValue(&'static str),
    /// An integer value is outside the allowed range for its key.
    IntegerOutOfRange(&'static str),
    /// `name` is not `[a-z0-9_-]{1,32}`.
    InvalidName,
    /// `exec` is not an absolute path.
    InvalidExec,
    /// The input or one of its values exceeds a documented limit.
    TooLarge,
}

impl Manifest {
    /// Parse one manifest using the exact TOML subset from CONTRACTS K10.
    ///
    /// The parser is line-based and never panics on arbitrary input. Errors identify the 1-based
    /// line where the problem was detected.
    pub fn parse(text: &str) -> Result<Self, ParseError> {
        if text.len() > MAX_TEXT_BYTES {
            return Err(error(1, ParseErrorKind::TooLarge));
        }

        let mut name = None;
        let mut exec = None;
        let mut args = Vec::new();
        let mut needs = Vec::new();
        let mut provides = Vec::new();
        let mut namespace = Vec::new();
        let mut rights = Vec::new();
        let mut restart = Restart::OnFailure;
        let mut restart_delay_ms = 200;
        let mut max_restarts = 5;
        let mut readiness = Readiness::None;
        let mut ready_timeout_ms = 5_000;
        let mut required = true;
        let mut window_ms = 60_000;
        let mut grants = Vec::new();
        let mut enable = true;
        let mut env = Vec::new();

        let mut seen = Vec::new();
        let mut env_seen = Vec::new();
        let mut in_env = false;
        let mut last_line = 1;

        for (index, raw_line) in text.lines().enumerate() {
            let line = index + 1;
            last_line = line;
            let trimmed = strip_comment(raw_line).trim();
            if trimmed.is_empty() {
                continue;
            }
            if trimmed.starts_with('[') {
                if trimmed != "[env]" {
                    return Err(error(line, ParseErrorKind::Syntax));
                }
                if in_env {
                    return Err(error(line, ParseErrorKind::DuplicateKey("env".to_string())));
                }
                in_env = true;
                continue;
            }
            let Some((key, value)) = split_key_value(trimmed) else {
                return Err(error(line, ParseErrorKind::Syntax));
            };
            if !is_key_text(key) {
                return Err(error(line, ParseErrorKind::Syntax));
            }

            if in_env {
                if !is_env_key(key) {
                    return Err(error(line, ParseErrorKind::InvalidValue("env")));
                }
                if env_seen.iter().any(|seen_key| seen_key == key) {
                    return Err(error(line, ParseErrorKind::DuplicateKey(key.to_string())));
                }
                let value = string_value(value, line, "env")?;
                env_seen.push(key.to_string());
                env.push((key.to_string(), value));
                continue;
            }

            if seen.iter().any(|seen_key| seen_key == key) {
                return Err(error(line, ParseErrorKind::DuplicateKey(key.to_string())));
            }

            match key {
                "name" => {
                    let value = string_value(value, line, "name")?;
                    if !is_service_name(&value) {
                        return Err(error(line, ParseErrorKind::InvalidName));
                    }
                    name = Some(value);
                }
                "exec" => {
                    let value = string_value(value, line, "exec")?;
                    if !value.starts_with('/') {
                        return Err(error(line, ParseErrorKind::InvalidExec));
                    }
                    exec = Some(value);
                }
                "args" => args = array_value(value, line, "args")?,
                "needs" => {
                    let values = array_value(value, line, "needs")?;
                    for need in &values {
                        if scheme_name(need).is_none() && !is_service_name(need) {
                            return Err(error(line, ParseErrorKind::InvalidValue("needs")));
                        }
                    }
                    needs = values;
                }
                "provides" => {
                    let values = array_value(value, line, "provides")?;
                    if values.iter().any(|item| scheme_name(item).is_none()) {
                        return Err(error(line, ParseErrorKind::InvalidValue("provides")));
                    }
                    provides = values;
                }
                "namespace" => {
                    let values = array_value(value, line, "namespace")?;
                    if values.iter().any(|item| !is_scheme_name(item)) {
                        return Err(error(line, ParseErrorKind::InvalidValue("namespace")));
                    }
                    namespace = values;
                }
                "rights" => rights = array_value(value, line, "rights")?,
                "restart" => {
                    let value = string_value(value, line, "restart")?;
                    restart = match value.as_str() {
                        "never" => Restart::Never,
                        "on-failure" => Restart::OnFailure,
                        "always" => Restart::Always,
                        _ => return Err(error(line, ParseErrorKind::InvalidValue("restart"))),
                    };
                }
                "restart_delay_ms" => {
                    let value = integer_value(value, line, "restart_delay_ms")?;
                    if value > 60_000 {
                        return Err(error(
                            line,
                            ParseErrorKind::IntegerOutOfRange("restart_delay_ms"),
                        ));
                    }
                    restart_delay_ms = value as u32;
                }
                "max_restarts" => {
                    let value = integer_value(value, line, "max_restarts")?;
                    let Ok(value) = u32::try_from(value) else {
                        return Err(error(
                            line,
                            ParseErrorKind::IntegerOutOfRange("max_restarts"),
                        ));
                    };
                    max_restarts = value;
                }
                "readiness" => {
                    let value = string_value(value, line, "readiness")?;
                    readiness = match value.as_str() {
                        "scheme" => Readiness::Scheme,
                        "exit" => Readiness::Exit,
                        "none" => Readiness::None,
                        _ => return Err(error(line, ParseErrorKind::InvalidValue("readiness"))),
                    };
                }
                "ready_timeout_ms" => {
                    let value = integer_value(value, line, "ready_timeout_ms")?;
                    if value > 60_000 {
                        return Err(error(
                            line,
                            ParseErrorKind::IntegerOutOfRange("ready_timeout_ms"),
                        ));
                    }
                    ready_timeout_ms = value as u32;
                }
                "required" => required = bool_value(value, line, "required")?,
                "window_ms" => {
                    let value = integer_value(value, line, "window_ms")?;
                    if value > 3_600_000 {
                        return Err(error(line, ParseErrorKind::IntegerOutOfRange("window_ms")));
                    }
                    window_ms = value as u32;
                }
                "grants" => {
                    let values = array_value(value, line, "grants")?;
                    if values.iter().any(|item| !is_service_name(item)) {
                        return Err(error(line, ParseErrorKind::InvalidValue("grants")));
                    }
                    grants = values;
                }
                "enable" => enable = bool_value(value, line, "enable")?,
                _ => return Err(error(line, ParseErrorKind::UnknownKey(key.to_string()))),
            }
            seen.push(key.to_string());
        }

        let error_line = last_line.max(1);
        let Some(name) = name else {
            return Err(error(error_line, ParseErrorKind::MissingKey("name")));
        };
        let Some(exec) = exec else {
            return Err(error(error_line, ParseErrorKind::MissingKey("exec")));
        };
        Ok(Self {
            name,
            exec,
            args,
            needs,
            provides,
            namespace,
            rights,
            restart,
            restart_delay_ms,
            max_restarts,
            readiness,
            ready_timeout_ms,
            required,
            window_ms,
            grants,
            enable,
            env,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Manifest {
        Manifest::parse(text).expect("manifest should parse")
    }

    fn kind(text: &str) -> ParseErrorKind {
        Manifest::parse(text)
            .expect_err("manifest should fail")
            .kind
    }

    fn line(text: &str) -> usize {
        Manifest::parse(text)
            .expect_err("manifest should fail")
            .line
    }

    #[test]
    fn valid_manifest_with_every_key() {
        let manifest = parse(
            r#"
name = "vfsd"
exec = "/sbin/vfsd"
args = ["--foreground", "x"]
needs = ["logd", "scheme:block"]
provides = ["scheme:file"]
namespace = ["log", "block", "proc"]
rights = ["register:file", "map"]
restart = "always"
restart_delay_ms = 1_500
max_restarts = 7
readiness = "scheme"
ready_timeout_ms = 7_500
required = false
window_ms = 120_000
grants = ["initfs"]
enable = false

[env]
PATH = "/bin"
FOO = "bar"
"#,
        );
        assert_eq!(manifest.name, "vfsd");
        assert_eq!(manifest.exec, "/sbin/vfsd");
        assert_eq!(manifest.args, ["--foreground", "x"]);
        assert_eq!(manifest.needs, ["logd", "scheme:block"]);
        assert_eq!(manifest.provides, ["scheme:file"]);
        assert_eq!(manifest.namespace, ["log", "block", "proc"]);
        assert_eq!(manifest.rights, ["register:file", "map"]);
        assert_eq!(manifest.restart, Restart::Always);
        assert_eq!(manifest.restart_delay_ms, 1500);
        assert_eq!(manifest.max_restarts, 7);
        assert_eq!(manifest.readiness, Readiness::Scheme);
        assert_eq!(manifest.ready_timeout_ms, 7500);
        assert!(!manifest.required);
        assert_eq!(manifest.window_ms, 120_000);
        assert_eq!(manifest.grants, ["initfs"]);
        assert!(!manifest.enable);
        assert_eq!(
            manifest.env,
            [
                ("PATH".to_string(), "/bin".to_string()),
                ("FOO".to_string(), "bar".to_string())
            ]
        );
    }

    #[test]
    fn minimal_defaults() {
        let manifest = parse("name = \"x\"\nexec = \"/x\"");
        assert_eq!(manifest.restart, Restart::OnFailure);
        assert_eq!(manifest.restart_delay_ms, 200);
        assert_eq!(manifest.max_restarts, 5);
        assert_eq!(manifest.readiness, Readiness::None);
        assert_eq!(manifest.ready_timeout_ms, 5_000);
        assert!(manifest.required);
        assert_eq!(manifest.window_ms, 60_000);
        assert!(manifest.grants.is_empty());
        assert!(manifest.enable);
        assert!(manifest.args.is_empty());
        assert!(manifest.env.is_empty());
    }

    #[test]
    fn comments_and_hash_inside_string() {
        let manifest = parse("name = \"x\" # comment\nexec = \"/x#y\" # another\n");
        assert_eq!(manifest.exec, "/x#y");
    }

    #[test]
    fn escapes_are_decoded() {
        let manifest = parse("name = \"x\"\nexec = \"/x\"\nargs = [\"a\\\"b\\\\c\\nd\\te\"]");
        assert_eq!(manifest.args, ["a\"b\\c\nd\te"]);
    }

    #[test]
    fn empty_arrays_are_allowed() {
        let manifest = parse(
            "name = \"x\"\nexec = \"/x\"\nargs = []\nneeds = []\nprovides = []\nnamespace = []\nrights = []",
        );
        assert!(manifest.args.is_empty());
        assert!(manifest.needs.is_empty());
        assert!(manifest.provides.is_empty());
        assert!(manifest.namespace.is_empty());
        assert!(manifest.rights.is_empty());
    }

    #[test]
    fn env_table_parses_only_strings() {
        let manifest = parse("name = \"x\"\nexec = \"/x\"\n[env]\nA = \"1\"\nB = \"two\"");
        assert_eq!(manifest.env.len(), 2);
        assert_eq!(manifest.env[0], ("A".to_string(), "1".to_string()));
        assert!(matches!(
            kind("name = \"x\"\nexec = \"/x\"\n[env]\nA = 1"),
            ParseErrorKind::InvalidValue("env")
        ));
    }

    #[test]
    fn name_boundaries() {
        let name32 = "a".repeat(32);
        let manifest = parse(&format!("name = \"{name32}\"\nexec = \"/x\""));
        assert_eq!(manifest.name.len(), 32);
        let name33 = "a".repeat(33);
        assert!(matches!(
            kind(&format!("name = \"{name33}\"\nexec = \"/x\"")),
            ParseErrorKind::InvalidName
        ));
        assert!(matches!(
            kind("name = \"UPPER\"\nexec = \"/x\""),
            ParseErrorKind::InvalidName
        ));
    }

    #[test]
    fn exec_must_be_absolute() {
        assert!(matches!(
            kind("name = \"x\"\nexec = \"bin/x\""),
            ParseErrorKind::InvalidExec
        ));
    }

    #[test]
    fn invalid_restart_and_readiness() {
        assert!(matches!(
            kind("name = \"x\"\nexec = \"/x\"\nrestart = \"sometimes\""),
            ParseErrorKind::InvalidValue("restart")
        ));
        assert!(matches!(
            kind("name = \"x\"\nexec = \"/x\"\nreadiness = \"ready\""),
            ParseErrorKind::InvalidValue("readiness")
        ));
    }

    #[test]
    fn unknown_key_reports_its_line() {
        let error = Manifest::parse("name = \"x\"\nexec = \"/x\"\nbogus = \"y\"").unwrap_err();
        assert_eq!(error.line, 3);
        assert!(matches!(error.kind, ParseErrorKind::UnknownKey(key) if key == "bogus"));
    }

    #[test]
    fn duplicate_key_reports_its_line() {
        let error = Manifest::parse("name = \"x\"\nname = \"y\"").unwrap_err();
        assert_eq!(error.line, 2);
        assert!(matches!(error.kind, ParseErrorKind::DuplicateKey(key) if key == "name"));
    }

    #[test]
    fn missing_required_keys() {
        assert!(matches!(
            kind("exec = \"/x\""),
            ParseErrorKind::MissingKey("name")
        ));
        assert!(matches!(
            kind("name = \"x\""),
            ParseErrorKind::MissingKey("exec")
        ));
        assert!(matches!(kind(""), ParseErrorKind::MissingKey("name")));
    }

    #[test]
    fn wrong_types_are_rejected() {
        assert!(matches!(
            kind("name = 1\nexec = \"/x\""),
            ParseErrorKind::InvalidValue("name")
        ));
        assert!(matches!(
            kind("name = \"x\"\nexec = \"/x\"\nenable = \"true\""),
            ParseErrorKind::InvalidValue("enable")
        ));
        assert!(matches!(
            kind("name = \"x\"\nexec = \"/x\"\nargs = \"a\""),
            ParseErrorKind::InvalidValue("args")
        ));
        assert!(matches!(
            kind("name = \"x\"\nexec = \"/x\"\nrestart_delay_ms = \"1\""),
            ParseErrorKind::InvalidValue("restart_delay_ms")
        ));
    }

    #[test]
    fn integer_underscores_and_ranges() {
        let manifest = parse("name = \"x\"\nexec = \"/x\"\nrestart_delay_ms = 60_000");
        assert_eq!(manifest.restart_delay_ms, 60_000);
        assert!(matches!(
            kind("name = \"x\"\nexec = \"/x\"\nrestart_delay_ms = 60001"),
            ParseErrorKind::IntegerOutOfRange("restart_delay_ms")
        ));
        assert!(matches!(
            kind("name = \"x\"\nexec = \"/x\"\nrestart_delay_ms = 1__5"),
            ParseErrorKind::Syntax
        ));
        assert!(matches!(
            kind("name = \"x\"\nexec = \"/x\"\nmax_restarts = 4294967296"),
            ParseErrorKind::IntegerOutOfRange("max_restarts")
        ));
    }

    #[test]
    fn array_entry_limit() {
        let entries = (0..65)
            .map(|index| format!("\"a{index}\""))
            .collect::<Vec<_>>()
            .join(", ");
        assert!(matches!(
            kind(&format!("name = \"x\"\nexec = \"/x\"\nargs = [{entries}]")),
            ParseErrorKind::TooLarge
        ));
    }

    #[test]
    fn string_length_limit() {
        let long = "x".repeat(257);
        assert!(matches!(
            kind(&format!("name = \"x\"\nexec = \"/{long}\"")),
            ParseErrorKind::TooLarge
        ));
    }

    #[test]
    fn text_size_limit() {
        let text = format!("#{}", "x".repeat(MAX_TEXT_BYTES));
        let error = Manifest::parse(&text).unwrap_err();
        assert_eq!(error.line, 1);
        assert!(matches!(error.kind, ParseErrorKind::TooLarge));
    }

    #[test]
    fn syntax_errors_are_line_tagged() {
        assert_eq!(line("name = \"x\"\nexec = \"/x\"\n[other]"), 3);
        assert!(matches!(
            kind("name = \"x\"\nexec = \"/x\"\n[env]\n[env]"),
            ParseErrorKind::DuplicateKey(key) if key == "env"
        ));
        assert!(matches!(kind("name = \"x\"\nexec"), ParseErrorKind::Syntax));
        assert!(matches!(
            kind("name = \"x\"\nexec = \"/x\"\nargs = [\"a\""),
            ParseErrorKind::Syntax
        ));
        assert!(matches!(
            kind("name = \"x\"\nexec = \"/x\"\nargs = [\"a\\q\"]"),
            ParseErrorKind::Syntax
        ));
        assert!(matches!(
            kind("name = \"x\"\nexec = \"/x\"\nica = \"y\""),
            ParseErrorKind::UnknownKey(_)
        ));
    }

    #[test]
    fn env_errors_are_line_tagged() {
        assert_eq!(line("name = \"x\"\nexec = \"/x\"\n[env]\n1BAD = \"v\""), 4);
        assert!(matches!(
            kind("name = \"x\"\nexec = \"/x\"\n[env]\nA = \"1\"\nA = \"2\""),
            ParseErrorKind::DuplicateKey(key) if key == "A"
        ));
        assert!(matches!(
            kind("name = \"x\"\nexec = \"/x\"\n[env]\nA = true"),
            ParseErrorKind::InvalidValue("env")
        ));
    }

    #[test]
    fn needs_and_provides_validate_shape() {
        assert!(matches!(
            kind("name = \"x\"\nexec = \"/x\"\nneeds = [\"Bad Name\"]"),
            ParseErrorKind::InvalidValue("needs")
        ));
        assert!(matches!(
            kind("name = \"x\"\nexec = \"/x\"\nprovides = [\"file\"]"),
            ParseErrorKind::InvalidValue("provides")
        ));
        assert!(matches!(
            kind("name = \"x\"\nexec = \"/x\"\nnamespace = [\"Bad Name\"]"),
            ParseErrorKind::InvalidValue("namespace")
        ));
    }

    #[test]
    fn rights_are_free_form() {
        let manifest = parse("name = \"x\"\nexec = \"/x\"\nrights = [\"any:thing\", \"\"]");
        assert_eq!(manifest.rights, ["any:thing", ""]);
    }

    #[test]
    fn max_restarts_boundaries() {
        let manifest = parse("name = \"x\"\nexec = \"/x\"\nmax_restarts = 4294967295");
        assert_eq!(manifest.max_restarts, u32::MAX);
        let manifest = parse("name = \"x\"\nexec = \"/x\"\nmax_restarts = 0");
        assert_eq!(manifest.max_restarts, 0);
    }

    #[test]
    fn grants_parse_in_order() {
        let manifest = parse("name = \"x\"\nexec = \"/x\"\ngrants = [\"initfs\", \"block\"]");
        assert_eq!(manifest.grants, ["initfs", "block"]);
    }

    #[test]
    fn grants_reject_bad_names() {
        assert!(matches!(
            kind("name = \"x\"\nexec = \"/x\"\ngrants = [\"Init FS\"]"),
            ParseErrorKind::InvalidValue("grants")
        ));
        assert!(matches!(
            kind("name = \"x\"\nexec = \"/x\"\ngrants = [\"\"]"),
            ParseErrorKind::InvalidValue("grants")
        ));
    }

    #[test]
    fn ready_timeout_defaults_and_boundaries() {
        let manifest = parse("name = \"x\"\nexec = \"/x\"");
        assert_eq!(manifest.ready_timeout_ms, 5_000);
        let manifest = parse("name = \"x\"\nexec = \"/x\"\nready_timeout_ms = 0");
        assert_eq!(manifest.ready_timeout_ms, 0);
        let manifest = parse("name = \"x\"\nexec = \"/x\"\nready_timeout_ms = 60_000");
        assert_eq!(manifest.ready_timeout_ms, 60_000);
        assert!(matches!(
            kind("name = \"x\"\nexec = \"/x\"\nready_timeout_ms = 60001"),
            ParseErrorKind::IntegerOutOfRange("ready_timeout_ms")
        ));
    }

    #[test]
    fn required_defaults_to_true() {
        let manifest = parse("name = \"x\"\nexec = \"/x\"");
        assert!(manifest.required);
        let manifest = parse("name = \"x\"\nexec = \"/x\"\nrequired = false");
        assert!(!manifest.required);
    }

    #[test]
    fn window_ms_defaults_and_boundaries() {
        let manifest = parse("name = \"x\"\nexec = \"/x\"");
        assert_eq!(manifest.window_ms, 60_000);
        let manifest = parse("name = \"x\"\nexec = \"/x\"\nwindow_ms = 3_600_000");
        assert_eq!(manifest.window_ms, 3_600_000);
        assert!(matches!(
            kind("name = \"x\"\nexec = \"/x\"\nwindow_ms = 3600001"),
            ParseErrorKind::IntegerOutOfRange("window_ms")
        ));
    }

    #[test]
    fn new_extension_types_are_strict() {
        assert!(matches!(
            kind("name = \"x\"\nexec = \"/x\"\ngrants = \"initfs\""),
            ParseErrorKind::InvalidValue("grants")
        ));
        assert!(matches!(
            kind("name = \"x\"\nexec = \"/x\"\nrequired = \"true\""),
            ParseErrorKind::InvalidValue("required")
        ));
        assert!(matches!(
            kind("name = \"x\"\nexec = \"/x\"\nready_timeout_ms = \"5000\""),
            ParseErrorKind::InvalidValue("ready_timeout_ms")
        ));
        assert!(matches!(
            kind("name = \"x\"\nexec = \"/x\"\nwindow_ms = \"60000\""),
            ParseErrorKind::InvalidValue("window_ms")
        ));
    }

    #[test]
    fn duplicate_extension_keys_are_rejected() {
        assert!(matches!(
            kind("name = \"x\"\nexec = \"/x\"\nrequired = true\nrequired = false"),
            ParseErrorKind::DuplicateKey(key) if key == "required"
        ));
        assert!(matches!(
            kind("name = \"x\"\nexec = \"/x\"\ngrants = []\ngrants = []"),
            ParseErrorKind::DuplicateKey(key) if key == "grants"
        ));
    }

    #[test]
    fn old_manifests_keep_their_meaning() {
        let manifest = parse(
            "name = \"vfsd\"\nexec = \"/sbin/vfsd\"\nneeds = [\"logd\"]\nreadiness = \"scheme\"",
        );
        assert_eq!(manifest.needs, ["logd"]);
        assert_eq!(manifest.readiness, Readiness::Scheme);
        assert_eq!(manifest.restart, Restart::OnFailure);
        assert_eq!(manifest.restart_delay_ms, 200);
        assert_eq!(manifest.max_restarts, 5);
        assert_eq!(manifest.ready_timeout_ms, 5_000);
        assert!(manifest.required);
        assert!(manifest.grants.is_empty());
    }
}
