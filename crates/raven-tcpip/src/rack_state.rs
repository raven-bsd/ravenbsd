// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// Copyright (c) 2016-2020 Netflix, Inc.
//
// Portions derived from FreeBSD `sys/netinet/tcp_stacks/rack.c` at
// 133344405f532bd54ff3629a77d4c4e062f01fdb (BSD-2-Clause). This state layer is
// distributed under the project BSD-3-Clause licence; the donor BSD-2 notice is
// retained verbatim below.
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions
// are met:
// 1. Redistributions of source code must retain the above copyright
//    notice, this list of conditions and the following disclaimer.
// 2. Redistributions in binary form must reproduce the above copyright
//    notice, this list of conditions and the following disclaimer in the
//    documentation and/or other materials provided with the distribution.
//
// THIS SOFTWARE IS PROVIDED BY THE REGENTS AND CONTRIBUTORS ``AS IS'' AND
// ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED.  IN NO EVENT SHALL THE REGENTS OR CONTRIBUTORS BE LIABLE
// FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
// DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
// OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
// HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
// LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
// OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
// SUCH DAMAGE.

//! RACK input/state handler port.
//!
//! The donor State functions are the `rack_do_*` state entry points,
//! `rack_process_data`, `rack_process_option`, and the `rack_handle_*`
//! paths. This file ports their state/transition control flow over the K313
//! timer fields, K314 `RackOs` state, and the K318 `RackStack`/`RackVtable`
//! contexts. The ACK, loss, SACK and retransmit bodies remain in
//! K318/K331/K333/K336; this module does not duplicate them.

use raven_abi::errno::Errno;

use crate::ecn::{TH_ACK, TH_SYN};
use crate::rack_vtable::{
    IPPROTO_TCP, PACE_TMR_DELACK, PACE_TMR_PERSIT, RackSegment, RackSegmentResult, RackSockopt,
    RackStack, RackVtable, TCPS_CLOSE_WAIT, TCPS_CLOSED, TCPS_ESTABLISHED, TCPS_LISTEN,
    TCPS_SYN_RECEIVED, TCPS_SYN_SENT, TH_FIN, TH_RST,
};

/// `TCPS_FIN_WAIT_1` from FreeBSD `tcp_fsm.h`.
pub const TCPS_FIN_WAIT_1: u8 = 6;
/// `TCPS_CLOSING` from FreeBSD `tcp_fsm.h`.
pub const TCPS_CLOSING: u8 = 7;
/// `TCPS_LAST_ACK` from FreeBSD `tcp_fsm.h`.
pub const TCPS_LAST_ACK: u8 = 8;
/// `TCPS_FIN_WAIT_2` from FreeBSD `tcp_fsm.h`.
pub const TCPS_FIN_WAIT_2: u8 = 9;
/// `TCPS_TIME_WAIT` from FreeBSD `tcp_fsm.h`.
pub const TCPS_TIME_WAIT: u8 = 10;

/// FreeBSD `TCPTV_DELACK` in microseconds (`MSEC_2_TICKS(40)`).
const TCPTV_DELACK_US: u32 = 40_000;

fn generic(stack: &mut RackStack, segment: &RackSegment) -> RackSegmentResult {
    stack
        .tfb_do_segment_nounlock(segment)
        .unwrap_or(RackSegmentResult::Ignored)
}

fn acked(stack: &RackStack, ack: u32) -> RackSegmentResult {
    RackSegmentResult::Acked(ack.wrapping_sub(stack.tp.snd_una))
}

/// `rack_do_syn_sent`: process a segment in `TCPS_SYN_SENT`.
#[must_use]
pub fn rack_do_syn_sent(stack: &mut RackStack, segment: &RackSegment) -> RackSegmentResult {
    if segment.flags & TH_RST != 0 {
        stack.tp.t_state = TCPS_CLOSED;
        return RackSegmentResult::Reset;
    }
    if segment.flags & TH_SYN != 0 {
        if segment.flags & TH_ACK != 0 {
            if stack.tp.snd_max != 0 && segment.ack > stack.tp.snd_max {
                return RackSegmentResult::Ignored;
            }
            let result = acked(stack, segment.ack);
            stack.tp.snd_una = segment.ack;
            stack.snd_wnd = u32::from(segment.window);
            stack.tp.t_state = TCPS_ESTABLISHED;
            let _ = stack.tfb_tcp_output();
            return result;
        }
        stack.tp.t_state = TCPS_SYN_RECEIVED;
        return RackSegmentResult::Data;
    }
    RackSegmentResult::Ignored
}

