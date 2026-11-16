// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Clone flag, frame and trampoline tests.

use raven_linux_abi::flags::clone as flags;
use raven_proto_linux::LinuxFrame;

use super::*;

fn frame() -> LinuxFrame {
    let mut gprs = [0u64; FRAME_GPRS];
    for (index, value) in gprs.iter_mut().enumerate() {
        *value = 0x1000 + index as u64;
    }
    LinuxFrame {
        rip: 0x40_1234,
        rsp: 0x7fff_0000,
        rflags: 0x202,
        fs_base: 0x7000,
        gprs,
    }
}

fn request(flags: u64, stack: u64) -> CloneRequest {
    CloneRequest {
        flags,
        child_stack: stack,
        ptid: 0,
        ctid: 0,
        tls: 0,
        frame: frame(),
    }
}

#[test]
fn thread_clone_needs_vm_sighandler_and_an_aligned_stack() {
    let good = flags::CLONE_VM | flags::CLONE_THREAD | flags::CLONE_SIGHAND;
    assert_eq!(classify(&request(good, 0x8000)), Ok(CloneKind::Thread));
    assert_eq!(
        classify(&request(flags::CLONE_VM | flags::CLONE_SIGHAND, 0x8000)),
        Err(raven_abi::errno::Errno::EINVAL)
    );
    assert_eq!(
        classify(&request(flags::CLONE_THREAD | flags::CLONE_SIGHAND, 0x8000)),
        Err(raven_abi::errno::Errno::EINVAL)
    );
    assert_eq!(
        classify(&request(good, 0)),
        Err(raven_abi::errno::Errno::EINVAL)
    );
    assert_eq!(
        classify(&request(good, 0x8008)),
        Err(raven_abi::errno::Errno::EINVAL)
    );
}

#[test]
fn fork_clone_rejects_shared_vm_and_child_settid() {
    assert_eq!(classify(&request(17, 0)), Ok(CloneKind::Fork));
    assert_eq!(
        classify(&request(17 | flags::CLONE_VM, 0)),
        Err(raven_abi::errno::Errno::EINVAL)
    );
    assert_eq!(
        classify(&request(17 | flags::CLONE_CHILD_SETTID, 0)),
        Err(raven_abi::errno::Errno::EINVAL)
    );
    assert_eq!(
        classify(&request(17 | flags::CLONE_FILES, 0)),
        Err(raven_abi::errno::Errno::EINVAL)
    );
    assert_eq!(
        classify(&request(0xdead_beef, 0)),
        Err(raven_abi::errno::Errno::EINVAL)
    );
}

#[test]
fn child_frames_zero_rax_and_choose_the_stack() {
    let thread = child_frame(
        &request(
            flags::CLONE_VM | flags::CLONE_THREAD | flags::CLONE_SIGHAND,
            0x9000,
        ),
        CloneKind::Thread,
    )
    .expect("a thread frame");
    assert_eq!(thread.gprs[0], 0);
    assert_eq!(thread.rsp, 0x9000);
    assert_eq!(thread.rip, 0x40_1234);
    assert_eq!(thread.gprs[1], 0x1001);

    let fork = child_frame(&request(17, 0), CloneKind::Fork).expect("a fork frame");
    assert_eq!(fork.gprs[0], 0);
    assert_eq!(fork.rsp, 0x7fff_0000);

    let alternate = child_frame(&request(17, 0x9000), CloneKind::Fork).expect("a fork frame");
    assert_eq!(alternate.rsp, 0x9000);
}

#[test]
fn the_trampoline_loads_every_register_and_jumps() {
    let frame = frame();
    let page = trampoline_page(&frame);
    assert_eq!(CODE_LEN, FRAME_GPRS * 7 + 7 + 6);
    // First instruction: mov rax, [rip+0x100-7].
    assert_eq!(page[..3], [0x48, 0x8b, 0x05]);
    assert_eq!(
        i32::from_le_bytes(page[3..7].try_into().unwrap()),
        (FRAME_OFFSET - 7) as i32
    );
    // The last general-register load is r15: mov r15, [rip+...].
    let last = (FRAME_GPRS - 1) * 7;
    assert_eq!(page[last..last + 3], [0x4c, 0x8b, 0x3d]);
    // Then RSP and the indirect jump.
    let rsp = FRAME_GPRS * 7;
    assert_eq!(page[rsp..rsp + 3], [0x48, 0x8b, 0x25]);
    let jump = rsp + 7;
    assert_eq!(page[jump..jump + 2], [0xff, 0x25]);
    assert_eq!(jump + 6, CODE_LEN);
    // The frame table holds RAX=..., RSP and RIP at their documented indexes.
    let word = |index: usize| {
        let offset = FRAME_OFFSET + index * 8;
        u64::from_le_bytes(page[offset..offset + 8].try_into().unwrap())
    };
    assert_eq!(word(0), frame.gprs[0]);
    assert_eq!(word(FRAME_GPRS - 1), frame.gprs[FRAME_GPRS - 1]);
    assert_eq!(word(RSP_INDEX), frame.rsp);
    assert_eq!(word(RIP_INDEX), frame.rip);
}

#[test]
fn the_trampoline_rejects_a_zero_frame() {
    let mut bad = request(
        flags::CLONE_VM | flags::CLONE_THREAD | flags::CLONE_SIGHAND,
        0x8000,
    );
    bad.frame.rip = 0;
    assert_eq!(
        child_frame(&bad, CloneKind::Thread),
        Err(raven_abi::errno::Errno::EINVAL)
    );
}
