// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// Ported from FreeBSD `sys/netinet/tcp_stacks/rack.c` at
// 133344405f532bd54ff3629a77d4c4e062f01fdb (BSD-2-Clause): init/fini,
// sendmap allocation, clone/merge, socket options, inheritance, MTU/send-buffer
// adjustment and the query/stack-information surface. The kernel pointers,
// kmem zones, sockopt copyin/copyout and locks are mapped to the K314 `RackOs`
// software context; no C expression is copied.

//! RACK init/alloc/sockopt/misc port.
//!
//! This module owns the non-recovery donor functions listed in the K334 report.
//! It uses the K313 HPTS/timer state, the K314 [`RackOs`]/[`RackTcpcb`] context
//! and the K318 socket-option/query shapes. No stack wiring is added here.

use alloc::collections::{BTreeMap, VecDeque};
use alloc::vec::Vec;
use core::mem::size_of;

use crate::rack_os::{MbufId, RackOs, RackTcpcb};
use crate::rack_types::{
    RACK_APP_LIMITED, RACK_HAD_PUSH, RACK_HAS_FIN, RACK_HAS_SYN, RACK_MUST_RXT,
    RACK_NUM_OF_RETRANS, RACK_PMTU_CHG, RACK_RWND_COLLAPSED, RACK_TLP, RackSendmap,
};
use crate::rack_vtable::{
    IPPROTO_TCP, PRUS_OOB, RackSockopt, SockoptDir, TCP_INFO, TCP_PACING_DND, TCP_QUERY_RACK_TIMES,
    TCP_QUERY_SENDMAP, TCP_QUERY_TIMERS_UP, TCP_RACK_PACE_MAX_SEG, TCP_RACK_PACE_MIN_SEG,
    TCP_RACK_PACING_BETA, TCP_RACK_PACING_DIVISOR, TCPS_CLOSED, TH_ACK, TH_FIN, TH_RST,
};
use crate::{Errno, RackStackInfo};

/// FreeBSD `rack_free_cache` default.
pub const RACK_FREE_CACHE: usize = 4;
/// FreeBSD `RACK_FREE_CNT_MAX`.
pub const RACK_FREE_CNT_MAX: usize = 8;
/// Largest deferred-option list the model accepts.
pub const RACK_DEFERRED_MAX: usize = 16;

/// `TCP_RACK_PROFILE`.
pub const TCP_RACK_PROFILE: i32 = 1129;
/// `TCP_DEFER_OPTIONS`.
pub const TCP_DEFER_OPTIONS: i32 = 1136;
/// `TCP_RACK_DSACK_OPT`.
pub const TCP_RACK_DSACK_OPT: i32 = 1141;
/// `TCP_RACK_SET_RXT_OPTIONS`.
pub const TCP_RACK_SET_RXT_OPTIONS: i32 = 1143;
/// `TCP_RACK_HI_BETA`.
pub const TCP_RACK_HI_BETA: i32 = 1144;
/// `TCP_RACK_TIMER_SLOP`.
pub const TCP_RACK_TIMER_SLOP: i32 = 1140;
/// `TCP_RACK_MEASURE_CNT`.
pub const TCP_RACK_MEASURE_CNT: i32 = 1135;
/// `TCP_NO_PRR`.
pub const TCP_NO_PRR: i32 = 1122;
/// `TCP_USE_CMP_ACKS`.
pub const TCP_USE_CMP_ACKS: i32 = 77;
/// `TCP_RACK_MBUF_QUEUE`.
pub const TCP_RACK_MBUF_QUEUE: i32 = 1050;
/// `TCP_RACK_PACE_TO_FILL`.
pub const TCP_RACK_PACE_TO_FILL: i32 = 1127;
/// `TCP_RACK_FORCE_MSEG`.
pub const TCP_RACK_FORCE_MSEG: i32 = 1117;
/// `TCP_RACK_ABC_VAL`.
pub const TCP_RACK_ABC_VAL: i32 = 1133;

/// The fast-send-block header state.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FsbBlock {
    /// `tcp_ip_hdr_len`.
    pub header_len: usize,
    /// `r_is_v6`.
    pub ipv6: bool,
    /// `tp->t_port` is nonzero.
    pub tunnel: bool,
    /// `recwin`.
    pub recwin: u32,
    /// `r_fsb_inited`.
    pub initialized: bool,
    /// `rack->r_ctl.fsb.hw_tls`.
    pub hw_tls: bool,
    /// The header backing store.
    pub mbuf: Option<MbufId>,
}

/// One deferred socket option.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeferredOption {
    /// `optname`.
    pub name: i32,
    /// `optval`.
    pub value: u64,
}

