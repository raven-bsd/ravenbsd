// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn full_caps_have_every_primitive() {
    let caps = KernelCaps::full();
    for flag in ALL_CAPS {
        assert!(caps.get(flag), "{flag:?}");
    }
}

#[test]
fn minimal_caps_have_none() {
    let caps = KernelCaps::minimal();
    for flag in ALL_CAPS {
        assert!(!caps.get(flag), "{flag:?}");
    }
}

#[test]
fn with_toggles_one_flag() {
    let caps = KernelCaps::minimal().with(CapFlag::Requeue, true);
    assert!(caps.requeue);
    assert!(!caps.tls);
    for flag in ALL_CAPS {
        if flag != CapFlag::Requeue {
            assert!(!caps.get(flag), "{flag:?}");
        }
    }
}

#[test]
fn each_flag_is_addressable() {
    for flag in ALL_CAPS {
        let caps = KernelCaps::minimal().with(flag, true);
        assert!(caps.get(flag));
        assert!(!caps.with(flag, false).get(flag));
    }
}

#[test]
fn default_caps_are_all_false() {
    let caps = KernelCaps::default();
    assert_eq!(caps, KernelCaps::minimal());
}

#[test]
fn all_caps_lists_twelve_flags_in_matrix_order() {
    assert_eq!(ALL_CAPS.len(), 12);
    assert_eq!(ALL_CAPS[0], CapFlag::Tls);
    assert_eq!(ALL_CAPS[11], CapFlag::ProcFork);
}

#[test]
fn create_args_is_copy_and_plain_data() {
    let args = CreateArgs {
        entry: 1,
        stack_top: 2,
        arg: 3,
        tp: 4,
        clear_tid_addr: 5,
    };
    let copy = args;
    assert_eq!(args, copy);
    assert_eq!(
        core::mem::size_of::<CreateArgs>(),
        5 * core::mem::size_of::<usize>()
    );
}

#[test]
fn the_default_exit_ex_falls_back_to_exit() {
    struct Minimal;
    impl ThreadKernel for Minimal {
        fn caps(&self) -> KernelCaps {
            KernelCaps::minimal()
        }
        fn thread_pointer(&self) -> usize {
            0
        }
        fn set_thread_pointer(&self, _tp: usize) -> Result<(), Errno> {
            Err(crate::limits::ENOSYS)
        }
        fn tid_self(&self) -> Result<i32, Errno> {
            Err(crate::limits::ENOSYS)
        }
        fn create(&self, _args: &CreateArgs) -> Result<u32, Errno> {
            Err(crate::limits::ENOSYS)
        }
        fn exit(&self, _code: i32) -> ! {
            panic!("exit")
        }
        fn futex_wait(&self, _w: &AtomicU32, _e: u32, _r: Option<u64>) -> Result<(), Errno> {
            Ok(())
        }
        fn futex_wake(&self, _w: &AtomicU32, _n: u32) -> u32 {
            0
        }
        fn futex_requeue(
            &self,
            _w: &AtomicU32,
            _a: u32,
            _t: &AtomicU32,
            _m: u32,
        ) -> Result<u32, Errno> {
            Err(crate::limits::ENOSYS)
        }
        fn map(&self, _len: usize) -> Result<usize, Errno> {
            Err(crate::limits::ENOMEM)
        }
        fn unmap(&self, _a: usize, _l: usize) -> Result<(), Errno> {
            Ok(())
        }
        fn protect_none(&self, _a: usize, _l: usize) -> Result<(), Errno> {
            Ok(())
        }
        fn kick(&self, _h: u32, _b: u64) -> Result<(), Errno> {
            Err(crate::limits::ENOSYS)
        }
        fn yield_now(&self) {}
        fn now_ns(&self, _clock: u32) -> u64 {
            0
        }
        fn set_robust_list(&self, _head: usize) -> Result<(), Errno> {
            Err(crate::limits::ENOSYS)
        }
        fn start_trampoline(&self) -> fn(usize) -> ! {
            |_| panic!("trampoline")
        }
    }
    let kernel = Minimal;
    assert!(kernel.thread_is_dead(0));
    assert_eq!(kernel.fork_process(), Err(crate::limits::ENOSYS));
    assert!(!kernel.caps().tls);
}
