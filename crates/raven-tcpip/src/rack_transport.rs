// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// Ported from FreeBSD sys/netinet/tcp_stacks/rack.c at
// 133344405f532bd54ff3629a77d4c4e062f01fdb (BSD-2-Clause):
// rack_start_hpts_timer, rack_process_timers, rack_stop_all_timers, tcp_addrack,
// rack_ctloutput, rack_hw_tls_change, rack_pru_options, rack_wake_check and
// rack_switch_failed. The upstream BSD-2 notice is retained below.
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
// THIS SOFTWARE IS PROVIDED BY THE AUTHOR AND CONTRIBUTORS ``AS IS'' AND
// ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED.  IN NO EVENT SHALL THE AUTHOR OR CONTRIBUTORS BE LIABLE
// FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
// DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
// OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
// HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
// LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
// OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
// SUCH DAMAGE.

//! RACK HPTS/transport integration callbacks.
//!
//! This module ports the donor's HPTS timer handoff, module registration, ctloutput,
//! hardware-TLS, protocol-option, early-wake and failed-switch glue over the K313 `RackHpts`,
//! K314 `RackOs` and K318 `RackStack` contexts. It is sans-IO and does not touch the stack
//! state machine.

use crate::{Errno, RackHpts, RackOs, RackSockopt, RackStack, RackVtable};

/// FreeBSD `TCPS_CLOSED`.
pub const TCPS_CLOSED: u8 = 0;
/// FreeBSD `TCPS_LISTEN`.
pub const TCPS_LISTEN: u8 = 1;
/// FreeBSD `PRUS_OOB`.
pub const PRUS_OOB: i32 = 0x1;
/// FreeBSD `PACE_TMR_DELACK`.
pub const PACE_TMR_DELACK: u8 = 0x01;
/// FreeBSD `PACE_TMR_RACK`.
pub const PACE_TMR_RACK: u8 = 0x02;
/// FreeBSD `PACE_TMR_TLP`.
pub const PACE_TMR_TLP: u8 = 0x04;
/// FreeBSD `PACE_TMR_RXT`.
pub const PACE_TMR_RXT: u8 = 0x08;
/// FreeBSD `PACE_TMR_PERSIT`.
pub const PACE_TMR_PERSIT: u8 = 0x10;
/// FreeBSD `PACE_TMR_KEEP`.
pub const PACE_TMR_KEEP: u8 = 0x20;
/// FreeBSD `PACE_TMR_MASK`.
pub const PACE_TMR_MASK: u8 =
    PACE_TMR_KEEP | PACE_TMR_PERSIT | PACE_TMR_RXT | PACE_TMR_TLP | PACE_TMR_RACK | PACE_TMR_DELACK;
/// FreeBSD `PACE_PKT_OUTPUT`.
pub const PACE_PKT_OUTPUT: u8 = 0x40;
/// FreeBSD `HPTS_USECS_PER_SLOT`.
pub const HPTS_USECS_PER_SLOT: u64 = 10;
/// The donor's largest requested HPTS timeout (`0x7ffffffe`).
pub const MAX_HPTS_TIMEOUT_US: u32 = 0x7fff_fffe;
/// FreeBSD `TF2_SUPPORTS_MBUFQ`.
pub const TF2_SUPPORTS_MBUFQ: u32 = 0x0001_0000;
/// FreeBSD `TF2_MBUF_ACKCMP`.
pub const TF2_MBUF_ACKCMP: u32 = 0x0000_8000;

/// Inputs for `rack_start_hpts_timer`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HptsStart {
    /// Connection id.
    pub id: u32,
    /// Current time in microseconds.
    pub cts_us: u64,
    /// Pacing delay requested by the caller.
    pub usecs: u32,
    /// Timer bits armed by `rack_timer_start`.
    pub timers: u8,
    /// The timer timeout in microseconds; zero when no transport timer runs.
    pub timer_us: u32,
    /// Delayed-ACK timeout in microseconds, when `TF_DELACK` is set.
    pub delayed_ack_us: u32,
    /// Keepalive timeout in microseconds, used when no timer/pacing is active.
    pub keepalive_us: u32,
    /// `rc_agg_early` carry-over.
    pub early_us: u32,
    /// `rc_agg_delayed` carry-over.
    pub late_us: u32,
    /// `r_use_hpts_min`.
    pub use_hpts_min: bool,
    /// `dgp_on`.
    pub dgp_on: bool,
    /// `get_hpts_min_sleep_time`.
    pub min_sleep_us: u32,
    /// `max_reduction` percentage.
    pub max_reduction: u32,
}

