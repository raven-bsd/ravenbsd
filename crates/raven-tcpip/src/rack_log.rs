// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// Ported from FreeBSD `sys/netinet/tcp_stacks/rack.c` at donor revision
// 133344405f532bd54ff3629a77d4c4e062f01fdb, BSD-2-Clause.
// Function map (donor name -> K329 function):
//   rack_log_gpset, sysctl_rack_clear, sysctl_rack_wma_divisor,
//   rack_init_sysctls, rack_log_hybrid_bw, rack_log_hybrid_sends,
//   rack_log_dsack_event, rack_log_hdwr_pacing, rack_log_retran_reason,
//   rack_log_to_start, rack_log_to_event, rack_log_map_chg,
//   rack_log_rtt_upd, rack_log_rtt_sample, rack_log_rtt_sample_calc,
//   rack_log_rtt_sendmap, rack_log_progress_event, rack_log_type_bbrsnd,
//   rack_log_doseg_done, rack_log_type_pacing_sizes,
//   rack_log_type_just_return, rack_log_to_cancel,
//   rack_log_alt_to_to_cancel, rack_log_to_processing, rack_log_to_prr,
//   rack_counter_destroy, rack_log_timely, rack_log_rtt_shrinks,
//   rack_log_gp_calc, rack_log_hpts_diag, rack_log_wakeup, rack_log_output,
//   rack_log_sack_passed, rack_log_ack, rack_log_hybrid,
//   rack_log_collapse, rack_log_hystart_event, rack_log_chg_info,
//   rack_log_input_packet, rack_log_pcm, rack_log_pacing_delay_calc,
//   rack_log_fsb, rack_log_queue_level.
// The kernel pointers, BTF buffer and sysctl tree are mapped to the K314 RackOs
// software context; the events carry the donor fields and are tested below.

//! FreeBSD RACK logging, sysctl, counter and debug/BTF paths.
//!
//! `RackLog` is the K329 slice of the literal `rack.c` port. It owns the BTF/
//! blackbox event stream, the RACK counters, the sysctl nodes and the logging
//! entry points for timers, RTT, recovery, pacing, queue level and ACK/SACK.
//! The K313 HPTS context supplies the timer fields through `RackOs::tcpcb` and
//! the K314 `RackOs` supplies the clock, sysctl tree and counter storage.
#![allow(clippy::too_many_arguments)] // donor signatures are wide; keep their shape

use alloc::vec;
use alloc::vec::Vec;

use crate::rack_os::{RackOs, RackOsError, TcpLog};

