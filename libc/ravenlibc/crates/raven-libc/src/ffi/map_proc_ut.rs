// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use core::ffi::c_int;

use raven_libc_abi::structs::Rusage;
use raven_libc_abi::structs::{PosixSpawnFileActionsT, PosixSpawnattrT};
use raven_libc_proc::errors::{EACCES, ECHILD, ENOENT, ENOSYS};
use raven_libc_proc::fake::FakeProcd;
use raven_libc_proc::local_backend::LocalBackend;
use raven_libc_proc::wire::Op;

struct TestEnv;

impl SpawnEnv for TestEnv {
    fn open(&mut self, _path: &[u8], _oflag: i32, _mode: u32) -> Result<u32, ProcErrno> {
        Err(EACCES)
    }

    fn is_dir(&mut self, _handle: u32) -> Result<bool, ProcErrno> {
        Ok(false)
    }

    fn close_temp(&mut self, _handle: u32) {}

    fn fd_path(&mut self, _handle: u32) -> Option<alloc::vec::Vec<u8>> {
        None
    }
}

fn context() -> SpawnCtx<'static> {
    SpawnCtx {
        table: &[],
        cwd: b"/",
        sigmask: [0; 128],
        ignored: [0; 128],
        umask: 0o022,
    }
}

#[test]
fn fork_is_enosys_without_procd() {
    let backend = LocalBackend::new(None);
    assert_eq!(fork(&backend), Err(ENOSYS));
}

#[test]
fn fake_backend_forks() {
    let backend = FakeProcd::new();
    assert_eq!(fork(&backend), Ok(100));
}

#[test]
fn exec_reports_the_backend_error() {
    let backend = FakeProcd::new();
    backend.fail_next(Op::Exec, ENOENT);
    assert_eq!(exec(&backend, b"/", b"/missing", &[], &[], None), ENOENT);
    backend.fail_next(Op::Exec, EACCES);
    assert_eq!(exec(&backend, b"/", b"/noexec", &[], &[], None), EACCES);
}

#[test]
fn wait_without_children_is_echild() {
    let backend = LocalBackend::new(None);
    assert_eq!(
        wait(&backend, core::ptr::null_mut(), core::ptr::null_mut()),
        Err(ECHILD)
    );
}

#[test]
fn wait_copies_the_status_and_zeroes_usage() {
    let backend = FakeProcd::new();
    backend.push_wait_result(Ok(wait_result(7, 0x100)));
    let mut status = 0 as c_int;
    let mut usage = Rusage::ZERO;
    assert_eq!(
        wait_status(&backend, P_ALL, 0, 0, &mut status, &mut usage),
        Ok(7)
    );
    assert_eq!(status, 0x100);
    assert_eq!(usage, Rusage::ZERO);
}

#[test]
fn spawn_with_the_fake_backend_returns_a_pid() {
    let backend = FakeProcd::new();
    let mut env = TestEnv;
    let ctx = context();
    assert_eq!(
        spawn(
            &backend,
            &mut env,
            &ctx,
            b"/bin/true",
            false,
            None,
            None,
            None,
            &[],
            &[]
        ),
        Ok(100)
    );
}

#[test]
fn failing_file_action_spawns_nothing() {
    let backend = FakeProcd::new();
    let mut actions = FileActions::new();
    actions.add_close(1).expect("valid close");
    actions
        .add_open(2, b"/missing", 0, 0)
        .expect("recorded action");
    let mut env = TestEnv;
    let ctx = context();
    assert_eq!(
        spawn(
            &backend,
            &mut env,
            &ctx,
            b"/bin/true",
            false,
            None,
            Some(&actions),
            None,
            &[],
            &[],
        ),
        Err(EACCES)
    );
    assert!(backend.requests().is_empty());
}

#[test]
fn attribute_view_is_the_abi_size() {
    assert_eq!(core::mem::size_of::<SpawnAttrView>(), 336);
    assert_eq!(core::mem::offset_of!(SpawnAttrView, pgroup), 4);
    assert_eq!(core::mem::offset_of!(SpawnAttrView, sigdefault), 8);
    assert_eq!(core::mem::offset_of!(SpawnAttrView, sigmask), 136);
    assert_eq!(core::mem::offset_of!(SpawnAttrView, sched_policy), 264);
    assert_eq!(core::mem::size_of::<PosixSpawnattrT>(), 336);
}

