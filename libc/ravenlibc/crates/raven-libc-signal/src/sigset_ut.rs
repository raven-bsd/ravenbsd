// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn empty_has_no_bits() {
    let set = SigSet::empty();
    assert!(set.is_empty());
    assert_eq!(set.app_word(), 0);
    for sig in 1..=SIG_MAXSIG as i32 {
        assert!(!set.is_member(sig).unwrap());
    }
}

#[test]
fn fill_sets_every_supported_bit() {
    let mut set = SigSet::empty();
    set.fill();
    assert_eq!(set.app_word(), APP_MASK);
    assert_eq!(set.app_word(), !0);
    for sig in 1..=SIG_MAXSIG as i32 {
        assert!(set.is_member(sig).unwrap(), "sig {sig}");
    }
}

#[test]
fn words_beyond_the_freebsd_set_stay_zero() {
    let mut set = SigSet::empty();
    set.fill();
    set.add(1).unwrap();
    set.add(64).unwrap();
    set.del(1).unwrap();
    let _ = set.and(&SigSet::singleton(9));
    let union = set.or(&SigSet::singleton(10));
    for word in union.0.iter().skip(SIG_MAXSIG / 64) {
        assert_eq!(*word, 0);
    }
}

#[test]
fn add_and_del_reject_out_of_range() {
    let mut set = SigSet::empty();
    for sig in [0, -1, 129, 1000, i32::MIN, i32::MAX] {
        assert_eq!(set.add(sig), Err(EINVAL_ERR));
        assert_eq!(set.del(sig), Err(EINVAL_ERR));
    }
    for sig in 1..=SIG_MAXSIG as i32 {
        set.add(sig).unwrap();
        assert!(set.is_member(sig).unwrap());
        set.del(sig).unwrap();
        assert!(!set.is_member(sig).unwrap());
    }
}

#[test]
fn is_member_range_is_one_to_maxsig() {
    let set = SigSet::empty();
    assert_eq!(set.is_member(0), Ok(false));
    assert_eq!(set.is_member(129), Ok(false));
    assert_eq!(set.is_member(-1), Ok(false));
    for sig in 1..=SIG_MAXSIG as i32 {
        set.is_member(sig).unwrap();
    }
}

#[test]
fn and_or_are_word_wise_on_the_128_byte_object() {
    let mut left = SigSet::empty();
    left.add(1).unwrap();
    left.add(3).unwrap();
    let mut right = SigSet::empty();
    right.add(3).unwrap();
    right.add(64).unwrap();
    let and = left.and(&right);
    assert!(and.is_member(3).unwrap());
    assert!(!and.is_member(1).unwrap());
    assert!(!and.is_member(64).unwrap());
    let or = left.or(&right);
    assert!(or.is_member(1).unwrap());
    assert!(or.is_member(3).unwrap());
    assert!(or.is_member(64).unwrap());
}

#[test]
fn is_empty_looks_at_every_word() {
    let mut set = SigSet::empty();
    assert!(set.is_empty());
    set.0[7] = 1 << 3;
    assert!(!set.is_empty());
}

#[test]
fn abi_view_is_the_same_bytes() {
    let mut set = SigSet::empty();
    set.add(17).unwrap();
    let abi = set.to_abi();
    assert_eq!(SigSet::from_abi(abi), set);
    assert_eq!(core::mem::size_of::<SigSet>(), 128);
}

#[test]
fn rtmin_is_thirty_five_and_rtmax_sixty_four() {
    assert_eq!(SIGRTMIN, 35);
    assert_eq!(SIGRTMAX, 64);
    assert_eq!(NSIG, 65);
}