/// The donor `tcp_query_resp` surface needed by the misc port.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MiscQuery {
    /// `req`.
    pub req: i32,
    /// `req_param`.
    pub req_param: u32,
    /// `sendmap_start`.
    pub sendmap_start: u32,
    /// `sendmap_end`.
    pub sendmap_end: u32,
    /// `sendmap_send_cnt`.
    pub sendmap_send_cnt: u8,
    /// `sendmap_fas`.
    pub sendmap_fas: u32,
    /// `sendmap_time`.
    pub sendmap_time: [u64; RACK_NUM_OF_RETRANS],
    /// `sendmap_ack_arrival`.
    pub sendmap_ack_arrival: u64,
    /// `sendmap_flags`.
    pub sendmap_flags: u32,
    /// `sendmap_r_rtr_bytes`.
    pub sendmap_r_rtr_bytes: u32,
    /// `sendmap_dupacks`.
    pub sendmap_dupacks: u8,
    /// `timer_hpts_flags`.
    pub timer_hpts_flags: u32,
    /// `timer_pacing_to`.
    pub timer_pacing_to: i64,
    /// `timer_timer_exp`.
    pub timer_timer_exp: i64,
    /// `rack_num_dsacks`.
    pub rack_num_dsacks: u32,
    /// `rack_reorder_ts`.
    pub rack_reorder_ts: u32,
    /// `rack_rxt_last_time`.
    pub rack_rxt_last_time: u32,
    /// `rack_min_rtt`.
    pub rack_min_rtt: u32,
    /// `rack_rtt`.
    pub rack_rtt: u32,
    /// `rack_tmit_time`.
    pub rack_tmit_time: u32,
    /// `rack_srtt_measured`.
    pub rack_srtt_measured: u32,
    /// `rack_sacked`.
    pub rack_sacked: u32,
    /// `rack_holes_rxt`.
    pub rack_holes_rxt: u32,
    /// `rack_prr_delivered`.
    pub rack_prr_delivered: u32,
    /// `rack_prr_recovery_fs`.
    pub rack_prr_recovery_fs: u32,
    /// `rack_prr_sndcnt`.
    pub rack_prr_sndcnt: u32,
    /// `rack_prr_out`.
    pub rack_prr_out: u32,
    /// `rack_tlp_out`.
    pub rack_tlp_out: u8,
    /// `rack_tlp_cnt_out`.
    pub rack_tlp_cnt_out: u32,
    /// `rack_time_went_idle`.
    pub rack_time_went_idle: u32,
    /// `rack_in_persist`.
    pub rack_in_persist: u8,
    /// `rack_wanted_output`.
    pub rack_wanted_output: u8,
}

/// A `tcp_info` snapshot for `rack_fill_info`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RackTcpInfo {
    /// `tcpi_state`.
    pub state: u8,
    /// `tcpi_options`.
    pub options: u32,
    /// `tcpi_rtt`.
    pub rtt: u32,
    /// `tcpi_rttvar`.
    pub rttvar: u32,
    /// `tcpi_rto`.
    pub rto: u32,
    /// `tcpi_snd_ssthresh`.
    pub snd_ssthresh: u32,
    /// `tcpi_snd_cwnd`.
    pub snd_cwnd: u32,
    /// `tcpi_rcv_space`.
    pub rcv_space: u32,
    /// `tcpi_rcv_nxt`.
    pub rcv_nxt: u32,
    /// `tcpi_snd_wnd`.
    pub snd_wnd: u32,
    /// `tcpi_snd_nxt`.
    pub snd_nxt: u32,
    /// `tcpi_snd_mss`.
    pub snd_mss: u16,
    /// `tcpi_rcv_mss`.
    pub rcv_mss: u16,
    /// `tcpi_snd_rexmitpack`.
    pub snd_rexmitpack: u32,
    /// `tcpi_rcv_ooopack`.
    pub rcv_ooopack: u32,
    /// `tcpi_snd_zerowin`.
    pub snd_zerowin: u32,
    /// `tcpi_total_tlp`.
    pub total_tlp: u32,
    /// `tcpi_total_tlp_bytes`.
    pub total_tlp_bytes: u32,
    /// `tcpi_rttmin`.
    pub rttmin: u32,
}