/// The result of `rack_start_hpts_timer`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HptsStartOutcome {
    /// Whether anything was queued.
    pub inserted: bool,
    /// Effective pacing delay after early/late compensation.
    pub usecs: u32,
    /// Transport timer timeout actually selected.
    pub timeout_us: u32,
    /// Time the HPTS entry is due, relative to `cts_us`.
    pub scheduled_us: u64,
    /// `rc_timer_exp` absolute microseconds, or zero.
    pub timer_exp_us: u64,
    /// Effective timer bits (including pacing).
    pub flags: u8,
    /// Early carry-over was consumed.
    pub early_consumed: bool,
    /// Late carry-over was reduced or consumed.
    pub late_compensated: bool,
}

/// Inputs for `rack_process_timers`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimerProcess {
    /// Connection id.
    pub id: u32,
    /// Current time in microseconds.
    pub cts_us: u64,
    /// Called from the HPTS softclock.
    pub hpts_calling: bool,
    /// `rc_on_min_to`.
    pub min_to: bool,
}

/// The donor's `rack_process_timers` result path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimerProcessResult {
    /// No timer bits were armed.
    NoTimers,
    /// A listen socket has no timers.
    Listen,
    /// A pacing wakeup ran before the transport timer.
    EarlyPaced,
    /// A user send saw an early transport timer.
    EarlyUser,
    /// The HPTS callback rearmed for the remaining time.
    EarlyRearm(u64),
    /// The timer fired and selected this action.
    Dispatched(crate::RackTimeout),
    /// The timer fired but no action matched.
    Expired,
}

/// The result of `rack_stop_all_timers`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimerStopOutcome {
    /// A pending persist timer was recorded.
    pub persist_pending: bool,
    /// An HPTS entry was removed.
    pub removed: bool,
    /// Timer bits that were pending.
    pub stopped_flags: u8,
}

/// The module-owned UMA/sysctl state from `tcp_addrack`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RackTransport {
    /// The `rack_map` UMA zone.
    pub map_zone: Option<u32>,
    /// The `rack_pcb` UMA zone.
    pub pcb_zone: Option<u32>,
    /// The sysctl context.
    pub sysctl_ctx: Option<u32>,
    /// The sysctl root node.
    pub sysctl_root: Option<u32>,
    /// `rack_mod_inited`.
    pub initialized: bool,
}

/// `tcp_addrack` module operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddRackMode {
    /// `MOD_LOAD`.
    Load,
    /// `MOD_QUIESCE`.
    Quiesce,
    /// `MOD_UNLOAD`.
    Unload,
}

/// The result of `tcp_addrack`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AddRackOutcome {
    /// Registration succeeded.
    pub registered: bool,
    /// Deregistration succeeded.
    pub unregistered: bool,
    /// The stack was quiesced.
    pub quiesced: bool,
    /// The map UMA zone.
    pub map_zone: Option<u32>,
    /// The PCB UMA zone.
    pub pcb_zone: Option<u32>,
    /// The sysctl context.
    pub sysctl_ctx: Option<u32>,
    /// The sysctl root node.
    pub sysctl_root: Option<u32>,
    /// `EOPNOTSUPP` for an unknown module operation.
    pub error: Option<Errno>,
}

/// The result of `rack_hw_tls_change`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HwTlsOutcome {
    /// The new hardware-TLS flag.
    pub enabled: bool,
    /// `tfb_hwtls_change` calls observed by the K314 transport context.
    pub changes: u64,
}

/// Inputs for `rack_switch_failed`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SwitchCaps {
    /// `r_mbuf_queue`.
    pub mbuf_queue: bool,
    /// `rc_always_pace`.
    pub always_pace: bool,
    /// `r_use_cmp_ack`.
    pub use_cmp_ack: bool,
    /// The connection is established.
    pub established: bool,
    /// `tp->t_in_hpts > IHPTS_NONE`.
    pub in_hpts: bool,
}

