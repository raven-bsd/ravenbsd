// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! One test per `_PC_*` id; every id is answered from the static table.

use raven_abi::errno::Errno;
use raven_libc_abi::sysconf;

use crate::conf::pathconf_value;

#[test]
fn pc_2_symlinks() {
    assert!(
        pathconf_value(sysconf::_PC_2_SYMLINKS, None).is_ok(),
        "_PC_2_SYMLINKS"
    );
}

#[test]
fn pc_alloc_size_min() {
    assert!(
        pathconf_value(sysconf::_PC_ALLOC_SIZE_MIN, None).is_ok(),
        "_PC_ALLOC_SIZE_MIN"
    );
}

#[test]
fn pc_async_io() {
    assert!(
        pathconf_value(sysconf::_PC_ASYNC_IO, None).is_ok(),
        "_PC_ASYNC_IO"
    );
}

#[test]
fn pc_chown_restricted() {
    assert!(
        pathconf_value(sysconf::_PC_CHOWN_RESTRICTED, None).is_ok(),
        "_PC_CHOWN_RESTRICTED"
    );
}

#[test]
fn pc_filesizebits() {
    assert!(
        pathconf_value(sysconf::_PC_FILESIZEBITS, None).is_ok(),
        "_PC_FILESIZEBITS"
    );
}

#[test]
fn pc_link_max() {
    assert!(
        pathconf_value(sysconf::_PC_LINK_MAX, None).is_ok(),
        "_PC_LINK_MAX"
    );
}

#[test]
fn pc_max_canon() {
    assert!(
        pathconf_value(sysconf::_PC_MAX_CANON, None).is_ok(),
        "_PC_MAX_CANON"
    );
}

#[test]
fn pc_max_input() {
    assert!(
        pathconf_value(sysconf::_PC_MAX_INPUT, None).is_ok(),
        "_PC_MAX_INPUT"
    );
}

#[test]
fn pc_name_max() {
    assert!(
        pathconf_value(sysconf::_PC_NAME_MAX, None).is_ok(),
        "_PC_NAME_MAX"
    );
}

#[test]
fn pc_no_trunc() {
    assert!(
        pathconf_value(sysconf::_PC_NO_TRUNC, None).is_ok(),
        "_PC_NO_TRUNC"
    );
}

#[test]
fn pc_path_max() {
    assert!(
        pathconf_value(sysconf::_PC_PATH_MAX, None).is_ok(),
        "_PC_PATH_MAX"
    );
}

#[test]
fn pc_pipe_buf() {
    assert!(
        pathconf_value(sysconf::_PC_PIPE_BUF, None).is_ok(),
        "_PC_PIPE_BUF"
    );
}

#[test]
fn pc_prio_io() {
    assert!(
        pathconf_value(sysconf::_PC_PRIO_IO, None).is_ok(),
        "_PC_PRIO_IO"
    );
}

#[test]
fn pc_rec_incr_xfer_size() {
    assert!(
        pathconf_value(sysconf::_PC_REC_INCR_XFER_SIZE, None).is_ok(),
        "_PC_REC_INCR_XFER_SIZE"
    );
}

#[test]
fn pc_rec_max_xfer_size() {
    assert!(
        pathconf_value(sysconf::_PC_REC_MAX_XFER_SIZE, None).is_ok(),
        "_PC_REC_MAX_XFER_SIZE"
    );
}

#[test]
fn pc_rec_min_xfer_size() {
    assert!(
        pathconf_value(sysconf::_PC_REC_MIN_XFER_SIZE, None).is_ok(),
        "_PC_REC_MIN_XFER_SIZE"
    );
}

#[test]
fn pc_rec_xfer_align() {
    assert!(
        pathconf_value(sysconf::_PC_REC_XFER_ALIGN, None).is_ok(),
        "_PC_REC_XFER_ALIGN"
    );
}

#[test]
fn pc_sock_maxbuf() {
    assert!(
        pathconf_value(sysconf::_PC_SOCK_MAXBUF, None).is_ok(),
        "_PC_SOCK_MAXBUF"
    );
}

#[test]
fn pc_symlink_max() {
    assert!(
        pathconf_value(sysconf::_PC_SYMLINK_MAX, None).is_ok(),
        "_PC_SYMLINK_MAX"
    );
}

#[test]
fn pc_sync_io() {
    assert!(
        pathconf_value(sysconf::_PC_SYNC_IO, None).is_ok(),
        "_PC_SYNC_IO"
    );
}

#[test]
fn pc_vdisable() {
    assert!(
        pathconf_value(sysconf::_PC_VDISABLE, None).is_ok(),
        "_PC_VDISABLE"
    );
}

#[test]
fn unknown_pc_is_einval() {
    assert_eq!(pathconf_value(-1, None), Err(Errno::EINVAL));
}

#[test]
fn ufs_constants_match_the_specification() {
    assert_eq!(pathconf_value(sysconf::_PC_LINK_MAX, None), Ok(32000));
    assert_eq!(pathconf_value(sysconf::_PC_NAME_MAX, None), Ok(255));
    assert_eq!(pathconf_value(sysconf::_PC_PATH_MAX, None), Ok(4096));
    assert_eq!(pathconf_value(sysconf::_PC_PIPE_BUF, None), Ok(4096));
    assert_eq!(pathconf_value(sysconf::_PC_FILESIZEBITS, None), Ok(64));
    assert_eq!(pathconf_value(sysconf::_PC_SYMLINK_MAX, None), Ok(-1));
}
