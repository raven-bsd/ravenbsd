// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Independent host-only model for RACK output/timer/HPTS scheduling.
//!
//! The model follows the FreeBSD `rack_start_hpts_timer`/`rack_timer_cancel`/
//! `rack_remxt_tmr`/`rack_stopall`/`rack_process_timers` control flow and the
//! `tcp_hpts` 10-microsecond wheel at `133344405f532bd54ff3629a77d4c4e062f01fdb`.
//! It is an oracle for the future RACK port, not product code: the caller
//! supplies the timer delay computed by K319/K320/K269, so the model does not
//! duplicate the loss, reordering-window or TLP-threshold core.
//!
//! The trace format follows the K319 `rack-10k.tsv` convention: comment
//! header, one row per seed, and an FNV-1a 64-bit digest of the canonical
//! event/action list.

#![forbid(unsafe_code)]

use std::collections::{BTreeMap, VecDeque};

/// `HPTS_USECS_PER_SLOT` from FreeBSD `tcp_hpts.h`.
pub const HPTS_USECS_PER_SLOT: u64 = 10;
/// `NUM_OF_HPTSI_SLOTS` from FreeBSD `tcp_hpts_internal.h`.
pub const NUM_OF_HPTSI_SLOTS: u32 = 102_400;
/// `rack_start_hpts_timer` caps `hpts_timeout` at `0x7ffffffe`.
pub const MAX_HPTS_TIMEOUT_US: u32 = 0x7fff_fffe;
/// `PACE_TMR_DELACK`.
pub const PACE_TMR_DELACK: u8 = 0x01;
/// `PACE_TMR_RACK`.
pub const PACE_TMR_RACK: u8 = 0x02;
/// `PACE_TMR_TLP`.
pub const PACE_TMR_TLP: u8 = 0x04;
/// `PACE_TMR_RXT`.
pub const PACE_TMR_RXT: u8 = 0x08;
/// `PACE_TMR_PERSIT`.
pub const PACE_TMR_PERSIT: u8 = 0x10;
/// `PACE_TMR_KEEP`.
pub const PACE_TMR_KEEP: u8 = 0x20;
/// `PACE_TMR_MASK`.
pub const PACE_TMR_MASK: u8 =
    PACE_TMR_KEEP | PACE_TMR_PERSIT | PACE_TMR_RXT | PACE_TMR_TLP | PACE_TMR_RACK | PACE_TMR_DELACK;
/// Number of deterministic traces in the golden set.
pub const TRACE_COUNT: u64 = 10_000;

/// A RACK/HPTS timer bit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Timer {
    /// Delayed ACK (`PACE_TMR_DELACK`).
    Delack,
    /// RACK loss timer (`PACE_TMR_RACK`).
    Rack,
    /// Tail-loss probe timer (`PACE_TMR_TLP`).
    Tlp,
    /// Retransmit timer (`PACE_TMR_RXT`).
    Rxt,
    /// Persist timer (`PACE_TMR_PERSIT`).
    Persist,
    /// Keepalive timer (`PACE_TMR_KEEP`).
    Keepalive,
}

impl Timer {
    /// The matching `PACE_TMR_*` bit.
    #[must_use]
    pub const fn bit(self) -> u8 {
        match self {
            Self::Delack => PACE_TMR_DELACK,
            Self::Rack => PACE_TMR_RACK,
            Self::Tlp => PACE_TMR_TLP,
            Self::Rxt => PACE_TMR_RXT,
            Self::Persist => PACE_TMR_PERSIT,
            Self::Keepalive => PACE_TMR_KEEP,
        }
    }

    /// Stable trace name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Delack => "DELACK",
            Self::Rack => "RACK",
            Self::Tlp => "TLP",
            Self::Rxt => "RXT",
            Self::Persist => "PERSIST",
            Self::Keepalive => "KEEP",
        }
    }
}

/// The connection states that gate `rack_start_hpts_timer`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    /// `TCPS_CLOSED`; no output timer.
    Closed,
    /// `TCPS_LISTEN`; no output timer.
    Listen,
    /// A SYN is outstanding.
    SynSent,
    /// The connection is established.
    Established,
    /// A close is in progress.
    Closing,
}

