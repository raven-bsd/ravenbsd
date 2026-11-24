// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! One matrix test per merged transport path, model comparison and K315 union row.

use rack_coverage::{Function, check, parse_matrix};
use rack_tlp_model::model::{Config as TlpConfig, RackModel};
use rack_vtable_model::model::{
    EOPNOTSUPP as MODEL_EOPNOTSUPP, RackOption, SockoptDir as ModelSockoptDir,
    TcpState as ModelTcpState, VtableModel,
};
use raven_tcpip::{
    AddRackMode, Errno, HptsStart, PACE_PKT_OUTPUT, PACE_TMR_PERSIT, PACE_TMR_TLP, PRUS_OOB,
    RackHpts, RackOs, RackSockopt, RackStack, RackTimeout, RackTransport, RackVtable, SockoptDir,
    TCP_RACK_PACE_MIN_SEG, TCPS_ESTABLISHED, TimerProcess, TimerProcessResult, rack_ctloutput,
    rack_hw_tls_change, rack_process_timers, rack_pru_options, rack_start_hpts_timer,
    rack_stop_all_timers, rack_switch_failed, rack_timeout_action, rack_wake_check, tcp_addrack,
};

fn running_stack() -> RackStack {
    let mut stack = RackStack::new();
    assert_eq!(stack.tfb_tcp_fb_init(), Ok(()));
    stack.tp.t_state = TCPS_ESTABLISHED;
    stack
}

fn hpts() -> RackHpts {
    let mut hpts = RackHpts::new(0);
    hpts.tcp_hpts_init(1);
    hpts
}

#[test]
fn rack_start_hpts_timer_matches_hpts_and_the_tlp_pto() {
    let mut hpts = hpts();
    let stack = running_stack();
    let model = RackModel::new(TlpConfig::default(), 10, 4);
    let request = HptsStart {
        id: 1,
        cts_us: 0,
        usecs: 0,
        timers: PACE_TMR_TLP,
        timer_us: model.pto(),
        delayed_ack_us: 0,
        keepalive_us: 0,
        early_us: 0,
        late_us: 0,
        use_hpts_min: false,
        dgp_on: false,
        min_sleep_us: 0,
        max_reduction: 0,
    };
    let outcome = rack_start_hpts_timer(&mut hpts, &stack, &request);
    assert!(outcome.inserted);
    assert_eq!(outcome.timeout_us, model.pto());
    assert_eq!(
        outcome.scheduled_us,
        raven_tcpip::hpts_usec_to_slots(u64::from(model.pto())) * 10
    );
    assert_eq!(outcome.flags, PACE_TMR_TLP);

    let mut closed = running_stack();
    closed.tp.t_state = raven_tcpip::TCPS_CLOSED;
    let none = rack_start_hpts_timer(&mut hpts, &closed, &request);
    assert!(!none.inserted);
}

#[test]
fn rack_process_timers_dispatches_the_k320_tlp_action() {
    let mut hpts = hpts();
    let mut stack = running_stack();
    stack.hpts_flags = u32::from(PACE_TMR_TLP);
    stack.timer_exp = 142;
    let request = TimerProcess {
        id: 1,
        cts_us: 200,
        hpts_calling: true,
        min_to: false,
    };
    let result = rack_process_timers(&mut hpts, &mut stack, &request);
    assert_eq!(result, TimerProcessResult::Dispatched(RackTimeout::Tlp));
    assert_eq!(rack_timeout_action(PACE_TMR_TLP), Some(RackTimeout::Tlp));
    assert_eq!(stack.hpts_flags & u32::from(PACE_TMR_TLP), 0);
}

#[test]
fn rack_stop_all_timers_records_pending_persist() {
    let mut hpts = hpts();
    let mut stack = running_stack();
    stack.hpts_flags = u32::from(PACE_TMR_PERSIT);
    let _ = hpts.rack_timer_start(1, PACE_TMR_PERSIT, 1_000);
    let outcome = rack_stop_all_timers(&mut hpts, &mut stack, 1);
    assert!(outcome.persist_pending);
    assert_eq!(outcome.stopped_flags, PACE_TMR_PERSIT);
    assert!(stack.in_persist);
}

