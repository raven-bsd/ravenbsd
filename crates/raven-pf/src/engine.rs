// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Stateless and stateful match engine with per-rule and global counters.

use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};

use crate::ast::{Action, IfaceSpec, Packet, Packet6, Ruleset, StatePolicy, Translation};
use crate::state::{StateDisposition, StateTable, TcpState, Timeouts};

pub use crate::parser::MAX_RULES;

/// One rule's counter block.
#[derive(Debug, Default)]
struct RuleStats {
    evaluations: AtomicU64,
    matches: AtomicU64,
    packets: AtomicU64,
    bytes: AtomicU64,
}

/// A plain snapshot of one rule's counters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RuleStatSnapshot {
    /// Times the rule was tested against a packet.
    pub evaluations: u64,
    /// Times the rule matched.
    pub matches: u64,
    /// Packets passed or blocked by this rule (including state hits).
    pub packets: u64,
    /// Bytes passed or blocked by this rule.
    pub bytes: u64,
}

/// Global counters.
#[derive(Debug, Default)]
struct Counters {
    in_pass: AtomicU64,
    in_block: AtomicU64,
    out_pass: AtomicU64,
    out_block: AtomicU64,
    states_created: AtomicU64,
    states_expired: AtomicU64,
    state_hits: AtomicU64,
}

/// A plain snapshot of the global counters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CounterSnapshot {
    /// Packets passed in the inbound direction.
    pub in_pass: u64,
    /// Packets blocked in the inbound direction.
    pub in_block: u64,
    /// Packets passed in the outbound direction.
    pub out_pass: u64,
    /// Packets blocked in the outbound direction.
    pub out_block: u64,
    /// Connections created.
    pub states_created: u64,
    /// Connections expired or evicted.
    pub states_expired: u64,
    /// Packets allowed by an existing state.
    pub state_hits: u64,
}

impl Counters {
    fn snapshot(&self) -> CounterSnapshot {
        CounterSnapshot {
            in_pass: self.in_pass.load(Ordering::Relaxed),
            in_block: self.in_block.load(Ordering::Relaxed),
            out_pass: self.out_pass.load(Ordering::Relaxed),
            out_block: self.out_block.load(Ordering::Relaxed),
            states_created: self.states_created.load(Ordering::Relaxed),
            states_expired: self.states_expired.load(Ordering::Relaxed),
            state_hits: self.state_hits.load(Ordering::Relaxed),
        }
    }
}

/// The filter verdict.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Forward the packet.
    Pass,
    /// Drop the packet.
    Block,
}

/// One evaluation result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Decision {
    /// Pass or block.
    pub verdict: Verdict,
    /// Creating or matching rule index, or `None` for the default action.
    pub rule: Option<usize>,
    /// State disposition for a stateful pass.
    pub state: Option<StateDisposition>,
    /// TCP state after the packet, when the packet belongs to a state.
    pub tcp: Option<TcpState>,
}

/// The packet filter engine.
///
/// Build it from a [`Ruleset`], then call [`Engine::evaluate`] for every packet with a
/// monotonic `now` in seconds. The engine is deterministic: the caller owns the clock and the
/// state table has a fixed bound.
pub struct Engine {
    ruleset: Ruleset,
    states: StateTable,
    rules: Vec<RuleStats>,
    counters: Counters,
}

impl Engine {
    /// Build an engine with pf default timeouts and state limit.
    #[must_use]
    pub fn new(ruleset: Ruleset) -> Self {
        Self::with_timeouts(ruleset, Timeouts::default())
    }

    /// Build an engine with explicit timeouts and the default state limit.
    #[must_use]
    pub fn with_timeouts(ruleset: Ruleset, timeouts: Timeouts) -> Self {
        Self::with_limit(ruleset, timeouts, crate::state::MAX_STATES)
    }

