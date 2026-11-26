// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// Ported from FreeBSD at donor revision 133344405f532bd54ff3629a77d4c4e062f01fdb:
//   sys/netinet/tcp_stacks/rack.c  do_rack_compute_pipe, rack_compute_pipe,
//       rack_mark_lost, rack_mark_nolonger_lost, rack_check_recovery_mode,
//       rack_post_recovery, rack_exit_recovery, rack_calc_thresh_rack,
//       rack_calc_thresh_tlp; rack_cong_signal CC_NDUPACK entry path
//   sys/netinet/tcp_stacks/tcp_rack.h  rack_sendmap flags and retransmit slots
//   sys/netinet/cc/cc_newreno.c  newreno_cong_signal/newreno_cc_post_recovery
// BSD-2/3. The kernel sendmap/list pointers and locks are mapped to a sequence-ordered
// `Vec<RackSendmap>` and the K318 `RackStack`; no kernel pointer or TAILQ is copied.

//! RACK pipe/lost/recovery functions for the literal `rack.c` port.
//!
//! The donor group in this module is the K333 slice: in-flight pipe accounting, RACK loss
//! marking, recovery entry/exit, post-recovery window handling and the RACK/TLP reordering
//! thresholds. The context is [`RackPipe`], which wraps the K318 [`RackStack`] (K314 OS/tcpcb plus
//! the vtable) and the K313 [`RackHpts`] clock.

use alloc::vec::Vec;

use crate::rack_hpts::RackHpts;
use crate::rack_types::{
    RACK_ACKED, RACK_DELAYED_ACK_TIME, RACK_INITIAL_RTO, RACK_PRR_ADDBACKMAX,
    RACK_REORDER_FADE_DEFAULT, RACK_RTO_MAX, RACK_SACK_PASSED, RACK_TLP_MIN, RACK_WAS_LOST,
    RACK_WAS_SACKPASS, RackSendmap, TF_CONGRECOVERY, TF_FASTRECOVERY, TF_WASCRECOVERY,
    TF_WASFRECOVERY,
};
use crate::rack_vtable::{RackStack, TCPS_ESTABLISHED, TF_SENTFIN};

/// `TLP_USE_ID`.
pub const TLP_USE_ID: u8 = 1;
/// `TLP_USE_TWO_ONE`.
pub const TLP_USE_TWO_ONE: u8 = 2;
/// `TLP_USE_TWO_TWO`.
pub const TLP_USE_TWO_TWO: u8 = 3;
/// `TF2_ECN_PERMIT`.
pub const TF2_ECN_PERMIT: u32 = 0x0000_0020;
/// `TF2_ECN_SND_CWR`.
pub const TF2_ECN_SND_CWR: u32 = 0x0000_0040;
/// `CC_NDUPACK`.
pub const CC_NDUPACK: u32 = 0x0800;
/// `EXIT_RECOVERY` mask.
pub const EXIT_RECOVERY_MASK: u32 = TF_CONGRECOVERY | TF_FASTRECOVERY;
/// `IN_RECOVERY` mask.
pub const IN_RECOVERY_MASK: u32 = TF_CONGRECOVERY | TF_FASTRECOVERY;

/// `true` when `a < b` in the donor `TSTMP_LT` wrapping-microsecond order.
#[must_use]
pub const fn tstmp_lt(a: u32, b: u32) -> bool {
    (a.wrapping_sub(b) as i32) < 0
}

/// `true` when `a >= b` in the donor `TSTMP_GEQ` wrapping-microsecond order.
#[must_use]
pub const fn tstmp_ge(a: u32, b: u32) -> bool {
    !tstmp_lt(a, b)
}

