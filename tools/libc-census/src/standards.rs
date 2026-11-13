// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Standard-tag mapping and the mechanical ring rule of doc 02 section 1.

use std::collections::BTreeSet;

use anyhow::{Result, bail};

use crate::model::{Assignment, Kind, Ring, donor_family};

/// The standards that make ring 0 (ticket 4.4).
pub const RING0_TAGS: [&str; 6] = ["iso-c", "iso-c11", "iso-c23", "posix", "posix-2024", "xsi"];

/// Map one glibc manual tag to the `standard` field spelling.
pub fn map_tag(tag: &str) -> String {
    let mapped = match tag {
        "ISO" | "C90" | "C99" => "iso-c",
        "C11" => "iso-c11",
        "C23" => "iso-c23",
        "POSIX" | "POSIX.1" | "POSIX.1b" | "POSIX.1-2008" | "POSIX.2" => "posix",
        "POSIX.1-2024" | "POSIX-1.2024" => "posix-2024",
        "Unix98" | "X/Open" | "XPG4.2" => "xsi",
        "SVID" => "svid",
        "BSD" => "bsd",
        "GNU" => "gnu",
        "Linux" | "Linux???" => "linux",
        "TS 18661-4:2015" => "ts18661",
        other => return other.to_lowercase().replace(' ', "-"),
    };
    mapped.to_string()
}

/// Map and sort a set of tags.
pub fn map_tags<I, S>(tags: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let set: BTreeSet<String> = tags
        .into_iter()
        .map(|tag| map_tag(tag.as_ref()))
        .filter(|tag| !tag.is_empty())
        .collect();
    set.into_iter().collect()
}

/// Whether any tag places the name in ring 0.
pub fn is_ring0(standard: &[String]) -> bool {
    standard
        .iter()
        .any(|tag| RING0_TAGS.contains(&tag.as_str()))
}

/// The result of the mechanical rule.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ComputedRing {
    /// Ring 0-3.
    pub ring: u8,
    /// Single family for a ring-2 name.
    pub personality: Option<String>,
}

/// Apply doc 02 section 1 to standards and families.
pub fn compute_ring(standard: &[String], families: &[String]) -> ComputedRing {
    if is_ring0(standard) {
        return ComputedRing {
            ring: 0,
            personality: None,
        };
    }
    match families.len() {
        0 => ComputedRing {
            ring: 3,
            personality: None,
        },
        1 => ComputedRing {
            ring: 2,
            personality: families.first().cloned(),
        },
        _ => ComputedRing {
            ring: 1,
            personality: None,
        },
    }
}

/// Families represented by a set of libc names, sorted.
pub fn families_of(libcs: &[String]) -> Vec<String> {
    let families: BTreeSet<&str> = libcs
        .iter()
        .filter_map(|donor| donor_family(donor))
        .collect();
    families.into_iter().map(str::to_string).collect()
}

/// The final ring decision for one name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RingDecision {
    /// Final ring.
    pub ring: Ring,
    /// `assignment` or `computed`.
    pub source: &'static str,
    /// Reason kept when the assignment differs.
    pub ring_override: Option<String>,
}

/// Combine the computed ring with the reviewer assignment.
///
/// A differing assignment requires a non-empty `ring_override`; this is the
/// invariant tested by ticket 7(f).
pub fn decide_ring(
    computed: &ComputedRing,
    assignment: Option<&Assignment>,
) -> Result<RingDecision> {
    let Some(assignment) = assignment else {
        return Ok(RingDecision {
            ring: Ring::Number(computed.ring),
            source: "computed",
            ring_override: None,
        });
    };
    let Some(ring) = assignment.ring.clone() else {
        return Ok(RingDecision {
            ring: Ring::Number(computed.ring),
            source: "computed",
            ring_override: None,
        });
    };
    ring.validate()?;
    if let Some(number) = ring.number()
        && number != computed.ring
        && assignment
            .ring_override
            .as_deref()
            .is_none_or(|reason| reason.trim().is_empty())
    {
        bail!(
            "assignment for `{}` changes ring {} -> {} without ring_override",
            assignment.name,
            computed.ring,
            number
        );
    }
    Ok(RingDecision {
        ring,
        source: "assignment",
        ring_override: assignment.ring_override.clone(),
    })
}

/// Kind from a standard entry, defaulting to `func`.
pub fn kind_from_standard(entry: Option<&crate::model::StandardEntry>) -> Kind {
    entry
        .and_then(|entry| Kind::parse(&entry.kind).ok())
        .unwrap_or(Kind::Func)
}

#[cfg(test)]
#[path = "standards_ut.rs"]
mod tests;