    /// Build an engine with explicit timeouts and state limit.
    #[must_use]
    pub fn with_limit(ruleset: Ruleset, timeouts: Timeouts, limit: usize) -> Self {
        let rules = (0..ruleset.rules.len())
            .map(|_| RuleStats::default())
            .collect();
        Self {
            ruleset,
            states: StateTable::with_limit(limit, timeouts),
            rules,
            counters: Counters::default(),
        }
    }

    /// The parsed ruleset.
    #[must_use]
    pub fn ruleset(&self) -> &Ruleset {
        &self.ruleset
    }

    /// The state table.
    #[must_use]
    pub fn states(&self) -> &StateTable {
        &self.states
    }

    /// Apply the first matching NAT/redirect rule.
    ///
    /// The caller rewrites the endpoint before forwarding; this is the stateless translation
    /// seam. Stateful reverse translation is a documented K274 integration point.
    #[must_use]
    pub fn translate(&self, packet: &Packet<'_>) -> Option<Translation> {
        self.ruleset
            .nat
            .iter()
            .find_map(|rule| rule.translation(packet))
    }

    /// The global counters as plain values.
    #[must_use]
    pub fn counters(&self) -> CounterSnapshot {
        self.counters.snapshot()
    }

    /// One rule's counters, or `None` when the index is out of range.
    #[must_use]
    pub fn rule_stats(&self, index: usize) -> Option<RuleStatSnapshot> {
        let stats = self.rules.get(index)?;
        Some(RuleStatSnapshot {
            evaluations: stats.evaluations.load(Ordering::Relaxed),
            matches: stats.matches.load(Ordering::Relaxed),
            packets: stats.packets.load(Ordering::Relaxed),
            bytes: stats.bytes.load(Ordering::Relaxed),
        })
    }

    /// Evaluate one packet.
    ///
    /// An existing connection state is checked before the rules; a stateful pass keeps the
    /// return traffic flowing even if a later block rule would otherwise match. Without a state,
    /// the last matching rule wins unless a matching rule is `quick`; when no rule matches, the
    /// ruleset's `default_action` applies.
    pub fn evaluate(&mut self, packet: &Packet<'_>, now: u64) -> Decision {
        let expired = self.states.expire(now);
        if expired > 0 {
            self.counters
                .states_expired
                .fetch_add(expired as u64, Ordering::Relaxed);
        }
        if let Some(hit) = self.states.lookup(packet, now) {
            self.counters.state_hits.fetch_add(1, Ordering::Relaxed);
            self.counters_for(packet, Verdict::Pass);
            self.hit_rule(hit.rule, packet);
            return Decision {
                verdict: Verdict::Pass,
                rule: Some(hit.rule),
                state: Some(StateDisposition::Existing),
                tcp: Some(hit.tcp),
            };
        }

        let mut matched: Option<usize> = None;
        for (index, rule) in self.ruleset.rules.iter().enumerate() {
            if let Some(stats) = self.rules.get(index) {
                stats.evaluations.fetch_add(1, Ordering::Relaxed);
            }
            if rule.matches(packet) {
                if let Some(stats) = self.rules.get(index) {
                    stats.matches.fetch_add(1, Ordering::Relaxed);
                }
                matched = Some(index);
                if rule.quick {
                    break;
                }
            }
        }

        let (action, rule) = match matched {
            Some(index) => (
                self.ruleset
                    .rules
                    .get(index)
                    .map_or(self.ruleset.default_action, |rule| rule.action),
                Some(index),
            ),
            None => (self.ruleset.default_action, None),
        };
        match action {
            Action::Pass => {
                let mut disposition = None;
                if let Some(index) = rule
                    && self
                        .ruleset
                        .rules
                        .get(index)
                        .is_some_and(|rule| rule.state == StatePolicy::Keep)
                {
                    let interface = match self.ruleset.rules.get(index).map(|rule| &rule.interface)
                    {
                        Some(IfaceSpec::Name(name)) => Some(name.as_str()),
                        _ => None,
                    };
                    let created = self.states.establish(index, packet, interface, now);
                    if created == StateDisposition::Created {
                        self.counters.states_created.fetch_add(1, Ordering::Relaxed);
                    }
                    disposition = Some(created);
                }
                self.counters_for(packet, Verdict::Pass);
                if let Some(index) = rule {
                    self.hit_rule(index, packet);
                }
                Decision {
                    verdict: Verdict::Pass,
                    rule,
                    state: disposition,
                    tcp: None,
                }
            }
            Action::Block => {
                self.counters_for(packet, Verdict::Block);
                if let Some(index) = rule {
                    self.hit_rule(index, packet);
                }
                Decision {
                    verdict: Verdict::Block,
                    rule,
                    state: None,
                    tcp: None,
                }
            }
        }
    }