/// The RACK misc state over the K314 context.
pub struct RackMisc {
    /// K314 buffers/zones/HPTS/pacing.
    pub os: RackOs,
    /// `tcpcb` mirror.
    pub tp: RackTcpcb,
    /// Sendmap zone.
    pub sendmap_zone: u32,
    /// `tcp_rack` zone.
    pub pcb_zone: u32,
    /// Active sendmaps.
    pub sendmaps: BTreeMap<u64, RackSendmap>,
    /// Free list.
    pub free: Vec<u64>,
    /// `rc_free_cnt`.
    pub free_count: usize,
    /// `rc_num_maps_alloced`.
    pub maps_alloced: u32,
    /// `rc_num_split_allocs`.
    pub split_allocs: u32,
    /// `rc_split_limit`.
    pub split_limit: u32,
    /// `V_tcp_map_entries_limit`.
    pub map_entries_limit: u32,
    /// `alloc_limit_reported`.
    pub alloc_limit_reported: bool,
    /// `r_is_v6`.
    pub ipv6: bool,
    /// `tp->t_port != 0`.
    pub tunnel: bool,
    /// `iss`.
    pub iss: u32,
    /// `snd_wnd`.
    pub snd_wnd: u32,
    /// `snd_cwnd` copy used by autoscaling.
    pub snd_cwnd: u32,
    /// `rc_fsb`.
    pub fsb: FsbBlock,
    /// `r_fsb_inited`.
    pub fsb_inited: bool,
    /// `rack_deferred_inited`.
    pub deferred_inited: bool,
    /// `r_ctl.roundends`.
    pub roundends: u32,
    /// `r_ctl.rc_high_rwnd`.
    pub high_rwnd: u32,
    /// `r_ctl.cwnd_to_use`.
    pub cwnd_to_use: u32,
    /// `full_size_rxt`.
    pub full_size_rxt: bool,
    /// `shape_rxt_to_pacing_min`.
    pub shape_rxt_to_pacing_min: bool,
    /// `defer_options`.
    pub defer_options: bool,
    /// Deferred options.
    pub deferred: VecDeque<DeferredOption>,
    /// `r_ctl.pace_len_divisor`.
    pub pacing_divisor: u32,
    /// `r_ctl.rc_user_set_min_segs`.
    pub min_segs: u16,
    /// `rc_user_set_max_segs`.
    pub max_segs: u16,
    /// `rc_force_max_seg`.
    pub force_max_seg: bool,
    /// `rc_pace_to_cwnd`.
    pub pace_to_cwnd: bool,
    /// `dgp_on`.
    pub dgp_on: bool,
    /// `rc_always_pace`.
    pub always_pace: bool,
    /// `r_use_cmp_ack`.
    pub use_cmp_ack: bool,
    /// `r_mbuf_queue`.
    pub mbuf_queue: bool,
    /// `rc_rack_tmr_std_based`.
    pub dsack_std_based: bool,
    /// `rc_rack_use_dsack`.
    pub dsack_use: bool,
    /// `r_ctl.req_measurements`.
    pub req_measurements: u32,
    /// `rack_hibeta`.
    pub hi_beta: bool,
    /// `r_ctl.timer_slop`.
    pub timer_slop: u32,
    /// `rack_no_prr`.
    pub no_prr: bool,
    /// `rc_labc`.
    pub abc_val: u8,
    /// `rc_pace_dnd`.
    pub pacing_dnd: bool,
    /// `r_profile`.
    pub profile: i32,
    /// `t_sndbytes`.
    pub bytes_transmitted: u64,
    /// `t_snd_rxt_bytes`.
    pub bytes_retransmitted: u64,
    /// `rc_app_limited_cnt`.
    pub app_limited_cnt: u32,
    /// `SB_AUTOSIZE`.
    pub sndbuf_autosize: bool,
    /// `tcp_autosndbuf_inc`.
    pub autosndbuf_inc: u32,
    /// `tcp_autosndbuf_max`.
    pub autosndbuf_max: u32,
    /// `initialized`.
    pub initialized: bool,
}

impl Default for RackMisc {
    fn default() -> Self {
        Self::new()
    }
}

