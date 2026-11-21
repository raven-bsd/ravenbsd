// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Stateless and stateful match engine with per-rule and global counters.
//!
//! Donor: `pf_test()` (`sys/netpfil/pf/pf.c:11646`) for the state-before-rules order,
//! `pf_test_rule()` (`pf.c:6339`) for the translation phase before state creation,
//! `pf_create_state()` (`pf.c:6631`) for the fail-closed `max_states` check, and
//! `pf_counters_inc()` (`pf.c:11489`) for the `PFRES_*` reason counters.

use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};

use crate::abi::{self, PFRES_MAX, PfStatus};
use crate::ast::{Action, IfaceSpec, Packet, Ruleset, StatePolicy};
use crate::log::{LogEntry, PacketLog};
use crate::nat::Translation;
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
#[derive(Debug)]
struct Counters {
    in_pass: AtomicU64,
    in_block: AtomicU64,
    out_pass: AtomicU64,
    out_block: AtomicU64,
    states_created: AtomicU64,
    states_expired: AtomicU64,
    state_hits: AtomicU64,
    reasons: Vec<AtomicU64>,
}

impl Counters {
    fn new() -> Self {
        Self {
            in_pass: AtomicU64::new(0),
            in_block: AtomicU64::new(0),
            out_pass: AtomicU64::new(0),
            out_block: AtomicU64::new(0),
            states_created: AtomicU64::new(0),
            states_expired: AtomicU64::new(0),
            state_hits: AtomicU64::new(0),
            reasons: (0..PFRES_MAX).map(|_| AtomicU64::new(0)).collect(),
        }
    }

