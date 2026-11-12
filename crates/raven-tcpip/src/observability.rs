// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Typed counters, per-socket information and the enumerable tunable property table (C19).

//! The inventory is normative in `spec/tcpip/observability.md`. Every field here has one
//! spec row; an increment is a wrapping add at the event site and never allocates.

use raven_abi::errno::Errno;

use crate::socket::Kind;
use crate::{Ipv4Addr, SocketId, Stack};

/// One `u64` per counter in the normative inventory.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Counters {
    /// `tcp.active_opens`.
    pub active_opens: u64,
    /// `tcp.passive_opens`.
    pub passive_opens: u64,
    /// `tcp.attempt_fails`.
    pub attempt_fails: u64,
    /// `tcp.estab_resets`.
    pub estab_resets: u64,
    /// `tcp.in_segments`.
    pub in_segments: u64,
    /// `tcp.out_segments`.
    pub out_segments: u64,
    /// `tcp.in_bytes`.
    pub in_bytes: u64,
    /// `tcp.out_bytes`.
    pub out_bytes: u64,
    /// `tcp.retrans_segments`.
    pub retrans_segments: u64,
    /// `tcp.retrans_bytes`.
    pub retrans_bytes: u64,
    /// `tcp.fast_retransmits`.
    pub fast_retransmits: u64,
    /// `tcp.sack_retransmits`.
    pub sack_retransmits: u64,
    /// `tcp.in_ooo_segments`.
    pub in_ooo_segments: u64,
    /// `tcp.in_ooo_bytes`.
    pub in_ooo_bytes: u64,
    /// `tcp.in_dup_segments`.
    pub in_dup_segments: u64,
    /// `tcp.in_dup_bytes`.
    pub in_dup_bytes: u64,
    /// `tcp.past_window_drops`.
    pub past_window_drops: u64,
    /// `tcp.past_window_bytes`.
    pub past_window_bytes: u64,
    /// `tcp.zwprobe_sent`.
    pub zwprobe_sent: u64,
    /// `tcp.in_zwnd_probes`.
    pub in_zwnd_probes: u64,
    /// `tcp.window_updates`.
    pub window_updates: u64,
    /// `tcp.delayed_acks_sent`.
    pub delayed_acks_sent: u64,
    /// `tcp.rtt_updates`.
    pub rtt_updates: u64,
    /// `tcp.rtt_no_updates`.
    pub rtt_no_updates: u64,
    /// `tcp.listen_drops`.
    pub listen_drops: u64,
    /// `tcp.half_open_drops`.
    pub half_open_drops: u64,
    /// `tcp.accept_queue_drops`.
    pub accept_queue_drops: u64,
    /// `tcp.reassembly_timeouts`.
    pub reassembly_timeouts: u64,
    /// `tcp.reassembly_overflows`.
    pub reassembly_overflows: u64,
    /// `tcp.acks_for_unsent`.
    pub acks_for_unsent: u64,
    /// `tcp.zero_window_drops`.
    pub zero_window_drops: u64,
    /// `tcp.rsts_for_unsent`.
    pub rsts_for_unsent: u64,
    /// `tcp.rst_sent`.
    pub rst_sent: u64,
    /// `tcp.rst_rcvd`.
    pub rst_rcvd: u64,
    /// `tcp.rst_out_of_window`.
    pub rst_out_of_window: u64,
    /// `tcp.rst_challenge`.
    pub rst_challenge: u64,
    /// `tcp.ack_challenge`.
    pub ack_challenge: u64,
    /// `tcp.challenge_acks_sent`.
    pub challenge_acks_sent: u64,
    /// `tcp.challenge_ack_suppressed`.
    pub challenge_ack_suppressed: u64,
    /// `tcp.paws_drops`.
    pub paws_drops: u64,
    /// `tcp.option_unknown`.
    pub option_unknown: u64,
    /// `tcp.option_malformed`.
    pub option_malformed: u64,
    /// `tcp.data_after_fin`.
    pub data_after_fin: u64,
    /// `tcp.finwait2_timeouts`.
    pub finwait2_timeouts: u64,
    /// `tcp.user_timeouts`.
    pub user_timeouts: u64,
    /// `tcp.timewait_fires`.
    pub timewait_fires: u64,
    /// `tcp.timewait_expiries`.
    pub timewait_expiries: u64,
    /// `tcp.timewait_recycled`.
    pub timewait_recycled: u64,
    /// `tcp.timewait_syn_drops`.
    pub timewait_syn_drops: u64,
    /// `tcp.timewait_data_drops`.
    pub timewait_data_drops: u64,
    /// `tcp.timewait_rst_ignored`.
    pub timewait_rst_ignored: u64,
    /// `tcp.connect_fires`.
    pub connect_fires: u64,
    /// `tcp.rto_fires`.
    pub rto_fires: u64,
    /// `tcp.delack_fires`.
    pub delack_fires: u64,
    /// `tcp.keepalive_fires`.
    pub keepalive_fires: u64,
    /// `tcp.keepalive_probes_rcvd`.
    pub keepalive_probes_rcvd: u64,
    /// `tcp.zwprobe_fires`.
    pub zwprobe_fires: u64,
    /// `tcp.finwait2_fires`.
    pub finwait2_fires: u64,
    /// `tcp.tlp_fires`.
    pub tlp_fires: u64,
    /// `tcp.rack_losses`.
    pub rack_losses: u64,
    /// `tcp.reordering_window_growth`.
    pub reordering_window_growth: u64,
    /// `tcp.spurious_retransmits`.
    pub spurious_retransmits: u64,
    /// `tcp.spurious_undo`.
    pub spurious_undo: u64,
    /// `tcp.sack_blocks_sent`.
    pub sack_blocks_sent: u64,
    /// `tcp.sack_blocks_dropped`.
    pub sack_blocks_dropped: u64,
    /// `tcp.syn_cookie_sent`.
    pub syn_cookie_sent: u64,
    /// `tcp.syn_cookie_valid`.
    pub syn_cookie_valid: u64,
    /// `tcp.syn_cookie_invalid`.
    pub syn_cookie_invalid: u64,
    /// `tcp.port_alloc_fails`.
    pub port_alloc_fails: u64,
    /// `tcp.port_alloc_probes`.
    pub port_alloc_probes: u64,
    /// `tcp.hash_chain_overflows`.
    pub hash_chain_overflows: u64,
    /// `tcp.pmtu_updates`.
    pub pmtu_updates: u64,
    /// `tcp.pmtu_invalid`.
    pub pmtu_invalid: u64,
    /// `tcp.plpmtud_probes`.
    pub plpmtud_probes: u64,
    /// `tcp.plpmtud_losses`.
    pub plpmtud_losses: u64,
    /// `tcp.plpmtud_reprobes`.
    pub plpmtud_reprobes: u64,
    /// `tcp.ecn_ce_marks`.
    pub ecn_ce_marks: u64,
    /// `tcp.ecn_ce_react`.
    pub ecn_ce_react: u64,
    /// `tcp.ecn_cwr_sent`.
    pub ecn_cwr_sent: u64,
    /// `tcp.timer_early`.
    pub timer_early: u64,
    /// `tcp.timer_missed`.
    pub timer_missed: u64,
    /// `tcp.in_checksum_errors`.
    pub in_checksum_errors: u64,
    /// `tcp.in_header_errors`.
    pub in_header_errors: u64,
    /// `tcp.out_errors`.
    pub out_errors: u64,
}