/// Event kind for `rack_log_gpset`.
pub const KIND_GPSET: u32 = 1001;
/// Event kind for `sysctl_rack_clear`.
pub const KIND_SYSCTL_CLEAR: u32 = 1002;
/// Event kind for `sysctl_rack_wma_divisor`.
pub const KIND_WMA_DIVISOR: u32 = 1003;
/// Event kind for `rack_init_sysctls`.
pub const KIND_INIT_SYSCTLS: u32 = 1004;
/// Event kind for `rack_log_hybrid_bw`.
pub const KIND_HYBRID_BW: u32 = 1005;
/// Event kind for `rack_log_hybrid_sends`.
pub const KIND_HYBRID_SENDS: u32 = 1006;
/// Event kind for `rack_log_dsack_event`.
pub const KIND_DSACK: u32 = 1007;
/// Event kind for `rack_log_hdwr_pacing`.
pub const KIND_HDWR_PACING: u32 = 1008;
/// Event kind for `rack_log_retran_reason`.
pub const KIND_RETRAN_REASON: u32 = 1009;
/// Event kind for `rack_log_to_start`.
pub const KIND_TO_START: u32 = 1010;
/// Event kind for `rack_log_to_event`.
pub const KIND_TO_EVENT: u32 = 1011;
/// Event kind for `rack_log_map_chg`.
pub const KIND_MAP_CHG: u32 = 1012;
/// Event kind for `rack_log_rtt_upd`.
pub const KIND_RTT_UPD: u32 = 1013;
/// Event kind for `rack_log_rtt_sample`.
pub const KIND_RTT_SAMPLE: u32 = 1014;
/// Event kind for `rack_log_rtt_sample_calc`.
pub const KIND_RTT_SAMPLE_CALC: u32 = 1015;
/// Event kind for `rack_log_rtt_sendmap`.
pub const KIND_RTT_SENDMAP: u32 = 1016;
/// Event kind for `rack_log_progress_event`.
pub const KIND_PROGRESS: u32 = 1017;
/// Event kind for `rack_log_type_bbrsnd`.
pub const KIND_BBRSND: u32 = 1018;
/// Event kind for `rack_log_doseg_done`.
pub const KIND_DOSEG_DONE: u32 = 1019;
/// Event kind for `rack_log_type_pacing_sizes`.
pub const KIND_PACING_SIZES: u32 = 1020;
/// Event kind for `rack_log_type_just_return`.
pub const KIND_JUST_RETURN: u32 = 1021;
/// Event kind for `rack_log_to_cancel`.
pub const KIND_TO_CANCEL: u32 = 1022;
/// Event kind for `rack_log_alt_to_to_cancel`.
pub const KIND_ALT_TO_CANCEL: u32 = 1023;
/// Event kind for `rack_log_to_processing`.
pub const KIND_TO_PROCESSING: u32 = 1024;
/// Event kind for `rack_log_to_prr`.
pub const KIND_TO_PRR: u32 = 1025;
/// Event kind for `rack_counter_destroy`.
pub const KIND_COUNTER_DESTROY: u32 = 1026;
/// Event kind for `rack_log_timely`.
pub const KIND_TIMELY: u32 = 1027;
/// Event kind for `rack_log_rtt_shrinks`.
pub const KIND_RTT_SHRINKS: u32 = 1028;
/// Event kind for `rack_log_gp_calc`.
pub const KIND_GP_CALC: u32 = 1029;
/// Event kind for `rack_log_hpts_diag`.
pub const KIND_HPTS_DIAG: u32 = 1030;
/// Event kind for `rack_log_wakeup`.
pub const KIND_WAKEUP: u32 = 1031;
/// Event kind for `rack_log_output`.
pub const KIND_OUTPUT: u32 = 1032;
/// Event kind for `rack_log_sack_passed`.
pub const KIND_SACK_PASSED: u32 = 1033;
/// Event kind for `rack_log_ack`.
pub const KIND_ACK: u32 = 1034;
/// Event kind for `rack_log_hybrid`.
pub const KIND_HYBRID: u32 = 1035;
/// Event kind for `rack_log_collapse`.
pub const KIND_COLLAPSE: u32 = 1036;
/// Event kind for `rack_log_hystart_event`.
pub const KIND_HYSTART: u32 = 1037;
/// Event kind for `rack_log_chg_info`.
pub const KIND_CHG_INFO: u32 = 1038;
/// Event kind for `rack_log_input_packet`.
pub const KIND_INPUT_PACKET: u32 = 1039;
/// Event kind for `rack_log_pcm`.
pub const KIND_PCM: u32 = 1040;
/// Event kind for `rack_log_pacing_delay_calc`.
pub const KIND_PACING_DELAY: u32 = 1041;
/// Event kind for `rack_log_fsb`.
pub const KIND_FSB: u32 = 1042;
/// Event kind for `rack_log_queue_level`.
pub const KIND_QUEUE_LEVEL: u32 = 1043;

/// One retained RACK log event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LogEntry {
    /// Donor event kind (`KIND_*`).
    pub kind: u32,
    /// Four scalar fields from the donor event.
    pub args: [u64; 4],
    /// Error/status field.
    pub status: i32,
}

