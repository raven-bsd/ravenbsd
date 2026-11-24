// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Every K334 sockopt option: defaults, valid set/get, invalid values and interactions.

use raven_tcpip::{
    Errno, IPPROTO_TCP, RackMisc, RackSockopt, SockoptDir, TCP_DEFER_OPTIONS, TCP_INFO, TCP_NO_PRR,
    TCP_PACING_DND, TCP_RACK_ABC_VAL, TCP_RACK_DSACK_OPT, TCP_RACK_FORCE_MSEG, TCP_RACK_HI_BETA,
    TCP_RACK_MBUF_QUEUE, TCP_RACK_MEASURE_CNT, TCP_RACK_PACE_MAX_SEG, TCP_RACK_PACE_MIN_SEG,
    TCP_RACK_PACE_TO_FILL, TCP_RACK_PACING_BETA, TCP_RACK_PACING_DIVISOR, TCP_RACK_PROFILE,
    TCP_RACK_SET_RXT_OPTIONS, TCP_RACK_TIMER_SLOP, TCP_USE_CMP_ACKS,
};

fn option(dir: SockoptDir, name: i32, value: u32) -> RackSockopt {
    RackSockopt {
        dir,
        level: IPPROTO_TCP,
        name,
        value,
    }
}

fn set(rack: &mut RackMisc, name: i32, value: u32) -> Result<(), Errno> {
    rack.rack_set_sockopt(&option(SockoptDir::Set, name, value))
}

fn get(rack: &RackMisc, name: i32) -> Result<Option<u32>, Errno> {
    rack.rack_get_sockopt(&option(SockoptDir::Get, name, 0))
}

#[test]
fn every_round_trip_option_has_its_default_and_valid_set() {
    // name, default, valid value, expected after set
    let cases = [
        (TCP_DEFER_OPTIONS, 0, 1, 1),
        (TCP_RACK_DSACK_OPT, 0, 3, 3),
        (TCP_RACK_HI_BETA, 0, 1, 1),
        (TCP_RACK_TIMER_SLOP, 0, 123, 123),
        (TCP_RACK_MEASURE_CNT, 1, 5, 5),
        (TCP_NO_PRR, 0, 1, 1),
        (TCP_USE_CMP_ACKS, 0, 1, 1),
        (TCP_RACK_MBUF_QUEUE, 0, 1, 1),
        (TCP_RACK_PACE_TO_FILL, 0, 1, 1),
        (TCP_RACK_FORCE_MSEG, 0, 1, 1),
        (TCP_RACK_ABC_VAL, 0, 7, 7),
        (TCP_RACK_PACING_DIVISOR, 250, 300, 300),
        (TCP_RACK_PACE_MIN_SEG, 0, 1_234, 1_234),
        (TCP_RACK_PACE_MAX_SEG, 0, 7, 7),
        (TCP_PACING_DND, 0, 1, 1),
    ];
    for (name, default, value, expected) in cases {
        let mut rack = RackMisc::new();
        assert_eq!(get(&rack, name), Ok(Some(default)), "default option {name}");
        set(&mut rack, name, value).unwrap_or_else(|error| panic!("set {name}: {error:?}"));
        assert_eq!(get(&rack, name), Ok(Some(expected)), "after set {name}");
    }
}

#[test]
fn profile_is_write_only_and_selects_the_donor_modes() {
    let mut rack = RackMisc::new();
    assert_eq!(get(&rack, TCP_RACK_PROFILE), Err(Errno::EINVAL));
    set(&mut rack, TCP_RACK_PROFILE, 1).expect("profile 1");
    assert_eq!(rack.profile, 1);
    assert!(rack.dgp_on && rack.always_pace);
    set(&mut rack, TCP_RACK_PROFILE, 6).expect("profile 6");
    assert!(rack.pace_to_cwnd);
    set(&mut rack, TCP_RACK_PROFILE, 0).expect("profile 0");
    assert_eq!(rack.profile, 0);
    assert_eq!(
        set(&mut rack, TCP_RACK_PROFILE, 9),
        Err(Errno::EINVAL),
        "invalid profile"
    );
}

#[test]
fn set_rxt_options_are_write_only_and_validate_the_mode() {
    let mut rack = RackMisc::new();
    assert_eq!(get(&rack, TCP_RACK_SET_RXT_OPTIONS), Err(Errno::EOPNOTSUPP));
    set(&mut rack, TCP_RACK_SET_RXT_OPTIONS, 2).expect("mode 2");
    assert!(rack.shape_rxt_to_pacing_min && !rack.full_size_rxt);
    set(&mut rack, TCP_RACK_SET_RXT_OPTIONS, 1).expect("mode 1");
    assert!(rack.full_size_rxt && !rack.shape_rxt_to_pacing_min);
    assert_eq!(
        set(&mut rack, TCP_RACK_SET_RXT_OPTIONS, 3),
        Err(Errno::EINVAL),
        "invalid rxt mode"
    );
}

