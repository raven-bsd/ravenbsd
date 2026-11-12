// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Parsing and rendering of the committed `CONFORMANCE.md` verdict table.

use std::fmt;
use std::fs;
use std::path::Path;

/// Verdict for one script.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// The script passes on the current stack.
    Pass,
    /// The script fails for a documented reason.
    Fail,
    /// The script cannot run in the current harness.
    Skip,
}

impl Verdict {
    /// Parse the table spelling.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "pass" => Some(Self::Pass),
            "fail" => Some(Self::Fail),
            "skip" => Some(Self::Skip),
            _ => None,
        }
    }

    /// The table spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
            Self::Skip => "skip",
        }
    }
}

/// One row of the conformance table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    /// Script path relative to `scripts/`.
    pub script: String,
    /// Area name.
    pub area: String,
    /// RFC and spec row checked.
    pub reference: String,
    /// Committed verdict.
    pub verdict: Verdict,
    /// One-line reason for a fail or skip.
    pub reason: String,
}

/// The whole table.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Table {
    /// Rows in file order.
    pub rows: Vec<Row>,
}

impl Table {
    /// Read `CONFORMANCE.md`.
    pub fn read(path: &Path) -> Result<Self, String> {
        let text = fs::read_to_string(path)
            .map_err(|error| format!("read {}: {error}", path.display()))?;
        let mut rows = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if !line.starts_with('|') || line.starts_with("|---") {
                continue;
            }
            let fields: Vec<&str> = line.split('|').map(str::trim).collect();
            if fields.len() < 7 || fields[1] == "Script" {
                continue;
            }
            let verdict = Verdict::parse(fields[4])
                .ok_or_else(|| format!("bad verdict {:?} in {line}", fields[4]))?;
            rows.push(Row {
                script: fields[1].trim_matches('`').to_string(),
                area: fields[2].to_string(),
                reference: fields[3].to_string(),
                verdict,
                reason: fields[5].to_string(),
            });
        }
        Ok(Self { rows })
    }

    /// Find a row by script path.
    #[must_use]
    pub fn row(&self, script: &str) -> Option<&Row> {
        self.rows.iter().find(|row| row.script == script)
    }

    /// Render the table back to Markdown.
    #[must_use]
    pub fn render(&self) -> String {
        let mut text = String::from(
            "| Script | Area | RFC / spec | Verdict | Reason |\n\
             |---|---|---|---|---|\n",
        );
        for row in &self.rows {
            text.push_str(&format!(
                "| `{}` | {} | {} | {} | {} |\n",
                row.script,
                row.area,
                row.reference,
                row.verdict.as_str(),
                row.reason
            ));
        }
        text
    }
}

impl fmt::Display for Table {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.render())
    }
}

/// Count verdicts, returning `(pass, fail, skip)`.
#[must_use]
pub fn counts(rows: &[Row]) -> (usize, usize, usize) {
    let mut pass = 0;
    let mut fail = 0;
    let mut skip = 0;
    for row in rows {
        match row.verdict {
            Verdict::Pass => pass += 1,
            Verdict::Fail => fail += 1,
            Verdict::Skip => skip += 1,
        }
    }
    (pass, fail, skip)
}
