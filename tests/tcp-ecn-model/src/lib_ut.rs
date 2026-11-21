// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Randomized model checks against the merged ECN, PMTU and PLPMTUD state.

use raven_tcpip::{
    ECN_CE, ECN_ECT0, ECN_ECT1, ECN_NOTECT, Ecn, EcnCounters, EcnState, IpAddr, IpFamily, Ipv4Addr,
    Ipv6Addr, MtuCache, Plpmtud, PlpmtudState, TH_CWR, TH_ECE, floor_for, mss_update, mssopt,
    next_pmtu, next_pmtu6, on_icmp_frag_needed, on_icmp6_frag_needed, probe_sequence,
};

use crate::ecn::{RefClassic, RefCounters, RefEcn};
use crate::plpmtud::{RefPlpmtud, ref_blackhole, ref_probe_sequence};
use crate::pmtu::{
    RefCache, ref_floor_for, ref_mss_update, ref_mssopt, ref_next_pmtu, ref_next_pmtu6,
    ref_on_icmp_frag_needed, ref_on_icmp6_frag_needed,
};
use crate::rng::Rng;

const SEEDS: [u64; 3] = [
    0x1234_5678_9abc_def0,
    0x0bad_f00d_dead_beef,
    0xa5a5_5a5a_1234_4321,
];

fn ipv4(last: u8) -> IpAddr {
    IpAddr::V4(Ipv4Addr::new([192, 0, 2, last]))
}

fn ipv6(last: u8) -> IpAddr {
    IpAddr::V6(Ipv6Addr::new([
        0x20, 0x01, 0x0d, 0xb8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, last,
    ]))
}

fn assert_ecn(product: &Ecn, reference: &RefEcn) {
    assert_eq!(product.negotiated, reference.negotiated);
    assert_eq!(product.ce_seen, reference.ce_seen);
    assert_eq!(product.reacted, reference.reacted);
    assert_eq!(product.ece_pending, reference.ece_pending);
    assert_eq!(product.cwr_pending, reference.cwr_pending);
}

#[test]
fn ecn_once_per_rtt_matches_the_reference() {
    for seed in SEEDS {
        let mut rng = Rng::new(seed);
        let mut product = Ecn::default();
        let mut reference = RefEcn::default();
        let mut reactions = 0;
        let mut cwrs = 0;
        for _ in 0..1_000 {
            match rng.below(6) {
                0 => {
                    let local = rng.coin();
                    let peer = rng.coin();
                    product.negotiate(local, peer);
                    reference.negotiate(local, peer);
                }
                1 => {
                    let left = product.on_ce();
                    let right = reference.on_ce();
                    assert_eq!(left, right);
                    if left {
                        reactions += 1;
                    }
                }
                2 => {
                    product.on_rtt();
                    reference.on_rtt();
                }
                3 => {
                    let left = product.take_cwr();
                    let right = reference.take_cwr();
                    assert_eq!(left, right);
                    if left {
                        cwrs += 1;
                    }
                }
                _ => assert_ecn(&product, &reference),
            }
            assert_ecn(&product, &reference);
        }
        assert!(reactions > 0, "seed {seed:#x} never reacted");
        assert!(cwrs > 0, "seed {seed:#x} never consumed CWR");
    }

    let mut product = Ecn::default();
    let mut reference = RefEcn::default();
    product.negotiate(true, true);
    reference.negotiate(true, true);
    assert!(product.on_ce());
    assert!(!product.on_ce());
    assert!(reference.on_ce());
    assert!(!reference.on_ce());
    product.on_rtt();
    reference.on_rtt();
    assert!(product.on_ce());
    assert!(reference.on_ce());
}

