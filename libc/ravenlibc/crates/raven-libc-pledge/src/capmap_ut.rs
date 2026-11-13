// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the capability plan.

use super::*;
use crate::promise::parse_promises;

fn set(text: &[u8]) -> PromiseSet {
    parse_promises(text).expect("promises")
}

macro_rules! case {
    ($name:ident, $text:expr, $vfs:expr, $net:expr, $procd:expr, $keep:expr, $drop:expr, $phase:expr, $getpw:expr) => {
        #[test]
        fn $name() {
            let plan = plan(set($text));
            assert_eq!(plan.vfs.0, $vfs, "vfs");
            assert_eq!(plan.net.0, $net, "net");
            assert_eq!(plan.procd.0, $procd, "procd");
            assert_eq!(plan.keep_ns.0, $keep, "keep");
            assert_eq!(plan.drop_transfer, $drop, "drop");
            assert_eq!(plan.phase, $phase, "phase");
            assert_eq!(plan.getpw, $getpw, "getpw");
        }
    };
}

case!(empty_plans_nothing, b"", 0, 0, 0, 0, true, 1, false);
case!(stdio_is_recorded, b"stdio", 0, 0, 0, 0, true, 1, false);
case!(
    rpath_reads,
    b"rpath",
    VfsMask::READ,
    0,
    0,
    NsKeep::FILE,
    true,
    2,
    false
);
case!(
    wpath_writes,
    b"wpath",
    VfsMask::WRITE,
    0,
    0,
    0,
    true,
    2,
    false
);
case!(
    cpath_creates,
    b"cpath",
    VfsMask::CREATE,
    0,
    0,
    0,
    true,
    2,
    false
);
case!(
    dpath_devnodes,
    b"dpath",
    VfsMask::DEVNODE,
    0,
    0,
    0,
    true,
    2,
    false
);
case!(
    fattr_attributes,
    b"fattr",
    VfsMask::ATTR,
    0,
    0,
    0,
    true,
    2,
    false
);
case!(
    chown_changes_owner,
    b"chown",
    VfsMask::CHOWN,
    0,
    0,
    0,
    true,
    2,
    false
);
case!(
    flock_locks,
    b"flock",
    VfsMask::FLOCK,
    0,
    0,
    0,
    true,
    2,
    false
);
case!(
    inet_networks,
    b"inet",
    0,
    NetMask::INET,
    0,
    NsKeep::NET,
    true,
    2,
    false
);
case!(
    unix_local_sockets,
    b"unix",
    0,
    NetMask::UNIX,
    0,
    0,
    true,
    2,
    false
);
case!(dns_resolves, b"dns", 0, NetMask::DNS, 0, 0, true, 2, false);
case!(
    mcast_multicasts,
    b"mcast",
    0,
    NetMask::MCAST,
    0,
    0,
    true,
    3,
    false
);
case!(
    route_reads,
    b"route",
    0,
    NetMask::ROUTE,
    0,
    0,
    true,
    3,
    false
);
case!(
    wroute_writes,
    b"wroute",
    0,
    NetMask::WROUTE,
    0,
    0,
    true,
    3,
    false
);
case!(
    sendfd_alone_drops_transfer,
    b"sendfd",
    0,
    0,
    0,
    0,
    true,
    2,
    false
);
case!(
    recvfd_alone_drops_transfer,
    b"recvfd",
    0,
    0,
    0,
    0,
    true,
    2,
    false
);
case!(
    sendfd_and_recvfd_keep_transfer,
    b"sendfd recvfd",
    0,
    0,
    0,
    0,
    false,
    2,
    false
);
case!(
    proc_allows_process_ops,
    b"proc",
    0,
    0,
    ProcdOps::PROC,
    0,
    true,
    1,
    false
);
case!(
    exec_allows_exec,
    b"exec",
    0,
    0,
    ProcdOps::EXEC,
    0,
    true,
    1,
    false
);
case!(
    id_allows_setcred,
    b"id",
    0,
    0,
    ProcdOps::ID,
    0,
    true,
    1,
    false
);
case!(
    prot_exec_is_recorded,
    b"prot_exec",
    0,
    0,
    0,
    0,
    true,
    3,
    false
);
case!(tty_is_recorded, b"tty", 0, 0, 0, 0, true, 2, false);
case!(getpw_is_an_exception, b"getpw", 0, 0, 0, 0, true, 2, true);
case!(
    settime_sets_the_clock,
    b"settime",
    0,
    0,
    ProcdOps::SETTIME,
    0,
    true,
    3,
    false
);
case!(ps_inspects, b"ps", 0, 0, ProcdOps::PS, 0, true, 3, false);
case!(
    vminfo_reads_memory,
    b"vminfo",
    0,
    0,
    ProcdOps::VMINFO,
    0,
    true,
    3,
    false
);
case!(pf_is_recorded, b"pf", 0, 0, 0, 0, true, 3, false);
case!(bpf_is_recorded, b"bpf", 0, 0, 0, 0, true, 3, false);
case!(audio_is_recorded, b"audio", 0, 0, 0, 0, true, 3, false);
case!(video_is_recorded, b"video", 0, 0, 0, 0, true, 3, false);
case!(drm_is_recorded, b"drm", 0, 0, 0, 0, true, 3, false);
case!(vmm_is_recorded, b"vmm", 0, 0, 0, 0, true, 3, false);
case!(tape_is_recorded, b"tape", 0, 0, 0, 0, true, 3, false);
case!(
    disklabel_is_recorded,
    b"disklabel",
    0,
    0,
    0,
    0,
    true,
    3,
    false
);
case!(unveil_is_recorded, b"unveil", 0, 0, 0, 0, true, 1, false);
case!(error_is_recorded, b"error", 0, 0, 0, 0, true, 2, false);

#[test]
fn a_union_sets_every_bit() {
    let plan = plan(set(b"rpath wpath inet proc"));
    assert_eq!(plan.vfs.0, VfsMask::READ | VfsMask::WRITE);
    assert_eq!(plan.net.0, NetMask::INET);
    assert_eq!(plan.procd.0, ProcdOps::PROC);
    assert_eq!(plan.keep_ns.0, NsKeep::FILE | NsKeep::NET);
}

#[test]
fn the_empty_set_is_the_baseline() {
    let plan = plan(PromiseSet::empty());
    assert_eq!(plan.vfs.0, 0);
    assert_eq!(plan.net.0, 0);
    assert_eq!(plan.procd.0, 0);
    assert_eq!(plan.keep_ns.0, 0);
    assert!(plan.drop_transfer);
    assert_eq!(plan.phase, 1);
    assert!(!plan.getpw);
}

#[test]
fn a_phase_three_promise_dominates() {
    let plan = plan(set(b"rpath route proc"));
    assert_eq!(plan.phase, 3);
}

#[test]
fn transfer_is_dropped_unless_both_names_are_present() {
    assert!(!plan(set(b"sendfd recvfd proc")).drop_transfer);
    assert!(plan(set(b"sendfd proc")).drop_transfer);
}