impl State {
    /// `rack_start_hpts_timer` returns immediately for CLOSED and LISTEN.
    #[must_use]
    pub const fn accepts_timers(self) -> bool {
        !matches!(self, Self::Closed | Self::Listen)
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Closed => "CLOSED",
            Self::Listen => "LISTEN",
            Self::SynSent => "SYN_SENT",
            Self::Established => "ESTABLISHED",
            Self::Closing => "CLOSING",
        }
    }
}

/// Why an output timer was not armed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// `rack_start_hpts_timer` rejects CLOSED and LISTEN.
    ClosedOrListen,
    /// The connection is already on the HPTS wheel.
    AlreadyQueued,
    /// `rack_stopall` has permanently stopped the timers.
    Stopped,
    /// `rack_stop_all_timers` entered persist.
    Persist,
    /// The caller supplied a zero timer delay and no timer was selected.
    NoTimer,
}

/// Result of one RACK output timer arming attempt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Output {
    /// Whether the timer was queued.
    pub scheduled: bool,
    /// The refusal reason, if any.
    pub refusal: Option<Refusal>,
    /// The `rc_hpts_flags` bit that was armed.
    pub flags: u8,
    /// Logical timer expiration, `now + delay`, if armed.
    pub deadline_us: Option<u64>,
    /// Absolute HPTS slot, if armed.
    pub slot: Option<u64>,
}

/// One callback returned by [`TimerModel::softclock`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Callback {
    /// Connection identifier.
    pub id: u16,
    /// `rc_hpts_flags` timer bits that fired.
    pub flags: u8,
    /// The `rack_process_timers` action selected from those bits.
    pub action: Option<Timer>,
    /// Logical expiry time in microseconds.
    pub deadline_us: u64,
    /// Final HPTS slot that delivered the callback.
    pub slot: u64,
    /// Time passed to `softclock`, in microseconds.
    pub fired_at_us: u64,
}

/// One `softclock` pass.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Softclock {
    /// Callbacks in slot-then-FIFO order.
    pub callbacks: Vec<Callback>,
    /// Due entries whose logical deadline was still in the future.
    pub early_reinserts: u32,
}

/// The exact outcome of one generated trace.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    /// Callbacks in delivery order.
    pub callbacks: Vec<Callback>,
    /// Output attempts that armed a timer.
    pub scheduled: u32,
    /// Output attempts refused by a donor gate.
    pub refused: u32,
    /// Cancel/remxt/stop calls that cleared an armed timer.
    pub cancelled: u32,
    /// Callback count.
    pub fired: u32,
    /// Early slot wakeups that were reinserted instead of firing.
    pub early_reinserts: u32,
}

#[derive(Clone, Copy, Debug, Default)]
struct Entry {
    flags: u8,
    stopped_flags: u8,
    deadline_us: u64,
    slot: Option<u64>,
    generation: u64,
    stopped: bool,
    persist: bool,
    override_: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Queued {
    id: u16,
    generation: u64,
}

/// Independent RACK output/timer/HPTS scheduler.
#[derive(Clone, Debug)]
pub struct TimerModel {
    now_us: u64,
    entries: BTreeMap<u16, Entry>,
    wheel: BTreeMap<u64, VecDeque<Queued>>,
}

impl TimerModel {
    /// Create an empty scheduler at `now_us`.
    #[must_use]
    pub const fn new(now_us: u64) -> Self {
        Self {
            now_us,
            entries: BTreeMap::new(),
            wheel: BTreeMap::new(),
        }
    }

    /// Current model time in microseconds.
    #[must_use]
    pub const fn now_us(&self) -> u64 {
        self.now_us
    }

    /// Current model time in milliseconds.
    #[must_use]
    pub const fn now_ms(&self) -> u64 {
        self.now_us / 1_000
    }

    /// Advance time; a backward step is ignored.
    pub fn advance_to(&mut self, now_us: u64) {
        if now_us > self.now_us {
            self.now_us = now_us;
        }
    }