impl Counters {
    /// Read a counter by its spec name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<u64> {
        Some(match name {
            "tcp.active_opens" => self.active_opens,
            "tcp.passive_opens" => self.passive_opens,
            "tcp.attempt_fails" => self.attempt_fails,
            "tcp.estab_resets" => self.estab_resets,
            "tcp.in_segments" => self.in_segments,
            "tcp.out_segments" => self.out_segments,
            "tcp.in_bytes" => self.in_bytes,
            "tcp.out_bytes" => self.out_bytes,
            "tcp.retrans_segments" => self.retrans_segments,
            "tcp.retrans_bytes" => self.retrans_bytes,
            "tcp.fast_retransmits" => self.fast_retransmits,
            "tcp.sack_retransmits" => self.sack_retransmits,
            "tcp.in_ooo_segments" => self.in_ooo_segments,
            "tcp.in_ooo_bytes" => self.in_ooo_bytes,
            "tcp.in_dup_segments" => self.in_dup_segments,
            "tcp.in_dup_bytes" => self.in_dup_bytes,
            "tcp.past_window_drops" => self.past_window_drops,
            "tcp.past_window_bytes" => self.past_window_bytes,
            "tcp.zwprobe_sent" => self.zwprobe_sent,
            "tcp.in_zwnd_probes" => self.in_zwnd_probes,
            "tcp.window_updates" => self.window_updates,
            "tcp.delayed_acks_sent" => self.delayed_acks_sent,
            "tcp.rtt_updates" => self.rtt_updates,
            "tcp.rtt_no_updates" => self.rtt_no_updates,
            "tcp.listen_drops" => self.listen_drops,
            "tcp.half_open_drops" => self.half_open_drops,
            "tcp.accept_queue_drops" => self.accept_queue_drops,
            "tcp.reassembly_timeouts" => self.reassembly_timeouts,
            "tcp.reassembly_overflows" => self.reassembly_overflows,
            "tcp.acks_for_unsent" => self.acks_for_unsent,
            "tcp.zero_window_drops" => self.zero_window_drops,
            "tcp.rsts_for_unsent" => self.rsts_for_unsent,
            "tcp.rst_sent" => self.rst_sent,
            "tcp.rst_rcvd" => self.rst_rcvd,
            "tcp.rst_out_of_window" => self.rst_out_of_window,
            "tcp.rst_challenge" => self.rst_challenge,
            "tcp.ack_challenge" => self.ack_challenge,
            "tcp.challenge_acks_sent" => self.challenge_acks_sent,
            "tcp.challenge_ack_suppressed" => self.challenge_ack_suppressed,
            "tcp.paws_drops" => self.paws_drops,
            "tcp.option_unknown" => self.option_unknown,
            "tcp.option_malformed" => self.option_malformed,
            "tcp.data_after_fin" => self.data_after_fin,
            "tcp.finwait2_timeouts" => self.finwait2_timeouts,
            "tcp.user_timeouts" => self.user_timeouts,
            "tcp.timewait_fires" => self.timewait_fires,
            "tcp.timewait_expiries" => self.timewait_expiries,
            "tcp.timewait_recycled" => self.timewait_recycled,
            "tcp.timewait_syn_drops" => self.timewait_syn_drops,
            "tcp.timewait_data_drops" => self.timewait_data_drops,
            "tcp.timewait_rst_ignored" => self.timewait_rst_ignored,
            "tcp.connect_fires" => self.connect_fires,
            "tcp.rto_fires" => self.rto_fires,
            "tcp.delack_fires" => self.delack_fires,
            "tcp.keepalive_fires" => self.keepalive_fires,
            "tcp.keepalive_probes_rcvd" => self.keepalive_probes_rcvd,
            "tcp.zwprobe_fires" => self.zwprobe_fires,
            "tcp.finwait2_fires" => self.finwait2_fires,
            "tcp.tlp_fires" => self.tlp_fires,
            "tcp.rack_losses" => self.rack_losses,
            "tcp.reordering_window_growth" => self.reordering_window_growth,
            "tcp.spurious_retransmits" => self.spurious_retransmits,
            "tcp.spurious_undo" => self.spurious_undo,
            "tcp.sack_blocks_sent" => self.sack_blocks_sent,
            "tcp.sack_blocks_dropped" => self.sack_blocks_dropped,
            "tcp.syn_cookie_sent" => self.syn_cookie_sent,
            "tcp.syn_cookie_valid" => self.syn_cookie_valid,
            "tcp.syn_cookie_invalid" => self.syn_cookie_invalid,
            "tcp.port_alloc_fails" => self.port_alloc_fails,
            "tcp.port_alloc_probes" => self.port_alloc_probes,
            "tcp.hash_chain_overflows" => self.hash_chain_overflows,
            "tcp.pmtu_updates" => self.pmtu_updates,
            "tcp.pmtu_invalid" => self.pmtu_invalid,
            "tcp.plpmtud_probes" => self.plpmtud_probes,
            "tcp.plpmtud_losses" => self.plpmtud_losses,
            "tcp.plpmtud_reprobes" => self.plpmtud_reprobes,
            "tcp.ecn_ce_marks" => self.ecn_ce_marks,
            "tcp.ecn_ce_react" => self.ecn_ce_react,
            "tcp.ecn_cwr_sent" => self.ecn_cwr_sent,
            "tcp.timer_early" => self.timer_early,
            "tcp.timer_missed" => self.timer_missed,
            "tcp.in_checksum_errors" => self.in_checksum_errors,
            "tcp.in_header_errors" => self.in_header_errors,
            "tcp.out_errors" => self.out_errors,
            _ => return None,
        })
    }

    /// The spec names, in normative order.
    #[must_use]
    pub const fn names() -> &'static [&'static str] {
        &[
            "tcp.active_opens",
            "tcp.passive_opens",
            "tcp.attempt_fails",
            "tcp.estab_resets",
            "tcp.in_segments",
            "tcp.out_segments",
            "tcp.in_bytes",
            "tcp.out_bytes",
            "tcp.retrans_segments",
            "tcp.retrans_bytes",
            "tcp.fast_retransmits",
            "tcp.sack_retransmits",
            "tcp.in_ooo_segments",
            "tcp.in_ooo_bytes",
            "tcp.in_dup_segments",
            "tcp.in_dup_bytes",
            "tcp.past_window_drops",
            "tcp.past_window_bytes",
            "tcp.zwprobe_sent",
            "tcp.in_zwnd_probes",
            "tcp.window_updates",
            "tcp.delayed_acks_sent",
            "tcp.rtt_updates",
            "tcp.rtt_no_updates",
            "tcp.listen_drops",
            "tcp.half_open_drops",
            "tcp.accept_queue_drops",
            "tcp.reassembly_timeouts",
            "tcp.reassembly_overflows",
            "tcp.acks_for_unsent",
            "tcp.zero_window_drops",
            "tcp.rsts_for_unsent",
            "tcp.rst_sent",
            "tcp.rst_rcvd",
            "tcp.rst_out_of_window",
            "tcp.rst_challenge",
            "tcp.ack_challenge",
            "tcp.challenge_acks_sent",
            "tcp.challenge_ack_suppressed",
            "tcp.paws_drops",
            "tcp.option_unknown",
            "tcp.option_malformed",
            "tcp.data_after_fin",
            "tcp.finwait2_timeouts",
            "tcp.user_timeouts",
            "tcp.timewait_fires",
            "tcp.timewait_expiries",
            "tcp.timewait_recycled",
            "tcp.timewait_syn_drops",
            "tcp.timewait_data_drops",
            "tcp.timewait_rst_ignored",
            "tcp.connect_fires",
            "tcp.rto_fires",
            "tcp.delack_fires",
            "tcp.keepalive_fires",
            "tcp.keepalive_probes_rcvd",
            "tcp.zwprobe_fires",
            "tcp.finwait2_fires",
            "tcp.tlp_fires",
            "tcp.rack_losses",
            "tcp.reordering_window_growth",
            "tcp.spurious_retransmits",
            "tcp.spurious_undo",
            "tcp.sack_blocks_sent",
            "tcp.sack_blocks_dropped",
            "tcp.syn_cookie_sent",
            "tcp.syn_cookie_valid",
            "tcp.syn_cookie_invalid",
            "tcp.port_alloc_fails",
            "tcp.port_alloc_probes",
            "tcp.hash_chain_overflows",
            "tcp.pmtu_updates",
            "tcp.pmtu_invalid",
            "tcp.plpmtud_probes",
            "tcp.plpmtud_losses",
            "tcp.plpmtud_reprobes",
            "tcp.ecn_ce_marks",
            "tcp.ecn_ce_react",
            "tcp.ecn_cwr_sent",
            "tcp.timer_early",
            "tcp.timer_missed",
            "tcp.in_checksum_errors",
            "tcp.in_header_errors",
            "tcp.out_errors",
        ]
    }

    /// Zero every counter.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// The storage type of a tunable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PropKind {
    /// An unsigned integer.
    Uint,
    /// A boolean encoded as 0 or 1.
    Bool,
    /// A small enumeration encoded as an integer.
    Enum,
}

