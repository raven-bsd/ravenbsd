// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! One test per `_SC_*` id: a value or the documented `EINVAL`.

use raven_abi::errno::Errno;
use raven_libc_abi::sysconf;

use crate::conf::{SysconfInputs, sysconf as evaluate};

fn inputs() -> SysconfInputs {
    SysconfInputs::default()
}

#[test]
fn sc_2_char_term() {
    let value = evaluate(sysconf::_SC_2_CHAR_TERM, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_2_CHAR_TERM: {value:?}"
    );
}

#[test]
fn sc_2_c_bind() {
    let value = evaluate(sysconf::_SC_2_C_BIND, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_2_C_BIND: {value:?}"
    );
}

#[test]
fn sc_2_c_dev() {
    let value = evaluate(sysconf::_SC_2_C_DEV, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_2_C_DEV: {value:?}"
    );
}

#[test]
fn sc_2_fort_dev() {
    let value = evaluate(sysconf::_SC_2_FORT_DEV, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_2_FORT_DEV: {value:?}"
    );
}

#[test]
fn sc_2_fort_run() {
    let value = evaluate(sysconf::_SC_2_FORT_RUN, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_2_FORT_RUN: {value:?}"
    );
}

#[test]
fn sc_2_localedef() {
    let value = evaluate(sysconf::_SC_2_LOCALEDEF, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_2_LOCALEDEF: {value:?}"
    );
}

#[test]
fn sc_2_pbs() {
    let value = evaluate(sysconf::_SC_2_PBS, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_2_PBS: {value:?}"
    );
}

#[test]
fn sc_2_pbs_accounting() {
    let value = evaluate(sysconf::_SC_2_PBS_ACCOUNTING, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_2_PBS_ACCOUNTING: {value:?}"
    );
}

#[test]
fn sc_2_pbs_checkpoint() {
    let value = evaluate(sysconf::_SC_2_PBS_CHECKPOINT, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_2_PBS_CHECKPOINT: {value:?}"
    );
}

#[test]
fn sc_2_pbs_locate() {
    let value = evaluate(sysconf::_SC_2_PBS_LOCATE, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_2_PBS_LOCATE: {value:?}"
    );
}

#[test]
fn sc_2_pbs_message() {
    let value = evaluate(sysconf::_SC_2_PBS_MESSAGE, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_2_PBS_MESSAGE: {value:?}"
    );
}

#[test]
fn sc_2_pbs_track() {
    let value = evaluate(sysconf::_SC_2_PBS_TRACK, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_2_PBS_TRACK: {value:?}"
    );
}

#[test]
fn sc_2_sw_dev() {
    let value = evaluate(sysconf::_SC_2_SW_DEV, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_2_SW_DEV: {value:?}"
    );
}

#[test]
fn sc_2_upe() {
    let value = evaluate(sysconf::_SC_2_UPE, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_2_UPE: {value:?}"
    );
}

#[test]
fn sc_2_version() {
    let value = evaluate(sysconf::_SC_2_VERSION, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_2_VERSION: {value:?}"
    );
}

#[test]
fn sc_advisory_info() {
    let value = evaluate(sysconf::_SC_ADVISORY_INFO, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_ADVISORY_INFO: {value:?}"
    );
}

#[test]
fn sc_aio_listio_max() {
    let value = evaluate(sysconf::_SC_AIO_LISTIO_MAX, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_AIO_LISTIO_MAX: {value:?}"
    );
}

#[test]
fn sc_aio_max() {
    let value = evaluate(sysconf::_SC_AIO_MAX, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_AIO_MAX: {value:?}"
    );
}

#[test]
fn sc_aio_prio_delta_max() {
    let value = evaluate(sysconf::_SC_AIO_PRIO_DELTA_MAX, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_AIO_PRIO_DELTA_MAX: {value:?}"
    );
}

#[test]
fn sc_arg_max() {
    let value = evaluate(sysconf::_SC_ARG_MAX, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_ARG_MAX: {value:?}"
    );
}

#[test]
fn sc_asynchronous_io() {
    let value = evaluate(sysconf::_SC_ASYNCHRONOUS_IO, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_ASYNCHRONOUS_IO: {value:?}"
    );
}