fn assert_classic(
    product: &EcnState,
    reference: &RefClassic,
    counters: &EcnCounters,
    refc: &RefCounters,
) {
    assert_eq!(product.permit, reference.permit);
    assert_eq!(product.snd_ece, reference.snd_ece);
    assert_eq!(product.snd_cwr, reference.snd_cwr);
    assert_eq!(product.use_ect1, reference.use_ect1);
    assert_eq!(counters.handshakes, refc.handshakes);
    assert_eq!(counters.received_ce, refc.received_ce);
    assert_eq!(counters.received_ect0, refc.received_ect0);
    assert_eq!(counters.received_ect1, refc.received_ect1);
    assert_eq!(counters.sent_ect0, refc.sent_ect0);
    assert_eq!(counters.sent_ect1, refc.sent_ect1);
}

#[test]
fn classic_ecn_state_matches_the_reference() {
    const FLAGS: [u16; 4] = [0, TH_ECE, TH_CWR, TH_ECE | TH_CWR];
    const CODEPOINTS: [u8; 4] = [ECN_NOTECT, ECN_ECT0, ECN_ECT1, ECN_CE];

    for seed in SEEDS {
        let mut rng = Rng::new(seed);
        let mut product = EcnState::new(2, 1);
        let mut reference = RefClassic::default();
        let mut counters = EcnCounters::default();
        let mut ref_counters = RefCounters::default();

        for _ in 0..1_000 {
            match rng.below(4) {
                0 => {
                    let flags = FLAGS[usize::try_from(rng.below(4)).unwrap_or(0)];
                    let codepoint = CODEPOINTS[usize::try_from(rng.below(4)).unwrap_or(0)];
                    product.input_syn_sent(flags, codepoint, &mut counters);
                    reference.input_syn_sent(flags, &mut ref_counters);
                }
                1 => {
                    let flags = FLAGS[usize::try_from(rng.below(4)).unwrap_or(0)];
                    let codepoint = CODEPOINTS[usize::try_from(rng.below(4)).unwrap_or(0)];
                    let len = rng.range(0, 1_000);
                    let pkts = rng.range(0, 4);
                    product.input_segment(flags, len, pkts, codepoint, &mut counters);
                    reference.input_segment(flags, codepoint, &mut ref_counters);
                }
                2 => {
                    let len = rng.range(0, 1_000);
                    let snd_nxt = rng.range(0, 2_000);
                    let snd_max = rng.range(0, 2_000);
                    let rxmit = rng.coin();
                    let forcedata = rng.coin();
                    let newdata =
                        len > 0 && snd_nxt >= snd_max && !rxmit && !(forcedata && len == 1);
                    product.snd_cwr = rng.coin();
                    reference.snd_cwr = product.snd_cwr;
                    product.use_ect1 = rng.coin();
                    reference.use_ect1 = product.use_ect1;
                    let mut flags = 0;
                    let ipecn = product.output_established(
                        &mut flags,
                        len,
                        snd_nxt,
                        snd_max,
                        rxmit,
                        forcedata,
                        &mut counters,
                    );
                    let (ref_ipecn, ref_flags) = reference.output(newdata, &mut ref_counters);
                    assert_eq!(ipecn, ref_ipecn);
                    assert_eq!(flags, ref_flags);
                }
                _ => assert_classic(&product, &reference, &counters, &ref_counters),
            }
            assert_classic(&product, &reference, &counters, &ref_counters);
        }
    }
}

#[test]
fn pmtu_cache_matches_the_reference() {
    let addresses = [ipv4(1), ipv4(2), ipv4(3), ipv6(1), ipv6(2), ipv6(3)];
    for seed in SEEDS {
        let mut rng = Rng::new(seed);
        let capacity = usize::try_from(rng.range(1, 4)).unwrap_or(1);
        let lifetime = i64::from(rng.range(1, 1_000));
        let mut product = MtuCache::new(capacity, lifetime);
        let mut reference = RefCache::new(capacity, lifetime);

        for _ in 0..300 {
            let destination = addresses[usize::try_from(rng.below(6)).unwrap_or(0)];
            let now = i64::from(rng.range(0, 2_000));
            match rng.below(4) {
                0 | 1 => {
                    let mtu = rng.range(68, 9_000);
                    product.insert(destination, mtu, now);
                    reference.insert(destination, mtu, now);
                }
                2 => {
                    assert_eq!(
                        product.get(destination, now),
                        reference.get(destination, now)
                    );
                }
                _ => {
                    assert_eq!(product.expire(now), reference.expire(now));
                }
            }
            assert_eq!(product.len(), reference.len());
            assert_eq!(floor_for(destination), ref_floor_for(destination));
        }
    }
}