/// The unit of a tunable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    /// `Ms`.
    Ms,
    /// `Bool`.
    Bool,
    /// `Count`.
    Count,
    /// `Entries`.
    Entries,
    /// `Holes`.
    Holes,
    /// `Segments`.
    Segments,
    /// `Bytes`.
    Bytes,
    /// `Enum`.
    Enum,
    /// `Ranges`.
    Ranges,
    /// `Port`.
    Port,
}

fn unit_name(unit: Unit) -> &'static str {
    match unit {
        Unit::Ms => "ms",
        Unit::Bool => "bool",
        Unit::Count => "count",
        Unit::Entries => "entries",
        Unit::Holes => "holes",
        Unit::Segments => "segments",
        Unit::Bytes => "bytes",
        Unit::Enum => "enum",
        Unit::Ranges => "ranges",
        Unit::Port => "port",
    }
}

impl Unit {
    /// The spec spelling of the unit.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        unit_name(self)
    }
}

/// Whether a name and meaning are promised (`Committed`) or may change (`Private`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stability {
    /// Stable name and meaning.
    Committed,
    /// May change without notice.
    Private,
}

/// One row of the enumerable property table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PropertyInfo {
    /// Stable property name.
    pub name: &'static str,
    /// Storage type.
    pub kind: PropKind,
    /// Inclusive minimum.
    pub min: u64,
    /// Inclusive maximum.
    pub max: u64,
    /// Production default.
    pub default: u64,
    /// Unit.
    pub unit: Unit,
    /// Stability marker.
    pub stability: Stability,
}