#[test]
fn tcp_addrack_lifecycle_matches_the_k327_vtable_model() {
    let mut transport = RackTransport::default();
    let mut os = RackOs::new();
    let mut stack = RackStack::new();
    let load = tcp_addrack(&mut transport, &mut os, &mut stack, AddRackMode::Load);
    assert!(load.registered);
    assert!(load.map_zone.is_some());
    assert!(load.pcb_zone.is_some());
    assert_eq!(stack.state, raven_tcpip::RackPortState::Running);

    let mut model = VtableModel::new();
    assert_eq!(model.init(), 0);
    assert!(model.initialized);
    assert!(model.fb_ptr);

    let unload = tcp_addrack(&mut transport, &mut os, &mut stack, AddRackMode::Unload);
    assert!(unload.unregistered);
    assert!(unload.map_zone.is_none());
    assert_eq!(stack.state, raven_tcpip::RackPortState::Fini);
    model.fini(true);
    assert!(model.purged);
    assert!(!model.fb_ptr);
}

#[test]
fn rack_ctloutput_matches_the_k327_vtable_model() {
    let mut stack = running_stack();
    let set = RackSockopt {
        dir: SockoptDir::Set,
        level: raven_tcpip::IPPROTO_TCP,
        name: TCP_RACK_PACE_MIN_SEG,
        value: 800,
    };
    assert_eq!(rack_ctloutput(&mut stack, &set), Ok(None));
    let get = RackSockopt {
        dir: SockoptDir::Get,
        ..set
    };
    assert_eq!(rack_ctloutput(&mut stack, &get), Ok(Some(800)));

    let mut model = VtableModel::new();
    assert_eq!(model.init(), 0);
    let set_reply = model.ctloutput(ModelSockoptDir::Set, RackOption::PaceMinSeg, 800);
    assert_eq!(set_reply.error, 0);
    assert_eq!(set_reply.value, None);
    let get_reply = model.ctloutput(ModelSockoptDir::Get, RackOption::PaceMinSeg, 0);
    assert_eq!(get_reply.error, 0);
    assert_eq!(get_reply.value, Some(800));
}

#[test]
fn rack_hw_tls_change_matches_the_k327_vtable_model() {
    let mut stack = running_stack();
    let outcome = rack_hw_tls_change(&mut stack, true);
    assert!(outcome.enabled);
    assert_eq!(outcome.changes, 1);
    let mut model = VtableModel::new();
    assert_eq!(model.init(), 0);
    model.hwtls_change(true);
    assert!(model.hw_tls);
    assert!(stack.hw_tls);
}

#[test]
fn rack_pru_options_matches_the_k327_vtable_model() {
    assert_eq!(rack_pru_options(0), Ok(()));
    assert_eq!(rack_pru_options(PRUS_OOB), Err(Errno::EOPNOTSUPP));
    let mut model = VtableModel::new();
    assert_eq!(model.init(), 0);
    assert_eq!(model.pru_options(0), 0);
    assert_eq!(
        model.pru_options(u32::try_from(PRUS_OOB).unwrap()),
        MODEL_EOPNOTSUPP
    );
}

#[test]
fn rack_wake_check_matches_the_k327_vtable_model() {
    let mut stack = running_stack();
    stack.hpts_flags = PACE_PKT_OUTPUT;
    stack.last_output_to = 100;
    stack.os.now_us = 100;
    assert!(rack_wake_check(&stack));

    let mut model = VtableModel::new();
    assert_eq!(model.init(), 0);
    model.hpts_flags = u8::try_from(PACE_PKT_OUTPUT).unwrap();
    model.last_output_to_us = 100;
    model.now_us = 100;
    assert_eq!(rack_wake_check(&stack), model.early_wake_check());

    stack.hpts_flags = 0;
    model.hpts_flags = 0;
    assert_eq!(rack_wake_check(&stack), model.early_wake_check());
}

#[test]
fn rack_switch_failed_matches_the_k327_vtable_model() {
    let mut hpts = hpts();
    let mut stack = running_stack();
    stack.t_flags2 = 0;
    stack.hpts_flags = 0;
    stack.os.now_us = 0;
    let caps = raven_tcpip::SwitchCaps {
        mbuf_queue: true,
        always_pace: true,
        use_cmp_ack: true,
        established: true,
        in_hpts: false,
    };
    let outcome = rack_switch_failed(&mut hpts, &mut stack, 1, &caps);
    assert!(outcome.inserted);
    assert_ne!(outcome.t_flags2 & raven_tcpip::TF2_SUPPORTS_MBUFQ, 0);
    assert_ne!(outcome.t_flags2 & raven_tcpip::TF2_MBUF_ACKCMP, 0);

    let mut model = VtableModel::new();
    assert_eq!(model.init(), 0);
    model.queue_ready = true;
    model.pacing = true;
    model.state = ModelTcpState::Established;
    model.switch_failed();
    assert_eq!(outcome.t_flags2, model.flags2);
    assert!(model.in_hpts);
}