#[test]
fn v4_pmtu_and_mss_match_the_reference() {
    for seed in SEEDS {
        let mut rng = Rng::new(seed);
        for _ in 0..500 {
            let reported = rng.range(0, 2_000);
            let ip_len = rng.range(0, 2_000);
            let minmss = rng.range(216, 600);
            assert_eq!(
                next_pmtu(reported, ip_len, minmss),
                ref_next_pmtu(reported, ip_len, minmss)
            );

            let snd_una = rng.range(0, 1_000);
            let snd_max = snd_una.saturating_add(rng.range(0, 100));
            let quoted = rng.range(snd_una.saturating_sub(50), snd_max.saturating_add(50));
            let maxseg = rng.range(64, 1_460);
            assert_eq!(
                on_icmp_frag_needed(quoted, snd_una, snd_max, reported, ip_len, maxseg, minmss),
                ref_on_icmp_frag_needed(quoted, snd_una, snd_max, reported, ip_len, maxseg, minmss)
            );

            let maxmtu = rng.range(0, 2_000);
            let hc_mtu = rng.range(0, 2_000);
            let default_mss = rng.range(536, 1_460);
            assert_eq!(
                mssopt(maxmtu, hc_mtu, default_mss, IpFamily::V4),
                ref_mssopt(maxmtu, hc_mtu, default_mss, IpFamily::V4)
            );

            let offer = i32::try_from(rng.range(0, 2_000)).unwrap_or(0) - 1;
            let mtuoffer = if rng.coin() {
                u32::MAX
            } else {
                rng.range(500, 2_000)
            };
            let path_mtu_discovery = rng.coin();
            let local = rng.coin();
            let send_buffer = rng.range(0, 9_000);
            assert_eq!(
                mss_update(
                    offer,
                    mtuoffer,
                    maxmtu,
                    hc_mtu,
                    default_mss,
                    minmss,
                    path_mtu_discovery,
                    local,
                    send_buffer,
                    IpFamily::V4,
                ),
                ref_mss_update(
                    offer,
                    mtuoffer,
                    maxmtu,
                    hc_mtu,
                    default_mss,
                    minmss,
                    path_mtu_discovery,
                    local,
                    send_buffer,
                    IpFamily::V4,
                )
            );
        }
    }
}

#[test]
fn v6_pmtu_and_mss_match_the_reference() {
    for seed in SEEDS {
        let mut rng = Rng::new(seed);
        for _ in 0..300 {
            let reported = rng.range(0, 9_000);
            assert_eq!(next_pmtu6(reported), ref_next_pmtu6(reported));
            assert_eq!(floor_for(ipv6(1)), ref_floor_for(ipv6(1)));

            let snd_una = rng.range(0, 1_000);
            let snd_max = snd_una.saturating_add(rng.range(0, 100));
            let quoted = rng.range(snd_una.saturating_sub(50), snd_max.saturating_add(50));
            let maxseg = rng.range(64, 1_440);
            assert_eq!(
                on_icmp6_frag_needed(quoted, snd_una, snd_max, reported, maxseg),
                ref_on_icmp6_frag_needed(quoted, snd_una, snd_max, reported, maxseg)
            );

            let maxmtu = rng.range(0, 9_000);
            let hc_mtu = rng.range(0, 9_000);
            let default_mss = rng.range(1_220, 1_440);
            assert_eq!(
                mssopt(maxmtu, hc_mtu, default_mss, IpFamily::V6),
                ref_mssopt(maxmtu, hc_mtu, default_mss, IpFamily::V6)
            );
            let offer = i32::try_from(rng.range(0, 1_440)).unwrap_or(0) - 1;
            let path_mtu_discovery = rng.coin();
            let local = rng.coin();
            let send_buffer = rng.range(0, 9_000);
            assert_eq!(
                mss_update(
                    offer,
                    u32::MAX,
                    maxmtu,
                    hc_mtu,
                    default_mss,
                    1_220,
                    path_mtu_discovery,
                    local,
                    send_buffer,
                    IpFamily::V6,
                ),
                ref_mss_update(
                    offer,
                    u32::MAX,
                    maxmtu,
                    hc_mtu,
                    default_mss,
                    1_220,
                    path_mtu_discovery,
                    local,
                    send_buffer,
                    IpFamily::V6,
                )
            );
        }
    }
}