/// Every tunable, in spec order.
pub static PROPERTIES: &[PropertyInfo] = &[
    PropertyInfo {
        name: "tcp.time_wait_interval",
        kind: PropKind::Uint,
        min: 1000,
        max: 600000,
        default: 60000,
        unit: Unit::Ms,
        stability: Stability::Committed,
    },
    PropertyInfo {
        name: "tcp.finwait2_timeout",
        kind: PropKind::Uint,
        min: 1000,
        max: 600000,
        default: 60000,
        unit: Unit::Ms,
        stability: Stability::Committed,
    },
    PropertyInfo {
        name: "tcp.user_timeout",
        kind: PropKind::Uint,
        min: 0,
        max: 86400000,
        default: 0,
        unit: Unit::Ms,
        stability: Stability::Committed,
    },
    PropertyInfo {
        name: "tcp.keepalive_enable",
        kind: PropKind::Bool,
        min: 0,
        max: 1,
        default: 1,
        unit: Unit::Bool,
        stability: Stability::Committed,
    },
    PropertyInfo {
        name: "tcp.keepalive_interval",
        kind: PropKind::Uint,
        min: 1000,
        max: 864000000,
        default: 7200000,
        unit: Unit::Ms,
        stability: Stability::Committed,
    },
    PropertyInfo {
        name: "tcp.keepalive_probes",
        kind: PropKind::Uint,
        min: 1,
        max: 255,
        default: 9,
        unit: Unit::Count,
        stability: Stability::Committed,
    },
    PropertyInfo {
        name: "tcp.half_open_limit",
        kind: PropKind::Uint,
        min: 1,
        max: 1048576,
        default: 1024,
        unit: Unit::Entries,
        stability: Stability::Committed,
    },
    PropertyInfo {
        name: "tcp.accept_queue_limit",
        kind: PropKind::Uint,
        min: 1,
        max: 1048576,
        default: 128,
        unit: Unit::Entries,
        stability: Stability::Committed,
    },
    PropertyInfo {
        name: "tcp.connection_max",
        kind: PropKind::Uint,
        min: 1,
        max: 16777216,
        default: 65536,
        unit: Unit::Entries,
        stability: Stability::Committed,
    },
    PropertyInfo {
        name: "tcp.timewait_max",
        kind: PropKind::Uint,
        min: 1,
        max: 16777216,
        default: 65536,
        unit: Unit::Entries,
        stability: Stability::Committed,
    },
    PropertyInfo {
        name: "tcp.reassembly_timeout",
        kind: PropKind::Uint,
        min: 100,
        max: 600000,
        default: 60000,
        unit: Unit::Ms,
        stability: Stability::Committed,
    },
    PropertyInfo {
        name: "tcp.reassembly_holes_max",
        kind: PropKind::Uint,
        min: 1,
        max: 1024,
        default: 32,
        unit: Unit::Holes,
        stability: Stability::Private,
    },
    PropertyInfo {
        name: "tcp.timer_late_threshold",
        kind: PropKind::Uint,
        min: 1,
        max: 60000,
        default: 1000,
        unit: Unit::Ms,
        stability: Stability::Private,
    },
    PropertyInfo {
        name: "tcp.rto_initial",
        kind: PropKind::Uint,
        min: 1,
        max: 60000,
        default: 1000,
        unit: Unit::Ms,
        stability: Stability::Committed,
    },
    PropertyInfo {
        name: "tcp.rto_min",
        kind: PropKind::Uint,
        min: 1,
        max: 60000,
        default: 200,
        unit: Unit::Ms,
        stability: Stability::Committed,
    },
    PropertyInfo {
        name: "tcp.rto_max",
        kind: PropKind::Uint,
        min: 1000,
        max: 3600000,
        default: 60000,
        unit: Unit::Ms,
        stability: Stability::Committed,
    },
    PropertyInfo {
        name: "tcp.rto_max_retries",
        kind: PropKind::Uint,
        min: 1,
        max: 255,
        default: 15,
        unit: Unit::Count,
        stability: Stability::Committed,
    },
    PropertyInfo {
        name: "tcp.connect_retries",
        kind: PropKind::Uint,
        min: 1,
        max: 255,
        default: 6,
        unit: Unit::Count,
        stability: Stability::Committed,
    },
    PropertyInfo {
        name: "tcp.delack_delay",
        kind: PropKind::Uint,
        min: 1,
        max: 500,
        default: 40,
        unit: Unit::Ms,
        stability: Stability::Committed,
    },
    PropertyInfo {
        name: "tcp.delack_segments",
        kind: PropKind::Uint,
        min: 1,
        max: 255,
        default: 2,
        unit: Unit::Segments,
        stability: Stability::Committed,
    },
    PropertyInfo {
        name: "tcp.zwprobe_max_interval",
        kind: PropKind::Uint,
        min: 1000,
        max: 3600000,
        default: 60000,
        unit: Unit::Ms,
        stability: Stability::Committed,
    },
    PropertyInfo {
        name: "tcp.dup_ack_threshold",
        kind: PropKind::Uint,
        min: 1,
        max: 255,
        default: 3,
        unit: Unit::Count,
        stability: Stability::Committed,
    },
    PropertyInfo {
        name: "tcp.initial_window",
        kind: PropKind::Uint,
        min: 1,
        max: 64,
        default: 10,
        unit: Unit::Segments,
        stability: Stability::Committed,
    },
    PropertyInfo {
        name: "tcp.cwnd_max",
        kind: PropKind::Uint,
        min: 2,
        max: 1073741824,
        default: 16777216,
        unit: Unit::Bytes,
        stability: Stability::Committed,
    },
    PropertyInfo {
        name: "tcp.cwnd_min",
        kind: PropKind::Uint,
        min: 1,
        max: 64,
        default: 2,
        unit: Unit::Segments,
        stability: Stability::Committed,
    },
    PropertyInfo {
        name: "tcp.cc_algorithm",
        kind: PropKind::Enum,
        min: 0,
        max: 1,
        default: 1,
        unit: Unit::Enum,
        stability: Stability::Committed,
    },
    PropertyInfo {
        name: "tcp.ignore_path_mtu",
        kind: PropKind::Bool,
        min: 0,
        max: 1,
        default: 0,
        unit: Unit::Bool,
        stability: Stability::Committed,
    },
    PropertyInfo {
        name: "tcp.pmtu_cache_max",
        kind: PropKind::Uint,
        min: 1,
        max: 65536,
        default: 1024,
        unit: Unit::Entries,
        stability: Stability::Private,
    },
    PropertyInfo {
        name: "tcp.pmtu_cache_ttl",
        kind: PropKind::Uint,
        min: 1000,
        max: 86400000,
        default: 600000,
        unit: Unit::Ms,
        stability: Stability::Committed,
    },
    PropertyInfo {
        name: "tcp.plpmtud_enable",
        kind: PropKind::Bool,
        min: 0,
        max: 1,
        default: 1,
        unit: Unit::Bool,
        stability: Stability::Committed,
    },
    PropertyInfo {
        name: "tcp.plpmtud_interval",
        kind: PropKind::Uint,
        min: 1000,
        max: 86400000,
        default: 600000,
        unit: Unit::Ms,
        stability: Stability::Committed,
    },
    PropertyInfo {
        name: "tcp.reordering_window_base",
        kind: PropKind::Uint,
        min: 0,
        max: 10000,
        default: 10,
        unit: Unit::Ms,
        stability: Stability::Committed,
    },
    PropertyInfo {
        name: "tcp.tlp_enable",
        kind: PropKind::Bool,
        min: 0,
        max: 1,
        default: 1,
        unit: Unit::Bool,
        stability: Stability::Committed,
    },
    PropertyInfo {
        name: "tcp.window_scale_enable",
        kind: PropKind::Bool,
        min: 0,
        max: 1,
        default: 1,
        unit: Unit::Bool,
        stability: Stability::Committed,
    },
    PropertyInfo {
        name: "tcp.timestamps_enable",
        kind: PropKind::Bool,
        min: 0,
        max: 1,
        default: 1,
        unit: Unit::Bool,
        stability: Stability::Committed,
    },
    PropertyInfo {
        name: "tcp.sack_enable",
        kind: PropKind::Bool,
        min: 0,
        max: 1,
        default: 1,
        unit: Unit::Bool,
        stability: Stability::Committed,
    },
    PropertyInfo {
        name: "tcp.ecn_enable",
        kind: PropKind::Bool,
        min: 0,
        max: 1,
        default: 1,
        unit: Unit::Bool,
        stability: Stability::Committed,
    },
    PropertyInfo {
        name: "tcp.syn_cookies_enable",
        kind: PropKind::Bool,
        min: 0,
        max: 1,
        default: 1,
        unit: Unit::Bool,
        stability: Stability::Committed,
    },
    PropertyInfo {
        name: "tcp.syn_cookie_lifetime",
        kind: PropKind::Uint,
        min: 1000,
        max: 600000,
        default: 60000,
        unit: Unit::Ms,
        stability: Stability::Committed,
    },
    PropertyInfo {
        name: "tcp.mss_floor",
        kind: PropKind::Uint,
        min: 64,
        max: 65535,
        default: 536,
        unit: Unit::Bytes,
        stability: Stability::Committed,
    },
    PropertyInfo {
        name: "tcp.mss_default",
        kind: PropKind::Uint,
        min: 64,
        max: 65535,
        default: 1460,
        unit: Unit::Bytes,
        stability: Stability::Committed,
    },
    PropertyInfo {
        name: "tcp.mss_ceiling",
        kind: PropKind::Uint,
        min: 64,
        max: 65535,
        default: 8960,
        unit: Unit::Bytes,
        stability: Stability::Committed,
    },
    PropertyInfo {
        name: "tcp.send_buffer_default",
        kind: PropKind::Uint,
        min: 2048,
        max: 1073741824,
        default: 65536,
        unit: Unit::Bytes,
        stability: Stability::Committed,
    },
    PropertyInfo {
        name: "tcp.send_buffer_max",
        kind: PropKind::Uint,
        min: 2048,
        max: 1073741824,
        default: 4194304,
        unit: Unit::Bytes,
        stability: Stability::Committed,
    },
    PropertyInfo {
        name: "tcp.recv_buffer_default",
        kind: PropKind::Uint,
        min: 2048,
        max: 1073741824,
        default: 65536,
        unit: Unit::Bytes,
        stability: Stability::Committed,
    },
    PropertyInfo {
        name: "tcp.recv_buffer_max",
        kind: PropKind::Uint,
        min: 2048,
        max: 1073741824,
        default: 4194304,
        unit: Unit::Bytes,
        stability: Stability::Committed,
    },
    PropertyInfo {
        name: "tcp.nagle",
        kind: PropKind::Bool,
        min: 0,
        max: 1,
        default: 1,
        unit: Unit::Bool,
        stability: Stability::Committed,
    },
    PropertyInfo {
        name: "tcp.sack_ranges_max",
        kind: PropKind::Uint,
        min: 1,
        max: 255,
        default: 16,
        unit: Unit::Ranges,
        stability: Stability::Private,
    },
    PropertyInfo {
        name: "tcp.urgent_mode",
        kind: PropKind::Bool,
        min: 0,
        max: 1,
        default: 1,
        unit: Unit::Bool,
        stability: Stability::Committed,
    },
    PropertyInfo {
        name: "tcp.port_range_first",
        kind: PropKind::Uint,
        min: 1024,
        max: 65535,
        default: 32768,
        unit: Unit::Port,
        stability: Stability::Committed,
    },
    PropertyInfo {
        name: "tcp.port_range_last",
        kind: PropKind::Uint,
        min: 1024,
        max: 65535,
        default: 60999,
        unit: Unit::Port,
        stability: Stability::Committed,
    },
    PropertyInfo {
        name: "tcp.isn_strength",
        kind: PropKind::Uint,
        min: 0,
        max: 2,
        default: 2,
        unit: Unit::Enum,
        stability: Stability::Committed,
    },
    PropertyInfo {
        name: "tcp.challenge_ack_burst",
        kind: PropKind::Uint,
        min: 1,
        max: 100000,
        default: 10,
        unit: Unit::Count,
        stability: Stability::Committed,
    },
    PropertyInfo {
        name: "tcp.challenge_ack_interval",
        kind: PropKind::Uint,
        min: 100,
        max: 3600000,
        default: 5000,
        unit: Unit::Ms,
        stability: Stability::Committed,
    },
    PropertyInfo {
        name: "tcp.challenge_ack_global_burst",
        kind: PropKind::Uint,
        min: 1,
        max: 1000000,
        default: 1000,
        unit: Unit::Count,
        stability: Stability::Committed,
    },
    PropertyInfo {
        name: "tcp.challenge_ack_global_interval",
        kind: PropKind::Uint,
        min: 100,
        max: 3600000,
        default: 1000,
        unit: Unit::Ms,
        stability: Stability::Committed,
    },
    PropertyInfo {
        name: "tcp.hash_chain_max",
        kind: PropKind::Uint,
        min: 1,
        max: 1024,
        default: 8,
        unit: Unit::Entries,
        stability: Stability::Private,
    },
    PropertyInfo {
        name: "tcp.port_alloc_attempts",
        kind: PropKind::Uint,
        min: 1,
        max: 65536,
        default: 64,
        unit: Unit::Count,
        stability: Stability::Private,
    },
];