/// The donor rack fields not carried by the K314/K318 contexts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RackPipeState {
    /// `rack_no_prr`.
    pub rack_no_prr: bool,
    /// `no_prr_addback`.
    pub no_prr_addback: bool,
    /// `rack_prr_addbackmax`.
    pub prr_addbackmax: u32,
    /// NewReno beta percentage used by `newreno_cong_signal`.
    pub beta: u32,
    /// `tp->snd_recover`.
    pub snd_recover: u32,
    /// `tp->snd_cwnd_prev`.
    pub snd_cwnd_prev: u32,
    /// `tp->snd_ssthresh_prev`.
    pub snd_ssthresh_prev: u32,
    /// `tp->snd_recover_prev`.
    pub snd_recover_prev: u32,
    /// `tp->t_bytes_acked`.
    pub t_bytes_acked: u32,
    /// `rack->rc_initial_ss_comp`.
    pub initial_ss_comp: bool,
    /// `rack->r_fast_output`.
    pub fast_output: bool,
    /// `rack->r_ctl.recovery_rxt_cnt`.
    pub recovery_rxt_cnt: u32,
    /// `rack->r_ctl.rc_cwnd_at_erec`.
    pub cwnd_at_erec: u32,
    /// `rack->r_ctl.rc_ssthresh_at_erec`.
    pub ssthresh_at_erec: u32,
    /// `rack->r_ent_rec_ns`.
    pub ent_rec_ns: bool,
    /// `rack->r_ctl.dsack_byte_cnt`.
    pub dsack_byte_cnt: u32,
    /// `rack->r_ctl.retran_during_recovery`.
    pub retran_during_recovery: u32,
    /// `rack->r_ctl.dsack_persist`.
    pub dsack_persist: u32,
    /// `rack->rto_from_rec`.
    pub rto_from_rec: bool,
    /// `rack->r_ctl.rto_ssthresh`.
    pub rto_ssthresh: u32,
    /// `rack->r_ctl.rc_reorder_ts`.
    pub reorder_ts: u32,
    /// `rack->r_ctl.rc_reorder_fade`.
    pub reorder_fade: u32,
    /// `rack->r_ctl.rc_pkt_delay`.
    pub pkt_delay: u32,
    /// `rack->r_ctl.rc_reorder_shift`.
    pub reorder_shift: u8,
    /// `rack->rc_rack_tmr_std_based`.
    pub tmr_std_based: bool,
    /// `rack->rc_rack_use_dsack`.
    pub use_dsack: bool,
    /// `rack->r_ctl.rc_tlp_threshold`.
    pub tlp_threshold: u32,
    /// `rack->rack_tlp_threshold_use`.
    pub tlp_threshold_use: u8,
    /// `rack_tlp_min`.
    pub tlp_min: u32,
    /// `rack_rto_max`.
    pub rto_max: u32,
    /// `rack_delayed_ack_time`.
    pub delayed_ack_time: u32,
}

impl Default for RackPipeState {
    fn default() -> Self {
        Self {
            rack_no_prr: false,
            no_prr_addback: false,
            prr_addbackmax: RACK_PRR_ADDBACKMAX,
            beta: 50,
            snd_recover: 0,
            snd_cwnd_prev: 0,
            snd_ssthresh_prev: 0,
            snd_recover_prev: 0,
            t_bytes_acked: 0,
            initial_ss_comp: false,
            fast_output: false,
            recovery_rxt_cnt: 0,
            cwnd_at_erec: 0,
            ssthresh_at_erec: 0,
            ent_rec_ns: false,
            dsack_byte_cnt: 0,
            retran_during_recovery: 0,
            dsack_persist: 0,
            rto_from_rec: false,
            rto_ssthresh: 0,
            reorder_ts: 0,
            reorder_fade: RACK_REORDER_FADE_DEFAULT,
            pkt_delay: 0,
            reorder_shift: 0,
            tmr_std_based: false,
            use_dsack: false,
            tlp_threshold: 1,
            tlp_threshold_use: TLP_USE_TWO_ONE,
            tlp_min: RACK_TLP_MIN,
            rto_max: RACK_RTO_MAX,
            delayed_ack_time: RACK_DELAYED_ACK_TIME,
        }
    }
}

/// The RACK pipe/lost/recovery context.
pub struct RackPipe {
    /// K318 transport stack (K314 OS/tcpcb and the vtable).
    pub stack: RackStack,
    /// K313 HPTS clock and timer context.
    pub hpts: RackHpts,
    /// Donor rack fields not owned by K314/K318.
    pub state: RackPipeState,
    /// Sendmap in donor `rc_tmap` order.
    sendmap: Vec<RackSendmap>,
    /// `rack->rc_rack_rtt` in microseconds.
    pub rack_rtt: u32,
}

impl Default for RackPipe {
    fn default() -> Self {
        Self::new()
    }
}

impl RackPipe {
    /// Create a pipe context over a fresh K318 stack and a zeroed K313 clock.
    #[must_use]
    pub fn new() -> Self {
        Self {
            stack: RackStack::new(),
            hpts: RackHpts::new(0),
            state: RackPipeState::default(),
            sendmap: Vec::new(),
            rack_rtt: 0,
        }
    }

    /// The sendmap in donor transmission order.
    #[must_use]
    pub fn sendmap(&self) -> &[RackSendmap] {
        &self.sendmap
    }