    fn bump(&self, reason: usize) {
        if let Some(counter) = self.reasons.get(reason) {
            counter.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn snapshot(&self) -> CounterSnapshot {
        CounterSnapshot {
            in_pass: self.in_pass.load(Ordering::Relaxed),
            in_block: self.in_block.load(Ordering::Relaxed),
            out_pass: self.out_pass.load(Ordering::Relaxed),
            out_block: self.out_block.load(Ordering::Relaxed),
            states_created: self.states_created.load(Ordering::Relaxed),
            states_expired: self.states_expired.load(Ordering::Relaxed),
            state_hits: self.state_hits.load(Ordering::Relaxed),
            reasons: core::array::from_fn(|index| {
                self.reasons
                    .get(index)
                    .map_or(0, |counter| counter.load(Ordering::Relaxed))
            }),
        }
    }
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
    /// Connections expired.
    pub states_expired: u64,
    /// Packets allowed by an existing state.
    pub state_hits: u64,
    /// `PFRES_*` reason counters.
    pub reasons: [u64; PFRES_MAX],
}

impl CounterSnapshot {
    /// One reason counter.
    #[must_use]
    pub fn reason(&self, reason: usize) -> u64 {
        self.reasons.get(reason).copied().unwrap_or(0)
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
    /// Address/port rewrite the caller must apply, when the packet is translated.
    pub translation: Option<Translation>,
    /// `PFRES_*` reason for a drop, when the drop has one.
    pub reason: Option<usize>,
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
    log: PacketLog,
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
            counters: Counters::new(),
            log: PacketLog::new(),
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

    /// The retained log records.
    #[must_use]
    pub fn logs(&self) -> &[LogEntry] {
        self.log.entries()
    }

    /// Take the retained log records.
    pub fn drain_logs(&mut self) -> Vec<LogEntry> {
        self.log.drain()
    }

    /// A `pf_status`-shaped snapshot for the pfctl client.
    #[must_use]
    pub fn status(&self) -> PfStatus {
        let counters = self.counters.snapshot();
        PfStatus {
            running: true,
            since: 0,
            states: u32::try_from(self.states.len()).unwrap_or(u32::MAX),
            max_states: u32::try_from(self.states.limit()).unwrap_or(u32::MAX),
            counters: counters.reasons,
            linserts: self.counters.states_created.load(Ordering::Relaxed),
            ldeletes: self.counters.states_expired.load(Ordering::Relaxed),
            fcounters_pass: counters.in_pass.saturating_add(counters.out_pass),
            fcounters_block: counters.in_block.saturating_add(counters.out_block),
        }
    }

    /// Evaluate one packet.
    ///
    /// An existing connection state is checked before the rules; a stateful pass keeps the
    /// return traffic flowing even if a later block rule would otherwise match. Without a state,
    /// the last matching rule wins unless a matching rule is `quick`; when no rule matches, the
    /// ruleset's `default_action` applies. A matched NAT/RDR rule rewrites the packet; a failed
    /// translation or a full state table drops the packet (fail-closed).
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
                translation: hit.translation,
                reason: None,
            };
        }

        // Translation phase first: donor pf_test_rule() calls pf_get_translation() and then
        // pf_translate_compat() before pf_match_rule(), so the filter sees the translated
        // packet view.
        let translation = match self.ruleset.nat.find(packet, &self.ruleset.tables) {
            Some(index) => {
                let result = self.ruleset.nat.translate(index, packet, |address, port| {
                    self.states.in_use(address, port)
                });
                match result {
                    Ok(translation) => translation,
                    Err(_) => {
                        self.counters.bump(abi::PFRES_MAPFAILED);
                        self.log_forced(packet, None, abi::PFRES_MAPFAILED, now);
                        self.counters_for(packet, Verdict::Block);
                        return Decision {
                            verdict: Verdict::Block,
                            rule: None,
                            state: None,
                            tcp: None,
                            translation: None,
                            reason: Some(abi::PFRES_MAPFAILED),
                        };
                    }
                }
            }
            None => None,
        };
        let mut filter_packet = *packet;
        if let Some(translation) = translation {
            filter_packet.source = translation.source;
            filter_packet.source_port = translation.source_port;
            filter_packet.destination = translation.destination;
            filter_packet.destination_port = translation.destination_port;
        }

        let mut matched: Option<usize> = None;
        for (index, rule) in self.ruleset.rules.iter().enumerate() {
            if let Some(stats) = self.rules.get(index) {
                stats.evaluations.fetch_add(1, Ordering::Relaxed);
            }
            if rule.matches(&filter_packet, &self.ruleset.tables) {
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
                    && !packet.fragment
                {
                    let interface = match self.ruleset.rules.get(index).map(|rule| &rule.interface)
                    {
                        Some(IfaceSpec::Name(name)) => Some(name.as_str()),
                        _ => None,
                    };
                    let created =
                        self.states
                            .establish_with(index, packet, interface, translation, now);
                    if created == StateDisposition::Rejected {
                        self.counters.bump(abi::PFRES_MAXSTATES);
                        self.log_forced(packet, rule, abi::PFRES_MAXSTATES, now);
                        self.counters_for(packet, Verdict::Block);
                        self.hit_rule(index, packet);
                        return Decision {
                            verdict: Verdict::Block,
                            rule,
                            state: Some(StateDisposition::Rejected),
                            tcp: None,
                            translation: None,
                            reason: Some(abi::PFRES_MAXSTATES),
                        };
                    }
                    if created == StateDisposition::Created {
                        self.counters.states_created.fetch_add(1, Ordering::Relaxed);
                    }
                    disposition = Some(created);
                }
                self.counters_for(packet, Verdict::Pass);
                if let Some(index) = rule {
                    self.hit_rule(index, packet);
                    if self.ruleset.rules.get(index).is_some_and(|rule| rule.log) {
                        self.log.push(LogEntry::from_packet(
                            &filter_packet,
                            Action::Pass,
                            rule,
                            None,
                            now,
                        ));
                    }
                }
                Decision {
                    verdict: Verdict::Pass,
                    rule,
                    state: disposition,
                    tcp: None,
                    translation,
                    reason: None,
                }
            }
            Action::Block => {
                self.counters.bump(abi::PFRES_MATCH);
                self.counters_for(packet, Verdict::Block);
                if let Some(index) = rule {
                    self.hit_rule(index, packet);
                    if self.ruleset.rules.get(index).is_some_and(|rule| rule.log) {
                        self.log.push(LogEntry::from_packet(
                            &filter_packet,
                            Action::Block,
                            rule,
                            Some(abi::PFRES_MATCH),
                            now,
                        ));
                    }
                }
                Decision {
                    verdict: Verdict::Block,
                    rule,
                    state: None,
                    tcp: None,
                    translation: None,
                    reason: Some(abi::PFRES_MATCH),
                }
            }
        }
    }

    fn log_forced(&mut self, packet: &Packet<'_>, rule: Option<usize>, reason: usize, now: u64) {
        self.log.push(LogEntry::from_packet(
            packet,
            Action::Block,
            rule,
            Some(reason),
            now,
        ));
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
}

#[cfg(test)]
#[path = "engine_ut.rs"]
mod tests;