/// The live value of every tunable. Field names are the spec name after `tcp.`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Tunables {
    pub(crate) time_wait_interval: u64,
    pub(crate) finwait2_timeout: u64,
    pub(crate) user_timeout: u64,
    pub(crate) keepalive_enable: u64,
    pub(crate) keepalive_interval: u64,
    pub(crate) keepalive_probes: u64,
    pub(crate) half_open_limit: u64,
    pub(crate) accept_queue_limit: u64,
    pub(crate) connection_max: u64,
    pub(crate) timewait_max: u64,
    pub(crate) reassembly_timeout: u64,
    pub(crate) reassembly_holes_max: u64,
    pub(crate) timer_late_threshold: u64,
    pub(crate) rto_initial: u64,
    pub(crate) rto_min: u64,
    pub(crate) rto_max: u64,
    pub(crate) rto_max_retries: u64,
    pub(crate) connect_retries: u64,
    pub(crate) delack_delay: u64,
    pub(crate) delack_segments: u64,
    pub(crate) zwprobe_max_interval: u64,
    pub(crate) dup_ack_threshold: u64,
    pub(crate) initial_window: u64,
    pub(crate) cwnd_max: u64,
    pub(crate) cwnd_min: u64,
    pub(crate) cc_algorithm: u64,
    pub(crate) ignore_path_mtu: u64,
    pub(crate) pmtu_cache_max: u64,
    pub(crate) pmtu_cache_ttl: u64,
    pub(crate) plpmtud_enable: u64,
    pub(crate) plpmtud_interval: u64,
    pub(crate) reordering_window_base: u64,
    pub(crate) tlp_enable: u64,
    pub(crate) window_scale_enable: u64,
    pub(crate) timestamps_enable: u64,
    pub(crate) sack_enable: u64,
    pub(crate) ecn_enable: u64,
    pub(crate) syn_cookies_enable: u64,
    pub(crate) syn_cookie_lifetime: u64,
    pub(crate) mss_floor: u64,
    pub(crate) mss_default: u64,
    pub(crate) mss_ceiling: u64,
    pub(crate) send_buffer_default: u64,
    pub(crate) send_buffer_max: u64,
    pub(crate) recv_buffer_default: u64,
    pub(crate) recv_buffer_max: u64,
    pub(crate) nagle: u64,
    pub(crate) sack_ranges_max: u64,
    pub(crate) urgent_mode: u64,
    pub(crate) port_range_first: u64,
    pub(crate) port_range_last: u64,
    pub(crate) isn_strength: u64,
    pub(crate) challenge_ack_burst: u64,
    pub(crate) challenge_ack_interval: u64,
    pub(crate) challenge_ack_global_burst: u64,
    pub(crate) challenge_ack_global_interval: u64,
    pub(crate) hash_chain_max: u64,
    pub(crate) port_alloc_attempts: u64,
}

