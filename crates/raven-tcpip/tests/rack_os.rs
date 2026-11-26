// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Public `RackOs` context checks (K314).

use raven_tcpip::{PacingMethod, RackOs, RackOsError};

#[test]
fn a_rack_like_send_sequence_walks_mbuf_sockbuf_and_ktls() {
    let mut os = RackOs::new();
    let header = os.m_gethdr().expect("header");
    assert!(os.mbufs.write(header, 0, b"rack-payload"));
    assert!(os.mbufs.set_len(header, 12));
    os.sbappendstream_locked(Some(header)).expect("append");
    assert_eq!(os.sbavail(), 12);
    assert_eq!(os.sbused(), 12);
    assert!(os.sbspace() > 0);
    assert_eq!(os.sbsndmbuf(5), Some((header, 5)));
    let (id, _) = os.sbsndptr_noadv(5).expect("pointer");
    assert!(os.sbsndptr_adv(id, 3));

    let cut = os.sbcut_locked(4).expect("cut");
    os.m_freem(Some(cut));
    assert_eq!(os.sbused(), 8);
    let tail = os.sbcut_locked(8);
    os.m_freem(tail);
    assert_eq!(os.sbused(), 0);
    assert_eq!(os.m_memcnt(), 0);

    let tls = os.ktls_enable(false, 16, 1_000);
    assert_eq!(os.tcp_estimate_tls_overhead(tls, 2_000), 32);
    assert_eq!(os.ktls_send(tls, 2_000), Ok(2_032));
    assert!(os.ktls_ready(tls));
}

#[test]
fn uma_and_sysctl_surfaces_are_usable_from_the_context() {
    let mut os = RackOs::new();
    let zone = os.uma_zcreate("rack_sendmap", 64);
    assert!(os.uma_zone_set_max(zone, 1));
    let item = os.uma_zalloc(zone).expect("item");
    assert!(os.uma_zalloc(zone).is_none());
    assert_eq!(os.uma_zone_get_cur(zone), Some(1));
    assert!(os.uma_zfree(zone, item));
    assert!(os.uma_zdestroy(zone));

    let ctx = os.sysctl_ctx_init();
    let root = os.sysctl_add_node(None, "rack", ctx);
    let divisor = os.sysctl_add_u32(Some(root), "divisor", 250, ctx);
    assert_eq!(os.sysctl_handle_32(divisor), Some(250));
    assert!(os.sysctl_set_u32(divisor, 400));
    assert_eq!(os.sysctl_handle_32(divisor), Some(400));
    assert_eq!(os.sysctl_ctx_free(ctx), 2);
}

#[test]
fn btf_log_ktls_pacing_and_tcpcb_are_available() {
    let mut os = RackOs::new();
    os.log.set_enabled(true);
    assert!(os.tcp_bblogging_on());
    assert!(os.tcp_log_event(1, [1, 2, 3, 4], 0));
    assert_eq!(os.log.tcp_log_buffer(), 1);

    let hardware = os.ktls_enable(true, 16, 1_000);
    assert_eq!(os.ktls_send(hardware, 10), Err(RackOsError::TlsNoTag));
    assert!(os.ktls_set_snd_tag(hardware, 3));
    assert_eq!(os.ktls_send(hardware, 10), Ok(26));
    assert!(os.ktls_release_snd_tag(hardware));

    os.pacing.max_segs = 4;
    assert_eq!(os.rack_get_pacing_len(100_000, 1_000), 4_000);
    assert!(os.rack_set_cc_pacing());
    assert!(os.rack_undo_cc_pacing());
    os.pacing.method = PacingMethod::Dgp;
    assert!(os.rack_remove_pacing());
    let deadline = os.tcp_hpts_insert(250);
    assert!(deadline > os.now_us);
    os.advance_us(250);
    assert!(os.tcp_hpts_due());
    assert!(os.tcp_hpts_remove());

    os.tcpcb.t_srtt = 800;
    os.tcpcb.t_rttvar = 100;
    assert_eq!(os.tcpcb.rack_rexmtval(), 200);
}

#[test]
fn m_adj_returns_the_new_head_and_locks_are_checked() {
    let mut os = RackOs::new();
    let first = os.m_get().expect("first");
    let second = os.m_get().expect("second");
    assert!(os.mbufs.write(first, 0, b"abcdef"));
    assert!(os.mbufs.set_len(first, 6));
    assert!(os.mbufs.write(second, 0, b"ghij"));
    assert!(os.mbufs.set_len(second, 4));
    os.mbufs.mbuf_mut(first).expect("first").next = Some(second);
    let head = os.m_adj(Some(first), 2).expect("head");
    assert_eq!(os.m_length(Some(head)), 8);
    let head = os.m_adj(Some(head), -2).expect("head");
    assert_eq!(os.m_length(Some(head)), 6);
    os.m_freem(Some(head));

    assert!(os.inp_wlock().is_ok());
    assert!(os.inp_wlock_assert().is_ok());
    assert!(os.inp_wlock().is_err());
    assert!(os.inp_wunlock());
    assert!(os.inp_wlock_assert().is_err());
    os.sched_pin();
    assert_eq!(os.pins(), 1);
    os.sched_unpin();
    assert_eq!(os.pins(), 0);
    assert!(os.kassert(true, "ok").is_ok());
    assert_eq!(os.kassert(false, "rack"), Err(RackOsError::Assert));
}