impl RackMisc {
    /// Create the model with the two UMA zones and the FreeBSD defaults.
    #[must_use]
    pub fn new() -> Self {
        let mut os = RackOs::new();
        let sendmap_zone = os.uma_zcreate("rack_map", size_of::<RackSendmap>());
        let pcb_zone = os.uma_zcreate("rack_pcb", size_of::<RackMisc>());
        Self {
            os,
            tp: RackTcpcb::default(),
            sendmap_zone,
            pcb_zone,
            sendmaps: BTreeMap::new(),
            free: Vec::new(),
            free_count: 0,
            maps_alloced: 0,
            split_allocs: 0,
            split_limit: 0,
            map_entries_limit: 0,
            alloc_limit_reported: false,
            ipv6: false,
            tunnel: false,
            iss: 0,
            snd_wnd: 65_535,
            snd_cwnd: 10,
            fsb: FsbBlock::default(),
            fsb_inited: false,
            deferred_inited: false,
            roundends: 0,
            high_rwnd: 0,
            cwnd_to_use: 0,
            full_size_rxt: false,
            shape_rxt_to_pacing_min: false,
            defer_options: false,
            deferred: VecDeque::new(),
            pacing_divisor: crate::RACK_DEFAULT_PACING_DIVISOR,
            min_segs: 0,
            max_segs: 0,
            force_max_seg: false,
            pace_to_cwnd: false,
            dgp_on: false,
            always_pace: false,
            use_cmp_ack: false,
            mbuf_queue: false,
            dsack_std_based: false,
            dsack_use: false,
            req_measurements: 1,
            hi_beta: false,
            timer_slop: 0,
            no_prr: false,
            abc_val: 0,
            pacing_dnd: false,
            profile: 0,
            bytes_transmitted: 0,
            bytes_retransmitted: 0,
            app_limited_cnt: 0,
            sndbuf_autosize: true,
            autosndbuf_inc: 1024,
            autosndbuf_max: 2 * 1024 * 1024,
            initialized: false,
        }
    }

    /// FreeBSD `rack_alloc`.
    pub fn rack_alloc(&mut self) -> Option<u64> {
        if self.free_count > RACK_FREE_CACHE {
            let id = self.free.pop()?;
            self.free_count = self.free_count.saturating_sub(1);
            return Some(id);
        }
        if let Some(item) = self.os.uma_zalloc(self.sendmap_zone) {
            self.maps_alloced = self.maps_alloced.saturating_add(1);
            self.sendmaps.insert(item, RackSendmap::default());
            return Some(item);
        }
        if let Some(id) = self.free.pop() {
            self.free_count = self.free_count.saturating_sub(1);
            return Some(id);
        }
        None
    }

    /// FreeBSD `rack_alloc_full_limit`.
    pub fn rack_alloc_full_limit(&mut self) -> Option<u64> {
        if self.map_entries_limit > 0 && self.maps_alloced >= self.map_entries_limit {
            if !self.alloc_limit_reported {
                self.alloc_limit_reported = true;
            }
            return None;
        }
        self.rack_alloc()
    }

    /// FreeBSD `rack_alloc_limit`.
    pub fn rack_alloc_limit(&mut self, limit_type: u8) -> Option<u64> {
        if limit_type != 0 && self.split_limit > 0 && self.split_allocs >= self.split_limit {
            if !self.alloc_limit_reported {
                self.alloc_limit_reported = true;
            }
            return None;
        }
        let id = self.rack_alloc()?;
        if limit_type != 0 {
            if let Some(map) = self.sendmaps.get_mut(&id) {
                map.limit_type = limit_type;
            }
            self.split_allocs = self.split_allocs.saturating_add(1);
        }
        Some(id)
    }

    /// FreeBSD `rack_free_trim`.
    pub fn rack_free_trim(&mut self) {
        while self.free_count > RACK_FREE_CACHE {
            let Some(id) = self.free.pop() else { break };
            self.free_count = self.free_count.saturating_sub(1);
            self.sendmaps.remove(&id);
            self.maps_alloced = self.maps_alloced.saturating_sub(1);
            let _ = self.os.uma_zfree(self.sendmap_zone, id);
        }
    }

    /// FreeBSD `rack_free`.
    pub fn rack_free(&mut self, id: u64) -> bool {
        let Some(mut map) = self.sendmaps.remove(&id) else {
            return false;
        };
        if map.flags & RACK_APP_LIMITED != 0 {
            self.app_limited_cnt = self.app_limited_cnt.saturating_sub(1);
        }
        if map.limit_type != 0 {
            self.split_allocs = self.split_allocs.saturating_sub(1);
        }
        map = RackSendmap::default();
        self.sendmaps.insert(id, map);
        self.free.push(id);
        self.free_count = self.free_count.saturating_add(1);
        if self.free_count + 1 > RACK_FREE_CNT_MAX {
            self.rack_free_trim();
        }
        true
    }

    /// FreeBSD `rack_clone_rsm`.
    pub fn rack_clone_rsm(&mut self, source: u64, start: u32) -> Option<u64> {
        let original = *self.sendmaps.get(&source)?;
        let id = self.rack_alloc_limit(0)?;
        let mut cloned = original;
        cloned.start = start;
        cloned.rtr_bytes = 0;
        cloned.flags &= !(RACK_HAS_SYN | RACK_HAS_FIN | RACK_HAD_PUSH);
        if original.flags & RACK_HAS_FIN != 0
            && let Some(map) = self.sendmaps.get_mut(&source)
        {
            map.flags &= !RACK_HAS_FIN;
        }
        if original.flags & RACK_APP_LIMITED != 0 {
            self.app_limited_cnt = self.app_limited_cnt.saturating_add(1);
        }
        self.sendmaps.insert(id, cloned);
        Some(id)
    }