impl Default for Tunables {
    fn default() -> Self {
        Self {
            time_wait_interval: 60000,
            finwait2_timeout: 60000,
            user_timeout: 0,
            keepalive_enable: 1,
            keepalive_interval: 7200000,
            keepalive_probes: 9,
            half_open_limit: 1024,
            accept_queue_limit: 128,
            connection_max: 65536,
            timewait_max: 65536,
            reassembly_timeout: 60000,
            reassembly_holes_max: 32,
            timer_late_threshold: 1000,
            rto_initial: 1000,
            rto_min: 200,
            rto_max: 60000,
            rto_max_retries: 15,
            connect_retries: 6,
            delack_delay: 40,
            delack_segments: 2,
            zwprobe_max_interval: 60000,
            dup_ack_threshold: 3,
            initial_window: 10,
            cwnd_max: 16777216,
            cwnd_min: 2,
            cc_algorithm: 1,
            ignore_path_mtu: 0,
            pmtu_cache_max: 1024,
            pmtu_cache_ttl: 600000,
            plpmtud_enable: 1,
            plpmtud_interval: 600000,
            reordering_window_base: 10,
            tlp_enable: 1,
            window_scale_enable: 1,
            timestamps_enable: 1,
            sack_enable: 1,
            ecn_enable: 1,
            syn_cookies_enable: 1,
            syn_cookie_lifetime: 60000,
            mss_floor: 536,
            mss_default: 1460,
            mss_ceiling: 8960,
            send_buffer_default: 65536,
            send_buffer_max: 4194304,
            recv_buffer_default: 65536,
            recv_buffer_max: 4194304,
            nagle: 1,
            sack_ranges_max: 16,
            urgent_mode: 1,
            port_range_first: 32768,
            port_range_last: 60999,
            isn_strength: 2,
            challenge_ack_burst: 10,
            challenge_ack_interval: 5000,
            challenge_ack_global_burst: 1000,
            challenge_ack_global_interval: 1000,
            hash_chain_max: 8,
            port_alloc_attempts: 64,
        }
    }
}