/// `rack_do_syn_recv`: process a segment in `TCPS_SYN_RECEIVED`.
#[must_use]
pub fn rack_do_syn_recv(stack: &mut RackStack, segment: &RackSegment) -> RackSegmentResult {
    if segment.flags & TH_RST != 0 {
        stack.tp.t_state = TCPS_CLOSED;
        return RackSegmentResult::Reset;
    }
    if segment.flags & TH_SYN != 0 && segment.flags & TH_ACK == 0 {
        return RackSegmentResult::Ignored;
    }
    if segment.flags & TH_ACK != 0 {
        if stack.tp.snd_max != 0 && segment.ack > stack.tp.snd_max {
            return RackSegmentResult::Ignored;
        }
        let result = acked(stack, segment.ack);
        stack.tp.snd_una = segment.ack;
        stack.snd_wnd = u32::from(segment.window);
        stack.tp.t_state = TCPS_ESTABLISHED;
        if segment.payload_len > 0 {
            let _ = rack_process_data(stack, segment);
        }
        if segment.flags & TH_FIN != 0 {
            stack.tp.t_state = TCPS_CLOSE_WAIT;
        }
        let _ = stack.tfb_tcp_output();
        return result;
    }
    if segment.flags & TH_FIN != 0 {
        stack.tp.t_state = TCPS_CLOSE_WAIT;
        return RackSegmentResult::Fin;
    }
    rack_process_data(stack, segment)
}

/// `rack_do_established`: process a segment in `TCPS_ESTABLISHED`.
#[must_use]
pub fn rack_do_established(stack: &mut RackStack, segment: &RackSegment) -> RackSegmentResult {
    let previous_window = stack.snd_wnd;
    let result = rack_process_data(stack, segment);
    if result == RackSegmentResult::Fin {
        stack.tp.t_state = TCPS_CLOSE_WAIT;
    } else if result == RackSegmentResult::Reset {
        stack.tp.t_state = TCPS_CLOSED;
    } else if matches!(result, RackSegmentResult::Acked(_)) {
        let _ = rack_handle_might_revert(stack);
    }
    if u32::from(segment.window) != previous_window {
        let _ = rack_do_win_updates(stack, segment);
    }
    result
}

/// `rack_do_fastnewdata`: process data and an ACK in `TCPS_ESTABLISHED`.
#[must_use]
pub fn rack_do_fastnewdata(stack: &mut RackStack, segment: &RackSegment) -> RackSegmentResult {
    if stack.tp.t_state != TCPS_ESTABLISHED {
        return RackSegmentResult::Ignored;
    }
    let result = rack_process_data(stack, segment);
    let _ = stack.tfb_tcp_output();
    result
}

/// `rack_do_close_wait`: process a segment in `TCPS_CLOSE_WAIT`.
#[must_use]
pub fn rack_do_close_wait(stack: &mut RackStack, segment: &RackSegment) -> RackSegmentResult {
    let result = rack_process_data(stack, segment);
    if result == RackSegmentResult::Fin {
        stack.tp.t_state = TCPS_LAST_ACK;
    }
    result
}

/// `rack_do_fin_wait_1`: process a segment in `TCPS_FIN_WAIT_1`.
#[must_use]
pub fn rack_do_fin_wait_1(stack: &mut RackStack, segment: &RackSegment) -> RackSegmentResult {
    let result = rack_process_data(stack, segment);
    if result == RackSegmentResult::Fin {
        stack.tp.t_state = if segment.flags & TH_ACK != 0 {
            TCPS_TIME_WAIT
        } else {
            TCPS_CLOSING
        };
    } else if matches!(result, RackSegmentResult::Acked(_)) && stack.tp.snd_una == stack.tp.snd_max
    {
        stack.tp.t_state = TCPS_FIN_WAIT_2;
    }
    result
}

/// `rack_do_fin_wait_2`: process a segment in `TCPS_FIN_WAIT_2`.
#[must_use]
pub fn rack_do_fin_wait_2(stack: &mut RackStack, segment: &RackSegment) -> RackSegmentResult {
    let result = rack_process_data(stack, segment);
    if result == RackSegmentResult::Fin || segment.flags & TH_FIN != 0 {
        stack.tp.t_state = TCPS_TIME_WAIT;
    }
    result
}

/// `rack_do_closing`: process a segment in `TCPS_CLOSING`.
#[must_use]
pub fn rack_do_closing(stack: &mut RackStack, segment: &RackSegment) -> RackSegmentResult {
    let result = rack_process_data(stack, segment);
    if matches!(result, RackSegmentResult::Acked(_))
        || (result == RackSegmentResult::Fin && segment.flags & TH_ACK != 0)
    {
        stack.tp.t_state = TCPS_TIME_WAIT;
    }
    result
}

/// `rack_do_lastack`: process a segment in `TCPS_LAST_ACK`.
#[must_use]
pub fn rack_do_lastack(stack: &mut RackStack, segment: &RackSegment) -> RackSegmentResult {
    let result = rack_process_data(stack, segment);
    if matches!(result, RackSegmentResult::Acked(_)) {
        stack.tp.t_state = TCPS_CLOSED;
    } else if result == RackSegmentResult::Fin {
        stack.tp.t_state = TCPS_TIME_WAIT;
    }
    result
}