    /// Append one sendmap entry.
    pub fn push_sendmap(&mut self, rsm: RackSendmap) {
        self.sendmap.push(rsm);
    }

    /// Mutable access to one sendmap entry.
    pub fn sendmap_mut(&mut self, index: usize) -> Option<&mut RackSendmap> {
        self.sendmap.get_mut(index)
    }

    /// The K313 clock in microseconds.
    #[must_use]
    pub fn now_us(&self) -> u64 {
        self.hpts.now_us()
    }

    /// `rack_grab_rtt`: the last RACK RTT, the stack SRTT, or `RACK_INITIAL_RTO`.
    #[must_use]
    pub fn rack_grab_rtt(&self) -> u32 {
        if self.rack_rtt != 0 {
            self.rack_rtt
        } else if self.stack.tp.t_srtt == 0 {
            RACK_INITIAL_RTO
        } else {
            self.stack.tp.t_srtt
        }
    }

    /// `ctf_outstanding`: outstanding sequence space, including the SYN and FIN slots.
    fn ctf_outstanding(&self) -> u32 {
        let mut outstanding = self.stack.tp.snd_max.wrapping_sub(self.stack.tp.snd_una);
        if self.stack.tp.t_state < TCPS_ESTABLISHED {
            outstanding = outstanding.wrapping_add(1);
        }
        if self.stack.tp.t_flags & TF_SENTFIN != 0 {
            outstanding = outstanding.wrapping_add(1);
        }
        outstanding
    }

    /// `do_rack_compute_pipe`.
    #[must_use]
    pub fn do_rack_compute_pipe(&self, snd_una: u32) -> u32 {
        let outstanding = self.stack.tp.snd_max.wrapping_sub(snd_una);
        let removed = self.stack.sacked.wrapping_add(self.stack.considered_lost);
        outstanding
            .wrapping_sub(removed)
            .wrapping_add(self.stack.holes_rxt)
    }

    /// `rack_compute_pipe`.
    #[must_use]
    pub fn rack_compute_pipe(&self) -> i32 {
        self.do_rack_compute_pipe(self.stack.tp.snd_una) as i32
    }

    /// `rack_calc_thresh_rack`.
    pub fn rack_calc_thresh_rack(&mut self, srtt: u32, cts: u32) -> u32 {
        let srtt = srtt.max(1);
        let mut lro = 0u32;
        if self.state.reorder_ts != 0 {
            if self.state.reorder_fade != 0 {
                let mut since = 0u32;
                if tstmp_ge(cts, self.state.reorder_ts) {
                    since = cts.wrapping_sub(self.state.reorder_ts);
                    if since == 0 {
                        since = 1;
                    }
                }
                if since > self.state.reorder_fade {
                    self.state.reorder_ts = 0;
                    since = 0;
                }
                lro = since;
            } else {
                lro = 1;
            }
        }
        let mut thresh = if self.state.tmr_std_based {
            srtt.saturating_add(srtt >> 2)
        } else {
            srtt.saturating_add(self.state.pkt_delay)
        };
        if lro != 0 && !self.state.tmr_std_based {
            let shift = if self.state.reorder_shift == 0 {
                2
            } else {
                u32::from(self.state.reorder_shift)
            };
            thresh = thresh.saturating_add(srtt >> shift);
        }
        if self.state.use_dsack && lro != 0 && self.stack.num_dsacks > 0 {
            thresh = thresh.saturating_add(self.stack.num_dsacks.saturating_mul(srtt >> 2));
        }
        let ceiling = srtt.saturating_mul(2);
        if thresh > ceiling {
            thresh = ceiling;
        }
        if thresh > self.state.rto_max {
            thresh = self.state.rto_max;
        }
        thresh
    }

