// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::{KernelSigAction, from_c, to_c};
use raven_libc_abi::structs::{Sigaction, SigsetT};

fn c_action(handler: usize, low: u64, high: u64, flags: i32) -> Sigaction {
    let mut action = Sigaction::ZERO;
    action.sa_handler = handler as *mut core::ffi::c_void;
    action.sa_mask.bytes[0] = low;
    action.sa_mask.bytes[1] = high;
    action.sa_flags = flags;
    action
}

#[test]
fn c_action_maps_to_the_40_byte_kernel_form() {
    let action = c_action(0x401234, 0x0000_0002, 0x8000_0000, 0x4);
    let kernel = from_c(&action);
    assert_eq!(kernel.handler, 0x401234);
    assert_eq!(kernel.mask, [0x0000_0002, 0x8000_0000]);
    assert_eq!(kernel.flags, 4);
    assert_eq!(kernel.restorer, 0);
    assert_eq!(core::mem::size_of::<KernelSigAction>(), 40);
}

#[test]
fn kernel_action_round_trips_only_the_128_bit_mask() {
    let kernel = KernelSigAction {
        handler: 0x401234,
        mask: [0x11, 0x22],
        flags: 0x4,
        restorer: 0,
    };
    let mut out = Sigaction::ZERO;
    out.sa_mask.bytes = [0xAA; 16];
    to_c(&kernel, &mut out);
    assert_eq!(out.sa_handler as usize, 0x401234);
    assert_eq!(out.sa_mask.bytes[0], 0x11);
    assert_eq!(out.sa_mask.bytes[1], 0x22);
    assert_eq!(out.sa_flags, 4);
    assert!(out.sa_restorer.is_null());
    assert!(out.sa_mask.bytes[2..].iter().all(|word| *word == 0));
    assert_eq!(
        out.sa_mask,
        SigsetT {
            bytes: [0x11, 0x22, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
        }
    );
}

#[test]
fn sig_dfl_and_ignore_are_preserved() {
    for handler in [0usize, 1] {
        let action = c_action(handler, 0, 0, 0);
        let kernel = from_c(&action);
        let mut out = Sigaction::ZERO;
        to_c(&kernel, &mut out);
        assert_eq!(out.sa_handler as usize, handler);
    }
}
