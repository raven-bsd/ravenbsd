// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// Ported from FreeBSD at donor revision 133344405f532bd54ff3629a77d4c4e062f01fdb:
//   sys/netinet/tcp_stacks/tcp_rack.h  RACK_* sendmap flags, RACK_NUM_OF_RETRANS,
//       RACK_INITIAL_RTO, struct rack_sendmap
//   sys/netinet/tcp_stacks/rack.c      rack_rto_min/rack_rto_max/rack_tlp_min/
//       rack_delayed_ack_time/rack_prr_addbackmax/rack_reorder_fade defaults
// BSD-2/3. This is the single canonical RACK flags/sendmap definition; the kernel TAILQ links,
// bitfield packing, mbuf pointer and locks are mapped to plain Rust fields.

//! Canonical RACK flags and sendmap entry for every `rack_*` module.
//!
//! The donor has one `struct rack_sendmap` and one `RACK_*` flag namespace. The group ports
//! previously repeated subsets of both; this module is the single definition. Field names drop
//! the donor `r_` prefix except where the name would otherwise collide with a method; the donor
//! name is recorded on every field.

use crate::rack_os::MbufId;

/// `RACK_ACKED`.
pub const RACK_ACKED: u32 = 0x000001;
/// `RACK_TO_REXT`.
pub const RACK_TO_REXT: u32 = 0x000002;
/// `RACK_DEFERRED`.
pub const RACK_DEFERRED: u32 = 0x000004;
/// `RACK_OVERMAX`.
pub const RACK_OVERMAX: u32 = 0x000008;
/// `RACK_SACK_PASSED`.
pub const RACK_SACK_PASSED: u32 = 0x000010;
/// `RACK_WAS_SACKPASS`.
pub const RACK_WAS_SACKPASS: u32 = 0x000020;
/// `RACK_HAS_FIN`.
pub const RACK_HAS_FIN: u32 = 0x000040;
/// `RACK_TLP`.
pub const RACK_TLP: u32 = 0x000080;
/// `RACK_RWND_COLLAPSED`.
pub const RACK_RWND_COLLAPSED: u32 = 0x000100;
/// `RACK_APP_LIMITED`.
pub const RACK_APP_LIMITED: u32 = 0x000200;
/// `RACK_WAS_ACKED`.
pub const RACK_WAS_ACKED: u32 = 0x000400;
/// `RACK_HAS_SYN`.
pub const RACK_HAS_SYN: u32 = 0x000800;
/// `RACK_SENT_W_DSACK`.
pub const RACK_SENT_W_DSACK: u32 = 0x001000;
/// `RACK_SENT_SP`.
pub const RACK_SENT_SP: u32 = 0x002000;
/// `RACK_SENT_FP`.
pub const RACK_SENT_FP: u32 = 0x004000;
/// `RACK_HAD_PUSH`.
pub const RACK_HAD_PUSH: u32 = 0x008000;
/// `RACK_MUST_RXT`.
pub const RACK_MUST_RXT: u32 = 0x010000;
/// `RACK_IN_GP_WIN`.
pub const RACK_IN_GP_WIN: u32 = 0x020000;
/// `RACK_SHUFFLED`.
pub const RACK_SHUFFLED: u32 = 0x040000;
/// `RACK_MERGED`.
pub const RACK_MERGED: u32 = 0x080000;
/// `RACK_PMTU_CHG`.
pub const RACK_PMTU_CHG: u32 = 0x100000;
/// `RACK_STRADDLE`.
pub const RACK_STRADDLE: u32 = 0x200000;
/// `RACK_WAS_LOST`.
pub const RACK_WAS_LOST: u32 = 0x400000;
/// `RACK_IS_PCM`.
pub const RACK_IS_PCM: u32 = 0x800000;
/// `RACK_NUM_OF_RETRANS`.
pub const RACK_NUM_OF_RETRANS: usize = 3;
/// `RACK_INITIAL_RTO` (1 s in microseconds).
pub const RACK_INITIAL_RTO: u32 = 1_000_000;
/// `rack_rto_min` (30 ms).
pub const RACK_RTO_MIN: u32 = 30_000;
/// `rack_rto_max` (4 s).
pub const RACK_RTO_MAX: u32 = 4_000_000;
/// `rack_tlp_min` (10 ms).
pub const RACK_TLP_MIN: u32 = 10_000;
/// `rack_delayed_ack_time` (40 ms).
pub const RACK_DELAYED_ACK_TIME: u32 = 40_000;
/// `rack_prr_addbackmax`.
pub const RACK_PRR_ADDBACKMAX: u32 = 2;
/// `rack_reorder_fade` default (60 s).
pub const RACK_REORDER_FADE_DEFAULT: u32 = 60_000_000;
/// `PACE_MAX_IP_BYTES` (`rack_bbr_common.h`).
pub const PACE_MAX_IP_BYTES: u32 = 65_536;
/// `TF_FASTRECOVERY`.
pub const TF_FASTRECOVERY: u32 = 0x0010_0000;
/// `TF_WASFRECOVERY`.
pub const TF_WASFRECOVERY: u32 = 0x0020_0000;
/// `TF_CONGRECOVERY`.
pub const TF_CONGRECOVERY: u32 = 0x2000_0000;
/// `TF_WASCRECOVERY`.
pub const TF_WASCRECOVERY: u32 = 0x4000_0000;