#[test]
fn sc_atexit_max() {
    let value = evaluate(sysconf::_SC_ATEXIT_MAX, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_ATEXIT_MAX: {value:?}"
    );
}

#[test]
fn sc_avphys_pages() {
    let value = evaluate(sysconf::_SC_AVPHYS_PAGES, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_AVPHYS_PAGES: {value:?}"
    );
}

#[test]
fn sc_barriers() {
    let value = evaluate(sysconf::_SC_BARRIERS, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_BARRIERS: {value:?}"
    );
}

#[test]
fn sc_bc_base_max() {
    let value = evaluate(sysconf::_SC_BC_BASE_MAX, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_BC_BASE_MAX: {value:?}"
    );
}

#[test]
fn sc_bc_dim_max() {
    let value = evaluate(sysconf::_SC_BC_DIM_MAX, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_BC_DIM_MAX: {value:?}"
    );
}

#[test]
fn sc_bc_scale_max() {
    let value = evaluate(sysconf::_SC_BC_SCALE_MAX, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_BC_SCALE_MAX: {value:?}"
    );
}

#[test]
fn sc_bc_string_max() {
    let value = evaluate(sysconf::_SC_BC_STRING_MAX, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_BC_STRING_MAX: {value:?}"
    );
}

#[test]
fn sc_child_max() {
    let value = evaluate(sysconf::_SC_CHILD_MAX, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_CHILD_MAX: {value:?}"
    );
}

#[test]
fn sc_clk_tck() {
    let value = evaluate(sysconf::_SC_CLK_TCK, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_CLK_TCK: {value:?}"
    );
}

#[test]
fn sc_clock_selection() {
    let value = evaluate(sysconf::_SC_CLOCK_SELECTION, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_CLOCK_SELECTION: {value:?}"
    );
}

#[test]
fn sc_coll_weights_max() {
    let value = evaluate(sysconf::_SC_COLL_WEIGHTS_MAX, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_COLL_WEIGHTS_MAX: {value:?}"
    );
}

#[test]
fn sc_cputime() {
    let value = evaluate(sysconf::_SC_CPUTIME, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_CPUTIME: {value:?}"
    );
}

#[test]
fn sc_delaytimer_max() {
    let value = evaluate(sysconf::_SC_DELAYTIMER_MAX, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_DELAYTIMER_MAX: {value:?}"
    );
}

#[test]
fn sc_expr_nest_max() {
    let value = evaluate(sysconf::_SC_EXPR_NEST_MAX, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_EXPR_NEST_MAX: {value:?}"
    );
}

#[test]
fn sc_fsync() {
    let value = evaluate(sysconf::_SC_FSYNC, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_FSYNC: {value:?}"
    );
}

#[test]
fn sc_getgr_r_size_max() {
    let value = evaluate(sysconf::_SC_GETGR_R_SIZE_MAX, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_GETGR_R_SIZE_MAX: {value:?}"
    );
}

#[test]
fn sc_getpw_r_size_max() {
    let value = evaluate(sysconf::_SC_GETPW_R_SIZE_MAX, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_GETPW_R_SIZE_MAX: {value:?}"
    );
}

#[test]
fn sc_host_name_max() {
    let value = evaluate(sysconf::_SC_HOST_NAME_MAX, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_HOST_NAME_MAX: {value:?}"
    );
}

#[test]
fn sc_iov_max() {
    let value = evaluate(sysconf::_SC_IOV_MAX, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_IOV_MAX: {value:?}"
    );
}

#[test]
fn sc_ipv6() {
    let value = evaluate(sysconf::_SC_IPV6, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_IPV6: {value:?}"
    );
}

#[test]
fn sc_job_control() {
    let value = evaluate(sysconf::_SC_JOB_CONTROL, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_JOB_CONTROL: {value:?}"
    );
}

#[test]
fn sc_line_max() {
    let value = evaluate(sysconf::_SC_LINE_MAX, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_LINE_MAX: {value:?}"
    );
}

#[test]
fn sc_login_name_max() {
    let value = evaluate(sysconf::_SC_LOGIN_NAME_MAX, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_LOGIN_NAME_MAX: {value:?}"
    );
}

#[test]
fn sc_mapped_files() {
    let value = evaluate(sysconf::_SC_MAPPED_FILES, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_MAPPED_FILES: {value:?}"
    );
}

