// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Dispatcher behaviour tests.

use std::any::Any;

use raven_linux_abi::sysnums;
use raven_linux_dispatch::{
    Arch, Dispatcher, Errno, LinuxContext, RegisterError, SyscallArgs, ret,
};

struct Context {
    seen_nr: Option<usize>,
    seen_args: [usize; 6],
    value: i32,
}

impl LinuxContext for Context {
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

fn record(context: &mut dyn LinuxContext, args: &SyscallArgs) -> isize {
    let context = context
        .as_any_mut()
        .downcast_mut::<Context>()
        .expect("context type");
    context.seen_nr = Some(args.nr);
    context.seen_args = args.a;
    context.value as isize
}

fn fail_with_eperm(_context: &mut dyn LinuxContext, _args: &SyscallArgs) -> isize {
    ret(Err(Errno::EPERM))
}

fn context() -> Context {
    Context {
        seen_nr: None,
        seen_args: [0; 6],
        value: 9,
    }
}

fn number(arch: Arch, name: &str) -> usize {
    sysnums::number(arch, name).unwrap() as usize
}

#[test]
fn x86_64_dispatches_write_with_the_arguments() {
    let mut dispatcher = Dispatcher::new(Arch::X86_64);
    dispatcher.register("write", record).unwrap();
    let mut context = context();
    let args = SyscallArgs {
        nr: number(Arch::X86_64, "write"),
        a: [1, 2, 3, 4, 5, 6],
    };
    assert_eq!(dispatcher.dispatch(&mut context, &args), 9);
    assert_eq!(context.seen_nr, Some(1));
    assert_eq!(context.seen_args, [1, 2, 3, 4, 5, 6]);
}

#[test]
fn aarch64_uses_the_generic_write_number() {
    assert_eq!(sysnums::number(Arch::Aarch64, "write"), Some(64));
    let mut dispatcher = Dispatcher::new(Arch::Aarch64);
    dispatcher.register("write", record).unwrap();
    let mut context = context();
    let args = SyscallArgs {
        nr: 64,
        a: [1, 0, 0, 0, 0, 0],
    };
    assert_eq!(dispatcher.dispatch(&mut context, &args), 9);
}

#[test]
fn unknown_names_are_rejected() {
    let mut dispatcher = Dispatcher::new(Arch::X86_64);
    assert_eq!(
        dispatcher.register("not_a_syscall", record),
        Err(RegisterError::UnknownName)
    );
}

#[test]
fn duplicate_registration_is_rejected() {
    let mut dispatcher = Dispatcher::new(Arch::X86_64);
    dispatcher.register("write", record).unwrap();
    assert_eq!(
        dispatcher.register("write", record),
        Err(RegisterError::Duplicate)
    );
}

#[test]
fn unregistered_known_number_returns_enosys_and_tracks_the_name() {
    let mut dispatcher = Dispatcher::new(Arch::X86_64);
    let nr = number(Arch::X86_64, "read");
    let args = SyscallArgs { nr, a: [0; 6] };
    let mut context = context();
    assert_eq!(dispatcher.dispatch(&mut context, &args), -38);
    assert_eq!(dispatcher.unimplemented_hits(), vec![(nr, Some("read"), 1)]);
    assert_eq!(dispatcher.dispatch(&mut context, &args), -38);
    assert_eq!(dispatcher.unimplemented_hits(), vec![(nr, Some("read"), 2)]);
}

#[test]
fn out_of_range_numbers_have_no_name() {
    let mut dispatcher = Dispatcher::new(Arch::X86_64);
    let mut context = context();
    let args = SyscallArgs {
        nr: u32::MAX as usize + 1,
        a: [0; 6],
    };
    assert_eq!(dispatcher.dispatch(&mut context, &args), -38);
    assert_eq!(
        dispatcher.unimplemented_hits(),
        vec![(u32::MAX as usize + 1, None, 1)]
    );
}

#[test]
fn implemented_names_are_sorted() {
    let mut dispatcher = Dispatcher::new(Arch::X86_64);
    dispatcher.register("write", record).unwrap();
    dispatcher.register("getpid", record).unwrap();
    dispatcher.register("read", record).unwrap();
    assert_eq!(
        dispatcher.implemented_names(),
        vec!["getpid", "read", "write"]
    );
}

#[test]
fn stats_count_calls_and_negative_results() {
    let mut dispatcher = Dispatcher::new(Arch::X86_64);
    dispatcher.register("write", record).unwrap();
    dispatcher.register("getpid", fail_with_eperm).unwrap();
    let mut context = context();
    let write = SyscallArgs {
        nr: number(Arch::X86_64, "write"),
        a: [0; 6],
    };
    let getpid = SyscallArgs {
        nr: number(Arch::X86_64, "getpid"),
        a: [0; 6],
    };
    assert_eq!(dispatcher.dispatch(&mut context, &write), 9);
    assert_eq!(dispatcher.dispatch(&mut context, &write), 9);
    assert_eq!(dispatcher.dispatch(&mut context, &getpid), -1);
    assert_eq!(dispatcher.stats(), vec![("getpid", 1, 1), ("write", 2, 0)]);
}

#[test]
fn ret_maps_success_and_errno() {
    assert_eq!(ret(Ok(0)), 0);
    assert_eq!(ret(Ok(42)), 42);
    assert_eq!(ret(Ok(isize::MAX as usize)), isize::MAX);
    assert_eq!(ret(Err(Errno::EPERM)), -1);
    assert_eq!(ret(Err(Errno::EOVERFLOW)), -75);
}

#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "exceeds isize::MAX")]
fn ret_rejects_oversized_success_in_debug() {
    let _ = ret(Ok(usize::MAX));
}

#[test]
#[cfg(not(debug_assertions))]
fn ret_clamps_oversized_success_in_release() {
    assert_eq!(ret(Ok(usize::MAX)), -(Errno::EOVERFLOW.0 as isize));
}

#[test]
fn int_and_fd_truncate_and_sign_extend() {
    let args = SyscallArgs {
        nr: 0,
        a: [
            0xffff_ffff_0000_0001,
            0xffff_ffff_ffff_ffff,
            0x8000_0000,
            0,
            0,
            0,
        ],
    };
    assert_eq!(args.int(0), 1);
    assert_eq!(args.int(1), -1);
    assert_eq!(args.int(2), i32::MIN);
    assert_eq!(args.fd(1), -1);
}

#[test]
fn long_sign_extends_pointer_width_values() {
    let args = SyscallArgs {
        nr: 0,
        a: [0xffff_ffff_ffff_ffff, 0x7fff_ffff_ffff_ffff, 0, 0, 0, 0],
    };
    assert_eq!(args.long(0), -1);
    assert_eq!(args.long(1), i64::MAX);
}

#[test]
fn ptr_and_size_are_pointer_sized() {
    let args = SyscallArgs {
        nr: 0,
        a: [0xdead_beef, 4096, 0, 0, 0, 0],
    };
    assert_eq!(args.ptr(0), 0xdead_beef);
    assert_eq!(args.size(1), 4096);
}

#[test]
fn out_of_range_accessors_return_zero() {
    let args = SyscallArgs {
        nr: 0,
        a: [1, 2, 3, 4, 5, 6],
    };
    assert_eq!(args.int(6), 0);
    assert_eq!(args.long(6), 0);
    assert_eq!(args.ptr(100), 0);
    assert_eq!(args.fd(100), 0);
    assert_eq!(args.size(100), 0);
}

#[test]
fn context_downcast_reaches_the_concrete_type() {
    let mut dispatcher = Dispatcher::new(Arch::X86_64);
    dispatcher.register("getpid", record).unwrap();
    let mut context = context();
    let args = SyscallArgs {
        nr: number(Arch::X86_64, "getpid"),
        a: [0; 6],
    };
    assert_eq!(dispatcher.dispatch(&mut context, &args), 9);
    assert_eq!(context.seen_nr, Some(args.nr));
}

#[test]
fn riscv64_table_is_available() {
    assert_eq!(sysnums::number(Arch::Riscv64, "write"), Some(64));
    let mut dispatcher = Dispatcher::new(Arch::Riscv64);
    dispatcher.register("write", record).unwrap();
    assert_eq!(dispatcher.implemented_names(), vec!["write"]);
}

#[test]
fn unimplemented_hits_are_sorted_by_number() {
    let mut dispatcher = Dispatcher::new(Arch::X86_64);
    let mut context = context();
    for nr in [200usize, 20, 200] {
        let args = SyscallArgs { nr, a: [0; 6] };
        assert_eq!(dispatcher.dispatch(&mut context, &args), -38);
    }
    let hits = dispatcher.unimplemented_hits();
    assert_eq!(hits[0].0, 20);
    assert_eq!(hits[1].0, 200);
    assert_eq!(hits[1].2, 2);
}
