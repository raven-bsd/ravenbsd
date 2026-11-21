// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Named tests and negative checks for every K329 donor function.

use super::*;

macro_rules! donor_bool_test {
    ($name:ident, $kind:expr, $body:expr) => {
        #[test]
        fn $name() {
            let mut log = RackLog::new();
            let before = log.event_count();
            let normal: bool = $body(&mut log);
            assert!(normal, concat!(stringify!($name), ": normal call"));
            assert_eq!(
                log.entries().last().expect("event").kind,
                $kind,
                concat!(stringify!($name), ": event kind")
            );
            assert_eq!(log.event_count(), before + 1);
            log.mutated = true;
            let mutated: bool = $body(&mut log);
            assert!(!mutated, concat!(stringify!($name), ": mutation"));
            assert_eq!(log.event_count(), before + 1, "mutation added an event");
            log.mutated = false;
        }
    };
}

donor_bool_test!(donor_rack_log_gpset, KIND_GPSET, |l: &mut RackLog| l
    .rack_log_gpset(9000, 8000, 9500, 7, 2, 7000));
donor_bool_test!(
    donor_rack_log_hybrid_bw,
    KIND_HYBRID_BW,
    |l: &mut RackLog| l.rack_log_hybrid_bw(9000, 1200, 20, 4096, 3, 7, 9)
);
donor_bool_test!(
    donor_rack_log_hybrid_sends,
    KIND_HYBRID_SENDS,
    |l: &mut RackLog| l.rack_log_hybrid_sends(1000, 2000, 4)
);
donor_bool_test!(donor_rack_log_dsack_event, KIND_DSACK, |l: &mut RackLog| l
    .rack_log_dsack_event(4, 1000, 2000, 3000));
donor_bool_test!(
    donor_rack_log_hdwr_pacing,
    KIND_HDWR_PACING,
    |l: &mut RackLog| l.rack_log_hdwr_pacing(10_000, 9_000, 8, 0, 2)
);
donor_bool_test!(
    donor_rack_log_retran_reason,
    KIND_RETRAN_REASON,
    |l: &mut RackLog| l.rack_log_retran_reason(4000, 1200, 400, 3)
);
donor_bool_test!(donor_rack_log_to_start, KIND_TO_START, |l: &mut RackLog| l
    .rack_log_to_start(1000, 2000, 30, 1));
donor_bool_test!(donor_rack_log_to_event, KIND_TO_EVENT, |l: &mut RackLog| l
    .rack_log_to_event(2, 5000));
donor_bool_test!(donor_rack_log_map_chg, KIND_MAP_CHG, |l: &mut RackLog| l
    .rack_log_map_chg(1000, 2000, 3000, 1, 4000, 12));
donor_bool_test!(donor_rack_log_rtt_upd, KIND_RTT_UPD, |l: &mut RackLog| l
    .rack_log_rtt_upd(120, 1000, 5000, 1));