#[test]
fn sc_memlock() {
    let value = evaluate(sysconf::_SC_MEMLOCK, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_MEMLOCK: {value:?}"
    );
}

#[test]
fn sc_memlock_range() {
    let value = evaluate(sysconf::_SC_MEMLOCK_RANGE, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_MEMLOCK_RANGE: {value:?}"
    );
}

#[test]
fn sc_memory_protection() {
    let value = evaluate(sysconf::_SC_MEMORY_PROTECTION, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_MEMORY_PROTECTION: {value:?}"
    );
}

#[test]
fn sc_message_passing() {
    let value = evaluate(sysconf::_SC_MESSAGE_PASSING, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_MESSAGE_PASSING: {value:?}"
    );
}

#[test]
fn sc_minsigstksz() {
    let value = evaluate(sysconf::_SC_MINSIGSTKSZ, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_MINSIGSTKSZ: {value:?}"
    );
}

#[test]
fn sc_monotonic_clock() {
    let value = evaluate(sysconf::_SC_MONOTONIC_CLOCK, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_MONOTONIC_CLOCK: {value:?}"
    );
}

#[test]
fn sc_mq_open_max() {
    let value = evaluate(sysconf::_SC_MQ_OPEN_MAX, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_MQ_OPEN_MAX: {value:?}"
    );
}

#[test]
fn sc_mq_prio_max() {
    let value = evaluate(sysconf::_SC_MQ_PRIO_MAX, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_MQ_PRIO_MAX: {value:?}"
    );
}

#[test]
fn sc_ngroups_max() {
    let value = evaluate(sysconf::_SC_NGROUPS_MAX, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_NGROUPS_MAX: {value:?}"
    );
}

#[test]
fn sc_nprocessors_conf() {
    let value = evaluate(sysconf::_SC_NPROCESSORS_CONF, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_NPROCESSORS_CONF: {value:?}"
    );
}

#[test]
fn sc_nprocessors_onln() {
    let value = evaluate(sysconf::_SC_NPROCESSORS_ONLN, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_NPROCESSORS_ONLN: {value:?}"
    );
}

#[test]
fn sc_nzero() {
    let value = evaluate(sysconf::_SC_NZERO, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_NZERO: {value:?}"
    );
}

#[test]
fn sc_open_max() {
    let value = evaluate(sysconf::_SC_OPEN_MAX, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_OPEN_MAX: {value:?}"
    );
}

#[test]
fn sc_pagesize() {
    let value = evaluate(sysconf::_SC_PAGESIZE, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_PAGESIZE: {value:?}"
    );
}

#[test]
fn sc_page_size() {
    let value = evaluate(sysconf::_SC_PAGE_SIZE, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_PAGE_SIZE: {value:?}"
    );
}

#[test]
fn sc_pass_max() {
    let value = evaluate(sysconf::_SC_PASS_MAX, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_PASS_MAX: {value:?}"
    );
}

#[test]
fn sc_phys_pages() {
    let value = evaluate(sysconf::_SC_PHYS_PAGES, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_PHYS_PAGES: {value:?}"
    );
}

#[test]
fn sc_prioritized_io() {
    let value = evaluate(sysconf::_SC_PRIORITIZED_IO, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_PRIORITIZED_IO: {value:?}"
    );
}

#[test]
fn sc_priority_scheduling() {
    let value = evaluate(sysconf::_SC_PRIORITY_SCHEDULING, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_PRIORITY_SCHEDULING: {value:?}"
    );
}

#[test]
fn sc_raw_sockets() {
    let value = evaluate(sysconf::_SC_RAW_SOCKETS, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_RAW_SOCKETS: {value:?}"
    );
}

#[test]
fn sc_reader_writer_locks() {
    let value = evaluate(sysconf::_SC_READER_WRITER_LOCKS, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_READER_WRITER_LOCKS: {value:?}"
    );
}

#[test]
fn sc_realtime_signals() {
    let value = evaluate(sysconf::_SC_REALTIME_SIGNALS, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_REALTIME_SIGNALS: {value:?}"
    );
}

#[test]
fn sc_regexp() {
    let value = evaluate(sysconf::_SC_REGEXP, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_REGEXP: {value:?}"
    );
}

