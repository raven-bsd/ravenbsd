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

    /// Add a declared class action.
    ///
    /// # Errors
    ///
    /// Returns `false` when the policy or class string is full/invalid.
    pub fn add_policy(&mut self, class: &str, action: Action) -> bool {
        if self.policy_len >= MAX_POLICY {
            return false;
        }
        let Ok(class) = Str32::new(class) else {
            return false;
        };
        if let Some(slot) = self.policy.get_mut(self.policy_len) {
            *slot = Some(PolicyEntry { class, action });
            self.policy_len += 1;
            true
        } else {
            false
        }
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

    /// The most recent audit records in write order, newest last.
    ///
    /// # Errors
    ///
    /// Returns `None` for an index outside the retained window.
    #[must_use]
    pub fn audit(&self, index: usize) -> Option<AuditRecord> {
        self.audit.get(index).copied().flatten()
    }
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