    /// FreeBSD `rack_merge_rsm`.
    pub fn rack_merge_rsm(&mut self, left: u64, right: u64) -> bool {
        let Some(right_map) = self.sendmaps.get(&right).cloned() else {
            return false;
        };
        let Some(left_map) = self.sendmaps.get_mut(&left) else {
            return false;
        };
        left_map.end = right_map.end;
        left_map.dupack = left_map.dupack.max(right_map.dupack);
        left_map.rtr_bytes = left_map.rtr_bytes.saturating_add(right_map.rtr_bytes);
        for flag in [RACK_HAS_FIN, RACK_TLP, RACK_RWND_COLLAPSED] {
            if right_map.flags & flag != 0 {
                left_map.flags |= flag;
            }
        }
        if right_map.flags & RACK_APP_LIMITED != 0 {
            left_map.flags |= RACK_APP_LIMITED;
            if let Some(right_map) = self.sendmaps.get_mut(&right) {
                right_map.flags &= !RACK_APP_LIMITED;
            }
        }
        self.sendmaps.remove(&right);
        self.free.push(right);
        self.free_count = self.free_count.saturating_add(1);
        true
    }

    /// FreeBSD `rack_init_fsb_block`.
    pub fn rack_init_fsb_block(&mut self) {
        let tcp_header = 20usize;
        let ip_header = if self.ipv6 { 40 } else { 20 };
        let udp_header = if self.tunnel { 8 } else { 0 };
        self.fsb.header_len = ip_header + tcp_header + udp_header;
        self.fsb.ipv6 = self.ipv6;
        self.fsb.tunnel = self.tunnel;
        self.fsb.recwin = self.os.sbspace().clamp(0, i64::from(u16::MAX) << 14) as u32;
        self.fsb_inited = true;
    }

    /// FreeBSD `rack_init_fsb`.
    pub fn rack_init_fsb(&mut self) -> Result<(), Errno> {
        let header_len = if self.ipv6 { 40 + 20 + 8 } else { 20 + 20 + 8 };
        self.fsb.header_len = header_len;
        self.fsb.mbuf = Some(self.os.m_gethdr().ok_or(Errno::ENOMEM)?);
        self.fsb_inited = false;
        Ok(())
    }

    /// FreeBSD `rack_deferred_init`.
    pub fn rack_deferred_init(&mut self) {
        self.deferred_inited = true;
        self.roundends = self.tp.snd_max;
        self.high_rwnd = self.snd_wnd;
        self.cwnd_to_use = self.tp.snd_cwnd;
    }

    /// FreeBSD `rack_init_retransmit_value`.
    pub fn rack_init_retransmit_value(&mut self, ctl: i32) {
        self.full_size_rxt = ctl == 1;
        self.shape_rxt_to_pacing_min = ctl == 2;
    }

    /// FreeBSD `rack_init_outstanding`.
    pub fn rack_init_outstanding(&mut self) -> Result<u64, Errno> {
        let id = self.rack_alloc().ok_or(Errno::ENOMEM)?;
        let (start, end, flags) = if self.tp.snd_una == self.iss {
            (
                self.iss,
                self.iss
                    .wrapping_add(self.tp.snd_max.wrapping_sub(self.tp.snd_una)),
                RACK_HAS_SYN,
            )
        } else {
            (self.tp.snd_una, self.tp.snd_max, 0)
        };
        if let Some(map) = self.sendmaps.get_mut(&id) {
            map.start = start;
            map.end = end;
            map.flags = flags;
            map.rtr_cnt = 1;
            map.no_rtt_allowed = true;
            map.in_tmap = true;
        }
        Ok(id)
    }

    /// FreeBSD `rack_init`.
    pub fn rack_init(&mut self) -> Result<(), Errno> {
        self.initialized = true;
        self.alloc_limit_reported = false;
        self.rack_init_retransmit_value(0);
        self.rack_init_fsb()?;
        if self.tp.t_state != TCPS_CLOSED && self.tp.t_state != 6 {
            self.rack_deferred_init();
        }
        Ok(())
    }

