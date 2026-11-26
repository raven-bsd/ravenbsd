// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Minimal userland FMA policy engine.
//!
//! Policy is data, not hard-coded Illumos behaviour: each class maps to a declared action and
//! every diagnosis/rejection is recorded in a bounded audit trail. The kernel only transports
//! ereports; it never diagnoses or retires.

#![no_std]
#![forbid(unsafe_code)]

use raven_proto_fm::{Ereport, Str32};

/// Maximum policy entries.
pub const MAX_POLICY: usize = 8;
/// Maximum audit records.
pub const MAX_AUDIT: usize = 16;

/// A rejected policy document.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PolicyError {
    /// More entries than [`MAX_POLICY`].
    TooMany,
    /// The class is empty, too long or not printable ASCII.
    BadClass,
    /// The action is not `ignore`, `retire` or `degrade`.
    BadAction,
    /// A line does not have exactly two whitespace-separated fields.
    BadLine,
}

/// One declared policy action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Ignore the event.
    Ignore,
    /// Retire the resource.
    Retire,
    /// Degrade the resource.
    Degrade,
}

impl Action {
    /// Stable text for an audit log.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ignore => "ignore",
            Self::Retire => "retire",
            Self::Degrade => "degrade",
        }
    }
}

/// One class -> action policy entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PolicyEntry {
    /// Event class.
    pub class: Str32,
    /// Declared action.
    pub action: Action,
}

/// One diagnosis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Diagnosis {
    /// Case identifier.
    pub case: u64,
    /// Declared action.
    pub action: Action,
    /// Affected class.
    pub class: Str32,
    /// Affected resource.
    pub resource: Str32,
    /// Whether a policy matched.
    pub accepted: bool,
}

/// One audit record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AuditRecord {
    /// Time in milliseconds.
    pub timestamp: u64,
    /// Event class.
    pub class: Str32,
    /// Affected resource.
    pub resource: Str32,
    /// Declared action.
    pub action: Action,
    /// Whether the event was accepted/diagnosed.
    pub accepted: bool,
}

/// The bounded fmd engine.
#[derive(Clone, Debug)]
pub struct Fmd {
    policy: [Option<PolicyEntry>; MAX_POLICY],
    policy_len: usize,
    audit: [Option<AuditRecord>; MAX_AUDIT],
    audit_len: usize,
    next_case: u64,
}

impl Default for Fmd {
    fn default() -> Self {
        Self::new()
    }
}

impl Fmd {
    /// Create an empty policy engine.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            policy: [None; MAX_POLICY],
            policy_len: 0,
            audit: [None; MAX_AUDIT],
            audit_len: 0,
            next_case: 1,
        }
    }

    /// Build an engine from a declared policy document.
    ///
    /// Each non-empty, non-comment line is `<class> <action>`. The document is data: no class or
    /// action is special-cased in the engine.
    ///
    /// # Errors
    ///
    /// Returns [`PolicyError`] for a malformed line, an unknown action, a bad class or more than
    /// [`MAX_POLICY`] entries. A rejected document leaves no partially loaded engine behind.
    pub fn from_policy(text: &str) -> Result<Self, PolicyError> {
        let mut engine = Self::new();
        for raw in text.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut fields = line.split_whitespace();
            let class = fields.next().ok_or(PolicyError::BadLine)?;
            let action = fields.next().ok_or(PolicyError::BadLine)?;
            if fields.next().is_some() {
                return Err(PolicyError::BadLine);
            }
            let action = match action {
                "ignore" => Action::Ignore,
                "retire" => Action::Retire,
                "degrade" => Action::Degrade,
                _ => return Err(PolicyError::BadAction),
            };
            let class = Str32::new(class).map_err(|_| PolicyError::BadClass)?;
            engine.push_policy(class, action)?;
        }
        Ok(engine)
    }

    /// Add a declared class action.
    ///
    /// # Errors
    ///
    /// Returns `false` when the policy or class string is full/invalid.
    pub fn add_policy(&mut self, class: &str, action: Action) -> bool {
        let Ok(class) = Str32::new(class) else {
            return false;
        };
        self.push_policy(class, action).is_ok()
    }

    fn push_policy(&mut self, class: Str32, action: Action) -> Result<(), PolicyError> {
        if self.policy_len >= MAX_POLICY {
            return Err(PolicyError::TooMany);
        }
        let slot = self
            .policy
            .get_mut(self.policy_len)
            .ok_or(PolicyError::TooMany)?;
        *slot = Some(PolicyEntry { class, action });
        self.policy_len += 1;
        Ok(())
    }

    /// Number of declared policy entries.
    #[must_use]
    pub const fn policy_count(&self) -> usize {
        self.policy_len
    }

    /// Diagnose one ereport against the declared policy.
    pub fn diagnose(&mut self, event: &Ereport, now: u64) -> Diagnosis {
        let matched = self.policy[..self.policy_len]
            .iter()
            .flatten()
            .find(|entry| entry.class == event.class)
            .map(|entry| entry.action);
        let action = matched.unwrap_or(Action::Ignore);
        let case = self.next_case;
        self.next_case = self.next_case.saturating_add(1);
        self.record(AuditRecord {
            timestamp: now,
            class: event.class,
            resource: event.resource,
            action,
            accepted: true,
        });
        Diagnosis {
            case,
            action,
            class: event.class,
            resource: event.resource,
            accepted: true,
        }
    }

    /// Record a rejected malformed event without diagnosing it.
    pub fn reject(&mut self, now: u64) -> bool {
        let Ok(class) = Str32::new("ereport.malformed") else {
            return false;
        };
        let Ok(resource) = Str32::new("fmd:input") else {
            return false;
        };
        self.record(AuditRecord {
            timestamp: now,
            class,
            resource,
            action: Action::Ignore,
            accepted: false,
        });
        true
    }

    fn record(&mut self, record: AuditRecord) {
        let index = self.audit_len % MAX_AUDIT;
        if let Some(slot) = self.audit.get_mut(index) {
            *slot = Some(record);
            self.audit_len = self.audit_len.saturating_add(1);
        }
    }

    /// Number of audit records written (may exceed [`MAX_AUDIT`] after wrap).
    #[must_use]
    pub const fn audit_count(&self) -> usize {
        self.audit_len
    }

    /// The retained audit records in write order, oldest first and newest last.
    ///
    /// # Errors
    ///
    /// Returns `None` for an index outside the retained window.
    #[must_use]
    pub fn audit(&self, index: usize) -> Option<AuditRecord> {
        let retained = self.audit_len.min(MAX_AUDIT);
        if index >= retained {
            return None;
        }
        let oldest = self.audit_len.saturating_sub(retained) % MAX_AUDIT;
        let slot = (oldest + index) % MAX_AUDIT;
        self.audit.get(slot).copied().flatten()
    }
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
