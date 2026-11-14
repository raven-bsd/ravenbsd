// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

fn name(sig: i32) -> Option<String> {
    let mut buffer = [0u8; SIG2STR_MAX];
    let length = sig2str(sig, &mut buffer).ok()?;
    String::from_utf8(buffer[..length].to_vec()).ok()
}

#[test]
fn classic_names() {
    assert_eq!(name(1).as_deref(), Some("HUP"));
    assert_eq!(name(6).as_deref(), Some("ABRT"));
    assert_eq!(name(10).as_deref(), Some("USR1"));
    assert_eq!(name(29).as_deref(), Some("IO"));
    assert_eq!(name(31).as_deref(), Some("SYS"));
}

#[test]
fn out_of_range_is_einval() {
    for sig in [-100, -1, 0, 65, 100, i32::MAX] {
        let mut buffer = [0u8; SIG2STR_MAX];
        assert_eq!(sig2str(sig, &mut buffer), Err(EINVAL_ERR));
    }
}

#[test]
fn internal_numbers_are_not_nameable() {
    for sig in [32, 33, 34] {
        let mut buffer = [0u8; SIG2STR_MAX];
        assert_eq!(sig2str(sig, &mut buffer), Err(EINVAL_ERR), "sig {sig}");
        assert_eq!(str2sig(format!("{sig}").as_bytes()), Ok(sig));
    }
}

#[test]
fn realtime_formatting_follows_the_donor() {
    assert_eq!(name(35).as_deref(), Some("RTMIN"));
    assert_eq!(name(36).as_deref(), Some("RTMIN+1"));
    assert_eq!(name(49).as_deref(), Some("RTMIN+14"));
    assert_eq!(name(50).as_deref(), Some("RTMAX-14"));
    assert_eq!(name(63).as_deref(), Some("RTMAX-1"));
    assert_eq!(name(64).as_deref(), Some("RTMAX"));
}

#[test]
fn str2sig_accepts_names_prefixes_and_case() {
    assert_eq!(str2sig(b"HUP"), Ok(1));
    assert_eq!(str2sig(b"SIGHUP"), Ok(1));
    assert_eq!(str2sig(b"sigusr1"), Ok(10));
    assert_eq!(str2sig(b"IOT"), Ok(6));
    assert_eq!(str2sig(b"POLL"), Ok(29));
    assert_eq!(str2sig(b"RTMIN"), Ok(35));
    assert_eq!(str2sig(b"rtmin+3"), Ok(38));
    assert_eq!(str2sig(b"RTMAX-3"), Ok(61));
    assert_eq!(str2sig(b"SIGRTMAX"), Ok(64));
}

#[test]
fn str2sig_rejects_malformed_rt_and_names() {
    for text in [
        b"".as_slice(),
        b"RTMIN+".as_slice(),
        b"RTMIN+0".as_slice(),
        b"RTMIN+30".as_slice(),
        b"RTMAX-30".as_slice(),
        b"RTMAX+1".as_slice(),
        b"RTMINX".as_slice(),
        b"RTMIN+-1".as_slice(),
        b"BOGUS".as_slice(),
        b"0".as_slice(),
        b"65".as_slice(),
        b"-1".as_slice(),
    ] {
        assert_eq!(str2sig(text), Err(EINVAL_ERR), "{:?}", text);
    }
}

#[test]
fn str2sig_decimal_range_is_one_to_sixty_four() {
    for number in 1..=64 {
        assert_eq!(str2sig(format!("{number}").as_bytes()), Ok(number));
    }
    assert_eq!(str2sig(b"0001"), Ok(1));
}

#[test]
fn rtmin_plus_29_reaches_rtmax() {
    assert_eq!(str2sig(b"RTMIN+29"), Ok(64));
    assert_eq!(str2sig(b"RTMAX-29"), Ok(35));
}

#[test]
fn every_named_number_round_trips() {
    for sig in 1..=64 {
        let mut buffer = [0u8; SIG2STR_MAX];
        match sig2str(sig, &mut buffer) {
            Ok(length) => {
                assert_eq!(str2sig(&buffer[..length]), Ok(sig), "sig {sig}");
            }
            Err(_) => assert!((32..=34).contains(&sig)),
        }
    }
}

#[test]
fn output_is_nul_terminated_and_bounded() {
    let mut buffer = [0xAAu8; SIG2STR_MAX];
    let length = sig2str(1, &mut buffer).unwrap();
    assert_eq!(buffer[length], 0);
    assert!(length < SIG2STR_MAX);
}
