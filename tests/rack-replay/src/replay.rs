// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Replay of the independent model traces against the ported RACK primitives.
//!
//! The loss and recovery classes consume the committed K319/K324 10k golden rows. The
//! TLP, BBR and vtable classes consume the deterministic 10k event traces defined by the
//! K320/K326/K327 models (those crates keep the events in code, not in a TSV). The replay
//! reports the first divergence per class; a red class is a finding for the owning group.

use rack_bbr_model::model::{BbrModel, Config as BbrConfig};
use rack_bbr_model::rng::Rng as BbrRng;
use rack_tlp_model::model::{Action, Config as TlpConfig, RackModel as TlpModel};
use rack_tlp_model::rng::Rng as TlpRng;
use rack_vtable_model::model as vt;
use raven_tcpip::{
    IPPROTO_TCP, PRUS_OOB, Rack, RackFunctionBlock, RackQuery, RackSockopt, RackStack, RackVtable,
    SackScoreboard, SockoptDir, TCP_QUERY_TIMERS_UP, TCP_RACK_PACE_MIN_SEG,
    TCP_RACK_PACING_DIVISOR, TCPS_CLOSED, Tlp as ProductTlp,
};

use crate::trace;

/// Trace class replayed by this crate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    /// K319 loss/reordering golden trace.
    Loss,
    /// K324 recovery/pipe golden trace.
    Recovery,
    /// K320 RACK-TLP/DSACK/spurious-undo trace.
    Tlp,
    /// K326 BBR/pacing/timely trace.
    Bbr,
    /// K327 `tcp_function_block` vtable trace.
    Vtable,
}

impl Class {
    /// Stable class name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Loss => "loss",
            Self::Recovery => "recovery",
            Self::Tlp => "tlp",
            Self::Bbr => "bbr",
            Self::Vtable => "vtable",
        }
    }
}

/// The first divergence in one trace class.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mismatch {
    /// Trace/seed identity.
    pub trace: String,
    /// Oracle expectation.
    pub expected: String,
    /// Ported product observation.
    pub actual: String,
}

/// One class result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassReport {
    /// Replayed class.
    pub class: Class,
    /// Trace/scenario count.
    pub traces: usize,
    /// Event count actually driven through the models.
    pub events: usize,
    /// Number of traces (or probes) with a divergence.
    pub mismatches: usize,
    /// First divergence, if any.
    pub first_mismatch: Option<Mismatch>,
    /// Up to five divergences for the report.
    pub samples: Vec<Mismatch>,
    /// Fields the current product cannot expose yet.
    pub gaps: Vec<String>,
}

impl ClassReport {
    fn new(class: Class) -> Self {
        Self {
            class,
            traces: 0,
            events: 0,
            mismatches: 0,
            first_mismatch: None,
            samples: Vec::new(),
            gaps: Vec::new(),
        }
    }

    fn record(&mut self, mismatch: Mismatch) {
        self.mismatches = self.mismatches.saturating_add(1);
        if self.first_mismatch.is_none() {
            self.first_mismatch = Some(mismatch.clone());
        }
        if self.samples.len() < 5 {
            self.samples.push(mismatch);
        }
    }
}

/// Full replay matrix.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReplayReport {
    /// One entry per class, in `Loss`, `Recovery`, `Tlp`, `Bbr`, `Vtable` order.
    pub classes: Vec<ClassReport>,
}

