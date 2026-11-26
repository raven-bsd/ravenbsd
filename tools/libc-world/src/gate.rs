// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The phase-1/phase-2 gate (section 4.8).

use crate::model::PackageResult;
use crate::world::World;

/// The rendered gate result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GateReport {
    /// One line per criterion.
    pub lines: Vec<String>,
    /// Every criterion met.
    pub met: bool,
}

/// Evaluate the gate.
#[must_use]
pub fn evaluate(results: &[PackageResult], world: &World, phase: u8, min_built: u32) -> GateReport {
    // Only packages named by the world list count: a result for an unknown package is not a
    // built world package and must never satisfy C13.
    let non_optional: Vec<&PackageResult> = results
        .iter()
        .filter(|result| {
            world
                .package(&result.name)
                .is_some_and(|package| !package.optional)
        })
        .collect();
    let built = non_optional
        .iter()
        .filter(|result| result.is_built())
        .count();
    let libc_patches: u32 = results.iter().map(|result| result.libc_patches).sum();
    let configure_flags: u32 = u32::try_from(
        results
            .iter()
            .filter(|result| !result.configure_flags_disabled_because_libc.is_empty())
            .count(),
    )
    .unwrap_or(u32::MAX);
    let probe_failures: Vec<&PackageResult> = results
        .iter()
        .filter(|result| result.is_built() && !result.libc_probe_failed.is_empty())
        .collect();
    let first_patched = results.iter().find(|result| result.libc_patches > 0);
    let first_disabled = results
        .iter()
        .find(|result| !result.configure_flags_disabled_because_libc.is_empty());

    let mut lines = Vec::new();
    let mut met = true;
    match phase {
        1 => {
            let required = min_built as usize;
            let enough = built >= required && built > 0;
            let patches_ok = libc_patches == 0;
            met &= enough && patches_ok;
            lines.push(format!(
                "C13 world build: {built} of {min_built} non-optional packages built; libc_patches {libc_patches} -> {}",
                verdict(enough && patches_ok)
            ));
            met &= patches_ok;
            lines.push(format!(
                "C13 libc patches: {libc_patches} -> {}",
                verdict(patches_ok)
            ));
            if let Some(result) = first_patched {
                lines.push(format!(
                    "refused: package `{}` has {} libc patch(es)",
                    result.name, result.libc_patches
                ));
            }
            met &= configure_flags == 0;
            lines.push(format!(
                "C13 configure flags disabled because libc: {configure_flags} package(s) -> {}",
                verdict(configure_flags == 0)
            ));
            if let Some(result) = first_disabled {
                lines.push(format!(
                    "refused: package `{}` disabled a feature because of libc",
                    result.name
                ));
            }
            met &= probe_failures.is_empty();
            lines.push(format!(
                "C13 libc probes failed in built packages: {} -> {}",
                probe_failures.len(),
                verdict(probe_failures.is_empty())
            ));
            if let Some(result) = probe_failures.first() {
                lines.push(format!(
                    "refused: package `{}` failed probe `{}`",
                    result.name,
                    result
                        .libc_probe_failed
                        .first()
                        .cloned()
                        .unwrap_or_default()
                ));
            }
        }
        _ => {
            let total = world
                .packages()
                .iter()
                .filter(|package| !package.optional)
                .count();
            let all = total > 0 && built == total;
            met &= all;
            lines.push(format!(
                "C13 phase 2: {built} of {total} non-optional packages built -> {}",
                verdict(all)
            ));
            met &= libc_patches == 0;
            lines.push(format!(
                "C13 libc patches: {libc_patches} -> {}",
                verdict(libc_patches == 0)
            ));
        }
    }
    if results.is_empty() {
        met = false;
        lines.push("C13 gate: no package results -> NOT MET".to_string());
    }
    GateReport { lines, met }
}

fn verdict(met: bool) -> &'static str {
    if met { "MET" } else { "NOT MET" }
}

#[cfg(test)]
#[path = "gate_ut.rs"]
mod tests;