donor_bool_test!(
    donor_rack_log_rtt_sample,
    KIND_RTT_SAMPLE,
    |l: &mut RackLog| l.rack_log_rtt_sample(132)
);
donor_bool_test!(
    donor_rack_log_rtt_sample_calc,
    KIND_RTT_SAMPLE_CALC,
    |l: &mut RackLog| l.rack_log_rtt_sample_calc(130, 1000, 1130, 2)
);
donor_bool_test!(
    donor_rack_log_rtt_sendmap,
    KIND_RTT_SENDMAP,
    |l: &mut RackLog| l.rack_log_rtt_sendmap(3, 0x1234_5678, 0x9abc)
);
donor_bool_test!(
    donor_rack_log_progress_event,
    KIND_PROGRESS,
    |l: &mut RackLog| l.rack_log_progress_event(10, 3, 44, 9000, 500)
);
donor_bool_test!(
    donor_rack_log_type_bbrsnd,
    KIND_BBRSND,
    |l: &mut RackLog| l.rack_log_type_bbrsnd(1460, 30, 1000, 55)
);
donor_bool_test!(
    donor_rack_log_doseg_done,
    KIND_DOSEG_DONE,
    |l: &mut RackLog| l.rack_log_doseg_done(1000, 2, 1, 3, 4)
);
donor_bool_test!(
    donor_rack_log_type_pacing_sizes,
    KIND_PACING_SIZES,
    |l: &mut RackLog| l.rack_log_type_pacing_sizes(10, 20, 30, 4)
);
donor_bool_test!(
    donor_rack_log_type_just_return,
    KIND_JUST_RETURN,
    |l: &mut RackLog| l.rack_log_type_just_return(1000, 0, 20, 1, 4, 12000)
);
donor_bool_test!(
    donor_rack_log_to_cancel,
    KIND_TO_CANCEL,
    |l: &mut RackLog| l.rack_log_to_cancel(1, 44, 1000, 0x20)
);
donor_bool_test!(
    donor_rack_log_alt_to_to_cancel,
    KIND_ALT_TO_CANCEL,
    |l: &mut RackLog| l.rack_log_alt_to_to_cancel(1, 2, 3, 4, 5, 6, 7, 8)
);
donor_bool_test!(
    donor_rack_log_to_processing,
    KIND_TO_PROCESSING,
    |l: &mut RackLog| l.rack_log_to_processing(1000, 1, 3)
);
donor_bool_test!(donor_rack_log_to_prr, KIND_TO_PRR, |l: &mut RackLog| l
    .rack_log_to_prr(7, 12000, 55));
donor_bool_test!(donor_rack_log_timely, KIND_TIMELY, |l: &mut RackLog| l
    .rack_log_timely(1, 1000, 800, 1200, 7, 2));
donor_bool_test!(
    donor_rack_log_rtt_shrinks,
    KIND_RTT_SHRINKS,
    |l: &mut RackLog| l.rack_log_rtt_shrinks(1000, 120, 9, 1)
);
donor_bool_test!(donor_rack_log_gp_calc, KIND_GP_CALC, |l: &mut RackLog| l
    .rack_log_gp_calc(10, 5, 100, 2000, 3000, 1, 9));
donor_bool_test!(
    donor_rack_log_hpts_diag,
    KIND_HPTS_DIAG,
    |l: &mut RackLog| l.rack_log_hpts_diag(1000, 1, 2, 3)
);
donor_bool_test!(donor_rack_log_wakeup, KIND_WAKEUP, |l: &mut RackLog| l
    .rack_log_wakeup(1460, 1));
donor_bool_test!(donor_rack_log_output, KIND_OUTPUT, |l: &mut RackLog| l
    .rack_log_output(1460, 9000, 0x18, 0, 1000, 0x2, 0, 1460));
donor_bool_test!(
    donor_rack_log_sack_passed,
    KIND_SACK_PASSED,
    |l: &mut RackLog| l.rack_log_sack_passed(5000, 1000, 55)
);
donor_bool_test!(donor_rack_log_hybrid, KIND_HYBRID, |l: &mut RackLog| l
    .rack_log_hybrid(9000, 2, 8, 0));
donor_bool_test!(donor_rack_log_collapse, KIND_COLLAPSE, |l: &mut RackLog| l
    .rack_log_collapse(1, 2, 3, 4, 1, 0x10));
donor_bool_test!(
    donor_rack_log_hystart_event,
    KIND_HYSTART,
    |l: &mut RackLog| l.rack_log_hystart_event(9000, 20)
);
donor_bool_test!(donor_rack_log_chg_info, KIND_CHG_INFO, |l: &mut RackLog| l
    .rack_log_chg_info(1, 2, 3, 4));
donor_bool_test!(
    donor_rack_log_input_packet,
    KIND_INPUT_PACKET,
    |l: &mut RackLog| l.rack_log_input_packet(1, 9000, 8000)
);
donor_bool_test!(donor_rack_log_pcm, KIND_PCM, |l: &mut RackLog| l
    .rack_log_pcm(2, 1, 2, 3));