/// One canonical `struct rack_sendmap` entry.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RackSendmap {
    /// Model pointer identity for the donor `rack_sendmap *` (not a donor field).
    pub id: u32,
    /// `bindex`.
    pub bindex: u32,
    /// `r_start`.
    pub start: u32,
    /// `r_end`.
    pub end: u32,
    /// `r_rtr_bytes`.
    pub rtr_bytes: u32,
    /// `r_flags`.
    pub flags: u32,
    /// `r_rtr_cnt`.
    pub rtr_cnt: u8,
    /// `r_act_rxt_cnt`.
    pub act_rxt_cnt: u32,
    /// `m` (K314 mbuf handle).
    pub mbuf: Option<MbufId>,
    /// `soff`.
    pub soff: u32,
    /// `orig_m_len`.
    pub orig_m_len: u32,
    /// `orig_t_space`.
    pub orig_t_space: u32,
    /// `r_nseq_appl`.
    pub nseq_appl: u32,
    /// `r_dupack`.
    pub dupack: u8,
    /// `r_in_tmap`.
    pub in_tmap: bool,
    /// `r_limit_type`.
    pub limit_type: u8,
    /// `r_just_ret`.
    pub just_ret: bool,
    /// `r_one_out_nr`.
    pub one_out_nr: bool,
    /// `r_no_rtt_allowed`.
    pub no_rtt_allowed: bool,
    /// `r_hw_tls`.
    pub hw_tls: bool,
    /// `r_avail`.
    pub avail: u8,
    /// `r_tim_lastsent[RACK_NUM_OF_RETRANS]`.
    pub tim_lastsent: [u64; RACK_NUM_OF_RETRANS],
    /// `r_ack_arrival`.
    pub ack_arrival: u64,
    /// `r_fas`.
    pub fas: u32,
    /// `r_bas`.
    pub bas: u8,
}

impl RackSendmap {
    /// Create an entry transmitted once at `now` (model id 0).
    #[must_use]
    pub fn new(start: u32, end: u32, now: u64) -> Self {
        let mut entry = Self {
            start,
            end,
            rtr_cnt: 1,
            in_tmap: true,
            ..Self::default()
        };
        entry.tim_lastsent[0] = now;
        entry
    }

    /// Create an entry with the model pointer identity `id`.
    #[must_use]
    pub fn with_id(id: u32, start: u32, end: u32, now: u64) -> Self {
        let mut entry = Self::new(start, end, now);
        entry.id = id;
        entry
    }

    /// `r_end - r_start`.
    #[must_use]
    pub const fn len(&self) -> u32 {
        self.end.wrapping_sub(self.start)
    }

    /// `true` when the entry covers no sequence space.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.start == self.end
    }

    /// `r_tim_lastsent[r_rtr_cnt - 1]`.
    #[must_use]
    pub const fn last_sent(&self) -> u64 {
        let index = if self.rtr_cnt == 0 {
            0
        } else {
            (self.rtr_cnt as usize) - 1
        };
        let index = if index >= RACK_NUM_OF_RETRANS {
            RACK_NUM_OF_RETRANS - 1
        } else {
            index
        };
        self.tim_lastsent[index]
    }
}

#[cfg(test)]
#[path = "rack_types_ut.rs"]
mod tests;