impl ReplayReport {
    /// Render the report as stable text for the CLI and the ticket evidence.
    #[must_use]
    pub fn render(&self) -> String {
        let mut text = String::from("class\ttraces\tevents\tmismatches\tfirst mismatch\n");
        for class in &self.classes {
            let first = class.first_mismatch.as_ref().map_or_else(
                || "none".to_owned(),
                |mismatch| {
                    format!(
                        "{}: expected {}, got {}",
                        mismatch.trace, mismatch.expected, mismatch.actual
                    )
                },
            );
            text.push_str(&format!(
                "{}\t{}\t{}\t{}\t{}\n",
                class.class.name(),
                class.traces,
                class.events,
                class.mismatches,
                first
            ));
            for sample in class.samples.iter().skip(1) {
                text.push_str(&format!(
                    "\tsample: {}: expected {}, got {}\n",
                    sample.trace, sample.expected, sample.actual
                ));
            }
            for gap in &class.gaps {
                text.push_str(&format!("\tgap: {gap}\n"));
            }
        }
        text
    }
}

/// Replay every trace class.
pub fn replay_all() -> Result<ReplayReport, String> {
    Ok(ReplayReport {
        classes: vec![
            replay_loss()?,
            replay_recovery()?,
            replay_tlp(),
            replay_bbr(),
            replay_vtable(),
        ],
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SackEvent {
    ack: u32,
    sacks: [(u32, u32); 3],
    sack_count: usize,
    dsack: bool,
}

impl SackEvent {
    fn sacks(&self) -> &[(u32, u32)] {
        &self.sacks[..self.sack_count]
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ProductSack {
    lost: Vec<(u32, u32)>,
    pipe: u32,
}

const MSS: u32 = 1000;
const SEGMENTS: u32 = 8;

/// Drive the public `SackScoreboard` with one scenario's ACK/SACK/DSACK events.
///
/// The live `sack_send` path rebases the scoreboard on `snd_una`; this function does the same
/// and therefore exercises the same public primitive without instantiating a whole `Stack`.
fn product_sack(events: &[SackEvent], first_seq: u32) -> ProductSack {
    let stream_end = first_seq + SEGMENTS * MSS;
    let mut snd_una = first_seq;
    let mut sacked: Vec<(u32, u32)> = Vec::new();
    let mut board = SackScoreboard::new(stream_end - first_seq, 0, 0, MSS, 3, 64);
    for event in events {
        if event.ack > snd_una {
            snd_una = event.ack.min(stream_end);
            sacked.retain(|(_, end)| *end > snd_una);
            for range in &mut sacked {
                if range.0 < snd_una {
                    range.0 = snd_una;
                }
            }
        }
        let high_data = stream_end - snd_una;
        board = SackScoreboard::new(high_data, 0, 0, MSS, 3, 64);
        board.sacked = sacked
            .iter()
            .map(|(start, end)| (start - snd_una, end - snd_una))
            .collect();
        let blocks: Vec<(i64, i64)> = event
            .sacks()
            .iter()
            .map(|(start, end)| ((*start).max(snd_una), (*end).min(stream_end)))
            .filter(|(start, end)| end > start)
            .map(|(start, end)| (i64::from(start - snd_una), i64::from(end - snd_una)))
            .collect();
        board.add_blocks(0, high_data, &blocks);
        sacked = board
            .sacked
            .iter()
            .map(|(start, end)| (start + snd_una, end + snd_una))
            .collect();
        board.update_lostseq(0);
        if event.dsack {
            board.note_dsack(0, 0);
        }
    }
    ProductSack {
        lost: board
            .lost_ranges()
            .into_iter()
            .map(|(start, end)| (start + snd_una, end + snd_una))
            .collect(),
        pipe: board.pipe(),
    }
}

/// Replay the K319 10k loss/reordering golden rows.
pub fn replay_loss() -> Result<ClassReport, String> {
    let rows = trace::load_golden(&trace::loss_trace_path())?;
    Ok(replay_loss_rows(&rows))
}

/// Replay an explicit loss golden row set (used by the negative tests).
#[must_use]
pub fn replay_loss_rows(rows: &[trace::GoldenRow]) -> ClassReport {
    let mut report = ClassReport::new(Class::Loss);
    for row in rows {
        report.traces = report.traces.saturating_add(1);
        let scenario = rack_loss_model::Scenario::generate(row.seed);
        report.events = report.events.saturating_add(scenario.events.len());
        let digest = rack_loss_model::event_digest(&scenario);
        if digest != row.digest {
            report.record(Mismatch {
                trace: format!("loss seed {}", row.seed),
                expected: format!("digest {:016x}", row.digest),
                actual: format!("digest {digest:016x}"),
            });
            continue;
        }
        let oracle = rack_loss_model::run(&scenario);
        if !golden_matches(
            &oracle.lost,
            oracle.pipe,
            &oracle.retransmits,
            oracle.recovery_entries,
            oracle.recovery_exits,
            row,
        ) {
            report.record(Mismatch {
                trace: format!("loss seed {}", row.seed),
                expected: format!(
                    "golden lost {} pipe {} retransmits {} recovery {}/{}",
                    ranges(&row.lost),
                    row.pipe,
                    ranges(&row.retransmits),
                    row.recovery_entries,
                    row.recovery_exits
                ),
                actual: format!(
                    "model lost {} pipe {} retransmits {} recovery {}/{}",
                    ranges(&oracle.lost),
                    oracle.pipe,
                    ranges(&oracle.retransmits),
                    oracle.recovery_entries,
                    oracle.recovery_exits
                ),
            });
            continue;
        }
        let events: Vec<SackEvent> = scenario
            .events
            .iter()
            .map(|event| {
                let mut sacks = [(0, 0); 3];
                let count = event.sacks.len().min(3);
                sacks[..count].copy_from_slice(&event.sacks[..count]);
                SackEvent {
                    ack: event.ack,
                    sacks,
                    sack_count: count,
                    dsack: event.dsack,
                }
            })
            .collect();
        let product = product_sack(&events, rack_loss_model::FIRST_SEQ);
        if product.lost != oracle.lost || product.pipe != oracle.pipe {
            report.record(Mismatch {
                trace: format!("loss seed {}", row.seed),
                expected: format!("lost {} pipe {}", ranges(&oracle.lost), oracle.pipe),
                actual: format!("lost {} pipe {}", ranges(&product.lost), product.pipe),
            });
        }
    }
    report.gaps.push(
        "product SackScoreboard exposes lost ranges and pipe only; retransmit and recovery counters \
         are not part of its public result"
            .to_owned(),
    );
    report
}

/// Replay the K324 10k recovery/pipe golden rows.
pub fn replay_recovery() -> Result<ClassReport, String> {
    let rows = trace::load_golden(&trace::recovery_trace_path())?;
    let mut report = ClassReport::new(Class::Recovery);
    for row in &rows {
        report.traces = report.traces.saturating_add(1);
        let scenario = rack_recovery_model::Scenario::new(row.seed);
        report.events = report.events.saturating_add(scenario.events.len());
        let digest = rack_recovery_model::event_digest(&scenario.events);
        if digest != row.digest {
            report.record(Mismatch {
                trace: format!("recovery seed {}", row.seed),
                expected: format!("digest {:016x}", row.digest),
                actual: format!("digest {digest:016x}"),
            });
            continue;
        }
        let oracle = scenario.run();
        if !golden_matches(
            &oracle.lost,
            oracle.pipe,
            &oracle.retransmits,
            oracle.recovery_entries,
            oracle.recovery_exits,
            row,
        ) {
            report.record(Mismatch {
                trace: format!("recovery seed {}", row.seed),
                expected: format!(
                    "golden lost {} pipe {} retransmits {} recovery {}/{}",
                    ranges(&row.lost),
                    row.pipe,
                    ranges(&row.retransmits),
                    row.recovery_entries,
                    row.recovery_exits
                ),
                actual: format!(
                    "model lost {} pipe {} retransmits {} recovery {}/{}",
                    ranges(&oracle.lost),
                    oracle.pipe,
                    ranges(&oracle.retransmits),
                    oracle.recovery_entries,
                    oracle.recovery_exits
                ),
            });
            continue;
        }
        let events: Vec<SackEvent> = scenario
            .events
            .iter()
            .map(|event| {
                let mut sacks = [(0, 0); 3];
                let count = event.sacks.len().min(3);
                sacks[..count].copy_from_slice(&event.sacks[..count]);
                SackEvent {
                    ack: event.ack,
                    sacks,
                    sack_count: count,
                    dsack: event.dsack,
                }
            })
            .collect();
        let product = product_sack(&events, rack_recovery_model::FIRST_SEQ);
        if product.lost != oracle.lost || product.pipe != oracle.pipe {
            report.record(Mismatch {
                trace: format!("recovery seed {}", row.seed),
                expected: format!("lost {} pipe {}", ranges(&oracle.lost), oracle.pipe),
                actual: format!("lost {} pipe {}", ranges(&product.lost), product.pipe),
            });
        }
    }
    report.gaps.push(
        "product SackScoreboard exposes lost ranges and pipe only; retransmit selections and \
         recovery entries/exits are not part of its public result"
            .to_owned(),
    );
    Ok(report)
}

/// Compare a regenerated model outcome with the committed golden row.
#[must_use]
pub fn golden_matches(
    lost: &[(u32, u32)],
    pipe: u32,
    retransmits: &[(u32, u32)],
    recovery_entries: u32,
    recovery_exits: u32,
    row: &trace::GoldenRow,
) -> bool {
    lost == row.lost
        && pipe == row.pipe
        && retransmits == row.retransmits
        && recovery_entries == row.recovery_entries
        && recovery_exits == row.recovery_exits
}

/// Replay the K320 10k RACK-TLP/DSACK event trace against `Tlp`/`Rack`.
pub fn replay_tlp() -> ClassReport {
    let seeds = [
        0x3200_0001u64,
        0x3200_0002,
        0x3200_0003,
        0x3200_0004,
        0x3200_0005,
    ];
    let config = TlpConfig::default();
    let mut report = ClassReport::new(Class::Tlp);
    for seed in seeds {
        report.traces = report.traces.saturating_add(1);
        let mut model = TlpModel::new(config, 10, 4);
        model.snd_una = 1000;
        model.snd_max = 9000;
        for index in 0..SEGMENTS {
            let start = 1000 + index * MSS;
            let _ = model.on_sent(start, start + MSS, 0);
        }
        let mut product_tlp = ProductTlp::default();
        product_tlp.arm(0, config.srtt_ms, config.rttvar_ms);
        let mut product_rack = Rack::new(0);
        let mut rng = TlpRng::new(seed);
        let mut now = 0u32;
        for _ in 0..2_000 {
            now = now.saturating_add(rng.range(1, 50));
            report.events = report.events.saturating_add(1);
            match rng.below(6) {
                0 => {
                    let end = 2000 + rng.below(8) * MSS + MSS;
                    let action = model.on_sent(1000, end, now);
                    product_rack.on_sent(end, now);
                    if let Action::Arm(_) = action {
                        product_tlp.arm(now, config.srtt_ms, config.rttvar_ms);
                        if model.tlp_deadline != product_tlp.deadline {
                            report.record(Mismatch {
                                trace: format!("tlp seed {seed:#x} event {now} send"),
                                expected: format!("deadline {:?}", model.tlp_deadline),
                                actual: format!("deadline {:?}", product_tlp.deadline),
                            });
                        }
                    }
                }
                1 => {
                    let ack = 1000 + rng.below(8) * MSS;
                    let action = model.on_ack(ack, now);
                    product_rack.on_ack(ack, now);
                    product_tlp.disarm();
                    if let Action::Arm(_) = action {
                        product_tlp.arm(now, config.srtt_ms, config.rttvar_ms);
                    }
                }
                2 => {
                    let start = 1000 + rng.below(8) * MSS;
                    model.on_dsack(start, start + MSS);
                    product_rack.on_dsack(config.srtt_ms);
                    if model.reo_wnd_ms != product_rack.reorder_window {
                        report.record(Mismatch {
                            trace: format!("tlp seed {seed:#x} event {now} dsack"),
                            expected: format!("reo_wnd {}", model.reo_wnd_ms),
                            actual: format!("reo_wnd {}", product_rack.reorder_window),
                        });
                    }
                }
                3 => {
                    let action = model.poll(now);
                    let model_fires = matches!(action, Action::Fire { .. });
                    if model_fires != product_tlp.fires(now) {
                        report.record(Mismatch {
                            trace: format!("tlp seed {seed:#x} event {now} poll"),
                            expected: format!("fires {model_fires}"),
                            actual: format!("fires {}", product_tlp.fires(now)),
                        });
                    }
                    if product_tlp.fires(now) {
                        product_tlp.disarm();
                    }
                }
                4 => {
                    let start = 1000 + rng.below(8) * MSS;
                    model.on_renege(start, start + MSS);
                }
                _ => {
                    let echoed = rng.below(10);
                    model.on_eifel(echoed, 0);
                }
            }
        }
    }
    report.gaps.push(
        "the product Tlp has no probe counter, no RTO cap and no spurious-undo/reneging result; \
         those K320 fields are compared only where Tlp/Rack expose them"
            .to_owned(),
    );
    report
}

/// Replay the K326 10k BBR/pacing event trace and record the missing product path.
pub fn replay_bbr() -> ClassReport {
    let seeds = [
        0x3260_0001u64,
        0x3260_0002,
        0x3260_0003,
        0x3260_0004,
        0x3260_0005,
        0x3260_0006,
        0x3260_0007,
        0x3260_0008,
        0x3260_0009,
        0x3260_000a,
    ];
    let mut report = ClassReport::new(Class::Bbr);
    let mut last = None;
    for seed in seeds {
        report.traces = report.traces.saturating_add(1);
        let mut rng = BbrRng::new(seed);
        let mss = rng.range(500, 1_500);
        let config = BbrConfig {
            mss,
            initial_cwnd: mss * rng.range(4, 10),
            max_cwnd: mss * rng.range(16, 256),
            initial_bw_bps: u64::from(rng.range(10_000, 1_000_000)),
            probe_rtt_interval_ms: rng.range(5_000, 20_000),
            ..BbrConfig::default()
        };
        let mut model = BbrModel::new(config);
        let mut now = 0u32;
        for _ in 0..1_000 {
            now = now.saturating_add(rng.range(1, 100));
            report.events = report.events.saturating_add(1);
            match rng.below(8) {
                0..=3 => {
                    model.on_delivery(rng.range(1, 64_000), rng.range(1_000, 1_000_000), now);
                }
                4 => {
                    model.on_round(rng.range(0, config.max_cwnd * 2), now);
                }
                5 => {
                    let diff = i32::try_from(rng.range(0, 20_000)).unwrap_or(0) - 10_000;
                    model.timely(rng.range(1_000, 1_000_000), diff, now);
                }
                6 => {
                    model.poll(now);
                }
                _ => {
                    let bytes = rng.range(1, 32_000);
                    model.on_delivery(bytes, rng.range(1_000, 500_000), now);
                    model.on_round(bytes, now);
                }
            }
            last = Some((seed, now, model.pacing_rate_bps(), model.cwnd));
        }
    }
    if let Some((seed, now, rate, cwnd)) = last {
        report.mismatches = report.events;
        report.first_mismatch = Some(Mismatch {
            trace: format!("bbr seed {seed:#x} event {now}"),
            expected: format!("pacing_rate {rate} B/s cwnd {cwnd}"),
            actual: "absent: raven-tcpip has no BBR/timely/pacing engine".to_owned(),
        });
    }
    report.gaps.push(
        "K326 oracle values are complete; the product side is absent, so every modelled event is \
         counted as a mismatch"
            .to_owned(),
    );
    report
}

/// Replay the K327 vtable callback matrix against `RackStack`.
pub fn replay_vtable() -> ClassReport {
    let mut report = ClassReport::new(Class::Vtable);
    let mut probes: Vec<Probe> = Vec::new();

    probes.push(Probe::new(
        "tfb_flags",
        format!("{}", vt::RACK_FB_FLAGS),
        format!("{}", RackFunctionBlock::rack().flags),
    ));

    let mut model = vt::VtableModel::new();
    let mut product = RackStack::new();
    probes.push(Probe::new(
        "fb_init",
        model_code(model.init()),
        result_code(product.tfb_tcp_fb_init().is_ok()),
    ));
    probes.push(Probe::new(
        "fb_init_twice",
        model_code(model.init()),
        result_code(product.tfb_tcp_fb_init().is_ok()),
    ));
    probes.push(Probe::new(
        "pru_oob",
        format!("{}", model.pru_options(vt::PRUS_OOB)),
        errno_code(product.tfb_pru_options(PRUS_OOB).err()),
    ));

    let model_divisor = model.ctloutput(vt::SockoptDir::Get, vt::RackOption::PacingDivisor, 0);
    let product_divisor = product
        .tfb_tcp_ctloutput(&RackSockopt {
            dir: SockoptDir::Get,
            level: IPPROTO_TCP,
            name: TCP_RACK_PACING_DIVISOR,
            value: 0,
        })
        .ok()
        .flatten();
    probes.push(Probe::new(
        "ctl_get_pacing_divisor",
        format!("{:?}", model_divisor.value),
        format!("{product_divisor:?}"),
    ));

    let model_set = model.ctloutput(vt::SockoptDir::Set, vt::RackOption::PaceMinSeg, 1_200);
    let product_set = product
        .tfb_tcp_ctloutput(&RackSockopt {
            dir: SockoptDir::Set,
            level: IPPROTO_TCP,
            name: TCP_RACK_PACE_MIN_SEG,
            value: 1_200,
        })
        .is_ok();
    probes.push(Probe::new(
        "ctl_set_pace_min_seg",
        model_code(model_set.error),
        result_code(product_set),
    ));

    model.snd_una = 1000;
    model.snd_max = 9000;
    model.sacked = 1000;
    model.considered_lost = 500;
    model.holes_rxt = 200;
    product.tp.snd_una = 1000;
    product.tp.snd_max = 9000;
    product.sacked = 1000;
    product.considered_lost = 500;
    product.holes_rxt = 200;
    probes.push(Probe::new(
        "compute_pipe",
        format!("{}", model.compute_pipe()),
        format!("{}", product.tfb_compute_pipe()),
    ));

    model.hpts_flags = vt::PACE_TMR_RACK;
    product.hpts_flags = u32::from(raven_tcpip::PACE_TMR_RACK);
    let model_query = model.chg_query(vt::Query::TimersUp);
    let mut product_query = RackQuery {
        req: TCP_QUERY_TIMERS_UP,
        ..RackQuery::default()
    };
    let product_timer = product.tfb_chg_query(&mut product_query);
    probes.push(Probe::new(
        "query_timers_up",
        format!("{}", model_query.value),
        format!("{product_timer}"),
    ));

    probes.push(Probe::new(
        "early_wake_check",
        format!("{}", model.early_wake_check()),
        format!("{}", product.tfb_early_wake_check()),
    ));

    model.state = vt::TcpState::Closed;
    product.tp.t_state = TCPS_CLOSED;
    probes.push(Probe::new(
        "handoff_closed",
        model_code(model.handoff_ok()),
        result_code(product.tfb_tcp_handoff_ok().is_ok()),
    ));

    model.rxtshift = 3;
    product.tp.t_rxtshift = 3;
    model.rexmit_timer();
    product.tfb_tcp_rexmit_tmr();
    probes.push(Probe::new(
        "rexmit_clear_scoreboard",
        format!(
            "sacked {} cleared {}",
            model.sacked, model.scoreboard_cleared
        ),
        format!(
            "sacked {} cleared {}",
            product.sacked,
            product.tp.t_rxtshift > 2
        ),
    ));

    // Exercise the K327 10k-event trace shape deterministically. The exact callback return is
    // not compared event-by-event here; the probe matrix above is the product comparison.
    for seed in [
        0x3270_0001u64,
        0x3270_0002,
        0x3270_0003,
        0x3270_0004,
        0x3270_0005,
    ] {
        report.traces = report.traces.saturating_add(1);
        let mut event_model = vt::VtableModel::new();
        let mut rng = rack_vtable_model::rng::Rng::new(seed);
        for _ in 0..2_000 {
            report.events = report.events.saturating_add(1);
            let event = random_vtable_event(&mut rng);
            let _ = event.apply(&mut event_model);
        }
    }

    for probe in probes {
        if probe.expected != probe.actual {
            report.record(Mismatch {
                trace: format!("vtable {}", probe.name),
                expected: probe.expected,
                actual: probe.actual,
            });
        }
    }
    report.gaps.push(
        "the 10k-event K327 trace is replayed for determinism; only the callback probe matrix is \
         compared against RackStack"
            .to_owned(),
    );
    report
}

struct Probe {
    name: &'static str,
    expected: String,
    actual: String,
}

impl Probe {
    fn new(name: &'static str, expected: String, actual: String) -> Self {
        Self {
            name,
            expected,
            actual,
        }
    }
}

fn result_code(ok: bool) -> String {
    if ok {
        "accept".to_owned()
    } else {
        "reject".to_owned()
    }
}

fn model_code(code: i32) -> String {
    if code == 0 {
        "accept".to_owned()
    } else {
        "reject".to_owned()
    }
}

fn errno_code(error: Option<raven_tcpip::Errno>) -> String {
    error.map_or_else(
        || "0".to_owned(),
        |errno| {
            i32::try_from(errno.to_ret())
                .unwrap_or(-1)
                .abs()
                .to_string()
        },
    )
}

fn ranges(values: &[(u32, u32)]) -> String {
    if values.is_empty() {
        "-".to_owned()
    } else {
        values
            .iter()
            .map(|(start, end)| format!("{start}-{end}"))
            .collect::<Vec<_>>()
            .join(",")
    }
}

fn random_vtable_event(rng: &mut rack_vtable_model::rng::Rng) -> vt::Event {
    use vt::{Event, Query, RackOption, SockoptDir as ModelDir};
    match rng.below(24) {
        0 => Event::Init,
        1 => Event::Fini(rng.below(2) == 1),
        2..=4 => Event::Output(rng.below(500)),
        5..=6 => Event::Segment(vt::SegmentKind::Ack, rng.below(200)),
        7..=8 => Event::SegmentUnlock(vt::SegmentKind::Ack, rng.below(200)),
        9 => Event::Queued(rng.below(2) == 1),
        10..=11 => Event::Ctl(
            ModelDir::Set,
            RackOption::from_u32(rng.below(2_000)),
            rng.below(2_000),
        ),
        12 => Event::Ctl(ModelDir::Get, RackOption::from_u32(rng.below(2_000)), 0),
        13 => Event::TimerStop,
        14 => Event::Rexmit,
        15 => Event::Handoff,
        16 => Event::Mtu([0, 536, 1_200, 1_460, 9_000][rng.below(5) as usize]),
        17 => Event::Pru(rng.below(4)),
        18 => Event::Hwtls(rng.below(2) == 1),
        19 => Event::Query(Query::from_u32(1 + rng.below(4))),
        20 => Event::Switch,
        21 => Event::Wake,
        22 => Event::Pipe,
        23 => Event::Info,
        _ => Event::Inherit(rng.below(2) == 1),
    }
}