#[test]
fn sc_re_dup_max() {
    let value = evaluate(sysconf::_SC_RE_DUP_MAX, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_RE_DUP_MAX: {value:?}"
    );
}

#[test]
fn sc_rtsig_max() {
    let value = evaluate(sysconf::_SC_RTSIG_MAX, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_RTSIG_MAX: {value:?}"
    );
}

#[test]
fn sc_saved_ids() {
    let value = evaluate(sysconf::_SC_SAVED_IDS, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_SAVED_IDS: {value:?}"
    );
}

#[test]
fn sc_semaphores() {
    let value = evaluate(sysconf::_SC_SEMAPHORES, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_SEMAPHORES: {value:?}"
    );
}

#[test]
fn sc_sem_nsems_max() {
    let value = evaluate(sysconf::_SC_SEM_NSEMS_MAX, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_SEM_NSEMS_MAX: {value:?}"
    );
}

#[test]
fn sc_sem_value_max() {
    let value = evaluate(sysconf::_SC_SEM_VALUE_MAX, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_SEM_VALUE_MAX: {value:?}"
    );
}

#[test]
fn sc_shared_memory_objects() {
    let value = evaluate(sysconf::_SC_SHARED_MEMORY_OBJECTS, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_SHARED_MEMORY_OBJECTS: {value:?}"
    );
}

#[test]
fn sc_shell() {
    let value = evaluate(sysconf::_SC_SHELL, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_SHELL: {value:?}"
    );
}

#[test]
fn sc_sigqueue_max() {
    let value = evaluate(sysconf::_SC_SIGQUEUE_MAX, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_SIGQUEUE_MAX: {value:?}"
    );
}

#[test]
fn sc_sigstksz() {
    let value = evaluate(sysconf::_SC_SIGSTKSZ, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_SIGSTKSZ: {value:?}"
    );
}

#[test]
fn sc_spawn() {
    let value = evaluate(sysconf::_SC_SPAWN, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_SPAWN: {value:?}"
    );
}

#[test]
fn sc_spin_locks() {
    let value = evaluate(sysconf::_SC_SPIN_LOCKS, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_SPIN_LOCKS: {value:?}"
    );
}

#[test]
fn sc_sporadic_server() {
    let value = evaluate(sysconf::_SC_SPORADIC_SERVER, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_SPORADIC_SERVER: {value:?}"
    );
}

#[test]
fn sc_ss_repl_max() {
    let value = evaluate(sysconf::_SC_SS_REPL_MAX, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_SS_REPL_MAX: {value:?}"
    );
}

#[test]
fn sc_streams() {
    let value = evaluate(sysconf::_SC_STREAMS, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_STREAMS: {value:?}"
    );
}

#[test]
fn sc_stream_max() {
    let value = evaluate(sysconf::_SC_STREAM_MAX, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_STREAM_MAX: {value:?}"
    );
}

#[test]
fn sc_symloop_max() {
    let value = evaluate(sysconf::_SC_SYMLOOP_MAX, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_SYMLOOP_MAX: {value:?}"
    );
}

#[test]
fn sc_synchronized_io() {
    let value = evaluate(sysconf::_SC_SYNCHRONIZED_IO, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_SYNCHRONIZED_IO: {value:?}"
    );
}

#[test]
fn sc_threads() {
    let value = evaluate(sysconf::_SC_THREADS, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_THREADS: {value:?}"
    );
}

#[test]
fn sc_thread_attr_stackaddr() {
    let value = evaluate(sysconf::_SC_THREAD_ATTR_STACKADDR, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_THREAD_ATTR_STACKADDR: {value:?}"
    );
}

#[test]
fn sc_thread_attr_stacksize() {
    let value = evaluate(sysconf::_SC_THREAD_ATTR_STACKSIZE, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_THREAD_ATTR_STACKSIZE: {value:?}"
    );
}

#[test]
fn sc_thread_cputime() {
    let value = evaluate(sysconf::_SC_THREAD_CPUTIME, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_THREAD_CPUTIME: {value:?}"
    );
}

#[test]
fn sc_thread_destructor_iterations() {
    let value = evaluate(sysconf::_SC_THREAD_DESTRUCTOR_ITERATIONS, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_THREAD_DESTRUCTOR_ITERATIONS: {value:?}"
    );
}