#[test]
fn pacing_beta_and_tcp_info_are_currently_read_only_findings() {
    let mut rack = RackMisc::new();
    assert_eq!(get(&rack, TCP_RACK_PACING_BETA), Ok(Some(0)));
    assert_eq!(
        set(&mut rack, TCP_RACK_PACING_BETA, 50),
        Err(Errno::EOPNOTSUPP),
        "K334 finding: PACING_BETA set is not wired to the CC beta"
    );
    set(&mut rack, TCP_RACK_PROFILE, 1).expect("profile");
    assert_eq!(get(&rack, TCP_RACK_PACING_BETA), Ok(Some(50)));
    assert_eq!(
        get(&rack, TCP_INFO),
        Err(Errno::EOPNOTSUPP),
        "K334 finding: TCP_INFO does not call rack_fill_info"
    );
}

#[test]
fn invalid_values_and_unknown_options_are_rejected() {
    let mut rack = RackMisc::new();
    assert_eq!(set(&mut rack, TCP_RACK_PROFILE, 9), Err(Errno::EINVAL));
    assert_eq!(
        set(&mut rack, TCP_RACK_SET_RXT_OPTIONS, 3),
        Err(Errno::EINVAL)
    );
    assert_eq!(set(&mut rack, 9_999, 1), Err(Errno::EOPNOTSUPP));
    let wrong_level = RackSockopt {
        dir: SockoptDir::Set,
        level: 0,
        name: TCP_RACK_HI_BETA,
        value: 1,
    };
    assert_eq!(rack.rack_set_sockopt(&wrong_level), Err(Errno::EOPNOTSUPP));
    let wrong_dir = option(SockoptDir::Get, TCP_RACK_HI_BETA, 0);
    assert_eq!(rack.rack_set_sockopt(&wrong_dir), Err(Errno::EOPNOTSUPP));
}

#[test]
fn numeric_clamps_and_zero_defaults_match_the_k334_port() {
    let mut rack = RackMisc::new();
    set(&mut rack, TCP_RACK_PACING_DIVISOR, 0).expect("divisor zero");
    assert_eq!(rack.pacing_divisor, 250);
    set(&mut rack, TCP_RACK_PACING_DIVISOR, 10).expect("divisor small");
    assert_eq!(rack.pacing_divisor, 50);
    set(&mut rack, TCP_RACK_MEASURE_CNT, 0).expect("measure zero");
    assert_eq!(rack.req_measurements, 1);
    set(&mut rack, TCP_RACK_ABC_VAL, 300).expect("abc clamp");
    assert_eq!(rack.abc_val, 255);
    set(&mut rack, TCP_RACK_PACE_MIN_SEG, 70_000).expect("min clamp");
    assert_eq!(rack.min_segs, u16::MAX);
    set(&mut rack, TCP_RACK_PACE_MAX_SEG, 100).expect("max clamp");
    assert_eq!(rack.max_segs, raven_tcpip::MAX_USER_SET_SEG as u16);
}

#[test]
fn deferred_options_wait_for_apply_and_then_round_trip() {
    let mut rack = RackMisc::new();
    set(&mut rack, TCP_RACK_PROFILE, 1).expect("profile");
    set(&mut rack, TCP_DEFER_OPTIONS, 1).expect("defer");
    set(&mut rack, TCP_RACK_HI_BETA, 1).expect("deferred hi beta");
    assert!(!rack.hi_beta);
    assert!(!rack.deferred.is_empty());
    assert_eq!(rack.rack_apply_deferred_options(), 1);
    assert!(rack.hi_beta);
    assert!(rack.deferred.is_empty());
}

#[test]
fn profile_interaction_restores_the_defaults() {
    let mut rack = RackMisc::new();
    set(&mut rack, TCP_RACK_PROFILE, 1).expect("profile 1");
    set(&mut rack, TCP_RACK_PROFILE, 0).expect("profile 0");
    assert!(!rack.dgp_on && !rack.always_pace && !rack.use_cmp_ack);
    assert_eq!(rack.profile, 0);
}

#[test]
fn inheritance_copies_the_same_option_surface() {
    let mut parent = RackMisc::new();
    set(&mut parent, TCP_RACK_HI_BETA, 1).expect("hi beta");
    set(&mut parent, TCP_RACK_DSACK_OPT, 3).expect("dsack");
    set(&mut parent, TCP_RACK_PACING_DIVISOR, 300).expect("divisor");
    let mut child = RackMisc::new();
    let changed = child.rack_inherit(&parent);
    assert!(changed >= 3);
    assert_eq!(get(&child, TCP_RACK_HI_BETA), Ok(Some(1)));
    assert_eq!(get(&child, TCP_RACK_DSACK_OPT), Ok(Some(3)));
    assert_eq!(get(&child, TCP_RACK_PACING_DIVISOR), Ok(Some(300)));
}