impl Tunables {
    pub(crate) fn get(&self, name: &str) -> Option<u64> {
        Some(match name {
            "tcp.time_wait_interval" => self.time_wait_interval,
            "tcp.finwait2_timeout" => self.finwait2_timeout,
            "tcp.user_timeout" => self.user_timeout,
            "tcp.keepalive_enable" => self.keepalive_enable,
            "tcp.keepalive_interval" => self.keepalive_interval,
            "tcp.keepalive_probes" => self.keepalive_probes,
            "tcp.half_open_limit" => self.half_open_limit,
            "tcp.accept_queue_limit" => self.accept_queue_limit,
            "tcp.connection_max" => self.connection_max,
            "tcp.timewait_max" => self.timewait_max,
            "tcp.reassembly_timeout" => self.reassembly_timeout,
            "tcp.reassembly_holes_max" => self.reassembly_holes_max,
            "tcp.timer_late_threshold" => self.timer_late_threshold,
            "tcp.rto_initial" => self.rto_initial,
            "tcp.rto_min" => self.rto_min,
            "tcp.rto_max" => self.rto_max,
            "tcp.rto_max_retries" => self.rto_max_retries,
            "tcp.connect_retries" => self.connect_retries,
            "tcp.delack_delay" => self.delack_delay,
            "tcp.delack_segments" => self.delack_segments,
            "tcp.zwprobe_max_interval" => self.zwprobe_max_interval,
            "tcp.dup_ack_threshold" => self.dup_ack_threshold,
            "tcp.initial_window" => self.initial_window,
            "tcp.cwnd_max" => self.cwnd_max,
            "tcp.cwnd_min" => self.cwnd_min,
            "tcp.cc_algorithm" => self.cc_algorithm,
            "tcp.ignore_path_mtu" => self.ignore_path_mtu,
            "tcp.pmtu_cache_max" => self.pmtu_cache_max,
            "tcp.pmtu_cache_ttl" => self.pmtu_cache_ttl,
            "tcp.plpmtud_enable" => self.plpmtud_enable,
            "tcp.plpmtud_interval" => self.plpmtud_interval,
            "tcp.reordering_window_base" => self.reordering_window_base,
            "tcp.tlp_enable" => self.tlp_enable,
            "tcp.window_scale_enable" => self.window_scale_enable,
            "tcp.timestamps_enable" => self.timestamps_enable,
            "tcp.sack_enable" => self.sack_enable,
            "tcp.ecn_enable" => self.ecn_enable,
            "tcp.syn_cookies_enable" => self.syn_cookies_enable,
            "tcp.syn_cookie_lifetime" => self.syn_cookie_lifetime,
            "tcp.mss_floor" => self.mss_floor,
            "tcp.mss_default" => self.mss_default,
            "tcp.mss_ceiling" => self.mss_ceiling,
            "tcp.send_buffer_default" => self.send_buffer_default,
            "tcp.send_buffer_max" => self.send_buffer_max,
            "tcp.recv_buffer_default" => self.recv_buffer_default,
            "tcp.recv_buffer_max" => self.recv_buffer_max,
            "tcp.nagle" => self.nagle,
            "tcp.sack_ranges_max" => self.sack_ranges_max,
            "tcp.urgent_mode" => self.urgent_mode,
            "tcp.port_range_first" => self.port_range_first,
            "tcp.port_range_last" => self.port_range_last,
            "tcp.isn_strength" => self.isn_strength,
            "tcp.challenge_ack_burst" => self.challenge_ack_burst,
            "tcp.challenge_ack_interval" => self.challenge_ack_interval,
            "tcp.challenge_ack_global_burst" => self.challenge_ack_global_burst,
            "tcp.challenge_ack_global_interval" => self.challenge_ack_global_interval,
            "tcp.hash_chain_max" => self.hash_chain_max,
            "tcp.port_alloc_attempts" => self.port_alloc_attempts,
            _ => return None,
        })
    }
    pub(crate) fn set(&mut self, name: &str, value: u64) -> bool {
        match name {
            "tcp.time_wait_interval" => self.time_wait_interval = value,
            "tcp.finwait2_timeout" => self.finwait2_timeout = value,
            "tcp.user_timeout" => self.user_timeout = value,
            "tcp.keepalive_enable" => self.keepalive_enable = value,
            "tcp.keepalive_interval" => self.keepalive_interval = value,
            "tcp.keepalive_probes" => self.keepalive_probes = value,
            "tcp.half_open_limit" => self.half_open_limit = value,
            "tcp.accept_queue_limit" => self.accept_queue_limit = value,
            "tcp.connection_max" => self.connection_max = value,
            "tcp.timewait_max" => self.timewait_max = value,
            "tcp.reassembly_timeout" => self.reassembly_timeout = value,
            "tcp.reassembly_holes_max" => self.reassembly_holes_max = value,
            "tcp.timer_late_threshold" => self.timer_late_threshold = value,
            "tcp.rto_initial" => self.rto_initial = value,
            "tcp.rto_min" => self.rto_min = value,
            "tcp.rto_max" => self.rto_max = value,
            "tcp.rto_max_retries" => self.rto_max_retries = value,
            "tcp.connect_retries" => self.connect_retries = value,
            "tcp.delack_delay" => self.delack_delay = value,
            "tcp.delack_segments" => self.delack_segments = value,
            "tcp.zwprobe_max_interval" => self.zwprobe_max_interval = value,
            "tcp.dup_ack_threshold" => self.dup_ack_threshold = value,
            "tcp.initial_window" => self.initial_window = value,
            "tcp.cwnd_max" => self.cwnd_max = value,
            "tcp.cwnd_min" => self.cwnd_min = value,
            "tcp.cc_algorithm" => self.cc_algorithm = value,
            "tcp.ignore_path_mtu" => self.ignore_path_mtu = value,
            "tcp.pmtu_cache_max" => self.pmtu_cache_max = value,
            "tcp.pmtu_cache_ttl" => self.pmtu_cache_ttl = value,
            "tcp.plpmtud_enable" => self.plpmtud_enable = value,
            "tcp.plpmtud_interval" => self.plpmtud_interval = value,
            "tcp.reordering_window_base" => self.reordering_window_base = value,
            "tcp.tlp_enable" => self.tlp_enable = value,
            "tcp.window_scale_enable" => self.window_scale_enable = value,
            "tcp.timestamps_enable" => self.timestamps_enable = value,
            "tcp.sack_enable" => self.sack_enable = value,
            "tcp.ecn_enable" => self.ecn_enable = value,
            "tcp.syn_cookies_enable" => self.syn_cookies_enable = value,
            "tcp.syn_cookie_lifetime" => self.syn_cookie_lifetime = value,
            "tcp.mss_floor" => self.mss_floor = value,
            "tcp.mss_default" => self.mss_default = value,
            "tcp.mss_ceiling" => self.mss_ceiling = value,
            "tcp.send_buffer_default" => self.send_buffer_default = value,
            "tcp.send_buffer_max" => self.send_buffer_max = value,
            "tcp.recv_buffer_default" => self.recv_buffer_default = value,
            "tcp.recv_buffer_max" => self.recv_buffer_max = value,
            "tcp.nagle" => self.nagle = value,
            "tcp.sack_ranges_max" => self.sack_ranges_max = value,
            "tcp.urgent_mode" => self.urgent_mode = value,
            "tcp.port_range_first" => self.port_range_first = value,
            "tcp.port_range_last" => self.port_range_last = value,
            "tcp.isn_strength" => self.isn_strength = value,
            "tcp.challenge_ack_burst" => self.challenge_ack_burst = value,
            "tcp.challenge_ack_interval" => self.challenge_ack_interval = value,
            "tcp.challenge_ack_global_burst" => self.challenge_ack_global_burst = value,
            "tcp.challenge_ack_global_interval" => self.challenge_ack_global_interval = value,
            "tcp.hash_chain_max" => self.hash_chain_max = value,
            "tcp.port_alloc_attempts" => self.port_alloc_attempts = value,
            _ => return false,
        }
        true
    }
}

/// Negotiated TCP options and their state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OptionFlags {
    /// Negotiated send window scale, if any.
    pub wscale: Option<u8>,
    /// Timestamps negotiated.
    pub timestamps: bool,
    /// SACK permitted by the peer.
    pub sack: bool,
    /// ECN negotiated (both sides offered).
    pub ecn: bool,
    /// Age of `TS.Recent` in milliseconds, saturated at `u32::MAX`.
    pub ts_recent_age_ms: u32,
}

/// A read-only snapshot for a netstat-style view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SocketInfo {
    /// TCB state.
    pub state: crate::TcpState,
    /// Local address and port.
    pub local: (crate::IpAddr, u16),
    /// Remote address and port, if connected.
    pub remote: Option<(crate::IpAddr, u16)>,
    /// Bytes queued but not yet acknowledged.
    pub send_queue: usize,
    /// Bytes queued for the application.
    pub recv_queue: usize,
    /// Current congestion window, bytes.
    pub cwnd: u32,
    /// Current slow-start threshold, bytes.
    pub ssthresh: u32,
    /// Smoothed RTT, milliseconds; zero before the first sample.
    pub srtt_ms: u32,
    /// RTT variance, milliseconds; zero before the first sample.
    pub rttvar_ms: u32,
    /// Current RTO, milliseconds.
    pub rto_ms: u32,
    /// Effective MSS after options and path MTU.
    pub mss: u16,
    /// Retransmissions on this connection.
    pub retransmits: u32,
    /// `"newreno"` or `"cubic"`.
    pub cc_algorithm: &'static str,
    /// Negotiated options.
    pub options: OptionFlags,
}