/// Named RACK counter selected by the donor logging paths.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Counter {
    /// `rack_total_bytes`.
    TotalBytes,
    /// `rack_fto_send`.
    FtoSend,
    /// `rack_fto_rsm_send`.
    FtoRsmSend,
    /// `rack_dsack`.
    Dsack,
    /// `rack_to_prr`.
    Prr,
    /// `rack_to_wakeup`.
    Wakeup,
    /// `rack_out_size`.
    OutBytes,
    /// `rack_tlp_tot`.
    Tlp,
}

impl Counter {
    const fn index(self) -> usize {
        self as usize
    }
}

/// K329 RACK logging/sysctl/counter context.
pub struct RackLog {
    /// K314 OS context (clock, sysctl tree, tcpcb mirror).
    pub os: RackOs,
    /// Retained structured events.
    entries: Vec<LogEntry>,
    /// Named counters.
    counters: [u64; 8],
    /// Per-option array cleared by `sysctl_rack_clear`.
    option_array: [u64; 4],
    /// Out-size array cleared by `sysctl_rack_clear`.
    out_sizes: [u64; 4],
    /// `rack_wma_divisor` value.
    wma_divisor: u32,
    /// Sysctl context id.
    sysctl_ctx: u32,
    /// Sysctl node ids created by `rack_init_sysctls`.
    sysctl_nodes: Vec<u32>,
    /// Test-only mutation switch: suppress every side effect.
    pub mutated: bool,
}

impl Default for RackLog {
    fn default() -> Self {
        Self::new()
    }
}

impl RackLog {
    /// Create a logging context with a 512-event BTF ring.
    #[must_use]
    pub fn new() -> Self {
        let mut os = RackOs::new();
        os.log = TcpLog::new(true, 512);
        Self {
            os,
            entries: Vec::new(),
            counters: [0; 8],
            option_array: [0; 4],
            out_sizes: [0; 4],
            wma_divisor: 4,
            sysctl_ctx: 0,
            sysctl_nodes: Vec::new(),
            mutated: false,
        }
    }

    /// Retained events.
    #[must_use]
    pub fn entries(&self) -> &[LogEntry] {
        &self.entries
    }

    /// Event count.
    #[must_use]
    pub fn event_count(&self) -> usize {
        self.entries.len()
    }

    /// Read a named counter.
    #[must_use]
    pub fn counter(&self, counter: Counter) -> u64 {
        self.counters[counter.index()]
    }

    /// Add to a named counter.
    pub fn add_counter(&mut self, counter: Counter, value: u64) -> u64 {
        if self.mutated {
            return self.counter(counter);
        }
        let slot = &mut self.counters[counter.index()];
        *slot = slot.saturating_add(value);
        *slot
    }

    fn record(&mut self, kind: u32, args: [u64; 4], status: i32) -> bool {
        if self.mutated {
            return false;
        }
        self.entries.push(LogEntry { kind, args, status });
        let bytes = u32::try_from(args[1]).unwrap_or(u32::MAX);
        self.os.log.tcp_log_stackspecific(kind, status, bytes)
    }

    /// FreeBSD `rack_log_gpset`.
    pub fn rack_log_gpset(
        &mut self,
        seq_end: u32,
        ack_end: u32,
        send_end: u32,
        line: i32,
        mode: u8,
        rsm_seq: u32,
    ) -> bool {
        self.record(
            KIND_GPSET,
            [
                u64::from(seq_end),
                u64::from(ack_end),
                u64::from(send_end),
                u64::from(mode) << 32 | u64::from(rsm_seq),
            ],
            line,
        )
    }

    /// FreeBSD `sysctl_rack_clear`: stat 1 clears counters, 2 the option array,
    /// 4 the out-size array, 3 is a no-op; the selector resets to zero.
    pub fn sysctl_rack_clear(&mut self, stat: u32) -> Result<u64, RackOsError> {
        if self.mutated {
            return Ok(0);
        }
        let cleared = match stat {
            1 => {
                let sum = self.counters.iter().copied().sum();
                self.counters = [0; 8];
                sum
            }
            2 => {
                let sum = self.option_array.iter().copied().sum();
                self.option_array = [0; 4];
                sum
            }
            4 => {
                let sum = self.out_sizes.iter().copied().sum();
                self.out_sizes = [0; 4];
                sum
            }
            3 => 0,
            _ => return Err(RackOsError::BadHandle),
        };
        self.record(KIND_SYSCTL_CLEAR, [u64::from(stat), cleared, 0, 0], 0);
        Ok(cleared)
    }