    /// FreeBSD `rack_fini`.
    pub fn rack_fini(&mut self) -> usize {
        if !self.initialized {
            return 0;
        }
        let mut freed = 0;
        if let Some(mbuf) = self.fsb.mbuf.take() {
            self.os.m_freem(Some(mbuf));
        }
        for id in self.sendmaps.keys().copied().collect::<Vec<_>>() {
            self.sendmaps.remove(&id);
            let _ = self.os.uma_zfree(self.sendmap_zone, id);
            freed += 1;
        }
        self.free.clear();
        self.free_count = 0;
        self.maps_alloced = 0;
        self.split_allocs = 0;
        self.deferred.clear();
        self.fsb_inited = false;
        self.deferred_inited = false;
        self.initialized = false;
        freed
    }

    /// FreeBSD `rack_mtu_change`.
    pub fn rack_mtu_change(&mut self) -> bool {
        if self.min_segs == self.tp.t_maxseg {
            return false;
        }
        for map in self.sendmaps.values_mut() {
            map.flags |= RACK_MUST_RXT | RACK_PMTU_CHG;
        }
        self.tp.snd_nxt = self.tp.snd_max;
        true
    }

    /// FreeBSD `rack_sndbuf_autoscale`.
    pub fn rack_sndbuf_autoscale(&mut self) -> bool {
        let sendwin = self.snd_cwnd.min(self.snd_wnd);
        let used = self.os.snd.ccc;
        let hiwat = self.os.snd.hiwat;
        if !self.sndbuf_autosize
            || (self.snd_wnd / 4 * 5) < hiwat
            || used < hiwat / 8 * 7
            || used >= self.autosndbuf_max
            || sendwin < used.saturating_sub(self.tp.snd_max.wrapping_sub(self.tp.snd_una))
        {
            return false;
        }
        let scaleup = (self.autosndbuf_inc * hiwat / 100)
            .max(self.autosndbuf_inc)
            .saturating_add(hiwat)
            .min(self.autosndbuf_max);
        if self.os.sbreserve_locked(scaleup) {
            self.os.snd.hiwat = scaleup;
            true
        } else {
            self.sndbuf_autosize = false;
            false
        }
    }

    /// FreeBSD `rack_set_profile`.
    pub fn rack_set_profile(&mut self, profile: i32) -> Result<(), Errno> {
        match profile {
            1 | 6 => {
                self.dgp_on = true;
                self.always_pace = true;
                self.pace_to_cwnd = profile == 6;
                self.full_size_rxt = true;
                self.shape_rxt_to_pacing_min = false;
                self.use_cmp_ack = true;
                self.mbuf_queue = true;
                self.profile = profile;
                Ok(())
            }
            0 => {
                self.dgp_on = false;
                self.always_pace = false;
                self.pace_to_cwnd = false;
                self.full_size_rxt = false;
                self.shape_rxt_to_pacing_min = false;
                self.use_cmp_ack = false;
                self.mbuf_queue = false;
                self.profile = 0;
                Ok(())
            }
            _ => Err(Errno::EINVAL),
        }
    }

    /// FreeBSD `rack_add_deferred_option`.
    pub fn rack_add_deferred_option(&mut self, name: i32, value: u64) -> bool {
        if self.deferred.len() >= RACK_DEFERRED_MAX {
            return false;
        }
        self.deferred.push_back(DeferredOption { name, value });
        true
    }

    /// FreeBSD `rack_stack_information`.
    #[must_use]
    pub fn rack_stack_information(&self) -> RackStackInfo {
        RackStackInfo {
            bytes_transmitted: self.bytes_transmitted,
            bytes_retransmitted: self.bytes_retransmitted,
        }
    }