    /// Arm the output timer, following `rack_start_hpts_timer` and the
    /// single flag selected by `rack_timer_start`.
    ///
    /// `delay_us` is the caller-computed timer value. Zero means that no timer
    /// was selected and the connection is left off the wheel.
    pub fn output(&mut self, id: u16, state: State, timer: Timer, delay_us: u32) -> Output {
        if !state.accepts_timers() {
            return Output {
                scheduled: false,
                refusal: Some(Refusal::ClosedOrListen),
                flags: 0,
                deadline_us: None,
                slot: None,
            };
        }
        let now = self.now_us;
        let entry = self.entries.entry(id).or_default();
        if entry.slot.is_some() {
            return Output {
                scheduled: false,
                refusal: Some(Refusal::AlreadyQueued),
                flags: entry.flags,
                deadline_us: Some(entry.deadline_us),
                slot: entry.slot,
            };
        }
        entry.flags = 0;
        entry.deadline_us = 0;
        entry.stopped_flags = 0;
        entry.override_ = false;
        if entry.stopped {
            return Output {
                scheduled: false,
                refusal: Some(Refusal::Stopped),
                flags: 0,
                deadline_us: None,
                slot: None,
            };
        }
        if entry.persist {
            return Output {
                scheduled: false,
                refusal: Some(Refusal::Persist),
                flags: 0,
                deadline_us: None,
                slot: None,
            };
        }
        if delay_us == 0 {
            return Output {
                scheduled: false,
                refusal: Some(Refusal::NoTimer),
                flags: 0,
                deadline_us: None,
                slot: None,
            };
        }
        let delay = u64::from(delay_us).min(u64::from(MAX_HPTS_TIMEOUT_US));
        let deadline = now.saturating_add(delay);
        let slot = now
            .saturating_div(HPTS_USECS_PER_SLOT)
            .saturating_add(hpts_usec_to_slots(delay));
        entry.flags = timer.bit();
        entry.deadline_us = deadline;
        entry.slot = Some(slot);
        entry.generation = entry.generation.wrapping_add(1);
        let generation = entry.generation;
        self.wheel
            .entry(slot)
            .or_default()
            .push_back(Queued { id, generation });
        Output {
            scheduled: true,
            refusal: None,
            flags: entry.flags,
            deadline_us: Some(deadline),
            slot: Some(slot),
        }
    }

    /// `rack_timer_cancel`: clear the timer flags and invalidate the wheel slot.
    ///
    /// Returns the flags that were active. The cleared value is retained as
    /// `rc_tmr_stopped` for the next output accounting pass, as in the donor.
    pub fn cancel(&mut self, id: u16) -> u8 {
        let Some(entry) = self.entries.get_mut(&id) else {
            return 0;
        };
        let flags = entry.flags;
        entry.stopped_flags = flags & PACE_TMR_MASK;
        entry.flags = 0;
        entry.deadline_us = 0;
        let was_queued = entry.slot.take().is_some();
        if was_queued || flags != 0 {
            entry.generation = entry.generation.wrapping_add(1);
        }
        flags
    }

    /// `rack_remxt_tmr`: cancel and set `r_timer_override`.
    pub fn remxt(&mut self, id: u16) -> u8 {
        let flags = self.cancel(id);
        self.entries.entry(id).or_default().override_ = true;
        flags
    }

    /// `rack_stopall`: permanently stop future timers and remove the entry.
    pub fn stopall(&mut self, id: u16) -> u8 {
        self.entries.entry(id).or_default().stopped = true;
        self.cancel(id)
    }

    /// `rack_stop_all_timers`: enter persist when the persist timer was armed,
    /// then remove the entry.
    pub fn stop_all_timers(&mut self, id: u16) -> u8 {
        let flags = self.entries.get(&id).map_or(0, |entry| entry.flags);
        if flags & PACE_TMR_PERSIT != 0 {
            self.entries.entry(id).or_default().persist = true;
        }
        self.cancel(id)
    }

    /// Leave persist (`rack_exit_persist` outcome); future timers may arm.
    pub fn exit_persist(&mut self, id: u16) {
        self.entries.entry(id).or_default().persist = false;
    }