    /// FreeBSD `sysctl_rack_wma_divisor`: values 1..=INT32_MAX are accepted.
    pub fn sysctl_rack_wma_divisor(&mut self, value: Option<u32>) -> Result<u32, RackOsError> {
        let old = self.wma_divisor;
        if let Some(value) = value {
            if value == 0 || value > i32::MAX as u32 {
                return Err(RackOsError::BadHandle);
            }
            if !self.mutated {
                self.wma_divisor = value;
            }
        }
        self.record(
            KIND_WMA_DIVISOR,
            [u64::from(old), u64::from(self.wma_divisor), 0, 0],
            0,
        );
        Ok(old)
    }

    /// FreeBSD `rack_init_sysctls`: create the RACK counter/pacing/timely nodes.
    pub fn rack_init_sysctls(&mut self) -> Vec<u32> {
        if self.mutated {
            return self.sysctl_nodes.clone();
        }
        if self.sysctl_ctx == 0 {
            self.sysctl_ctx = self.os.sysctl.sysctl_ctx_init();
        }
        let root = self
            .os
            .sysctl
            .sysctl_add_node(None, "net.inet.tcp.rack", self.sysctl_ctx);
        let counters = self
            .os
            .sysctl
            .sysctl_add_node(Some(root), "counters", self.sysctl_ctx);
        let pacing = self
            .os
            .sysctl
            .sysctl_add_node(Some(root), "pacing", self.sysctl_ctx);
        let timely = self
            .os
            .sysctl
            .sysctl_add_node(Some(root), "timely", self.sysctl_ctx);
        self.sysctl_nodes = vec![root, counters, pacing, timely];
        self.record(
            KIND_INIT_SYSCTLS,
            [
                u64::from(root),
                u64::from(counters),
                u64::from(pacing),
                u64::from(timely),
            ],
            0,
        );
        self.sysctl_nodes.clone()
    }

    /// FreeBSD `rack_counter_destroy`: free the counters and sysctl context.
    pub fn rack_counter_destroy(&mut self) -> usize {
        if self.mutated {
            return self.sysctl_nodes.len();
        }
        let freed = if self.sysctl_ctx == 0 {
            0
        } else {
            self.os.sysctl.sysctl_ctx_free(self.sysctl_ctx)
        };
        self.sysctl_ctx = 0;
        self.counters = [0; 8];
        self.sysctl_nodes.clear();
        self.record(KIND_COUNTER_DESTROY, [freed as u64, 0, 0, 0], 0);
        freed
    }

    /// FreeBSD `rack_log_hybrid_bw`.
    pub fn rack_log_hybrid_bw(
        &mut self,
        seq: u32,
        cbw: u64,
        tim: u64,
        data: u64,
        mod_: u8,
        aux: u16,
        line: i32,
    ) -> bool {
        self.record(
            KIND_HYBRID_BW,
            [u64::from(seq), cbw, tim, data],
            line.wrapping_add(i32::from(mod_))
                .wrapping_add(i32::from(aux)),
        )
    }

    /// FreeBSD `rack_log_hybrid_sends`.
    pub fn rack_log_hybrid_sends(&mut self, start: u32, end: u32, line: i32) -> bool {
        self.record(
            KIND_HYBRID_SENDS,
            [u64::from(start), u64::from(end), line as u64, 0],
            0,
        )
    }

    /// FreeBSD `rack_log_dsack_event`.
    pub fn rack_log_dsack_event(&mut self, mod_: u8, flex4: u32, flex5: u32, flex6: u32) -> bool {
        self.add_counter(Counter::Dsack, 1);
        self.record(
            KIND_DSACK,
            [
                u64::from(mod_),
                u64::from(flex4),
                u64::from(flex5),
                u64::from(flex6),
            ],
            0,
        )
    }

