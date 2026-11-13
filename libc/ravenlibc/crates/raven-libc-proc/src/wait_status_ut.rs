// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the wait-status codec and the `wait4` mapping.

use super::*;
use crate::consts::{WCONTINUED, WNOHANG, WNOWAIT, WUNTRACED};

fn reference(raw: i32) -> Option<WaitStatus> {
    if raw == 0xffff {
        return Some(WaitStatus::Continued);
    }
    if raw & 0x7f == 0 {
        return Some(WaitStatus::Exited((raw >> 8) as u8));
    }
    let stopped = {
        let value = raw as u16 as u32;
        ((value.wrapping_mul(0x1_0001) >> 8) as u16 as i16) > 0x7f00i16
    };
    if stopped {
        let sig = (raw >> 8) as u8;
        return if sig == 0 {
            None
        } else {
            Some(WaitStatus::Stopped(sig))
        };
    }
    if raw & 0xff == 0x7f && raw >> 8 == 0 {
        return None;
    }
    let signaled = (raw as u16).wrapping_sub(1) < 0xff;
    if signaled {
        let sig = (raw & 0x7f) as u8;
        return if sig == 0 {
            None
        } else {
            Some(WaitStatus::Signaled {
                sig,
                core: raw & 0x80 != 0,
            })
        };
    }
    None
}

#[test]
fn exit_codes_round_trip() {
    for code in 0..=255u8 {
        let raw = encode(WaitStatus::Exited(code));
        assert_eq!(decode(raw), Some(WaitStatus::Exited(code)));
        assert_eq!(raw, i32::from(code) << 8);
    }
}

#[test]
fn signals_round_trip_with_and_without_core() {
    for sig in 1..=64u8 {
        for core in [false, true] {
            let status = WaitStatus::Signaled { sig, core };
            assert_eq!(decode(encode(status)), Some(status));
        }
    }
}

#[test]
fn stopped_statuses_round_trip() {
    for sig in 1..=64u8 {
        let status = WaitStatus::Stopped(sig);
        assert_eq!(decode(encode(status)), Some(status));
    }
}

#[test]
fn continued_is_ffff() {
    assert_eq!(encode(WaitStatus::Continued), 0xffff);
    assert_eq!(decode(0xffff), Some(WaitStatus::Continued));
}

#[test]
fn invalid_low_values_are_rejected() {
    assert_eq!(decode(0x7f), None);
    assert_eq!(decode(0x80), Some(WaitStatus::Exited(0)));
    assert_eq!(decode(0), Some(WaitStatus::Exited(0)));
    assert_eq!(
        decode(1),
        Some(WaitStatus::Signaled {
            sig: 1,
            core: false
        })
    );
}

#[test]
fn every_16_bit_value_matches_the_macro_reference() {
    for raw in 0..=0xffffi32 {
        assert_eq!(decode(raw), reference(raw), "raw={raw:#x}");
    }
}

#[test]
fn wait4_maps_a_positive_pid() {
    assert_eq!(
        wait4_to_wait6(5, 9, WNOHANG),
        Ok((IdType::Pid, 5, WNOHANG | WEXITED))
    );
}

#[test]
fn wait4_maps_all_children() {
    assert_eq!(wait4_to_wait6(-1, 9, 0), Ok((IdType::All, 0, WEXITED)));
}

#[test]
fn wait4_maps_the_own_process_group() {
    assert_eq!(wait4_to_wait6(0, 9, 0), Ok((IdType::Pgid, 9, WEXITED)));
}

#[test]
fn wait4_maps_a_process_group() {
    assert_eq!(wait4_to_wait6(-7, 9, 0), Ok((IdType::Pgid, 7, WEXITED)));
}

#[test]
fn wait4_rejects_unknown_option_bits() {
    assert_eq!(wait4_to_wait6(1, 0, 0x8000_0000), Err(EINVAL));
    assert_eq!(wait4_to_wait6(1, 0, 0x20), Err(EINVAL));
}

#[test]
fn wall_is_accepted_and_ignored() {
    assert_eq!(wait4_to_wait6(1, 0, __WALL), Ok((IdType::Pid, 1, WEXITED)));
    assert_eq!(
        wait4_to_wait6(1, 0, __WALL | WNOHANG),
        Ok((IdType::Pid, 1, WNOHANG | WEXITED))
    );
}

#[test]
fn waitid_zeroes_the_record() {
    let mut info = [0xa5u8; 128];
    waitid_info_on_no_child(&mut info);
    assert!(info.iter().all(|byte| *byte == 0));
}

#[test]
fn exited_zero_is_zero() {
    assert_eq!(encode(WaitStatus::Exited(0)), 0);
    assert_eq!(decode(0), Some(WaitStatus::Exited(0)));
}

#[test]
fn exited_255_is_ff00() {
    assert_eq!(encode(WaitStatus::Exited(255)), 0xff00);
    assert_eq!(decode(0xff00), Some(WaitStatus::Exited(255)));
}

#[test]
fn signal_eleven_has_no_core_bit() {
    assert_eq!(
        encode(WaitStatus::Signaled {
            sig: 11,
            core: false
        }),
        11
    );
}

#[test]
fn signal_eleven_with_core_sets_the_bit() {
    assert_eq!(
        encode(WaitStatus::Signaled {
            sig: 11,
            core: true
        }),
        0x8b
    );
}

#[test]
fn stopped_one_is_017f() {
    assert_eq!(encode(WaitStatus::Stopped(1)), 0x17f);
    assert_eq!(decode(0x17f), Some(WaitStatus::Stopped(1)));
}

#[test]
fn stopped_sixty_four_is_407f() {
    assert_eq!(encode(WaitStatus::Stopped(64)), 0x407f);
    assert_eq!(decode(0x407f), Some(WaitStatus::Stopped(64)));
}

#[test]
fn high_exit_codes_are_not_signals() {
    assert_eq!(decode(0x0100), Some(WaitStatus::Exited(1)));
    assert_eq!(decode(0x0200), Some(WaitStatus::Exited(2)));
}

#[test]
fn wait4_keeps_wuntraced_and_wcontinued() {
    assert_eq!(
        wait4_to_wait6(2, 0, WUNTRACED | WCONTINUED),
        Ok((IdType::Pid, 2, WUNTRACED | WCONTINUED | WEXITED))
    );
}

#[test]
fn wait4_keeps_wno_wait() {
    assert_eq!(
        wait4_to_wait6(2, 0, WNOWAIT),
        Ok((IdType::Pid, 2, WNOWAIT | WEXITED))
    );
}