const MATRIX: &str = "\
| Function | Rust | Test | Mutation | Status |
|---|---|---|---|---|
| `rack_start_hpts_timer` | `rack_transport::rack_start_hpts_timer` | `rack_start_hpts_timer_matches_hpts_and_the_tlp_pto` | closed state no insert | PASS |
| `rack_process_timers` | `rack_transport::rack_process_timers` | `rack_process_timers_dispatches_the_k320_tlp_action` | no-timer path | PASS |
| `rack_stop_all_timers` | `rack_transport::rack_stop_all_timers` | `rack_stop_all_timers_records_pending_persist` | persist flag removed | PASS |
| `tcp_addrack` | `rack_transport::tcp_addrack` | `tcp_addrack_lifecycle_matches_the_k327_vtable_model` | unload leaves zone | PASS |
| `rack_ctloutput` | `rack_transport::rack_ctloutput` | `rack_ctloutput_matches_the_k327_vtable_model` | wrong option value | PASS |
| `rack_hw_tls_change` | `rack_transport::rack_hw_tls_change` | `rack_hw_tls_change_matches_the_k327_vtable_model` | flag not set | PASS |
| `rack_pru_options` | `rack_transport::rack_pru_options` | `rack_pru_options_matches_the_k327_vtable_model` | OOB accepted | PASS |
| `rack_wake_check` | `rack_transport::rack_wake_check` | `rack_wake_check_matches_the_k327_vtable_model` | flags ignored | PASS |
| `rack_switch_failed` | `rack_transport::rack_switch_failed` | `rack_switch_failed_matches_the_k327_vtable_model` | MBUFQ not set | PASS |
";

fn donor() -> Vec<Function> {
    [
        (6444, "rack_start_hpts_timer"),
        (7868, "rack_process_timers"),
        (8031, "rack_stop_all_timers"),
        (24723, "tcp_addrack"),
        (24689, "rack_ctloutput"),
        (24028, "rack_hw_tls_change"),
        (24041, "rack_pru_options"),
        (24049, "rack_wake_check"),
        (14258, "rack_switch_failed"),
    ]
    .into_iter()
    .map(|(line, name)| Function {
        line,
        name: name.to_owned(),
    })
    .collect()
}

#[test]
fn k315_union_row_is_complete_and_rejects_negative_fixtures() {
    let donor = donor();
    let rows = parse_matrix(MATRIX);
    let report = check(&donor, &rows);
    assert!(report.is_ok(), "{:?}", report.errors());
    assert_eq!(report.donor_count, 9);
    assert_eq!(report.row_count, 9);

    let missing: String = MATRIX
        .lines()
        .filter(|line| !line.contains("rack_pru_options"))
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(
        check(&donor, &parse_matrix(&missing)).missing,
        ["rack_pru_options"]
    );

    let pru = MATRIX
        .lines()
        .find(|line| line.contains("rack_pru_options"))
        .expect("pru row");
    let duplicate = MATRIX.replace(
        "| `rack_wake_check`",
        &format!("{pru}\n| `rack_wake_check`"),
    );
    let duplicate = parse_matrix(&duplicate);
    assert_eq!(check(&donor, &duplicate).duplicates, ["rack_pru_options"]);

    let blank = MATRIX.replace("`rack_wake_check_matches_the_k327_vtable_model`", "`GAP`");
    assert_eq!(
        check(&donor, &parse_matrix(&blank)).empty_test,
        ["rack_wake_check"]
    );

    let blocked = MATRIX.replace(
        "| `rack_switch_failed` | `rack_transport::rack_switch_failed` | `rack_switch_failed_matches_the_k327_vtable_model` | MBUFQ not set | PASS |",
        "| `rack_switch_failed` | `rack_transport::rack_switch_failed` | `rack_switch_failed_matches_the_k327_vtable_model` | MBUFQ not set | BLOCKED |",
    );
    assert_eq!(
        check(&donor, &parse_matrix(&blocked)).not_pass,
        ["rack_switch_failed"]
    );
}