    /// `tcp_hpts`/`rack_process_timers` pass over all slots due at `now_us`.
    ///
    /// An entry whose HPTS slot is due before its logical deadline is
    /// reinserted for the remaining time instead of firing, matching
    /// `rack_process_timers`' early-wakeup path.
    pub fn softclock(&mut self, now_us: u64) -> Softclock {
        self.advance_to(now_us);
        let now = self.now_us;
        let target = now / HPTS_USECS_PER_SLOT;
        let keys: Vec<u64> = self.wheel.range(..=target).map(|(slot, _)| *slot).collect();
        let mut result = Softclock::default();
        for key in keys {
            let Some(refs) = self.wheel.remove(&key) else {
                continue;
            };
            for queued in refs {
                let Some(entry) = self.entries.get_mut(&queued.id) else {
                    continue;
                };
                if entry.generation != queued.generation || entry.slot != Some(key) {
                    continue;
                }
                if now < entry.deadline_us {
                    let left = entry.deadline_us - now;
                    let slot = now
                        .saturating_div(HPTS_USECS_PER_SLOT)
                        .saturating_add(hpts_usec_to_slots(left));
                    entry.slot = Some(slot);
                    entry.generation = entry.generation.wrapping_add(1);
                    let generation = entry.generation;
                    result.early_reinserts += 1;
                    self.wheel.entry(slot).or_default().push_back(Queued {
                        id: queued.id,
                        generation,
                    });
                    continue;
                }
                let flags = entry.flags;
                let action = timeout_action(flags);
                let deadline_us = entry.deadline_us;
                entry.flags = 0;
                entry.deadline_us = 0;
                entry.stopped_flags = 0;
                entry.slot = None;
                entry.generation = entry.generation.wrapping_add(1);
                result.callbacks.push(Callback {
                    id: queued.id,
                    flags,
                    action,
                    deadline_us,
                    slot: key,
                    fired_at_us: now,
                });
            }
        }
        result
    }

    /// Logical deadline of the armed timer, if any.
    #[must_use]
    pub fn deadline(&self, id: u16) -> Option<u64> {
        self.entries
            .get(&id)
            .filter(|entry| entry.slot.is_some())
            .map(|entry| entry.deadline_us)
    }

    /// Whether the connection is on the HPTS wheel.
    #[must_use]
    pub fn in_hpts(&self, id: u16) -> bool {
        self.entries
            .get(&id)
            .is_some_and(|entry| entry.slot.is_some())
    }

    /// Whether `rack_stopall` has stopped the connection.
    #[must_use]
    pub fn stopped(&self, id: u16) -> bool {
        self.entries.get(&id).is_some_and(|entry| entry.stopped)
    }

    /// Whether `rack_stop_all_timers` entered persist.
    #[must_use]
    pub fn persist(&self, id: u16) -> bool {
        self.entries.get(&id).is_some_and(|entry| entry.persist)
    }

    /// Whether `rack_remxt_tmr` set `r_timer_override` and no output cleared it.
    #[must_use]
    pub fn timer_override(&self, id: u16) -> bool {
        self.entries.get(&id).is_some_and(|entry| entry.override_)
    }
}

/// `HPTS_USEC_TO_SLOTS`: round microseconds up to 10-microsecond slots.
#[must_use]
pub const fn hpts_usec_to_slots(usecs: u64) -> u64 {
    usecs.saturating_add(HPTS_USECS_PER_SLOT - 1) / HPTS_USECS_PER_SLOT
}

/// `rack_process_timers` timeout priority.
#[must_use]
pub const fn timeout_action(flags: u8) -> Option<Timer> {
    if flags & PACE_TMR_DELACK != 0 {
        Some(Timer::Delack)
    } else if flags & PACE_TMR_RACK != 0 {
        Some(Timer::Rack)
    } else if flags & PACE_TMR_TLP != 0 {
        Some(Timer::Tlp)
    } else if flags & PACE_TMR_RXT != 0 {
        Some(Timer::Rxt)
    } else if flags & PACE_TMR_PERSIT != 0 {
        Some(Timer::Persist)
    } else if flags & PACE_TMR_KEEP != 0 {
        Some(Timer::Keepalive)
    } else {
        None
    }
}

/// One event in a generated scenario.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Event {
    /// Event time in microseconds.
    pub at_us: u64,
    /// Event kind.
    pub kind: EventKind,
}