#[cfg(test)]
#[path = "observability_ut.rs"]
mod tests;

impl<D: crate::Device> Stack<D> {
    /// Stack-wide counters.
    #[must_use]
    pub fn counters(&self) -> &Counters {
        &self.counters
    }

    /// Zero every stack-wide counter.
    pub fn counters_reset(&mut self) {
        self.counters.reset();
    }

    /// The enumerable tunable property table.
    #[must_use]
    pub fn properties() -> &'static [PropertyInfo] {
        PROPERTIES
    }

    /// Read one tunable by its committed or private name.
    pub fn get_property(&self, name: &str) -> Result<u64, Errno> {
        self.tunables.get(name).ok_or(Errno::ENOENT)
    }

    /// Set one tunable, atomically validating its range and cross-field constraints.
    pub fn set_property(&mut self, name: &str, value: u64) -> Result<(), Errno> {
        let Some(info) = PROPERTIES.iter().find(|info| info.name == name) else {
            return Err(Errno::ENOENT);
        };
        if value < info.min || value > info.max {
            return Err(Errno::EINVAL);
        }
        if !self.cross_field_ok(name, value) {
            return Err(Errno::EINVAL);
        }
        if !self.tunables.set(name, value) {
            return Err(Errno::ENOENT);
        }
        self.apply_property(name, value);
        Ok(())
    }

    fn cross_field_ok(&self, name: &str, value: u64) -> bool {
        match name {
            "tcp.rto_min" => value <= self.tunables.rto_max,
            "tcp.rto_max" => value >= self.tunables.rto_min,
            "tcp.port_range_first" => value <= self.tunables.port_range_last,
            "tcp.port_range_last" => value >= self.tunables.port_range_first,
            "tcp.send_buffer_default" => value <= self.tunables.send_buffer_max,
            "tcp.send_buffer_max" => value >= self.tunables.send_buffer_default,
            "tcp.recv_buffer_default" => value <= self.tunables.recv_buffer_max,
            "tcp.recv_buffer_max" => value >= self.tunables.recv_buffer_default,
            "tcp.half_open_limit" => value <= self.tunables.connection_max,
            "tcp.accept_queue_limit" => value <= self.tunables.connection_max,
            "tcp.connection_max" => {
                value >= self.tunables.half_open_limit && value >= self.tunables.accept_queue_limit
            }
            _ => true,
        }
    }

    fn apply_property(&mut self, name: &str, value: u64) {
        match name {
            "tcp.hash_chain_max" => self
                .sockets
                .set_index_max_chain(usize::try_from(value).unwrap_or(8)),
            "tcp.timewait_max" => self
                .timewait
                .set_capacity(usize::try_from(value).unwrap_or(65_536)),
            "tcp.port_range_first" | "tcp.port_range_last" => self.ports.set_range(
                u16::try_from(self.tunables.port_range_first).unwrap_or(32_768),
                u16::try_from(self.tunables.port_range_last).unwrap_or(60_999),
            ),
            "tcp.reassembly_timeout" => self.reassembly.set_timeout_ms(value),
            "tcp.reassembly_holes_max" => {
                self.reassembly
                    .set_max_fragments(usize::try_from(value).unwrap_or(usize::MAX));
            }
            "tcp.challenge_ack_global_burst" => {
                self.challenge_global_tokens = u32::try_from(value).unwrap_or(u32::MAX);
            }
            "tcp.challenge_ack_global_interval" => {
                self.challenge_global_last = self.now;
            }
            "tcp.pmtu_cache_max" => {
                let max = usize::try_from(value).unwrap_or(usize::MAX);
                while self.pmtu4.len() > max {
                    let Some(key) = self.pmtu4.keys().next().copied() else {
                        break;
                    };
                    self.pmtu4.remove(&key);
                }
            }
            _ => {}
        }
    }

    /// A read-only snapshot of one TCP socket or listener.
    pub fn socket_info(&self, socket: SocketId) -> Result<SocketInfo, Errno> {
        let record = self.sockets.record(socket)?;
        match &record.kind {
            Kind::Tcp(connection) => Ok(SocketInfo {
                state: connection.state,
                local: (connection.local, connection.local_port),
                remote: connection.remote,
                send_queue: record.tx.len(),
                recv_queue: record.rx.len(),
                cwnd: connection.cc.cwnd(),
                ssthresh: connection.cc.ssthresh(),
                srtt_ms: u32::try_from(connection.rto.srtt_ms()).unwrap_or(u32::MAX),
                rttvar_ms: u32::try_from(connection.rto.rttvar_ms()).unwrap_or(u32::MAX),
                rto_ms: u32::try_from(connection.rto.rto_ms()).unwrap_or(u32::MAX),
                mss: connection.mss,
                retransmits: connection.retransmits_total,
                cc_algorithm: connection.cc.name(),
                options: OptionFlags {
                    wscale: connection.negotiated_wscale,
                    timestamps: connection.timestamps,
                    sack: connection.sack_permitted,
                    ecn: connection.ecn_negotiated,
                    ts_recent_age_ms: u32::try_from(
                        self.now
                            .saturating_since(connection.ts_recent_age)
                            .as_millis(),
                    )
                    .unwrap_or(u32::MAX),
                },
            }),
            Kind::Listener { port, .. } => Ok(SocketInfo {
                state: crate::TcpState::Listen,
                local: (
                    self.ipv4
                        .map_or(Ipv4Addr::UNSPECIFIED, |config| config.addr)
                        .into(),
                    *port,
                ),
                remote: None,
                send_queue: 0,
                recv_queue: 0,
                cwnd: 0,
                ssthresh: 0,
                srtt_ms: 0,
                rttvar_ms: 0,
                rto_ms: 0,
                mss: 0,
                retransmits: 0,
                cc_algorithm: "newreno",
                options: OptionFlags::default(),
            }),
            _ => Err(Errno::ENOTSOCK),
        }
    }

    /// Enumerate live sockets, including listeners and TIME-WAIT entries.
    pub fn sockets(&self) -> impl Iterator<Item = SocketId> + '_ {
        self.sockets
            .records
            .iter()
            .filter(|(_, record)| !record.closed)
            .map(|(id, _)| SocketId(*id))
    }
}