    /// `rack_calc_thresh_tlp`.
    #[must_use]
    pub fn rack_calc_thresh_tlp(&self, index: usize, srtt: u32) -> u32 {
        let srtt = srtt.max(1);
        let mut thresh = match self.state.tlp_threshold {
            0 => srtt.saturating_mul(2),
            divisor => srtt.saturating_add(srtt / divisor),
        };
        let Some(rsm) = self.sendmap.get(index) else {
            return thresh;
        };
        let segsiz = u32::from(self.stack.tp.t_maxseg).min(u32::from(self.stack.pace_min_segs));
        let len = rsm.len();
        let alt = |thresh: u32| {
            thresh.max(
                srtt.saturating_add(srtt / 2)
                    .saturating_add(self.state.delayed_ack_time),
            )
        };
        match self.state.tlp_threshold_use {
            TLP_USE_ID => {
                let pipe = self
                    .stack
                    .tp
                    .snd_max
                    .wrapping_sub(self.stack.tp.snd_una)
                    .wrapping_sub(self.stack.sacked)
                    .wrapping_add(self.stack.holes_rxt);
                if pipe <= segsiz {
                    thresh = alt(thresh);
                }
            }
            TLP_USE_TWO_ONE => {
                if index > 0 && len <= segsiz {
                    let previous = self.sendmap[index - 1];
                    let idx = usize::from(rsm.rtr_cnt.saturating_sub(1));
                    let nidx = usize::from(previous.rtr_cnt.saturating_sub(1));
                    let gap = if rsm.tim_lastsent[nidx] >= previous.tim_lastsent[idx] {
                        (rsm.tim_lastsent[nidx] - previous.tim_lastsent[idx]) as u32
                    } else {
                        0
                    };
                    thresh = thresh.saturating_add(gap);
                } else if len <= segsiz {
                    thresh = alt(thresh);
                }
            }
            TLP_USE_TWO_TWO if len <= segsiz => {
                thresh = alt(thresh);
            }
            _ => {}
        }
        if thresh > self.stack.tp.t_rxtcur {
            thresh = self.stack.tp.t_rxtcur;
        }
        if thresh > self.state.rto_max {
            thresh = self.state.rto_max;
        }
        if thresh < self.state.tlp_min {
            thresh = self.state.tlp_min;
        }
        thresh
    }

    /// `rack_mark_lost`.
    pub fn rack_mark_lost(&mut self, index: usize, cts: u32) {
        let Some(base) = self.sendmap.get(index).copied() else {
            return;
        };
        let thresh = self.rack_calc_thresh_rack(self.rack_grab_rtt(), cts);
        for rsm in &mut self.sendmap[index..] {
            if rsm.flags & RACK_SACK_PASSED == 0 {
                break;
            }
            if rsm.flags & RACK_WAS_LOST != 0 {
                continue;
            }
            let expires = (base.last_sent() as u32).wrapping_add(thresh);
            if tstmp_lt(expires, cts) || expires == cts {
                rsm.flags |= RACK_WAS_LOST;
                self.stack.considered_lost = self.stack.considered_lost.saturating_add(rsm.len());
            } else {
                break;
            }
        }
    }

    /// `rack_mark_nolonger_lost`.
    pub fn rack_mark_nolonger_lost(&mut self, index: usize) {
        let Some(len) = self.sendmap.get(index).map(RackSendmap::len) else {
            return;
        };
        if let Some(rsm) = self.sendmap.get_mut(index) {
            rsm.flags &= !RACK_WAS_LOST;
        }
        if self.stack.considered_lost >= len {
            self.stack.considered_lost -= len;
        } else {
            self.stack.considered_lost = 0;
        }
    }

    /// `rack_check_recovery_mode`.
    pub fn rack_check_recovery_mode(&mut self, tsused: u32) -> Option<usize> {
        let index = self
            .sendmap
            .iter()
            .position(|rsm| rsm.flags & RACK_ACKED == 0)?;
        let rsm = self.sendmap[index];
        let idx = usize::from(rsm.rtr_cnt.saturating_sub(1));
        let thresh = self.rack_calc_thresh_rack(self.rack_grab_rtt(), tsused);
        let last_sent = rsm.tim_lastsent[idx] as u32;
        if tstmp_lt(tsused, last_sent) {
            return None;
        }
        if tsused.wrapping_sub(last_sent) < thresh {
            return None;
        }
        self.rack_enter_recovery();
        Some(index)
    }

    /// `rack_check_recovery_mode` using the K313 clock.
    pub fn rack_check_recovery_mode_now(&mut self) -> Option<usize> {
        let now = u32::try_from(self.hpts.now_us()).unwrap_or(u32::MAX);
        self.rack_check_recovery_mode(now)
    }

