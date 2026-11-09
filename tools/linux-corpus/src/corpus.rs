// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Corpus file parsing and placeholder expansion.

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

/// Categories accepted in `corpus.toml`.
pub const CATEGORIES: &[&str] = &[
    "static",
    "dynamic",
    "service",
    "language",
    "toolchain",
    "shell",
    "net",
];

/// One `[[program]]` entry.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Program {
    /// Stable identifier; also the trace/JSON basename.
    pub id: String,
    /// One of [`CATEGORIES`].
    pub category: String,
    /// Command and arguments after `{work}`/`{corpus}` expansion.
    pub argv: Vec<String>,
    /// Binaries that must exist before the program is run.
    #[serde(default)]
    pub requires: Vec<String>,
    /// Shell-free setup commands; each item is an argv array.
    #[serde(default)]
    pub setup: Vec<Vec<String>>,
    /// Optional stdin contents.
    #[serde(default)]
    pub stdin: Option<String>,
    /// Optional environment overrides.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// Expected exit status (default 0).
    #[serde(default)]
    pub expect_exit: i32,
    /// Per-program timeout in seconds (default 30).
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

fn default_timeout() -> u64 {
    30
}

/// The parsed `corpus.toml`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Corpus {
    /// Programs in file order.
    #[serde(default, rename = "program")]
    pub programs: Vec<Program>,
}

impl Corpus {
    /// Parse and validate corpus TOML.
    pub fn parse(text: &str) -> Result<Self> {
        let corpus: Corpus = toml::from_str(text).context("parsing corpus TOML")?;
        corpus.validate()?;
        Ok(corpus)
    }

    /// Read and validate a corpus file.
    pub fn load(path: &Path) -> Result<Self> {
        let text =
            fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        Self::parse(&text).with_context(|| format!("validating corpus {}", path.display()))
    }

    /// Check identifiers, categories, commands and timeouts.
    pub fn validate(&self) -> Result<()> {
        let mut ids = HashSet::new();
        for program in &self.programs {
            validate_id(&program.id)?;
            if !ids.insert(program.id.as_str()) {
                bail!("duplicate program id `{}`", program.id);
            }
            if !CATEGORIES.contains(&program.category.as_str()) {
                bail!(
                    "program `{}` has category `{}`; expected one of {}",
                    program.id,
                    program.category,
                    CATEGORIES.join(", ")
                );
            }
            if program.argv.is_empty() || program.argv[0].is_empty() {
                bail!("program `{}` has an empty argv", program.id);
            }
            if program.timeout_secs == 0 {
                bail!("program `{}` has timeout_secs = 0", program.id);
            }
            for command in &program.setup {
                if command.is_empty() || command[0].is_empty() {
                    bail!("program `{}` has an empty setup command", program.id);
                }
            }
            for required in &program.requires {
                if required.trim().is_empty() {
                    bail!("program `{}` has an empty requires entry", program.id);
                }
            }
            for key in program.env.keys() {
                if key.is_empty() {
                    bail!("program `{}` has an empty environment name", program.id);
                }
            }
        }
        Ok(())
    }
}

fn validate_id(id: &str) -> Result<()> {
    if id.is_empty() {
        bail!("program id must not be empty");
    }
    if id == "." || id == ".." {
        bail!("program id must not be `.` or `..`");
    }
    if id.contains('/') || id.contains('\\') {
        bail!("program id `{id}` must not contain path separators");
    }
    Ok(())
}

/// Replace `{work}` and `{corpus}` in a value.
pub fn expand_placeholders(value: &str, work: &Path, corpus_dir: &Path) -> String {
    value
        .replace("{work}", &work.display().to_string())
        .replace("{corpus}", &corpus_dir.display().to_string())
}

impl Program {
    /// Expand the main command's placeholders.
    pub fn expanded_argv(&self, work: &Path, corpus_dir: &Path) -> Vec<String> {
        self.argv
            .iter()
            .map(|value| expand_placeholders(value, work, corpus_dir))
            .collect()
    }

    /// Expand each setup command's placeholders.
    pub fn expanded_setup(&self, work: &Path, corpus_dir: &Path) -> Vec<Vec<String>> {
        self.setup
            .iter()
            .map(|command| {
                command
                    .iter()
                    .map(|value| expand_placeholders(value, work, corpus_dir))
                    .collect()
            })
            .collect()
    }

    /// Expand environment values.
    pub fn expanded_env(&self, work: &Path, corpus_dir: &Path) -> BTreeMap<String, String> {
        self.env
            .iter()
            .map(|(key, value)| (key.clone(), expand_placeholders(value, work, corpus_dir)))
            .collect()
    }