    /// FreeBSD `rack_log_hdwr_pacing`.
    pub fn rack_log_hdwr_pacing(
        &mut self,
        rate: u64,
        hw_rate: u64,
        line: i32,
        error: i32,
        mod_: u16,
    ) -> bool {
        self.record(
            KIND_HDWR_PACING,
            [rate, hw_rate, u64::from(mod_), 0],
            line.wrapping_add(error),
        )
    }

    /// FreeBSD `rack_log_retran_reason`.
    pub fn rack_log_retran_reason(
        &mut self,
        seq: u32,
        tsused: u32,
        thresh: u32,
        mod_: i32,
    ) -> bool {
        self.add_counter(Counter::FtoSend, 1);
        self.record(
            KIND_RETRAN_REASON,
            [u64::from(seq), u64::from(tsused), u64::from(thresh), 0],
            mod_,
        )
    }

    /// FreeBSD `rack_log_to_start`.
    pub fn rack_log_to_start(&mut self, cts: u32, to: u32, pacing_delay: i32, which: u8) -> bool {
        self.add_counter(Counter::Tlp, 1);
        self.record(
            KIND_TO_START,
            [
                u64::from(cts),
                u64::from(to),
                pacing_delay as u64,
                u64::from(which),
            ],
            0,
        )
    }

    /// FreeBSD `rack_log_to_event`.
    pub fn rack_log_to_event(&mut self, to_num: i32, rsm_seq: u32) -> bool {
        self.record(KIND_TO_EVENT, [to_num as u64, u64::from(rsm_seq), 0, 0], 0)
    }

    /// FreeBSD `rack_log_map_chg`.
    pub fn rack_log_map_chg(
        &mut self,
        prev_seq: u32,
        rsm_seq: u32,
        next_seq: u32,
        flag: i32,
        th_ack: u32,
        line: i32,
    ) -> bool {
        self.record(
            KIND_MAP_CHG,
            [
                u64::from(prev_seq),
                u64::from(rsm_seq),
                u64::from(next_seq),
                u64::from(th_ack),
            ],
            flag.wrapping_add(line),
        )
    }

    /// FreeBSD `rack_log_rtt_upd`.
    pub fn rack_log_rtt_upd(&mut self, t: u32, len: u32, rsm_seq: u32, conf: i32) -> bool {
        self.record(
            KIND_RTT_UPD,
            [u64::from(t), u64::from(len), u64::from(rsm_seq), 0],
            conf,
        )
    }

    /// FreeBSD `rack_log_rtt_sample`.
    pub fn rack_log_rtt_sample(&mut self, rtt: u32) -> bool {
        self.record(KIND_RTT_SAMPLE, [u64::from(rtt), 0, 0, 0], 0)
    }

    /// FreeBSD `rack_log_rtt_sample_calc`.
    pub fn rack_log_rtt_sample_calc(
        &mut self,
        rtt: u32,
        send_time: u32,
        ack_time: u32,
        where_: i32,
    ) -> bool {
        self.record(
            KIND_RTT_SAMPLE_CALC,
            [u64::from(rtt), u64::from(send_time), u64::from(ack_time), 0],
            where_,
        )
    }

    /// FreeBSD `rack_log_rtt_sendmap`.
    pub fn rack_log_rtt_sendmap(&mut self, idx: u32, tsv: u64, tsecho: u32) -> bool {
        self.record(
            KIND_RTT_SENDMAP,
            [u64::from(idx), tsv, u64::from(tsecho), 0],
            0,
        )
    }

    /// FreeBSD `rack_log_progress_event`.
    pub fn rack_log_progress_event(
        &mut self,
        tick: u32,
        event: i32,
        line: i32,
        maxunack: u32,
        acktime: u32,
    ) -> bool {
        self.record(
            KIND_PROGRESS,
            [
                u64::from(tick),
                event as u64,
                u64::from(maxunack),
                u64::from(acktime),
            ],
            line,
        )
    }

