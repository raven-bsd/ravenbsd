// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the RACK OS context (K314).

use super::*;

#[test]
fn mbuf_allocation_memory_and_copy_accounting() {
    let mut pool = MbufPool::new();
    let id = pool.m_gethdr(M_NOWAIT).expect("header");
    assert_eq!(pool.m_memcnt(), MSIZE as u64);
    assert_eq!(pool.m_trailingroom(id), MHLEN);
    assert!(pool.write(id, 0, b"hello"));
    assert!(pool.set_len(id, 5));
    assert_eq!(pool.m_length(Some(id)), 5);
    let mut out = [0u8; 5];
    assert_eq!(pool.m_copydata(Some(id), 0, 5, &mut out), Ok(5));
    assert_eq!(&out, b"hello");

    let copy = pool.m_copym(Some(id), 1, 3).expect("copy");
    assert_eq!(pool.m_length(Some(copy)), 3);
    let mut copy_out = [0u8; 3];
    assert_eq!(pool.m_copydata(Some(copy), 0, 3, &mut copy_out), Ok(3));
    assert_eq!(&copy_out, b"ell");

    let cluster = pool.m_getcl(M_NOWAIT, 0).expect("cluster");
    assert_eq!(pool.m_memcnt(), (2 * MSIZE + MCLBYTES) as u64);

    assert_eq!(pool.m_free(id), None);
    assert_eq!(pool.m_free(copy), None);
    assert_eq!(pool.m_free(cluster), None);
    assert_eq!(pool.m_memcnt(), 0);
}

#[test]
fn mbuf_adjust_walks_the_chain_and_reports_the_new_head() {
    let mut pool = MbufPool::new();
    let first = pool.m_get(M_NOWAIT).expect("first");
    let second = pool.m_get(M_NOWAIT).expect("second");
    assert!(pool.write(first, 0, b"abcd"));
    assert!(pool.set_len(first, 4));
    assert!(pool.write(second, 0, b"efgh"));
    assert!(pool.set_len(second, 4));
    pool.mbuf_mut(first).expect("first").next = Some(second);
    assert_eq!(pool.m_length(Some(first)), 8);

    let head = pool.m_adj(Some(first), 2).expect("head");
    assert_eq!(pool.m_length(Some(head)), 6);
    let head = pool.m_adj(Some(head), -1).expect("head");
    assert_eq!(pool.m_length(Some(head)), 5);
    let head = pool.m_adj(Some(head), 10);
    assert!(head.is_none());
    assert_eq!(pool.m_memcnt(), 0);
}

#[test]
fn socket_buffer_accounts_and_cuts_the_send_queue() {
    let mut pool = MbufPool::new();
    let mut sb = SockBuf::new(64, 1_024);
    let mbuf = pool.m_get(M_NOWAIT).expect("mbuf");
    assert!(pool.write(mbuf, 0, b"0123456789"));
    assert!(pool.set_len(mbuf, 10));
    sb.sbappendstream_locked(&mut pool, Some(mbuf))
        .expect("append");
    assert_eq!(sb.sbavail(), 10);
    assert_eq!(sb.sbused(), 10);
    assert_eq!(sb.sbspace(), 54);
    assert_eq!(sb.sbsndmbuf(&pool, 5), Some((mbuf, 5)));
    assert_eq!(sb.sbsndptr_noadv(&pool, 5), Some((mbuf, 5)));

    let cut = sb.sbcut_locked(&mut pool, 4).expect("cut");
    assert_eq!(sb.sbused(), 6);
    assert_eq!(pool.m_length(Some(cut)), 4);
    pool.m_freem(Some(cut));
    assert_eq!(sb.sbspace(), 58, "mbcnt is unchanged by a prefix split");

    let cut = sb.sbcut_locked(&mut pool, 6).expect("cut");
    assert_eq!(sb.sbused(), 0);
    assert_eq!(sb.sbavail(), 0);
    assert_eq!(sb.sbspace(), 64);
    pool.m_freem(Some(cut));
    assert_eq!(pool.m_memcnt(), 0);
}

#[test]
fn socket_buffer_mbcnt_bounds_space_and_is_released_on_cut() {
    let mut pool = MbufPool::new();
    let mut sb = SockBuf::new(1_024, 300);
    let mbuf = pool.m_get(M_NOWAIT).expect("mbuf");
    assert!(pool.write(mbuf, 0, b"0123456789"));
    assert!(pool.set_len(mbuf, 10));
    sb.sbappendstream_locked(&mut pool, Some(mbuf))
        .expect("append");
    assert_eq!(sb.sbspace(), 44, "mbmax 300 minus one MSIZE mbuf");
    let cut = sb.sbcut_locked(&mut pool, 10).expect("cut");
    assert_eq!(sb.sbspace(), 300);
    pool.m_freem(Some(cut));
}

