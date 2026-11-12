// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Stable textual frame trace.

use core::fmt;

use crate::Instant;

/// Frame direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    /// From endpoint A to endpoint B.
    AtoB,
    /// From endpoint B to endpoint A.
    BtoA,
}

impl Direction {
    fn label(self) -> &'static str {
        match self {
            Self::AtoB => "A>B",
            Self::BtoA => "B>A",
        }
    }
}

/// One recorded frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceEntry {
    /// Direction.
    pub direction: Direction,
    /// Virtual time at which the frame was put on the wire.
    pub now: Instant,
    /// Frame bytes.
    pub bytes: Vec<u8>,
}

/// A stable, diffable packet trace.
#[derive(Clone, Debug, Default)]
pub struct Trace {
    entries: Vec<TraceEntry>,
}

impl Trace {
    /// Create an empty trace.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record one frame.
    pub fn record(&mut self, direction: Direction, now: Instant, bytes: &[u8]) {
        self.entries.push(TraceEntry {
            direction,
            now,
            bytes: bytes.to_vec(),
        });
    }

    /// All recorded entries.
    #[must_use]
    pub fn entries(&self) -> &[TraceEntry] {
        &self.entries
    }

    /// Remove every entry.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Render one line per frame in a stable tcpdump-ish format:
    /// `A>B t=0 len=4 70696e67`.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut text = String::new();
        for entry in &self.entries {
            let mut hex = String::with_capacity(entry.bytes.len() * 2);
            for byte in &entry.bytes {
                use core::fmt::Write as _;
                let _ = write!(hex, "{byte:02x}");
            }
            use core::fmt::Write as _;
            let _ = writeln!(
                text,
                "{} t={} len={} {}",
                entry.direction.label(),
                entry.now.as_millis(),
                entry.bytes.len(),
                hex
            );
        }
        text
    }

    /// Compare the rendered trace against an exact expected string.
    pub fn assert_matches(&self, expected: &str) -> Result<(), Mismatch> {
        let actual = self.to_text();
        if actual == expected {
            Ok(())
        } else {
            Err(Mismatch {
                expected: expected.to_string(),
                actual,
            })
        }
    }
}

/// A trace comparison failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mismatch {
    /// The expected text.
    pub expected: String,
    /// The actual text.
    pub actual: String,
}

impl fmt::Display for Mismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "trace mismatch\nexpected:\n{}\nactual:\n{}",
            self.expected, self.actual
        )
    }
}

impl std::error::Error for Mismatch {}