#[test]
fn sc_thread_keys_max() {
    let value = evaluate(sysconf::_SC_THREAD_KEYS_MAX, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_THREAD_KEYS_MAX: {value:?}"
    );
}

#[test]
fn sc_thread_priority_scheduling() {
    let value = evaluate(sysconf::_SC_THREAD_PRIORITY_SCHEDULING, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_THREAD_PRIORITY_SCHEDULING: {value:?}"
    );
}

#[test]
fn sc_thread_prio_inherit() {
    let value = evaluate(sysconf::_SC_THREAD_PRIO_INHERIT, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_THREAD_PRIO_INHERIT: {value:?}"
    );
}

#[test]
fn sc_thread_prio_protect() {
    let value = evaluate(sysconf::_SC_THREAD_PRIO_PROTECT, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_THREAD_PRIO_PROTECT: {value:?}"
    );
}

#[test]
fn sc_thread_process_shared() {
    let value = evaluate(sysconf::_SC_THREAD_PROCESS_SHARED, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_THREAD_PROCESS_SHARED: {value:?}"
    );
}

#[test]
fn sc_thread_robust_prio_inherit() {
    let value = evaluate(sysconf::_SC_THREAD_ROBUST_PRIO_INHERIT, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_THREAD_ROBUST_PRIO_INHERIT: {value:?}"
    );
}

#[test]
fn sc_thread_robust_prio_protect() {
    let value = evaluate(sysconf::_SC_THREAD_ROBUST_PRIO_PROTECT, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_THREAD_ROBUST_PRIO_PROTECT: {value:?}"
    );
}

#[test]
fn sc_thread_safe_functions() {
    let value = evaluate(sysconf::_SC_THREAD_SAFE_FUNCTIONS, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_THREAD_SAFE_FUNCTIONS: {value:?}"
    );
}

#[test]
fn sc_thread_sporadic_server() {
    let value = evaluate(sysconf::_SC_THREAD_SPORADIC_SERVER, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_THREAD_SPORADIC_SERVER: {value:?}"
    );
}

#[test]
fn sc_thread_stack_min() {
    let value = evaluate(sysconf::_SC_THREAD_STACK_MIN, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_THREAD_STACK_MIN: {value:?}"
    );
}

#[test]
fn sc_thread_threads_max() {
    let value = evaluate(sysconf::_SC_THREAD_THREADS_MAX, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_THREAD_THREADS_MAX: {value:?}"
    );
}

#[test]
fn sc_timeouts() {
    let value = evaluate(sysconf::_SC_TIMEOUTS, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_TIMEOUTS: {value:?}"
    );
}

#[test]
fn sc_timers() {
    let value = evaluate(sysconf::_SC_TIMERS, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_TIMERS: {value:?}"
    );
}

#[test]
fn sc_timer_max() {
    let value = evaluate(sysconf::_SC_TIMER_MAX, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_TIMER_MAX: {value:?}"
    );
}

#[test]
fn sc_trace() {
    let value = evaluate(sysconf::_SC_TRACE, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_TRACE: {value:?}"
    );
}

#[test]
fn sc_trace_event_filter() {
    let value = evaluate(sysconf::_SC_TRACE_EVENT_FILTER, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_TRACE_EVENT_FILTER: {value:?}"
    );
}

#[test]
fn sc_trace_event_name_max() {
    let value = evaluate(sysconf::_SC_TRACE_EVENT_NAME_MAX, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_TRACE_EVENT_NAME_MAX: {value:?}"
    );
}

#[test]
fn sc_trace_inherit() {
    let value = evaluate(sysconf::_SC_TRACE_INHERIT, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_TRACE_INHERIT: {value:?}"
    );
}

#[test]
fn sc_trace_log() {
    let value = evaluate(sysconf::_SC_TRACE_LOG, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_TRACE_LOG: {value:?}"
    );
}

#[test]
fn sc_trace_name_max() {
    let value = evaluate(sysconf::_SC_TRACE_NAME_MAX, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_TRACE_NAME_MAX: {value:?}"
    );
}

#[test]
fn sc_trace_sys_max() {
    let value = evaluate(sysconf::_SC_TRACE_SYS_MAX, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_TRACE_SYS_MAX: {value:?}"
    );
}