    /// FreeBSD `rack_log_type_bbrsnd`.
    pub fn rack_log_type_bbrsnd(
        &mut self,
        len: u32,
        pacing_delay: u32,
        cts: u32,
        line: i32,
    ) -> bool {
        self.record(
            KIND_BBRSND,
            [u64::from(len), u64::from(pacing_delay), u64::from(cts), 0],
            line,
        )
    }

    /// FreeBSD `rack_log_doseg_done`.
    pub fn rack_log_doseg_done(
        &mut self,
        cts: u32,
        nxt_pkt: i32,
        did_out: i32,
        way_out: i32,
        nsegs: i32,
    ) -> bool {
        self.record(
            KIND_DOSEG_DONE,
            [
                u64::from(cts),
                nxt_pkt as u64,
                did_out as u64,
                (way_out as u64) << 32 | u64::from(nsegs as u32),
            ],
            0,
        )
    }

    /// FreeBSD `rack_log_type_pacing_sizes`.
    pub fn rack_log_type_pacing_sizes(&mut self, arg1: u32, arg2: u32, arg3: u32, frm: u8) -> bool {
        self.record(
            KIND_PACING_SIZES,
            [
                u64::from(arg1),
                u64::from(arg2),
                u64::from(arg3),
                u64::from(frm),
            ],
            0,
        )
    }

    /// FreeBSD `rack_log_type_just_return`.
    pub fn rack_log_type_just_return(
        &mut self,
        cts: u32,
        tlen: u32,
        pacing_delay: u32,
        hpts_calling: u8,
        reason: i32,
        cwnd_to_use: u32,
    ) -> bool {
        self.record(
            KIND_JUST_RETURN,
            [
                u64::from(cts),
                u64::from(tlen),
                u64::from(pacing_delay),
                u64::from(cwnd_to_use),
            ],
            reason.wrapping_add(i32::from(hpts_calling)),
        )
    }

    /// FreeBSD `rack_log_to_cancel`.
    pub fn rack_log_to_cancel(
        &mut self,
        hpts_removed: i32,
        line: i32,
        us_cts: u32,
        flags_on_entry: u32,
    ) -> bool {
        self.record(
            KIND_TO_CANCEL,
            [
                hpts_removed as u64,
                u64::from(us_cts),
                u64::from(flags_on_entry),
                0,
            ],
            line,
        )
    }

    /// FreeBSD `rack_log_alt_to_to_cancel`.
    pub fn rack_log_alt_to_to_cancel(
        &mut self,
        flex1: u32,
        flex2: u32,
        flex3: u32,
        flex4: u32,
        flex5: u32,
        flex6: u32,
        flex7: u16,
        mod_: u8,
    ) -> bool {
        self.record(
            KIND_ALT_TO_CANCEL,
            [
                u64::from(flex1),
                u64::from(flex2),
                u64::from(flex3) << 32 | u64::from(flex4),
                u64::from(flex5) << 32 | u64::from(flex6),
            ],
            i32::from(flex7) + i32::from(mod_),
        )
    }

    /// FreeBSD `rack_log_to_processing`.
    pub fn rack_log_to_processing(&mut self, cts: u32, ret: i32, timers: i32) -> bool {
        self.record(
            KIND_TO_PROCESSING,
            [u64::from(cts), ret as u64, timers as u64, 0],
            0,
        )
    }

    /// FreeBSD `rack_log_to_prr`.
    pub fn rack_log_to_prr(&mut self, frm: i32, orig_cwnd: i32, line: i32) -> bool {
        self.add_counter(Counter::Prr, 1);
        self.record(
            KIND_TO_PRR,
            [frm as u64, orig_cwnd as u64, line as u64, 0],
            0,
        )
    }