#[test]
fn socket_buffer_send_pointer_advance_stays_within_the_chain() {
    let mut pool = MbufPool::new();
    let mut sb = SockBuf::new(64, 1_024);
    let mbuf = pool.m_get(M_NOWAIT).expect("mbuf");
    assert!(pool.write(mbuf, 0, b"0123456789"));
    assert!(pool.set_len(mbuf, 10));
    sb.sbappendstream_locked(&mut pool, Some(mbuf))
        .expect("append");
    let (id, off) = sb.sbsndptr_noadv(&pool, 2).expect("pointer");
    assert_eq!((id, off), (mbuf, 2));
    assert!(sb.sbsndptr_adv(&pool, id, 3));
    assert_eq!(sb.sndptroff, 3);
    assert_eq!(sb.sbsndmbuf(&pool, 7), Some((mbuf, 7)));
}

#[test]
fn uma_zones_enforce_max_and_track_current() {
    let mut uma = Uma::new();
    let zone = uma.uma_zcreate("rack_sendmap", 64);
    assert!(uma.uma_zone_set_max(zone, 2));
    assert_eq!(uma.uma_zone_get_max(zone), Some(2));
    let first = uma.uma_zalloc(zone).expect("first");
    let second = uma.uma_zalloc(zone).expect("second");
    assert!(uma.uma_zalloc(zone).is_none());
    assert_eq!(uma.uma_zone_get_cur(zone), Some(2));
    assert_eq!(uma.zone(zone).expect("zone").failures, 1);
    assert!(uma.uma_zfree(zone, first));
    assert!(uma.uma_zfree(zone, second));
    assert!(!uma.uma_zfree(zone, first));
    assert_eq!(uma.uma_zone_get_cur(zone), Some(0));
    assert!(uma.uma_zdestroy(zone));
    assert!(uma.uma_zone_get_cur(zone).is_none());
}

#[test]
fn sysctl_tree_adds_reads_sets_and_frees_by_context() {
    let mut tree = SysctlTree::new();
    let ctx = tree.sysctl_ctx_init();
    let root = tree.sysctl_add_node(None, "rack", ctx);
    let divisor = tree.sysctl_add_u32(Some(root), "divisor", 250, ctx);
    let counter = tree.sysctl_add_counter(Some(root), "paced", 0, ctx);
    assert_eq!(tree.sysctl_handle_32(divisor), Some(250));
    assert!(tree.sysctl_set_u32(divisor, 400));
    assert_eq!(tree.sysctl_handle_32(divisor), Some(400));
    assert!(tree.sysctl_counter_add(counter, 7));
    assert_eq!(tree.sysctl_handle_32(counter), Some(7));
    assert_eq!(tree.sysctl_children(Some(root)).len(), 2);
    let mut out = [0u8; 8];
    assert_eq!(tree.sysctl_out(divisor, &mut out), Ok(3));
    assert_eq!(&out[..3], b"400");
    assert_eq!(tree.sysctl_ctx_free(ctx), 3);
    assert_eq!(tree.sysctl_handle_32(divisor), None);
}

#[test]
fn the_btf_log_is_bounded_and_counts_drops() {
    let mut log = TcpLog::new(true, 2);
    assert!(log.tcp_bblogging_on());
    assert!(log.tcp_log_event(1, [1, 2, 3, 4], 0));
    assert!(log.tcp_log_event(2, [0; 4], 0));
    assert!(log.tcp_log_stackspecific(3, -1, 1_460));
    assert_eq!(log.tcp_log_buffer(), 2);
    assert_eq!(log.dropped(), 1);
    assert_eq!(log.events().back().expect("last").kind, 3);
    log.set_enabled(false);
    assert!(!log.tcp_log_event(4, [0; 4], 0));
}