    /// FreeBSD `rack_process_option` for the K334 option subset.
    pub fn rack_process_option(&mut self, name: i32, value: u64) -> Result<(), Errno> {
        let value32 = u32::try_from(value & 0xffff_ffff).unwrap_or(u32::MAX);
        match name {
            TCP_RACK_PROFILE => self.rack_set_profile(i32::try_from(value).unwrap_or(0))?,
            TCP_DEFER_OPTIONS => self.defer_options = value != 0,
            TCP_RACK_SET_RXT_OPTIONS if value <= 2 => {
                self.rack_init_retransmit_value(i32::try_from(value).unwrap_or(0));
            }
            TCP_RACK_SET_RXT_OPTIONS => return Err(Errno::EINVAL),
            TCP_RACK_DSACK_OPT => {
                self.dsack_std_based = value & 1 != 0;
                self.dsack_use = value & 2 != 0;
            }
            TCP_RACK_HI_BETA => self.hi_beta = value != 0,
            TCP_RACK_TIMER_SLOP => self.timer_slop = value32,
            TCP_RACK_MEASURE_CNT => self.req_measurements = value32.max(1),
            TCP_NO_PRR => self.no_prr = value != 0,
            TCP_USE_CMP_ACKS => self.use_cmp_ack = value != 0,
            TCP_RACK_MBUF_QUEUE => self.mbuf_queue = value != 0,
            TCP_RACK_PACE_TO_FILL => self.pace_to_cwnd = value != 0,
            TCP_RACK_FORCE_MSEG => self.force_max_seg = value != 0,
            TCP_RACK_ABC_VAL => self.abc_val = u8::try_from(value).unwrap_or(u8::MAX),
            TCP_RACK_PACING_DIVISOR => {
                self.pacing_divisor = if value32 == 0 {
                    crate::RACK_DEFAULT_PACING_DIVISOR
                } else {
                    value32.max(crate::RL_MIN_DIVISOR)
                };
            }
            TCP_RACK_PACE_MIN_SEG => {
                self.min_segs = u16::try_from(value).unwrap_or(u16::MAX);
            }
            TCP_RACK_PACE_MAX_SEG => {
                self.max_segs = u16::try_from(value.min(u64::from(crate::MAX_USER_SET_SEG)))
                    .unwrap_or(u16::MAX);
            }
            TCP_PACING_DND => self.pacing_dnd = value != 0,
            _ => return Err(Errno::EOPNOTSUPP),
        }
        Ok(())
    }

    /// FreeBSD `rack_inherit`.
    pub fn rack_inherit(&mut self, parent: &Self) -> u32 {
        let mut count = 0;
        macro_rules! inherit {
            ($field:ident) => {
                if self.$field != parent.$field {
                    self.$field = parent.$field;
                    count += 1;
                }
            };
        }
        inherit!(full_size_rxt);
        inherit!(shape_rxt_to_pacing_min);
        inherit!(dsack_std_based);
        inherit!(dsack_use);
        inherit!(pacing_divisor);
        inherit!(hi_beta);
        inherit!(timer_slop);
        inherit!(req_measurements);
        inherit!(pacing_dnd);
        inherit!(no_prr);
        inherit!(use_cmp_ack);
        inherit!(mbuf_queue);
        inherit!(force_max_seg);
        inherit!(min_segs);
        inherit!(max_segs);
        inherit!(dgp_on);
        if parent.dgp_on && !self.dgp_on {
            self.dgp_on = true;
            count += 1;
        }
        count
    }

    /// FreeBSD `rack_apply_deferred_options`.
    pub fn rack_apply_deferred_options(&mut self) -> usize {
        let pending: Vec<DeferredOption> = self.deferred.drain(..).collect();
        let mut applied = 0;
        for option in pending {
            if self.rack_process_option(option.name, option.value).is_ok() {
                applied += 1;
            }
        }
        applied
    }

    /// FreeBSD `rack_set_sockopt`.
    pub fn rack_set_sockopt(&mut self, option: &RackSockopt) -> Result<(), Errno> {
        if option.dir != SockoptDir::Set || option.level != IPPROTO_TCP {
            return Err(Errno::EOPNOTSUPP);
        }
        if self.defer_options
            && option.name != TCP_DEFER_OPTIONS
            && option.name != TCP_RACK_SET_RXT_OPTIONS
            && option.name != TCP_RACK_MEASURE_CNT
        {
            return if self.rack_add_deferred_option(option.name, u64::from(option.value)) {
                Ok(())
            } else {
                Err(Errno::ENOMEM)
            };
        }
        self.rack_process_option(option.name, u64::from(option.value))
    }

    /// FreeBSD `rack_fill_info`.
    #[must_use]
    pub fn rack_fill_info(&self) -> RackTcpInfo {
        let mut info = RackTcpInfo {
            state: self.tp.t_state,
            options: 0,
            rtt: self.tp.t_srtt,
            rttvar: self.tp.t_rttvar,
            rto: self.tp.t_rxtcur,
            snd_ssthresh: self.tp.snd_ssthresh,
            snd_cwnd: self.tp.snd_cwnd,
            rcv_space: self.os.snd.hiwat,
            rcv_nxt: 0,
            snd_wnd: self.snd_wnd,
            snd_nxt: self.tp.snd_nxt,
            snd_mss: self.tp.t_maxseg,
            rcv_mss: self.tp.t_maxseg,
            snd_rexmitpack: 0,
            rcv_ooopack: 0,
            snd_zerowin: 0,
            total_tlp: self.tp.t_rxtshift.into(),
            total_tlp_bytes: 0,
            rttmin: self.tp.t_rttmin,
        };
        if self.tp.t_flags & crate::TF_SACK_PERMIT != 0 {
            info.options |= 1;
        }
        info
    }