#[test]
fn attribute_round_trip() {
    let mut blob = PosixSpawnattrT::ZERO;
    // SAFETY: the local blob is 336 bytes and writable.
    let view = unsafe { attr_view(&mut blob) };
    let mut model = attr_model(view);
    model.set_flags(0x0a).expect("known flags");
    model.set_pgroup(9);
    model.sched_policy = raven_libc_proc::consts::SCHED_OTHER;
    attr_store(view, &model);
    let back = attr_model(view);
    assert_eq!(back.flags, 0x0a);
    assert_eq!(back.pgroup, 9);
    assert_eq!(back.sched_policy, raven_libc_proc::consts::SCHED_OTHER);
}

#[test]
fn file_actions_lifecycle() {
    assert_eq!(core::mem::size_of::<FileActionsView>(), 80);
    assert_eq!(core::mem::size_of::<PosixSpawnFileActionsT>(), 80);
    let mut blob = PosixSpawnFileActionsT::ZERO;
    // SAFETY: the local blob is 80 bytes and writable.
    let view = unsafe { actions_view(&mut blob) };
    view.init();
    assert!(view.get().is_none());
    view.load().expect("allocates").add_close(1).expect("valid");
    assert_eq!(view.get().expect("present").len(), 1);
    view.destroy();
    assert!(view.get().is_none());
}

#[test]
fn scheduler_table() {
    assert_eq!(
        sched_bounds(raven_libc_proc::consts::SCHED_OTHER),
        Ok((0, 0))
    );
    assert_eq!(
        sched_bounds(raven_libc_proc::consts::SCHED_BATCH),
        Ok((0, 0))
    );
    assert_eq!(
        sched_bounds(raven_libc_proc::consts::SCHED_FIFO),
        Ok((99, 1))
    );
    assert_eq!(sched_bounds(raven_libc_proc::consts::SCHED_RR), Ok((99, 1)));
    assert_eq!(sched_bounds(42), Err(EINVAL));
}

#[test]
fn affinity_reports_cpu_zero() {
    let mut set = [0xffu8; 16];
    assert_eq!(fill_affinity(&mut set), Ok(()));
    assert_eq!(set[0], 1);
    assert_eq!(set[1], 0);
    let mut tiny = [0u8; 4];
    assert_eq!(fill_affinity(&mut tiny), Err(EINVAL));
}

#[test]
fn rlimit_defaults_and_view() {
    let backend = LocalBackend::new(None);
    let pair = rlimit(
        &backend,
        0,
        raven_libc_proc::consts::RLIMIT_NOFILE as c_int,
        core::ptr::null(),
    )
    .expect("default table");
    assert_eq!(pair, (1024, 32768));
    let view = limit(pair);
    assert_eq!(view.rlim_cur, 1024);
    assert_eq!(view.rlim_max, 32768);
}

#[test]
fn times_converts_ticks() {
    assert_eq!(core::mem::size_of::<Tms>(), 32);
    let mut tms = Tms::default();
    assert_eq!(times(&mut tms, 10, 500_000, 100), Ok(1050));
    assert!(tms == Tms::default());
}

#[test]
fn attribute_rejects_unknown_flags() {
    let mut blob = PosixSpawnattrT::ZERO;
    // SAFETY: the local blob is 336 bytes and writable.
    let view = unsafe { attr_view(&mut blob) };
    let mut model = attr_model(view);
    assert_eq!(model.set_flags(0x4000), Err(EINVAL));
}

#[test]
fn groups_go_through_the_backend() {
    let backend = LocalBackend::new(None);
    assert_eq!(getgroups(&backend, 0, core::ptr::null_mut()), Ok(0));
    let mut list = [0u32; 4];
    assert_eq!(getgroups(&backend, 4, list.as_mut_ptr()), Ok(0));
    assert_eq!(setgroups(&backend, 0, core::ptr::null()), Err(ENOSYS));
}