    /// Evaluate one IPv6 packet statelessly.
    ///
    /// IPv6 state entries are not part of the B17 state key; this path enforces the same rule
    /// order and default action without creating state.
    pub fn evaluate_v6(&mut self, packet: &Packet6<'_>) -> Decision {
        let mut matched = None;
        for (index, rule) in self.ruleset.rules.iter().enumerate() {
            if let Some(stats) = self.rules.get(index) {
                stats.evaluations.fetch_add(1, Ordering::Relaxed);
            }
            if rule.matches_v6(packet) {
                if let Some(stats) = self.rules.get(index) {
                    stats.matches.fetch_add(1, Ordering::Relaxed);
                }
                matched = Some(index);
                if rule.quick {
                    break;
                }
            }
        }
        let (action, rule) = match matched {
            Some(index) => (
                self.ruleset
                    .rules
                    .get(index)
                    .map_or(self.ruleset.default_action, |rule| rule.action),
                Some(index),
            ),
            None => (self.ruleset.default_action, None),
        };
        let verdict = match action {
            Action::Pass => Verdict::Pass,
            Action::Block => Verdict::Block,
        };
        self.counters_for_v6(packet, verdict);
        if let Some(index) = rule {
            self.hit_rule_v6(index, packet);
        }
        Decision {
            verdict,
            rule,
            state: None,
            tcp: None,
        }
    }

    fn counters_for(&self, packet: &Packet<'_>, verdict: Verdict) {
        let counter = match (packet.direction, verdict) {
            (crate::ast::Direction::In, Verdict::Pass) => &self.counters.in_pass,
            (crate::ast::Direction::In, Verdict::Block) => &self.counters.in_block,
            (crate::ast::Direction::Out, Verdict::Pass) => &self.counters.out_pass,
            (crate::ast::Direction::Out, Verdict::Block) => &self.counters.out_block,
        };
        counter.fetch_add(1, Ordering::Relaxed);
    }

    fn hit_rule(&self, index: usize, packet: &Packet<'_>) {
        if let Some(stats) = self.rules.get(index) {
            stats.packets.fetch_add(1, Ordering::Relaxed);
            stats
                .bytes
                .fetch_add(u64::from(packet.length), Ordering::Relaxed);
        }
    }

    fn counters_for_v6(&self, packet: &Packet6<'_>, verdict: Verdict) {
        let counter = match (packet.direction, verdict) {
            (crate::ast::Direction::In, Verdict::Pass) => &self.counters.in_pass,
            (crate::ast::Direction::In, Verdict::Block) => &self.counters.in_block,
            (crate::ast::Direction::Out, Verdict::Pass) => &self.counters.out_pass,
            (crate::ast::Direction::Out, Verdict::Block) => &self.counters.out_block,
        };
        counter.fetch_add(1, Ordering::Relaxed);
    }

    fn hit_rule_v6(&self, index: usize, packet: &Packet6<'_>) {
        if let Some(stats) = self.rules.get(index) {
            stats.packets.fetch_add(1, Ordering::Relaxed);
            stats
                .bytes
                .fetch_add(u64::from(packet.length), Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
#[path = "engine_ut.rs"]
mod tests;