/// The result of `rack_switch_failed`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SwitchFailedOutcome {
    /// Whether the HPTS entry was rearmed.
    pub inserted: bool,
    /// Rearm delay in microseconds.
    pub toval_us: u32,
    /// `t_flags2` after the MBUFQ/ACKCMP update.
    pub t_flags2: u32,
}

/// Apply the donor's early/late pacing carry-over to `usecs`.
fn compensate_late(
    usecs: u32,
    late: u32,
    use_hpts_min: bool,
    dgp_on: bool,
    max_reduction: u32,
) -> (u32, u32) {
    let mut usecs = usecs;
    let mut late = late;
    if !use_hpts_min || !dgp_on {
        let slot = u32::try_from(HPTS_USECS_PER_SLOT).unwrap_or(10);
        if late >= usecs {
            if usecs <= slot {
                late = late.saturating_add(slot - usecs);
                usecs = slot;
            } else {
                late -= usecs - slot;
                usecs = slot;
            }
        } else {
            usecs -= late;
            late = 0;
            if usecs < slot {
                late = slot - usecs;
                usecs = slot;
            }
        }
    } else {
        let reduction = usecs.saturating_mul(max_reduction) / 100;
        if reduction >= late {
            usecs -= late;
            late = 0;
        } else {
            usecs -= reduction;
            late -= reduction;
        }
    }
    (usecs, late)
}

/// Build the response fields for `rack_start_hpts_timer`.
#[must_use]
pub fn rack_start_hpts_timer(
    hpts: &mut RackHpts,
    stack: &RackStack,
    request: &HptsStart,
) -> HptsStartOutcome {
    let mut outcome = HptsStartOutcome {
        inserted: false,
        usecs: 0,
        timeout_us: 0,
        scheduled_us: 0,
        timer_exp_us: 0,
        flags: 0,
        early_consumed: false,
        late_compensated: false,
    };
    if stack.tp.t_state == TCPS_CLOSED || stack.tp.t_state == TCPS_LISTEN {
        return outcome;
    }
    if stack.hpts_flags != 0 {
        return outcome;
    }
    let mut usecs = request.usecs;
    if request.early_us > 0 {
        usecs = usecs.saturating_add(request.early_us);
        outcome.early_consumed = true;
    }
    if request.late_us > 0 {
        let (compensated, _late) = compensate_late(
            usecs,
            request.late_us,
            request.use_hpts_min,
            request.dgp_on,
            request.max_reduction,
        );
        usecs = compensated;
        outcome.late_compensated = true;
    }
    if request.use_hpts_min && request.dgp_on && usecs > 0 && request.min_sleep_us > usecs {
        usecs = request.min_sleep_us;
    }
    let mut flags = request.timers;
    let mut timeout = request.timer_us;
    if flags & PACE_TMR_DELACK != 0
        && request.delayed_ack_us > 0
        && (timeout == 0 || request.delayed_ack_us < timeout)
    {
        timeout = request.delayed_ack_us;
    } else {
        flags &= !PACE_TMR_DELACK;
    }
    if timeout == 0 && usecs == 0 && request.keepalive_us > 0 {
        timeout = request.keepalive_us;
        flags |= PACE_TMR_KEEP;
    }
    if timeout > MAX_HPTS_TIMEOUT_US {
        timeout = MAX_HPTS_TIMEOUT_US;
    }
    if timeout > 0 {
        let due = hpts.rack_timer_start(request.id, flags, u64::from(timeout));
        outcome.inserted = due != 0;
        outcome.scheduled_us = due.saturating_sub(request.cts_us);
        outcome.timer_exp_us = request.cts_us.saturating_add(u64::from(timeout));
    } else if usecs > 0 {
        hpts.tcp_hpts_insert(request.id, u64::from(usecs));
        flags |= PACE_PKT_OUTPUT;
        outcome.inserted = true;
        outcome.scheduled_us = u64::from(usecs);
    }
    outcome.usecs = usecs;
    outcome.timeout_us = timeout;
    outcome.flags = flags;
    outcome
}