    /// FreeBSD `rack_log_timely`.
    pub fn rack_log_timely(
        &mut self,
        logged: u32,
        cur_bw: u64,
        low_bnd: u64,
        up_bnd: u64,
        line: i32,
        method: u8,
    ) -> bool {
        self.record(
            KIND_TIMELY,
            [u64::from(logged), cur_bw, low_bnd, up_bnd],
            line.wrapping_add(i32::from(method)),
        )
    }

    /// FreeBSD `rack_log_rtt_shrinks`.
    pub fn rack_log_rtt_shrinks(&mut self, us_cts: u32, rtt: u32, line: u32, reas: u8) -> bool {
        self.record(
            KIND_RTT_SHRINKS,
            [
                u64::from(us_cts),
                u64::from(rtt),
                u64::from(line),
                u64::from(reas),
            ],
            0,
        )
    }

    /// FreeBSD `rack_log_gp_calc`.
    pub fn rack_log_gp_calc(
        &mut self,
        add_part: u32,
        sub_part: u32,
        srtt: u32,
        meas_bw: u64,
        utim: u64,
        meth: u8,
        line: u32,
    ) -> bool {
        self.record(
            KIND_GP_CALC,
            [
                u64::from(add_part),
                u64::from(sub_part) << 32 | u64::from(srtt),
                meas_bw,
                utim,
            ],
            i32::from(meth) + i32::try_from(line).unwrap_or(i32::MAX),
        )
    }

    /// FreeBSD `rack_log_hpts_diag`.
    pub fn rack_log_hpts_diag(&mut self, cts: u32, diag1: u32, diag2: u32, diag3: u32) -> bool {
        self.record(
            KIND_HPTS_DIAG,
            [
                u64::from(cts),
                u64::from(diag1),
                u64::from(diag2),
                u64::from(diag3),
            ],
            0,
        )
    }

    /// FreeBSD `rack_log_wakeup`.
    pub fn rack_log_wakeup(&mut self, len: u32, type_: i32) -> bool {
        self.add_counter(Counter::Wakeup, 1);
        self.record(KIND_WAKEUP, [u64::from(len), type_ as u64, 0, 0], 0)
    }

    /// FreeBSD `rack_log_output`.
    pub fn rack_log_output(
        &mut self,
        len: u32,
        seq_out: u32,
        th_flags: u16,
        err: i32,
        ts: u64,
        add_flags: u32,
        hw_tls: i32,
        segsiz: u32,
    ) -> bool {
        self.add_counter(Counter::TotalBytes, u64::from(len));
        self.add_counter(Counter::OutBytes, u64::from(len));
        self.record(
            KIND_OUTPUT,
            [
                u64::from(len),
                u64::from(seq_out),
                ts,
                u64::from(th_flags) << 48 | u64::from(add_flags),
            ],
            err.wrapping_add(hw_tls).wrapping_add(segsiz as i32),
        )
    }

    /// FreeBSD `rack_log_sack_passed`.
    pub fn rack_log_sack_passed(&mut self, rsm_seq: u32, cts: u32, line: i32) -> bool {
        self.record(
            KIND_SACK_PASSED,
            [u64::from(rsm_seq), u64::from(cts), 0, 0],
            line,
        )
    }

    /// FreeBSD `rack_log_ack`.
    pub fn rack_log_ack(
        &mut self,
        ack: u32,
        entered_rec: i32,
        dup_ack_struck: i32,
        dsack_seen: &mut bool,
        sacks_seen: &mut u32,
    ) -> bool {
        if !self.mutated {
            *dsack_seen = *dsack_seen || dup_ack_struck > 0;
            *sacks_seen = sacks_seen.saturating_add(u32::from(*dsack_seen));
        }
        self.record(
            KIND_ACK,
            [
                u64::from(ack),
                entered_rec as u64,
                dup_ack_struck as u64,
                u64::from(*sacks_seen),
            ],
            0,
        )
    }

    /// FreeBSD `rack_log_hybrid`.
    pub fn rack_log_hybrid(&mut self, seq: u32, mod_: u8, line: i32, err: i32) -> bool {
        self.record(
            KIND_HYBRID,
            [u64::from(seq), u64::from(mod_), 0, 0],
            line.wrapping_add(err),
        )
    }