donor_bool_test!(
    donor_rack_log_pacing_delay_calc,
    KIND_PACING_DELAY,
    |l: &mut RackLog| l.rack_log_pacing_delay_calc(1460, 30, 1000, 2000, 3000, 1, 9, 2)
);
donor_bool_test!(donor_rack_log_fsb, KIND_FSB, |l: &mut RackLog| l
    .rack_log_fsb(0x1, 20, 1460, 1440, 0, 0, 20, 9, 2));

#[test]
fn donor_sysctl_rack_clear() {
    let mut log = RackLog::new();
    log.add_counter(Counter::TotalBytes, 5);
    assert_eq!(log.sysctl_rack_clear(1).unwrap(), 5);
    assert_eq!(log.counter(Counter::TotalBytes), 0);
    assert_eq!(log.entries().last().unwrap().kind, KIND_SYSCTL_CLEAR);
    assert!(log.sysctl_rack_clear(9).is_err());
    let before = log.event_count();
    log.mutated = true;
    assert_eq!(log.sysctl_rack_clear(1).unwrap(), 0);
    assert_eq!(log.event_count(), before);
}

#[test]
fn donor_sysctl_rack_wma_divisor() {
    let mut log = RackLog::new();
    assert_eq!(log.sysctl_rack_wma_divisor(Some(8)).unwrap(), 4);
    assert_eq!(log.sysctl_rack_wma_divisor(None).unwrap(), 8);
    assert!(log.sysctl_rack_wma_divisor(Some(0)).is_err());
    let before = log.event_count();
    log.mutated = true;
    assert_eq!(log.sysctl_rack_wma_divisor(Some(9)).unwrap(), 8);
    assert_eq!(log.event_count(), before);
}

#[test]
fn donor_rack_init_sysctls() {
    let mut log = RackLog::new();
    let nodes = log.rack_init_sysctls();
    assert_eq!(nodes.len(), 4);
    assert_eq!(log.entries().last().unwrap().kind, KIND_INIT_SYSCTLS);
    let before = log.event_count();
    log.mutated = true;
    assert_eq!(log.rack_init_sysctls(), nodes);
    assert_eq!(log.event_count(), before);
}

#[test]
fn donor_rack_counter_destroy() {
    let mut log = RackLog::new();
    log.rack_init_sysctls();
    log.add_counter(Counter::TotalBytes, 9);
    assert_eq!(log.rack_counter_destroy(), 4);
    assert_eq!(log.counter(Counter::TotalBytes), 0);
    assert_eq!(log.entries().last().unwrap().kind, KIND_COUNTER_DESTROY);
    let before = log.event_count();
    log.mutated = true;
    log.rack_counter_destroy();
    assert_eq!(log.event_count(), before);
}

#[test]
fn donor_rack_log_ack() {
    let mut log = RackLog::new();
    let mut dsack = false;
    let mut sacks = 0;
    assert!(log.rack_log_ack(9000, 1, 1, &mut dsack, &mut sacks));
    assert!(dsack);
    assert_eq!(sacks, 1);
    assert_eq!(log.entries().last().unwrap().kind, KIND_ACK);
    let before = log.event_count();
    log.mutated = true;
    let mut dsack = false;
    let mut sacks = 0;
    assert!(!log.rack_log_ack(9000, 1, 1, &mut dsack, &mut sacks));
    assert!(!dsack);
    assert_eq!(sacks, 0);
    assert_eq!(log.event_count(), before);
}

#[test]
fn donor_rack_log_queue_level() {
    let mut log = RackLog::new();
    assert_eq!(log.rack_log_queue_level(2000, 1000), (2_000_000, 2000));
    assert_eq!(log.entries().last().unwrap().kind, KIND_QUEUE_LEVEL);
    let before = log.event_count();
    log.mutated = true;
    assert_eq!(log.rack_log_queue_level(2000, 1000), (2_000_000, 2000));
    assert_eq!(log.event_count(), before);
}