#[test]
fn plpmtud_search_matches_the_reference() {
    for seed in SEEDS {
        let mut rng = Rng::new(seed);
        for _ in 0..100 {
            let base = rng.range(1, 1_500);
            let max = rng.range(base, 9_000);
            let budget = usize::try_from(rng.range(1, 24)).unwrap_or(1);
            let mut product = Plpmtud::new(base, max, budget);
            let mut reference = RefPlpmtud::new(base, max, budget);
            product.start();
            reference.start();
            assert_eq!(product.state, reference.state);
            assert_eq!(product.current, reference.current);

            for _ in 0..100 {
                let left = product.next_probe();
                let right = reference.next_probe();
                assert_eq!(left, right);
                let Some(size) = left else {
                    break;
                };
                if rng.below(10) < 7 {
                    product.on_probe_ack(size);
                    reference.on_probe_ack(size);
                } else {
                    product.on_probe_loss(size);
                    reference.on_probe_loss(size);
                }
                assert_eq!(product.state, reference.state);
                assert_eq!(product.current, reference.current);
            }
        }
    }
}

#[test]
fn probe_sequences_match_the_reference() {
    for seed in SEEDS {
        let mut rng = Rng::new(seed);
        for _ in 0..100 {
            let base = rng.range(1, 1_500);
            let max = rng.range(base, 9_000);
            let budget = usize::try_from(rng.range(1, 24)).unwrap_or(1);
            assert_eq!(
                probe_sequence(base, max, budget),
                ref_probe_sequence(base, max, budget)
            );
        }
    }
}

#[test]
fn the_blackhole_ladder_returns_to_the_floor() {
    for seed in SEEDS {
        let mut rng = Rng::new(seed);
        for _ in 0..100 {
            let floor = rng.range(216, 800);
            let mss = rng.range(floor, 1_500);
            let mut product = Plpmtud::new(floor, mss, 24);
            let mut reference = ref_blackhole(floor, mss, 24);
            product.start();
            assert_eq!(product.state, PlpmtudState::Searching);
            assert_eq!(product.current, floor);
            assert_eq!(reference.state, PlpmtudState::Searching);
            assert_eq!(reference.current, floor);

            while let Some(size) = product.next_probe() {
                assert_eq!(Some(size), reference.next_probe());
                product.on_probe_loss(size);
                reference.on_probe_loss(size);
                assert_eq!(product.state, reference.state);
                assert_eq!(product.current, reference.current);
            }
            assert_eq!(product.state, PlpmtudState::SearchComplete);
            assert_eq!(product.current, floor);
            assert_eq!(reference.state, PlpmtudState::SearchComplete);
            assert_eq!(reference.current, floor);
        }
    }
}

#[test]
fn the_model_detects_a_mutated_reference() {
    let product = next_pmtu(1_500, 1_500, 536);
    let reference = ref_next_pmtu(1_500, 1_500, 536);
    assert_eq!(product, reference);
    let mutant = reference.wrapping_add(1);
    assert_ne!(product, mutant, "a mutated reference must be detected");

    let mut product = Ecn::default();
    let mut reference = RefEcn::default();
    product.negotiate(true, true);
    reference.negotiate(true, true);
    assert!(product.on_ce());
    assert!(reference.on_ce());
    reference.reacted = false;
    assert!(
        reference.on_ce(),
        "a mutated once-per-RTT latch must be detected"
    );
}