/// Generated event kinds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventKind {
    /// RACK output arming attempt.
    Output {
        /// Connection identifier.
        id: u16,
        /// Connection state.
        state: State,
        /// Timer selected by the caller.
        timer: Timer,
        /// Caller-computed delay in microseconds.
        delay_us: u32,
    },
    /// `rack_timer_cancel`.
    Cancel {
        /// Connection identifier.
        id: u16,
    },
    /// `rack_remxt_tmr`.
    Remxt {
        /// Connection identifier.
        id: u16,
    },
    /// `rack_stopall`.
    StopAll {
        /// Connection identifier.
        id: u16,
    },
    /// `rack_stop_all_timers`.
    StopAllTimers {
        /// Connection identifier.
        id: u16,
    },
    /// Leave persist.
    ExitPersist {
        /// Connection identifier.
        id: u16,
    },
    /// Run the HPTS/process-timers pass.
    Softclock,
}

impl EventKind {
    fn canonical(self) -> String {
        match self {
            Self::Output {
                id,
                state,
                timer,
                delay_us,
            } => format!("O{id}:{}:{}:{delay_us}", state.name(), timer.name()),
            Self::Cancel { id } => format!("C{id}"),
            Self::Remxt { id } => format!("M{id}"),
            Self::StopAll { id } => format!("A{id}"),
            Self::StopAllTimers { id } => format!("T{id}"),
            Self::ExitPersist { id } => format!("P{id}"),
            Self::Softclock => "S".to_owned(),
        }
    }
}

/// One deterministic input scenario.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scenario {
    /// Stable seed.
    pub seed: u64,
    /// Monotonic events.
    pub events: Vec<Event>,
}

impl Scenario {
    /// Build a deterministic scenario from `seed`.
    #[must_use]
    pub fn generate(seed: u64) -> Self {
        let mut rng = Lcg::new(seed);
        let count = rng.range(24, 48) as usize;
        let mut events = Vec::with_capacity(count + 1);
        let mut at_us = 0u64;
        for _ in 0..count {
            at_us = at_us.saturating_add(rng.range(0, 800));
            let id = rng.range(0, 3) as u16;
            let kind = match rng.range(0, 10) {
                0..=4 => {
                    let state = match rng.range(0, 7) {
                        0 => State::Closed,
                        1 => State::Listen,
                        2 => State::SynSent,
                        3 => State::Closing,
                        _ => State::Established,
                    };
                    let timer = match rng.range(0, 3) {
                        0 => Timer::Rack,
                        1 => Timer::Tlp,
                        2 => Timer::Rxt,
                        _ => Timer::Persist,
                    };
                    let delay_us = if rng.bool(16) {
                        0
                    } else {
                        rng.range(1, 5_000) as u32
                    };
                    EventKind::Output {
                        id,
                        state,
                        timer,
                        delay_us,
                    }
                }
                5 => EventKind::Cancel { id },
                6 => EventKind::Remxt { id },
                7 => EventKind::StopAll { id },
                8 => EventKind::StopAllTimers { id },
                9 => EventKind::ExitPersist { id },
                _ => EventKind::Softclock,
            };
            events.push(Event { at_us, kind });
        }
        // Flush every armed timer so each trace has an observable action list.
        at_us = at_us.saturating_add(5_000);
        events.push(Event {
            at_us,
            kind: EventKind::Softclock,
        });
        Self { seed, events }
    }
}