    /// FreeBSD `rack_log_collapse`.
    pub fn rack_log_collapse(
        &mut self,
        cnt: u32,
        split: u32,
        out: u32,
        line: i32,
        dir: i32,
        flags: u32,
    ) -> bool {
        self.record(
            KIND_COLLAPSE,
            [
                u64::from(cnt),
                u64::from(split),
                u64::from(out),
                u64::from(flags),
            ],
            line.wrapping_add(dir),
        )
    }

    /// FreeBSD `rack_log_hystart_event`.
    pub fn rack_log_hystart_event(&mut self, high_seq: u32, mod_: u8) -> bool {
        self.record(
            KIND_HYSTART,
            [u64::from(high_seq), u64::from(mod_), 0, 0],
            0,
        )
    }

    /// FreeBSD `rack_log_chg_info`.
    pub fn rack_log_chg_info(&mut self, mod_: u8, flex1: u32, flex2: u32, flex3: u32) -> bool {
        self.record(
            KIND_CHG_INFO,
            [
                u64::from(mod_),
                u64::from(flex1),
                u64::from(flex2),
                u64::from(flex3),
            ],
            0,
        )
    }

    /// FreeBSD `rack_log_input_packet`.
    pub fn rack_log_input_packet(&mut self, ackval: i32, high_seq: u32, ack_seq: u32) -> bool {
        self.record(
            KIND_INPUT_PACKET,
            [ackval as u64, u64::from(high_seq), u64::from(ack_seq), 0],
            0,
        )
    }

    /// FreeBSD `rack_log_pcm`.
    pub fn rack_log_pcm(&mut self, mod_: u8, flex1: u32, flex2: u32, flex3: u32) -> bool {
        self.record(
            KIND_PCM,
            [
                u64::from(mod_),
                u64::from(flex1),
                u64::from(flex2),
                u64::from(flex3),
            ],
            0,
        )
    }

    /// FreeBSD `rack_log_pacing_delay_calc`.
    pub fn rack_log_pacing_delay_calc(
        &mut self,
        len: u32,
        pacing_delay: u32,
        bw_est: u64,
        bw: u64,
        len_time: u64,
        method: i32,
        line: i32,
        quality: u8,
    ) -> bool {
        self.record(
            KIND_PACING_DELAY,
            [
                u64::from(len),
                u64::from(pacing_delay),
                bw_est,
                bw ^ len_time,
            ],
            method.wrapping_add(line).wrapping_add(i32::from(quality)),
        )
    }

    /// FreeBSD `rack_log_fsb`.
    pub fn rack_log_fsb(
        &mut self,
        flags: u32,
        ipoptlen: u32,
        orig_len: i32,
        len: i32,
        error: i32,
        rsm_is_null: i32,
        optlen: i32,
        line: i32,
        mode: u16,
    ) -> bool {
        self.record(
            KIND_FSB,
            [
                u64::from(flags),
                u64::from(ipoptlen),
                orig_len as u64,
                len as u64,
            ],
            error
                .wrapping_add(rsm_is_null)
                .wrapping_add(optlen)
                .wrapping_add(line)
                .wrapping_add(i32::from(mode)),
        )
    }

    /// FreeBSD `rack_log_queue_level`: returns `(p_rate, p_queue)`.
    pub fn rack_log_queue_level(&mut self, len: i32, cts: u32) -> (u32, u32) {
        let p_rate = (len.max(0) as u32)
            .saturating_mul(1_000_000)
            .checked_div(cts)
            .unwrap_or(0);
        let p_queue = len.max(0) as u32;
        self.record(
            KIND_QUEUE_LEVEL,
            [u64::from(p_rate), u64::from(p_queue), u64::from(cts), 0],
            0,
        );
        (p_rate, p_queue)
    }
}

#[cfg(test)]
#[path = "rack_log_ut.rs"]
mod tests;