#[test]
fn sc_trace_user_event_max() {
    let value = evaluate(sysconf::_SC_TRACE_USER_EVENT_MAX, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_TRACE_USER_EVENT_MAX: {value:?}"
    );
}

#[test]
fn sc_tty_name_max() {
    let value = evaluate(sysconf::_SC_TTY_NAME_MAX, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_TTY_NAME_MAX: {value:?}"
    );
}

#[test]
fn sc_typed_memory_objects() {
    let value = evaluate(sysconf::_SC_TYPED_MEMORY_OBJECTS, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_TYPED_MEMORY_OBJECTS: {value:?}"
    );
}

#[test]
fn sc_tzname_max() {
    let value = evaluate(sysconf::_SC_TZNAME_MAX, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_TZNAME_MAX: {value:?}"
    );
}

#[test]
fn sc_uio_maxiov() {
    let value = evaluate(sysconf::_SC_UIO_MAXIOV, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_UIO_MAXIOV: {value:?}"
    );
}

#[test]
fn sc_v6_ilp32_off32() {
    let value = evaluate(sysconf::_SC_V6_ILP32_OFF32, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_V6_ILP32_OFF32: {value:?}"
    );
}

#[test]
fn sc_v6_ilp32_offbig() {
    let value = evaluate(sysconf::_SC_V6_ILP32_OFFBIG, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_V6_ILP32_OFFBIG: {value:?}"
    );
}

#[test]
fn sc_v6_lp64_off64() {
    let value = evaluate(sysconf::_SC_V6_LP64_OFF64, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_V6_LP64_OFF64: {value:?}"
    );
}

#[test]
fn sc_v6_lpbig_offbig() {
    let value = evaluate(sysconf::_SC_V6_LPBIG_OFFBIG, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_V6_LPBIG_OFFBIG: {value:?}"
    );
}

#[test]
fn sc_v7_ilp32_off32() {
    let value = evaluate(sysconf::_SC_V7_ILP32_OFF32, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_V7_ILP32_OFF32: {value:?}"
    );
}

#[test]
fn sc_v7_ilp32_offbig() {
    let value = evaluate(sysconf::_SC_V7_ILP32_OFFBIG, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_V7_ILP32_OFFBIG: {value:?}"
    );
}

#[test]
fn sc_v7_lp64_off64() {
    let value = evaluate(sysconf::_SC_V7_LP64_OFF64, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_V7_LP64_OFF64: {value:?}"
    );
}

#[test]
fn sc_v7_lpbig_offbig() {
    let value = evaluate(sysconf::_SC_V7_LPBIG_OFFBIG, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_V7_LPBIG_OFFBIG: {value:?}"
    );
}

#[test]
fn sc_version() {
    let value = evaluate(sysconf::_SC_VERSION, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_VERSION: {value:?}"
    );
}

#[test]
fn sc_xbs5_ilp32_off32() {
    let value = evaluate(sysconf::_SC_XBS5_ILP32_OFF32, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_XBS5_ILP32_OFF32: {value:?}"
    );
}

#[test]
fn sc_xbs5_ilp32_offbig() {
    let value = evaluate(sysconf::_SC_XBS5_ILP32_OFFBIG, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_XBS5_ILP32_OFFBIG: {value:?}"
    );
}

#[test]
fn sc_xbs5_lp64_off64() {
    let value = evaluate(sysconf::_SC_XBS5_LP64_OFF64, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_XBS5_LP64_OFF64: {value:?}"
    );
}

#[test]
fn sc_xbs5_lpbig_offbig() {
    let value = evaluate(sysconf::_SC_XBS5_LPBIG_OFFBIG, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_XBS5_LPBIG_OFFBIG: {value:?}"
    );
}

#[test]
fn sc_xopen_crypt() {
    let value = evaluate(sysconf::_SC_XOPEN_CRYPT, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_XOPEN_CRYPT: {value:?}"
    );
}

#[test]
fn sc_xopen_enh_i18n() {
    let value = evaluate(sysconf::_SC_XOPEN_ENH_I18N, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_XOPEN_ENH_I18N: {value:?}"
    );
}