    /// FreeBSD `rack_get_sockopt`.
    pub fn rack_get_sockopt(&self, option: &RackSockopt) -> Result<Option<u32>, Errno> {
        if option.dir != SockoptDir::Get || option.level != IPPROTO_TCP {
            return Err(Errno::EOPNOTSUPP);
        }
        let value = match option.name {
            TCP_INFO => return Err(Errno::EOPNOTSUPP),
            TCP_RACK_PROFILE => return Err(Errno::EINVAL),
            TCP_DEFER_OPTIONS => u32::from(self.defer_options),
            TCP_RACK_DSACK_OPT => {
                u32::from(self.dsack_std_based) | (u32::from(self.dsack_use) << 1)
            }
            TCP_RACK_HI_BETA => u32::from(self.hi_beta),
            TCP_RACK_TIMER_SLOP => self.timer_slop,
            TCP_RACK_MEASURE_CNT => self.req_measurements,
            TCP_NO_PRR => u32::from(self.no_prr),
            TCP_USE_CMP_ACKS => u32::from(self.use_cmp_ack),
            TCP_RACK_MBUF_QUEUE => u32::from(self.mbuf_queue),
            TCP_RACK_PACE_TO_FILL => u32::from(self.pace_to_cwnd),
            TCP_RACK_FORCE_MSEG => u32::from(self.force_max_seg),
            TCP_RACK_ABC_VAL => u32::from(self.abc_val),
            TCP_RACK_PACING_DIVISOR => self.pacing_divisor,
            TCP_RACK_PACE_MIN_SEG => u32::from(self.min_segs),
            TCP_RACK_PACE_MAX_SEG => u32::from(self.max_segs),
            TCP_PACING_DND => u32::from(self.pacing_dnd),
            TCP_RACK_PACING_BETA => {
                if self.full_size_rxt {
                    50
                } else {
                    0
                }
            }
            _ => return Err(Errno::EOPNOTSUPP),
        };
        Ok(Some(value))
    }

    /// FreeBSD `rack_ctor`.
    pub fn rack_ctor(memory: &mut [u8]) -> i32 {
        memory.fill(0);
        0
    }

    /// FreeBSD `rack_dtor`.
    pub fn rack_dtor(_memory: &mut [u8]) {}

    /// FreeBSD `rack_chg_query`.
    pub fn rack_chg_query(&self, query: &mut MiscQuery) -> i32 {
        match query.req {
            TCP_QUERY_SENDMAP => {
                if query.req_param == self.tp.snd_max || self.tp.snd_max == self.tp.snd_una {
                    return 0;
                }
                let Some(map) = self
                    .sendmaps
                    .values()
                    .find(|map| map.start <= query.req_param && query.req_param < map.end)
                else {
                    return 0;
                };
                query.sendmap_start = map.start;
                query.sendmap_end = map.end;
                query.sendmap_send_cnt = map.rtr_cnt;
                query.sendmap_fas = map.fas;
                query.sendmap_time = map.tim_lastsent;
                query.sendmap_flags = map.flags;
                query.sendmap_r_rtr_bytes = map.rtr_bytes;
                query.sendmap_dupacks = map.dupack;
                1
            }
            TCP_QUERY_TIMERS_UP => {
                if self.os.pacing.method == crate::PacingMethod::None {
                    return 0;
                }
                query.timer_hpts_flags = 1;
                query.timer_pacing_to = self.os.now_us;
                query.timer_timer_exp = self.os.now_us;
                1
            }
            TCP_QUERY_RACK_TIMES => {
                query.rack_num_dsacks = 0;
                query.rack_reorder_ts = 0;
                query.rack_rxt_last_time = 0;
                query.rack_min_rtt = 0;
                query.rack_rtt = self.tp.t_srtt;
                query.rack_tmit_time = 0;
                query.rack_srtt_measured = u32::from(self.tp.t_srtt != 0);
                1
            }
            _ => -22,
        }
    }

    /// FreeBSD `rack_pru_options`.
    pub fn rack_pru_options(flags: i32) -> Result<(), Errno> {
        if flags & PRUS_OOB != 0 {
            Err(Errno::EOPNOTSUPP)
        } else {
            Ok(())
        }
    }
}

/// `TH_ACK` made available to callers of this module.
pub const ACK_FLAG: u16 = TH_ACK;
/// `TH_FIN` made available to callers of this module.
pub const FIN_FLAG: u16 = TH_FIN;
/// `TH_RST` made available to callers of this module.
pub const RST_FLAG: u16 = TH_RST;

#[cfg(test)]
#[path = "rack_misc_ut.rs"]
mod tests;