/// Apply the donor's HPTS timer dispatch.
#[must_use]
pub fn rack_process_timers(
    hpts: &mut RackHpts,
    stack: &mut RackStack,
    request: &TimerProcess,
) -> TimerProcessResult {
    let timers = u8::try_from(stack.hpts_flags & u32::from(PACE_TMR_MASK)).unwrap_or(0);
    if timers == 0 {
        return TimerProcessResult::NoTimers;
    }
    if stack.tp.t_state == TCPS_LISTEN {
        if stack.hpts_flags & u32::from(PACE_PKT_OUTPUT) != 0 {
            return TimerProcessResult::NoTimers;
        }
        return TimerProcessResult::Listen;
    }
    let timer_exp = u64::try_from(stack.timer_exp).unwrap_or(0);
    if (timers & PACE_TMR_RACK == 0 || !request.min_to) && request.cts_us < timer_exp {
        if stack.hpts_flags & u32::from(PACE_PKT_OUTPUT) != 0 {
            return TimerProcessResult::EarlyPaced;
        }
        if !request.hpts_calling {
            return TimerProcessResult::EarlyUser;
        }
        let left = timer_exp.saturating_sub(request.cts_us);
        hpts.tcp_hpts_insert(request.id, left);
        return TimerProcessResult::EarlyRearm(left);
    }
    stack.tmr_stopped = 0;
    stack.hpts_flags &= !u32::from(PACE_TMR_MASK);
    match crate::rack_timeout_action(timers) {
        Some(action) => TimerProcessResult::Dispatched(action),
        None => TimerProcessResult::Expired,
    }
}

/// Apply the donor's `rack_stop_all_timers`.
#[must_use]
pub fn rack_stop_all_timers(
    hpts: &mut RackHpts,
    stack: &mut RackStack,
    id: u32,
) -> TimerStopOutcome {
    let stopped_flags = u8::try_from(stack.hpts_flags & u32::from(PACE_TMR_MASK)).unwrap_or(0);
    let persist_pending = stopped_flags & PACE_TMR_PERSIT != 0;
    if persist_pending {
        stack.in_persist = true;
    }
    let removed_flags = hpts.rack_stop_all_timers(id);
    stack.hpts_flags &= !u32::from(PACE_TMR_MASK);
    TimerStopOutcome {
        persist_pending,
        removed: removed_flags != 0 || stopped_flags != 0,
        stopped_flags,
    }
}

/// Apply the donor's `tcp_addrack` module operation.
#[must_use]
pub fn tcp_addrack(
    transport: &mut RackTransport,
    os: &mut RackOs,
    stack: &mut RackStack,
    mode: AddRackMode,
) -> AddRackOutcome {
    match mode {
        AddRackMode::Load => {
            let map = os.uma_zcreate("rack_map", 128);
            let pcb = os.uma_zcreate("rack_pcb", 256);
            let ctx = os.sysctl_ctx_init();
            let root = os.sysctl_add_node(None, "rack", ctx);
            transport.map_zone = Some(map);
            transport.pcb_zone = Some(pcb);
            transport.sysctl_ctx = Some(ctx);
            transport.sysctl_root = Some(root);
            transport.initialized = true;
            stack.zone = Some(pcb);
            stack.fb_item = os.uma_zalloc(pcb);
            stack.state = crate::RackPortState::Running;
            AddRackOutcome {
                registered: true,
                unregistered: false,
                quiesced: false,
                map_zone: Some(map),
                pcb_zone: Some(pcb),
                sysctl_ctx: Some(ctx),
                sysctl_root: Some(root),
                error: None,
            }
        }
        AddRackMode::Quiesce => AddRackOutcome {
            registered: false,
            unregistered: false,
            quiesced: true,
            map_zone: transport.map_zone,
            pcb_zone: transport.pcb_zone,
            sysctl_ctx: transport.sysctl_ctx,
            sysctl_root: transport.sysctl_root,
            error: None,
        },
        AddRackMode::Unload => {
            if let Some(zone) = transport.pcb_zone {
                let _ = os.uma_zdestroy(zone);
            }
            if let Some(zone) = transport.map_zone {
                let _ = os.uma_zdestroy(zone);
            }
            if let Some(ctx) = transport.sysctl_ctx {
                let _ = os.sysctl_ctx_free(ctx);
            }
            transport.map_zone = None;
            transport.pcb_zone = None;
            transport.sysctl_ctx = None;
            transport.sysctl_root = None;
            transport.initialized = false;
            stack.zone = None;
            stack.fb_item = None;
            stack.state = crate::RackPortState::Fini;
            AddRackOutcome {
                registered: false,
                unregistered: true,
                quiesced: false,
                map_zone: None,
                pcb_zone: None,
                sysctl_ctx: None,
                sysctl_root: None,
                error: None,
            }
        }
    }
}