    /// The `rack_cong_signal(CC_NDUPACK)` recovery-entry path.
    ///
    /// The donor has no `rack_enter_recovery` symbol: this is the named model of the
    /// `rack_cong_signal` CC_NDUPACK arm plus the `newreno_cong_signal` entry it calls.
    pub fn rack_enter_recovery(&mut self) {
        let in_recovery_at_entry = self.stack.tp.t_flags & IN_RECOVERY_MASK != 0;
        let cwnd_enter = self.stack.tp.snd_cwnd;
        let ssthresh_enter = self.stack.tp.snd_ssthresh;
        self.stack.tp.t_flags &= !(TF_WASFRECOVERY | TF_WASCRECOVERY);
        if self.stack.tp.t_flags & TF_FASTRECOVERY == 0 {
            // `newreno_cong_signal(CC_NDUPACK)`: multiplicative decrease and ENTER_RECOVERY.
            let mss = u32::from(self.stack.tp.t_maxseg).max(1);
            let scaled = (u64::from(self.stack.tp.snd_cwnd) * u64::from(self.state.beta))
                / (100 * u64::from(mss));
            let cwin =
                u32::try_from(scaled.max(2).saturating_mul(u64::from(mss))).unwrap_or(u32::MAX);
            if self.stack.tp.t_flags & TF_CONGRECOVERY == 0 {
                self.stack.tp.snd_ssthresh = cwin;
            }
            self.stack.tp.t_flags |= TF_CONGRECOVERY | TF_FASTRECOVERY;

            self.state.initial_ss_comp = true;
            self.stack.prr_delivered = 0;
            self.stack.prr_out = 0;
            self.state.fast_output = false;
            self.state.recovery_rxt_cnt = 0;
            for rsm in &self.sendmap {
                if rsm.flags & RACK_WAS_SACKPASS != 0 && rsm.rtr_cnt > 1 {
                    let bytes = rsm.len().saturating_mul(u32::from(rsm.rtr_cnt - 1));
                    self.state.recovery_rxt_cnt = self.state.recovery_rxt_cnt.saturating_add(bytes);
                }
            }
            if !self.state.rack_no_prr {
                self.stack.prr_sndcnt = mss;
            }
            self.stack.prr_recovery_fs = self.stack.tp.snd_max.wrapping_sub(self.stack.tp.snd_una);
            self.state.snd_recover = self.stack.tp.snd_max;
            if self.stack.t_flags2 & TF2_ECN_PERMIT != 0 {
                self.stack.t_flags2 |= TF2_ECN_SND_CWR;
            }
        }
        if !in_recovery_at_entry && self.stack.tp.t_flags & IN_RECOVERY_MASK != 0 {
            self.state.dsack_byte_cnt = 0;
            self.state.retran_during_recovery = 0;
            self.state.cwnd_at_erec = cwnd_enter;
            self.state.ssthresh_at_erec = ssthresh_enter;
            self.state.ent_rec_ns = true;
        }
    }

    /// `rack_exit_recovery`.
    pub fn rack_exit_recovery(&mut self, _how: i32) {
        self.stack.tp.t_flags &= !EXIT_RECOVERY_MASK;
    }

    /// `rack_post_recovery`.
    pub fn rack_post_recovery(&mut self, _th_ack: u32) {
        let mss = u32::from(self.stack.tp.t_maxseg).max(1);
        if self.stack.tp.t_flags & TF_FASTRECOVERY != 0 {
            // `newreno_cc_post_recovery`: RFC 6582 window recovery, then the RACK floor.
            let pipe = self.rack_compute_pipe().max(0) as u32;
            if pipe < self.stack.tp.snd_ssthresh {
                self.stack.tp.snd_cwnd = pipe.max(mss).saturating_add(mss);
            } else {
                self.stack.tp.snd_cwnd = self.stack.tp.snd_ssthresh;
            }
        }
        if self.stack.tp.snd_cwnd < self.stack.tp.snd_ssthresh {
            self.stack.tp.snd_cwnd = self.stack.tp.snd_ssthresh;
        }
        if !self.state.rack_no_prr && !self.state.no_prr_addback && self.stack.prr_sndcnt > 0 {
            if self.ctf_outstanding() <= self.stack.os.sbavail() {
                let addback = mss
                    .saturating_mul(self.state.prr_addbackmax)
                    .min(self.stack.prr_sndcnt);
                self.stack.tp.snd_cwnd = self.stack.tp.snd_cwnd.saturating_add(addback);
            }
            self.stack.prr_sndcnt = 0;
        }
        self.state.snd_recover = self.stack.tp.snd_una;
        if self.state.dsack_persist > 0 {
            self.state.dsack_persist -= 1;
            if self.stack.num_dsacks != 0 && self.state.dsack_persist == 0 {
                self.stack.num_dsacks = 0;
            }
        }
        if self.state.rto_from_rec {
            self.state.rto_from_rec = false;
            if self.state.rto_ssthresh > self.stack.tp.snd_ssthresh {
                self.stack.tp.snd_ssthresh = self.state.rto_ssthresh;
            }
        }
        self.rack_exit_recovery(1);
    }
}

#[cfg(test)]
#[path = "rack_pipe_ut.rs"]
mod tests;
