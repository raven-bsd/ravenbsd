// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Donor-formula oracle for the K359 CUBIC/NewReno/RACK model audit.
//!
//! The formulas are transcribed from FreeBSD `sys/netinet/cc/cc.c`,
//! `sys/netinet/cc/cc_cubic.c`, `sys/netinet/cc/cc_cubic.h` and
//! `sys/netinet/tcp_stacks/rack.c` at 133344405f for comparison only. No donor expression is
//! copied into product code; this is test/audit code.

#![forbid(unsafe_code)]

/// Sender MSS used by the K341 model.
pub const MSS: u32 = 1_000;
/// First byte sequence used by the K341 model.
pub const FIRST_SEQ: u32 = 1_000;
/// Segments initially in flight in the K341 model.
pub const SEGMENTS: usize = 8;
/// Number of K341 golden traces.
pub const TRACE_COUNT: u64 = 10_000;

/// FreeBSD `newreno_beta` default, percent.
pub const NEWRENO_BETA: u64 = 50;
/// FreeBSD `CUBIC_BETA` (`179/256`).
pub const CUBIC_BETA: u64 = 179;
/// FreeBSD `CUBIC_FC_FACTOR` (`217/256`).
pub const CUBIC_FC: u64 = 217;
/// FreeBSD `CUBIC_SHIFT`.
pub const CUBIC_SHIFT: u32 = 8;
/// RFC 9438 beta.
pub const RFC_CUBIC_BETA: f64 = 0.7;

/// `newreno_cc_cwnd_on_multiplicative_decrease`: `max(cwnd*beta/(100*mss), 2)*mss`.
#[must_use]
pub fn donor_newreno_md(cwnd: u32, mss: u32) -> u32 {
    let mss = mss.max(1);
    let segments = (u64::from(cwnd) * NEWRENO_BETA / (100 * u64::from(mss))).max(2);
    u32::try_from(segments.saturating_mul(u64::from(mss))).unwrap_or(u32::MAX)
}

/// `newreno_cc_post_recovery`: RFC 6582 floor, otherwise `ssthresh`.
#[must_use]
pub fn donor_newreno_post_recovery(pipe: u32, ssthresh: u32, mss: u32) -> u32 {
    if pipe < ssthresh {
        pipe.max(mss).saturating_add(mss)
    } else {
        ssthresh
    }
}

/// `newreno_cc_cwnd_in_slow_start` with ABC `l_var = 2` and one segment.
#[must_use]
pub fn donor_newreno_slow_start(cwnd: u32, acked: u32, mss: u32, maxwin: u32) -> u32 {
    cwnd.saturating_add(acked.min(2 * mss)).min(maxwin)
}

/// `newreno_cc_cwnd_in_cong_avoid` without the per-ACK ABC latch (no-ABC form).
#[must_use]
pub fn donor_newreno_cong_avoid(cwnd: u32, mss: u32, maxwin: u32) -> u32 {
    let increment = ((u64::from(mss) * u64::from(mss)) / u64::from(cwnd.max(1))).max(1);
    let next = u64::from(cwnd).saturating_add(increment);
    u32::try_from(next.min(u64::from(maxwin))).unwrap_or(maxwin)
}

/// RFC 9438 `K = ((W_max - cwnd_epoch) / C)^(1/3)` in seconds.
#[must_use]
pub fn donor_cubic_k(wmax_segs: f64, epoch_segs: f64) -> f64 {
    if wmax_segs <= epoch_segs {
        0.0
    } else {
        ((wmax_segs - epoch_segs) / 0.4).powf(1.0 / 3.0)
    }
}

/// RFC 9438 `W_cubic(t) = C*(t-K)^3 + W_max` in segments.
#[must_use]
pub fn donor_cubic_w_cubic(t_s: f64, wmax_segs: f64, epoch_segs: f64) -> f64 {
    let k = donor_cubic_k(wmax_segs, epoch_segs);
    0.4 * (t_s - k).powi(3) + wmax_segs
}

/// RFC 9438 `W_est(t) = W_max*beta + alpha*(t/RTT)` in segments.
#[must_use]
pub fn donor_cubic_w_est(wmax_segs: f64, t_s: f64, rtt_s: f64) -> f64 {
    let alpha = 3.0 * (1.0 - RFC_CUBIC_BETA) / (1.0 + RFC_CUBIC_BETA);
    wmax_segs * RFC_CUBIC_BETA + alpha * (t_s / rtt_s.max(f64::EPSILON))
}

/// `cubic_get_ssthresh`: fast convergence, then NewReno in TF or `pipe*179/256`.
#[must_use]
pub fn donor_cubic_get_ssthresh(cwnd: u32, w_max: u32, pipe: u32, in_tf: bool, mss: u32) -> u32 {
    let cwnd = if cwnd < w_max {
        u32::try_from(u64::from(cwnd) * CUBIC_FC / 256).unwrap_or(cwnd)
    } else {
        cwnd
    };
    if in_tf {
        donor_newreno_md(cwnd, mss)
    } else {
        u32::try_from(u64::from(pipe) * CUBIC_BETA / 256).unwrap_or(0)
    }
}

/// The donor `cubic_ack_received` target cap: `min(W_cubic, 1.5*cwnd)`.
#[must_use]
pub fn donor_cubic_target(w_cubic_segs: f64, cwnd_segs: f64) -> f64 {
    if w_cubic_segs < cwnd_segs {
        cwnd_segs
    } else {
        w_cubic_segs.min(cwnd_segs * 1.5)
    }
}

/// `do_rack_compute_pipe`, using the donor unsigned arithmetic.
#[must_use]
pub fn donor_rack_pipe(
    snd_max: u32,
    snd_una: u32,
    sacked: u32,
    considered_lost: u32,
    holes_rxt: u32,
) -> u32 {
    snd_max
        .wrapping_sub(snd_una)
        .wrapping_sub(sacked.wrapping_add(considered_lost))
        .wrapping_add(holes_rxt)
}

/// The K341 header, kept here so the trace audit does not depend on a private constant.
pub const TRACE_HEADER: &str = "\
# CUBIC/NewReno/RACK congestion-control reference traces (K341)
# seed\tevents\tcwnd\tssthresh\tin_recovery\tlost\tretransmits\tpipe\tdigest
";

/// The K341 trace digest, recomputed independently from the scenario events.
#[must_use]
pub fn trace_digest(events: &[cc_model::Event]) -> u64 {
    let mut digest = 0xcbf2_9ce4_8422_2325u64;
    for event in events {
        for byte in event.ack.to_le_bytes() {
            digest ^= u64::from(byte);
            digest = digest.wrapping_mul(0x100_0000_01b3);
        }
        digest ^= event.at;
    }
    digest
}