#[test]
fn sc_xopen_legacy() {
    let value = evaluate(sysconf::_SC_XOPEN_LEGACY, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_XOPEN_LEGACY: {value:?}"
    );
}

#[test]
fn sc_xopen_realtime() {
    let value = evaluate(sysconf::_SC_XOPEN_REALTIME, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_XOPEN_REALTIME: {value:?}"
    );
}

#[test]
fn sc_xopen_realtime_threads() {
    let value = evaluate(sysconf::_SC_XOPEN_REALTIME_THREADS, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_XOPEN_REALTIME_THREADS: {value:?}"
    );
}

#[test]
fn sc_xopen_shm() {
    let value = evaluate(sysconf::_SC_XOPEN_SHM, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_XOPEN_SHM: {value:?}"
    );
}

#[test]
fn sc_xopen_streams() {
    let value = evaluate(sysconf::_SC_XOPEN_STREAMS, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_XOPEN_STREAMS: {value:?}"
    );
}

#[test]
fn sc_xopen_unix() {
    let value = evaluate(sysconf::_SC_XOPEN_UNIX, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_XOPEN_UNIX: {value:?}"
    );
}

#[test]
fn sc_xopen_version() {
    let value = evaluate(sysconf::_SC_XOPEN_VERSION, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_XOPEN_VERSION: {value:?}"
    );
}

#[test]
fn sc_xopen_xcu_version() {
    let value = evaluate(sysconf::_SC_XOPEN_XCU_VERSION, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_XOPEN_XCU_VERSION: {value:?}"
    );
}

#[test]
fn sc_xopen_xpg2() {
    let value = evaluate(sysconf::_SC_XOPEN_XPG2, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_XOPEN_XPG2: {value:?}"
    );
}

#[test]
fn sc_xopen_xpg3() {
    let value = evaluate(sysconf::_SC_XOPEN_XPG3, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_XOPEN_XPG3: {value:?}"
    );
}

#[test]
fn sc_xopen_xpg4() {
    let value = evaluate(sysconf::_SC_XOPEN_XPG4, &inputs());
    assert!(
        value.is_ok() || value == Err(Errno::EINVAL),
        "_SC_XOPEN_XPG4: {value:?}"
    );
}

#[test]
fn named_rows_are_all_handled() {
    for (name, id) in crate::conf::sysconf::NAMED {
        let value = evaluate(*id, &inputs());
        assert!(value.is_ok(), "{name} is named but returns {value:?}");
    }
}

#[test]
fn unknown_name_is_einval() {
    assert_eq!(evaluate(-1, &inputs()), Err(Errno::EINVAL));
    assert_eq!(evaluate(9999, &inputs()), Err(Errno::EINVAL));
}

#[test]
fn fixed_values_match_the_specification() {
    let inputs = inputs();
    assert_eq!(evaluate(sysconf::_SC_PAGE_SIZE, &inputs), Ok(4096));
    assert_eq!(evaluate(sysconf::_SC_CLK_TCK, &inputs), Ok(100));
    assert_eq!(evaluate(sysconf::_SC_ARG_MAX, &inputs), Ok(131072));
    assert_eq!(evaluate(sysconf::_SC_NGROUPS_MAX, &inputs), Ok(1023));
    assert_eq!(evaluate(sysconf::_SC_SYMLOOP_MAX, &inputs), Ok(40));
    assert_eq!(evaluate(sysconf::_SC_HOST_NAME_MAX, &inputs), Ok(64));
    assert_eq!(evaluate(sysconf::_SC_IOV_MAX, &inputs), Ok(1024));
    assert_eq!(evaluate(sysconf::_SC_OPEN_MAX, &inputs), Ok(1024));
    assert_eq!(evaluate(sysconf::_SC_NPROCESSORS_ONLN, &inputs), Ok(1));
    assert_eq!(evaluate(sysconf::_SC_VERSION, &inputs), Ok(200809));
    assert_eq!(evaluate(188, &inputs), Ok(0));
    assert_eq!(evaluate(sysconf::_SC_2_FORT_DEV, &inputs), Ok(-1));
}

#[test]
fn open_max_follows_the_rlimit_cell() {
    let mut inputs = inputs();
    inputs.open_max = 77;
    assert_eq!(evaluate(sysconf::_SC_OPEN_MAX, &inputs), Ok(77));
}