/// `rack_do_segment_nounlock`: dispatch one segment by the current TCB state.
#[must_use]
pub fn rack_do_segment_nounlock(stack: &mut RackStack, segment: &RackSegment) -> RackSegmentResult {
    match stack.tp.t_state {
        TCPS_CLOSED | TCPS_LISTEN => RackSegmentResult::Ignored,
        TCPS_SYN_SENT => rack_do_syn_sent(stack, segment),
        TCPS_SYN_RECEIVED => rack_do_syn_recv(stack, segment),
        TCPS_ESTABLISHED => rack_do_established(stack, segment),
        TCPS_CLOSE_WAIT => rack_do_close_wait(stack, segment),
        TCPS_FIN_WAIT_1 => rack_do_fin_wait_1(stack, segment),
        TCPS_FIN_WAIT_2 => rack_do_fin_wait_2(stack, segment),
        TCPS_CLOSING => rack_do_closing(stack, segment),
        TCPS_LAST_ACK => rack_do_lastack(stack, segment),
        TCPS_TIME_WAIT => {
            if segment.flags & TH_RST != 0 {
                RackSegmentResult::Ignored
            } else {
                generic(stack, segment)
            }
        }
        _ => RackSegmentResult::Ignored,
    }
}

/// `rack_do_segment`: locked entry point; the sans-IO layer has no lock.
#[must_use]
pub fn rack_do_segment(stack: &mut RackStack, segment: &RackSegment) -> RackSegmentResult {
    rack_do_segment_nounlock(stack, segment)
}

/// `rack_process_data`: account for payload and handle the delayed ACK.
#[must_use]
pub fn rack_process_data(stack: &mut RackStack, segment: &RackSegment) -> RackSegmentResult {
    let mut result = generic(stack, segment);
    // `tfb_do_segment_nounlock` checks the ACK before data/FIN, so a duplicate
    // ACK would otherwise hide new data or a FIN from the state handler.
    if result == RackSegmentResult::DuplicateAck {
        if segment.payload_len > 0 {
            result = RackSegmentResult::Data;
        } else if segment.flags & TH_FIN != 0 {
            result = RackSegmentResult::Fin;
        }
    }
    if segment.payload_len > 0 {
        stack.tp.t_rcvtime = stack.os.tcp_ts_getticks();
        let _ = rack_handle_delayed_ack(stack);
    }
    result
}

/// `rack_process_option`: route a RACK socket option through the vtable.
pub fn rack_process_option(
    stack: &mut RackStack,
    option: &RackSockopt,
) -> Result<Option<u32>, Errno> {
    if option.level != IPPROTO_TCP {
        return Err(Errno::EOPNOTSUPP);
    }
    stack.tfb_tcp_ctloutput(option)
}

/// `rack_handle_delayed_ack`: arm the 40 ms delayed-ACK timer once.
pub fn rack_handle_delayed_ack(stack: &mut RackStack) -> bool {
    if stack.hpts_flags & PACE_TMR_DELACK != 0 {
        return false;
    }
    stack.tp.t_acktime = stack.os.tcp_ts_getticks();
    stack.hpts_flags |= PACE_TMR_DELACK;
    stack.timer_exp = stack.os.tcp_hpts_insert(TCPTV_DELACK_US);
    true
}

/// `rack_handle_might_revert`: clear an RTO retransmit override once the ACK
/// covers `rc_snd_max_at_rto`.
pub fn rack_handle_might_revert(stack: &mut RackStack) -> bool {
    if !stack.timer_override {
        return false;
    }
    if stack.snd_max_at_rto != 0 && stack.tp.snd_una < stack.snd_max_at_rto {
        return false;
    }
    stack.must_retran = false;
    stack.timer_override = false;
    stack.snd_max_at_rto = 0;
    true
}

/// `rack_handle_probe_response`: a non-zero window opens persist.
pub fn rack_handle_probe_response(stack: &mut RackStack, window: u16) -> bool {
    if window == 0 {
        return false;
    }
    stack.snd_wnd = u32::from(window);
    if stack.in_persist {
        stack.in_persist = false;
    }
    if stack.hpts_flags & PACE_TMR_PERSIT != 0 {
        stack.hpts_flags &= !PACE_TMR_PERSIT;
        let _ = stack.os.tcp_hpts_remove();
    }
    true
}

/// `rack_do_win_updates`: apply a receive-window update and leave persist.
pub fn rack_do_win_updates(stack: &mut RackStack, segment: &RackSegment) -> bool {
    let window = u32::from(segment.window);
    let changed = stack.snd_wnd != window;
    stack.snd_wnd = window;
    if window > 0 && stack.in_persist {
        stack.in_persist = false;
    }
    if window > 0 && stack.hpts_flags & PACE_TMR_PERSIT != 0 {
        stack.hpts_flags &= !PACE_TMR_PERSIT;
        let _ = stack.os.tcp_hpts_remove();
    }
    changed
}

/// `rack_do_compressed_ack_processing`: drain the queued segments.
pub fn rack_do_compressed_ack_processing(stack: &mut RackStack, have_pkt: bool) -> bool {
    stack.tfb_do_queued_segments(have_pkt)
}

#[cfg(test)]
#[path = "rack_state_ut.rs"]
mod tests;