#[test]
fn ktls_software_and_simulated_hardware_paths() {
    let mut tls = TlsContext::new();
    let software = tls.ktls_enable(false, 20, 1_000);
    assert!(tls.ktls_ready(software));
    assert_eq!(tls.ktls_get_maxlen(software), 1_000);
    assert_eq!(tls.tcp_estimate_tls_overhead(software, 2_500), 60);
    assert_eq!(tls.ktls_send(software, 2_500), Ok(2_560));

    let hardware = tls.ktls_enable(true, 16, 1_000);
    assert_eq!(tls.ktls_send(hardware, 10), Err(RackOsError::TlsNoTag));
    assert!(tls.ktls_set_snd_tag(hardware, 7));
    assert_eq!(tls.ktls_send(hardware, 10), Ok(26));
    tls.session_mut(hardware).expect("session").output_eagain = true;
    assert_eq!(tls.ktls_send(hardware, 10), Err(RackOsError::TlsEagain));
    assert!(tls.ktls_output_eagain(hardware));
    assert_eq!(tls.ktls_send(hardware, 10), Ok(26));
    assert!(tls.ktls_release_snd_tag(hardware));
    assert_eq!(tls.ktls_send(hardware, 10), Err(RackOsError::TlsNoTag));
}

#[test]
fn hpts_and_pacing_state_are_explicit() {
    let mut pacing = Pacing::default();
    assert!(!pacing.tcp_hpts_remove());
    let deadline = pacing.tcp_hpts_insert(1_000, 250);
    assert_eq!(deadline, 1_250);
    assert!(pacing.tcp_hpts_due(1_250));
    assert!(pacing.tcp_hpts_remove());
    assert!(!pacing.tcp_hpts_due(2_000));
    assert_eq!(pacing.hpts.inserts, 1);
    assert_eq!(pacing.hpts.removes, 2);

    assert!(pacing.rack_set_cc_pacing());
    assert!(!pacing.rack_set_cc_pacing());
    assert!(pacing.rack_undo_cc_pacing());
    assert!(!pacing.rack_undo_cc_pacing());
    pacing.method = PacingMethod::Reg;
    pacing.enabled = true;
    assert!(pacing.rack_remove_pacing());
    assert_eq!(pacing.method, PacingMethod::None);
    assert!(!pacing.enabled);

    pacing.max_segs = 4;
    assert_eq!(pacing.rack_get_pacing_len(100_000, 1_000), 4_000);
    assert_eq!(pacing.rack_get_pacing_len(0, 1_000), 0);
    assert!(pacing.rack_set_pace_segments(2));
    assert_eq!(pacing.rack_get_pacing_len(100_000, 1_000), 2_000);
}

#[test]
fn the_tcpcb_mirror_uses_the_rack_rexmtval() {
    let mut tp = RackTcpcb {
        t_srtt: 800,
        t_rttvar: 100,
        t_rttmin: 2,
        ..RackTcpcb::default()
    };
    assert_eq!(tp.rack_rexmtval(), 200);
    tp.t_rttvar = 1;
    assert_eq!(tp.rack_rexmtval(), 101);
    tp.t_srtt = 0;
    tp.t_rttvar = 0;
    assert_eq!(tp.rack_rexmtval(), 2);
    assert!(!tp.have_established());
    tp.t_state = 4;
    assert!(tp.have_established());
}

#[test]
fn transport_stack_callbacks_and_clock_are_recorded() {
    let mut os = RackOs::new();
    assert_eq!(os.tfb_tcp_output(), 0);
    os.tfb_tcp_rexmit_tmr();
    os.tfb_hwtls_change(true);
    assert_eq!(os.stacks.output_calls, 1);
    assert_eq!(os.stacks.rexmit_timer_calls, 1);
    assert_eq!(os.stacks.hwtls_changes, 1);
    assert_eq!(os.tcp_get_usecs(), 0);
    os.advance_us(1_500);
    assert_eq!(os.get_cyclecount(), 1_500);
    assert_eq!(os.tcp_ts_getticks(), 1);
    assert_eq!(RackOs::tcp_tv_to_usec(1, 2), 1_000_002);
}

#[test]
fn rackos_locks_pins_and_assertions_are_contract_checked() {
    let mut os = RackOs::new();
    assert!(os.inp_wlock().is_ok());
    assert!(os.inp_wlock().is_err());
    assert!(os.inp_wlock_assert().is_ok());
    assert!(os.inp_wunlock());
    assert!(os.inp_wlock_assert().is_err());
    assert!(os.sock_sendbuf_lock().is_ok());
    assert!(os.sock_sendbuf_lock().is_err());
    assert!(os.sock_sendbuf_unlock());
    os.sched_pin();
    os.sched_pin();
    assert_eq!(os.pins(), 2);
    os.sched_unpin();
    os.sched_unpin();
    assert_eq!(os.pins(), 0);
    assert!(os.kassert(true, "ok").is_ok());
    assert_eq!(
        os.kassert(false, "rack invariant"),
        Err(RackOsError::Assert)
    );
    assert_eq!(os.last_fault.as_deref(), Some("rack invariant"));
}