/// Apply the donor's `rack_ctloutput` direction dispatch.
///
/// # Errors
///
/// Returns [`Errno::EINVAL`] for an invalid direction and forwards the socket-option error.
pub fn rack_ctloutput(stack: &mut RackStack, option: &RackSockopt) -> Result<Option<u32>, Errno> {
    stack.tfb_tcp_ctloutput(option)
}

/// Apply the donor's `rack_hw_tls_change`.
#[must_use]
pub fn rack_hw_tls_change(stack: &mut RackStack, enable: bool) -> HwTlsOutcome {
    stack.hw_tls = enable;
    stack.os.stacks.tfb_hwtls_change(enable);
    HwTlsOutcome {
        enabled: stack.hw_tls,
        changes: stack.os.stacks.hwtls_changes,
    }
}

/// Apply the donor's `rack_pru_options`.
///
/// # Errors
///
/// Returns [`Errno::EOPNOTSUPP`] for out-of-band data.
pub fn rack_pru_options(flags: i32) -> Result<(), Errno> {
    if flags & PRUS_OOB != 0 {
        Err(Errno::EOPNOTSUPP)
    } else {
        Ok(())
    }
}

/// Apply the donor's `rack_wake_check`.
#[must_use]
pub fn rack_wake_check(stack: &RackStack) -> bool {
    if stack.hpts_flags == 0 {
        return false;
    }
    let cts = stack.os.now_us;
    if stack.hpts_flags & u32::from(PACE_PKT_OUTPUT) != 0 {
        cts >= stack.last_output_to
    } else if stack.hpts_flags & u32::from(PACE_TMR_MASK) != 0 {
        cts >= stack.timer_exp
    } else {
        false
    }
}

/// Apply the donor's `rack_switch_failed`.
#[must_use]
pub fn rack_switch_failed(
    hpts: &mut RackHpts,
    stack: &mut RackStack,
    id: u32,
    caps: &SwitchCaps,
) -> SwitchFailedOutcome {
    let mut t_flags2 = stack.t_flags2;
    if caps.mbuf_queue || caps.always_pace || caps.use_cmp_ack {
        t_flags2 |= TF2_SUPPORTS_MBUFQ;
    } else {
        t_flags2 &= !TF2_SUPPORTS_MBUFQ;
    }
    if caps.use_cmp_ack && caps.established {
        t_flags2 |= TF2_MBUF_ACKCMP;
    }
    stack.t_flags2 = t_flags2;
    if caps.in_hpts {
        return SwitchFailedOutcome {
            inserted: false,
            toval_us: 0,
            t_flags2,
        };
    }
    let cts = stack.os.now_us;
    let slot = u32::try_from(HPTS_USECS_PER_SLOT).unwrap_or(10);
    let toval = if stack.hpts_flags & u32::from(PACE_PKT_OUTPUT) != 0 {
        if stack.last_output_to > cts {
            u32::try_from(stack.last_output_to - cts).unwrap_or(slot)
        } else {
            slot
        }
    } else if stack.hpts_flags & u32::from(PACE_TMR_MASK) != 0 {
        if stack.timer_exp > cts {
            u32::try_from(stack.timer_exp - cts).unwrap_or(slot)
        } else {
            slot
        }
    } else {
        slot
    };
    hpts.tcp_hpts_insert(id, u64::from(toval));
    SwitchFailedOutcome {
        inserted: true,
        toval_us: toval,
        t_flags2,
    }
}

#[cfg(test)]
#[path = "rack_transport_ut.rs"]
mod tests;