/// Run one scenario through the independent model.
#[must_use]
pub fn run(scenario: &Scenario) -> Outcome {
    let mut model = TimerModel::new(0);
    let mut outcome = Outcome::default();
    for event in &scenario.events {
        model.advance_to(event.at_us);
        match event.kind {
            EventKind::Output {
                id,
                state,
                timer,
                delay_us,
            } => {
                let result = model.output(id, state, timer, delay_us);
                if result.scheduled {
                    outcome.scheduled += 1;
                } else {
                    outcome.refused += 1;
                }
            }
            EventKind::Cancel { id } => {
                if model.cancel(id) != 0 {
                    outcome.cancelled += 1;
                }
            }
            EventKind::Remxt { id } => {
                if model.remxt(id) != 0 {
                    outcome.cancelled += 1;
                }
            }
            EventKind::StopAll { id } => {
                if model.stopall(id) != 0 {
                    outcome.cancelled += 1;
                }
            }
            EventKind::StopAllTimers { id } => {
                if model.stop_all_timers(id) != 0 {
                    outcome.cancelled += 1;
                }
            }
            EventKind::ExitPersist { id } => model.exit_persist(id),
            EventKind::Softclock => {
                let result = model.softclock(event.at_us);
                outcome.early_reinserts += result.early_reinserts;
                outcome.fired += u32::try_from(result.callbacks.len()).unwrap_or(u32::MAX);
                outcome.callbacks.extend(result.callbacks);
            }
        }
    }
    outcome
}

/// Canonical text for the trace digest; never uses `Debug` output.
#[must_use]
pub fn canonical_trace(scenario: &Scenario, outcome: &Outcome) -> String {
    let mut text = String::new();
    for event in &scenario.events {
        text.push_str(&format!("E{}:{};", event.at_us, event.kind.canonical()));
    }
    for callback in &outcome.callbacks {
        let action = callback.action.map_or("-", Timer::name);
        text.push_str(&format!(
            "C{}:{}:{action}:{}:{}:{};",
            callback.id, callback.flags, callback.deadline_us, callback.slot, callback.fired_at_us
        ));
    }
    text.push_str(&format!(
        "S{}R{}X{}F{}E{};",
        outcome.scheduled,
        outcome.refused,
        outcome.cancelled,
        outcome.fired,
        outcome.early_reinserts
    ));
    text
}

/// FNV-1a 64-bit digest of the canonical trace.
#[must_use]
pub fn trace_digest(scenario: &Scenario, outcome: &Outcome) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in canonical_trace(scenario, outcome).bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn format_order(outcome: &Outcome) -> String {
    if outcome.callbacks.is_empty() {
        return "-".to_owned();
    }
    outcome
        .callbacks
        .iter()
        .map(|callback| {
            format!(
                "{}@{}/{}",
                callback.id, callback.deadline_us, callback.fired_at_us
            )
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn format_actions(outcome: &Outcome) -> String {
    if outcome.callbacks.is_empty() {
        return "-".to_owned();
    }
    outcome
        .callbacks
        .iter()
        .map(|callback| callback.action.map_or("-", Timer::name))
        .collect::<Vec<_>>()
        .join(",")
}

/// One trace-file row: seed, events, counts, callback order, actions and digest.
#[must_use]
pub fn trace_row(scenario: &Scenario, outcome: &Outcome) -> String {
    format!(
        "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{:016x}",
        scenario.seed,
        scenario.events.len(),
        outcome.scheduled,
        outcome.refused,
        outcome.cancelled,
        outcome.fired,
        outcome.early_reinserts,
        format_order(outcome),
        format_actions(outcome),
        trace_digest(scenario, outcome),
    )
}

/// The two comment lines shared by the golden trace artifact.
pub const TRACE_HEADER: &str = "\
# RACK output/timer/HPTS reference traces (K325)
# seed\tevents\tscheduled\trefused\tcancelled\tfired\tearly\torder\tactions\tdigest
";

/// Render the complete 10k trace file.
#[must_use]
pub fn render_trace() -> String {
    let mut text = String::from(TRACE_HEADER);
    for seed in 1..=TRACE_COUNT {
        let scenario = Scenario::generate(seed);
        let outcome = run(&scenario);
        text.push_str(&trace_row(&scenario, &outcome));
        text.push('\n');
    }
    text
}

struct Lcg {
    state: u64,
}

impl Lcg {
    const fn new(seed: u64) -> Self {
        Self {
            state: seed
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407),
        }
    }

    fn next(&mut self) -> u64 {
        self.state = self
            .state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        self.state
    }

    fn range(&mut self, low: u64, high: u64) -> u64 {
        debug_assert!(low <= high);
        low + self.next() % (high - low + 1)
    }

    fn bool(&mut self, one_in: u64) -> bool {
        self.range(1, one_in) == one_in
    }
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