    /// Expand the optional stdin text.
    pub fn expanded_stdin(&self, work: &Path, corpus_dir: &Path) -> Option<String> {
        self.stdin
            .as_ref()
            .map(|value| expand_placeholders(value, work, corpus_dir))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL: &str = r#"
[[program]]
id = "hello"
category = "static"
argv = ["{work}/hello"]
"#;

    #[test]
    fn minimal_program_uses_defaults() {
        let corpus = Corpus::parse(MINIMAL).unwrap();
        assert_eq!(corpus.programs.len(), 1);
        let program = &corpus.programs[0];
        assert_eq!(program.expect_exit, 0);
        assert_eq!(program.timeout_secs, 30);
        assert!(program.requires.is_empty());
        assert!(program.setup.is_empty());
        assert!(program.stdin.is_none());
    }

    #[test]
    fn all_optional_fields_parse() {
        let text = r#"
[[program]]
id = "full"
category = "shell"
argv = ["sh", "-c", "echo hi"]
requires = ["sh"]
setup = [["sh", "-c", "true"]]
stdin = "input"
env = { FOO = "bar" }
expect_exit = 3
timeout_secs = 7
"#;
        let corpus = Corpus::parse(text).unwrap();
        let program = &corpus.programs[0];
        assert_eq!(program.requires, vec!["sh"]);
        assert_eq!(program.setup.len(), 1);
        assert_eq!(program.stdin.as_deref(), Some("input"));
        assert_eq!(program.env["FOO"], "bar");
        assert_eq!(program.expect_exit, 3);
        assert_eq!(program.timeout_secs, 7);
    }

    #[test]
    fn unknown_key_is_rejected() {
        let text = format!("{MINIMAL}mystery = 1\n");
        let error = format!("{:#}", Corpus::parse(&text).unwrap_err());
        assert!(error.contains("unknown field"), "{error}");
    }

    #[test]
    fn duplicate_id_is_rejected() {
        let text = format!("{MINIMAL}{MINIMAL}");
        let error = Corpus::parse(&text).unwrap_err().to_string();
        assert!(error.contains("duplicate"), "{error}");
    }

    #[test]
    fn bad_category_is_rejected() {
        let text = MINIMAL.replace("static", "misc");
        let error = Corpus::parse(&text).unwrap_err().to_string();
        assert!(error.contains("category"), "{error}");
    }

    #[test]
    fn empty_argv_is_rejected() {
        let text = MINIMAL.replace("argv = [\"{work}/hello\"]", "argv = []");
        let error = Corpus::parse(&text).unwrap_err().to_string();
        assert!(error.contains("argv"), "{error}");
    }

    #[test]
    fn zero_timeout_is_rejected() {
        let text = format!("{MINIMAL}timeout_secs = 0\n");
        let error = Corpus::parse(&text).unwrap_err().to_string();
        assert!(error.contains("timeout"), "{error}");
    }

    #[test]
    fn path_separator_in_id_is_rejected() {
        let text = MINIMAL.replace("id = \"hello\"", "id = \"../hello\"");
        let error = Corpus::parse(&text).unwrap_err().to_string();
        assert!(error.contains("path separators"), "{error}");
    }

    #[test]
    fn placeholders_are_expanded() {
        let program = Program {
            id: "p".to_string(),
            category: "static".to_string(),
            argv: vec!["{work}/bin".to_string(), "{corpus}/p".to_string()],
            requires: Vec::new(),
            setup: vec![vec![
                "cp".to_string(),
                "{corpus}/a".to_string(),
                "{work}/a".to_string(),
            ]],
            stdin: Some("{work}".to_string()),
            env: BTreeMap::from([("X".to_string(), "{corpus}".to_string())]),
            expect_exit: 0,
            timeout_secs: 30,
        };
        let work = Path::new("/tmp/w");
        let corpus = Path::new("/tmp/c");
        assert_eq!(
            program.expanded_argv(work, corpus),
            vec!["/tmp/w/bin", "/tmp/c/p"]
        );
        assert_eq!(program.expanded_setup(work, corpus)[0][1], "/tmp/c/a");
        assert_eq!(
            program.expanded_stdin(work, corpus).as_deref(),
            Some("/tmp/w")
        );
        assert_eq!(program.expanded_env(work, corpus)["X"], "/tmp/c");
    }

    #[test]
    fn committed_corpus_is_valid_and_large_enough() {
        let text = include_str!("../corpus.toml");
        let corpus = Corpus::parse(text).unwrap();
        assert!(
            corpus.programs.len() >= 30,
            "only {} programs",
            corpus.programs.len()
        );
        for category in CATEGORIES {
            assert!(
                corpus
                    .programs
                    .iter()
                    .any(|program| program.category == *category),
                "corpus has no {category} program"
            );
        }
    }
}
