// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Property checks for the full feature set: CUBIC, RACK, ECN and PLPMTUD.
//!
//! These are RFC-based invariants over the current `raven-tcpip` public types (RFC 8312, 8985,
//! 3168 and 8899/9002). No donor code is copied; SACK is exercised by the SACK module.

use raven_tcpip::{
    CcContext, CongestionControl, Cubic, Duration, Ecn, IPV4_FLOOR, LossKind, Plpmtud, Rack,
    probe_sequence,
};

use crate::rng::Rng;

/// Run the mixed congestion/ECN/PLPMTUD property checks.
///
/// # Errors
///
/// Returns a minimal repro description when an invariant is violated.
pub fn check(seed: u64, ops: u32) -> Result<(), String> {
    let mut rng = Rng::new(seed);
    let mss = 1_200u32;
    let cwnd_max = 256 * mss;
    let mut cubic = Cubic::new(mss, 2 * mss, 32 * mss, cwnd_max);
    let mut context = CcContext {
        mss,
        flight: 0,
        pipe: 0,
        rtt: Some(Duration::from_millis(50)),
        snd_nxt: 0,
        snd_una: 0,
    };
    let mut rack = Rack::new(10);
    let mut ecn = Ecn::default();
    let mut plpmtud = Plpmtud::new(IPV4_FLOOR, 1_500, 16);
    plpmtud.start();

    for step in 0..ops {
        match rng.below(6) {
            0 | 1 => {
                let acked = rng.range(0, 2 * mss);
                cubic.on_ack(&mut context, acked, Some(Duration::from_millis(50)));
            }
            2 => cubic.on_loss(&mut context, LossKind::DupAck),
            3 => cubic.on_loss(&mut context, LossKind::Sack),
            4 => cubic.on_rto(&mut context),
            _ => cubic.on_ack(&mut context, mss, Some(Duration::from_millis(80))),
        }
        if cubic.cwnd() < mss || cubic.cwnd() > cwnd_max {
            return Err(format!(
                "CUBIC cwnd out of bounds at step {step}: {}",
                cubic.cwnd()
            ));
        }
        if cubic.ssthresh() < 2 * mss {
            return Err(format!(
                "CUBIC ssthresh below two MSS at step {step}: {}",
                cubic.ssthresh()
            ));
        }

        let now = step.saturating_mul(10);
        let srtt = 100u32;
        if step % 13 == 0 {
            rack.on_sent(step, now);
        }
        let bound = (srtt / 4).max(rack.reorder_window).max(1);
        let expected = now.saturating_sub(rack.xmit_ts) > bound;
        if rack.is_lost(now, srtt) != expected {
            return Err(format!("RACK loss decision mismatch at step {step}"));
        }
        if step % 7 == 0 {
            rack.on_dsack(srtt);
        }
        if rack.reorder_window > srtt.max(1) {
            return Err(format!(
                "RACK reorder window {} exceeds srtt {srtt} at step {step}",
                rack.reorder_window
            ));
        }
        if step % 11 == 0 {
            rack.on_ack(now, now);
        }

        if step % 9 == 0 {
            let local = rng.below(2) == 0;
            let peer = rng.below(2) == 0;
            ecn.negotiate(local, peer);
            let first = ecn.on_ce();
            let second = ecn.on_ce();
            if first && !ecn.negotiated {
                return Err(format!("ECN reacted without negotiation at step {step}"));
            }
            if first && second {
                return Err(format!("ECN reacted twice in one RTT at step {step}"));
            }
            ecn.on_rtt();
            let _ = ecn.take_cwr();
        }

        if let Some(size) = plpmtud.next_probe() {
            if !(IPV4_FLOOR..=1_500).contains(&size) {
                return Err(format!("PLPMTUD probe {size} out of range at step {step}"));
            }
            if rng.below(3) == 0 {
                plpmtud.on_probe_loss(size);
            } else {
                plpmtud.on_probe_ack(size);
            }
        }
    }

    for size in probe_sequence(IPV4_FLOOR, 1_500, 16) {
        if !(IPV4_FLOOR..=1_500).contains(&size) {
            return Err(format!("PLPMTUD sequence size {size} out of range"));
        }
    }
    Ok(())
}
