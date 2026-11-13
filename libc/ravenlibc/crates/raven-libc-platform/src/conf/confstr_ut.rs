// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! One test per `_CS_*` id: a value, an empty value, or documented `EINVAL`.

use raven_abi::errno::Errno;
use raven_libc_abi::sysconf;

use crate::conf::confstr;

#[test]
fn cs_gnu_libc_version() {
    let result = confstr(sysconf::_CS_GNU_LIBC_VERSION, None);
    assert!(
        result.is_ok() || result == Err(Errno::EINVAL),
        "_CS_GNU_LIBC_VERSION: {result:?}"
    );
}

#[test]
fn cs_gnu_libpthread_version() {
    let result = confstr(sysconf::_CS_GNU_LIBPTHREAD_VERSION, None);
    assert!(
        result.is_ok() || result == Err(Errno::EINVAL),
        "_CS_GNU_LIBPTHREAD_VERSION: {result:?}"
    );
}

#[test]
fn cs_path() {
    let result = confstr(sysconf::_CS_PATH, None);
    assert!(
        result.is_ok() || result == Err(Errno::EINVAL),
        "_CS_PATH: {result:?}"
    );
}

#[test]
fn cs_posix_v5_width_restricted_envs() {
    let result = confstr(sysconf::_CS_POSIX_V5_WIDTH_RESTRICTED_ENVS, None);
    assert!(
        result.is_ok() || result == Err(Errno::EINVAL),
        "_CS_POSIX_V5_WIDTH_RESTRICTED_ENVS: {result:?}"
    );
}

#[test]
fn cs_posix_v6_ilp32_off32_cflags() {
    let result = confstr(sysconf::_CS_POSIX_V6_ILP32_OFF32_CFLAGS, None);
    assert!(
        result.is_ok() || result == Err(Errno::EINVAL),
        "_CS_POSIX_V6_ILP32_OFF32_CFLAGS: {result:?}"
    );
}

#[test]
fn cs_posix_v6_ilp32_off32_ldflags() {
    let result = confstr(sysconf::_CS_POSIX_V6_ILP32_OFF32_LDFLAGS, None);
    assert!(
        result.is_ok() || result == Err(Errno::EINVAL),
        "_CS_POSIX_V6_ILP32_OFF32_LDFLAGS: {result:?}"
    );
}

#[test]
fn cs_posix_v6_ilp32_off32_libs() {
    let result = confstr(sysconf::_CS_POSIX_V6_ILP32_OFF32_LIBS, None);
    assert!(
        result.is_ok() || result == Err(Errno::EINVAL),
        "_CS_POSIX_V6_ILP32_OFF32_LIBS: {result:?}"
    );
}

#[test]
fn cs_posix_v6_ilp32_off32_lintflags() {
    let result = confstr(sysconf::_CS_POSIX_V6_ILP32_OFF32_LINTFLAGS, None);
    assert!(
        result.is_ok() || result == Err(Errno::EINVAL),
        "_CS_POSIX_V6_ILP32_OFF32_LINTFLAGS: {result:?}"
    );
}

#[test]
fn cs_posix_v6_ilp32_offbig_cflags() {
    let result = confstr(sysconf::_CS_POSIX_V6_ILP32_OFFBIG_CFLAGS, None);
    assert!(
        result.is_ok() || result == Err(Errno::EINVAL),
        "_CS_POSIX_V6_ILP32_OFFBIG_CFLAGS: {result:?}"
    );
}

#[test]
fn cs_posix_v6_ilp32_offbig_ldflags() {
    let result = confstr(sysconf::_CS_POSIX_V6_ILP32_OFFBIG_LDFLAGS, None);
    assert!(
        result.is_ok() || result == Err(Errno::EINVAL),
        "_CS_POSIX_V6_ILP32_OFFBIG_LDFLAGS: {result:?}"
    );
}

#[test]
fn cs_posix_v6_ilp32_offbig_libs() {
    let result = confstr(sysconf::_CS_POSIX_V6_ILP32_OFFBIG_LIBS, None);
    assert!(
        result.is_ok() || result == Err(Errno::EINVAL),
        "_CS_POSIX_V6_ILP32_OFFBIG_LIBS: {result:?}"
    );
}

#[test]
fn cs_posix_v6_ilp32_offbig_lintflags() {
    let result = confstr(sysconf::_CS_POSIX_V6_ILP32_OFFBIG_LINTFLAGS, None);
    assert!(
        result.is_ok() || result == Err(Errno::EINVAL),
        "_CS_POSIX_V6_ILP32_OFFBIG_LINTFLAGS: {result:?}"
    );
}

#[test]
fn cs_posix_v6_lp64_off64_cflags() {
    let result = confstr(sysconf::_CS_POSIX_V6_LP64_OFF64_CFLAGS, None);
    assert!(
        result.is_ok() || result == Err(Errno::EINVAL),
        "_CS_POSIX_V6_LP64_OFF64_CFLAGS: {result:?}"
    );
}

#[test]
fn cs_posix_v6_lp64_off64_ldflags() {
    let result = confstr(sysconf::_CS_POSIX_V6_LP64_OFF64_LDFLAGS, None);
    assert!(
        result.is_ok() || result == Err(Errno::EINVAL),
        "_CS_POSIX_V6_LP64_OFF64_LDFLAGS: {result:?}"
    );
}

#[test]
fn cs_posix_v6_lp64_off64_libs() {
    let result = confstr(sysconf::_CS_POSIX_V6_LP64_OFF64_LIBS, None);
    assert!(
        result.is_ok() || result == Err(Errno::EINVAL),
        "_CS_POSIX_V6_LP64_OFF64_LIBS: {result:?}"
    );
}

#[test]
fn cs_posix_v6_lp64_off64_lintflags() {
    let result = confstr(sysconf::_CS_POSIX_V6_LP64_OFF64_LINTFLAGS, None);
    assert!(
        result.is_ok() || result == Err(Errno::EINVAL),
        "_CS_POSIX_V6_LP64_OFF64_LINTFLAGS: {result:?}"
    );
}

#[test]
fn cs_posix_v6_lpbig_offbig_cflags() {
    let result = confstr(sysconf::_CS_POSIX_V6_LPBIG_OFFBIG_CFLAGS, None);
    assert!(
        result.is_ok() || result == Err(Errno::EINVAL),
        "_CS_POSIX_V6_LPBIG_OFFBIG_CFLAGS: {result:?}"
    );
}

#[test]
fn cs_posix_v6_lpbig_offbig_ldflags() {
    let result = confstr(sysconf::_CS_POSIX_V6_LPBIG_OFFBIG_LDFLAGS, None);
    assert!(
        result.is_ok() || result == Err(Errno::EINVAL),
        "_CS_POSIX_V6_LPBIG_OFFBIG_LDFLAGS: {result:?}"
    );
}

#[test]
fn cs_posix_v6_lpbig_offbig_libs() {
    let result = confstr(sysconf::_CS_POSIX_V6_LPBIG_OFFBIG_LIBS, None);
    assert!(
        result.is_ok() || result == Err(Errno::EINVAL),
        "_CS_POSIX_V6_LPBIG_OFFBIG_LIBS: {result:?}"
    );
}

#[test]
fn cs_posix_v6_lpbig_offbig_lintflags() {
    let result = confstr(sysconf::_CS_POSIX_V6_LPBIG_OFFBIG_LINTFLAGS, None);
    assert!(
        result.is_ok() || result == Err(Errno::EINVAL),
        "_CS_POSIX_V6_LPBIG_OFFBIG_LINTFLAGS: {result:?}"
    );
}

#[test]
fn cs_posix_v6_width_restricted_envs() {
    let result = confstr(sysconf::_CS_POSIX_V6_WIDTH_RESTRICTED_ENVS, None);
    assert!(
        result.is_ok() || result == Err(Errno::EINVAL),
        "_CS_POSIX_V6_WIDTH_RESTRICTED_ENVS: {result:?}"
    );
}

#[test]
fn cs_posix_v7_ilp32_off32_cflags() {
    let result = confstr(sysconf::_CS_POSIX_V7_ILP32_OFF32_CFLAGS, None);
    assert!(
        result.is_ok() || result == Err(Errno::EINVAL),
        "_CS_POSIX_V7_ILP32_OFF32_CFLAGS: {result:?}"
    );
}

#[test]
fn cs_posix_v7_ilp32_off32_ldflags() {
    let result = confstr(sysconf::_CS_POSIX_V7_ILP32_OFF32_LDFLAGS, None);
    assert!(
        result.is_ok() || result == Err(Errno::EINVAL),
        "_CS_POSIX_V7_ILP32_OFF32_LDFLAGS: {result:?}"
    );
}

#[test]
fn cs_posix_v7_ilp32_off32_libs() {
    let result = confstr(sysconf::_CS_POSIX_V7_ILP32_OFF32_LIBS, None);
    assert!(
        result.is_ok() || result == Err(Errno::EINVAL),
        "_CS_POSIX_V7_ILP32_OFF32_LIBS: {result:?}"
    );
}

#[test]
fn cs_posix_v7_ilp32_off32_lintflags() {
    let result = confstr(sysconf::_CS_POSIX_V7_ILP32_OFF32_LINTFLAGS, None);
    assert!(
        result.is_ok() || result == Err(Errno::EINVAL),
        "_CS_POSIX_V7_ILP32_OFF32_LINTFLAGS: {result:?}"
    );
}

#[test]
fn cs_posix_v7_ilp32_offbig_cflags() {
    let result = confstr(sysconf::_CS_POSIX_V7_ILP32_OFFBIG_CFLAGS, None);
    assert!(
        result.is_ok() || result == Err(Errno::EINVAL),
        "_CS_POSIX_V7_ILP32_OFFBIG_CFLAGS: {result:?}"
    );
}

#[test]
fn cs_posix_v7_ilp32_offbig_ldflags() {
    let result = confstr(sysconf::_CS_POSIX_V7_ILP32_OFFBIG_LDFLAGS, None);
    assert!(
        result.is_ok() || result == Err(Errno::EINVAL),
        "_CS_POSIX_V7_ILP32_OFFBIG_LDFLAGS: {result:?}"
    );
}

#[test]
fn cs_posix_v7_ilp32_offbig_libs() {
    let result = confstr(sysconf::_CS_POSIX_V7_ILP32_OFFBIG_LIBS, None);
    assert!(
        result.is_ok() || result == Err(Errno::EINVAL),
        "_CS_POSIX_V7_ILP32_OFFBIG_LIBS: {result:?}"
    );
}

#[test]
fn cs_posix_v7_ilp32_offbig_lintflags() {
    let result = confstr(sysconf::_CS_POSIX_V7_ILP32_OFFBIG_LINTFLAGS, None);
    assert!(
        result.is_ok() || result == Err(Errno::EINVAL),
        "_CS_POSIX_V7_ILP32_OFFBIG_LINTFLAGS: {result:?}"
    );
}

#[test]
fn cs_posix_v7_lp64_off64_cflags() {
    let result = confstr(sysconf::_CS_POSIX_V7_LP64_OFF64_CFLAGS, None);
    assert!(
        result.is_ok() || result == Err(Errno::EINVAL),
        "_CS_POSIX_V7_LP64_OFF64_CFLAGS: {result:?}"
    );
}

#[test]
fn cs_posix_v7_lp64_off64_ldflags() {
    let result = confstr(sysconf::_CS_POSIX_V7_LP64_OFF64_LDFLAGS, None);
    assert!(
        result.is_ok() || result == Err(Errno::EINVAL),
        "_CS_POSIX_V7_LP64_OFF64_LDFLAGS: {result:?}"
    );
}

#[test]
fn cs_posix_v7_lp64_off64_libs() {
    let result = confstr(sysconf::_CS_POSIX_V7_LP64_OFF64_LIBS, None);
    assert!(
        result.is_ok() || result == Err(Errno::EINVAL),
        "_CS_POSIX_V7_LP64_OFF64_LIBS: {result:?}"
    );
}

#[test]
fn cs_posix_v7_lp64_off64_lintflags() {
    let result = confstr(sysconf::_CS_POSIX_V7_LP64_OFF64_LINTFLAGS, None);
    assert!(
        result.is_ok() || result == Err(Errno::EINVAL),
        "_CS_POSIX_V7_LP64_OFF64_LINTFLAGS: {result:?}"
    );
}

#[test]
fn cs_posix_v7_lpbig_offbig_cflags() {
    let result = confstr(sysconf::_CS_POSIX_V7_LPBIG_OFFBIG_CFLAGS, None);
    assert!(
        result.is_ok() || result == Err(Errno::EINVAL),
        "_CS_POSIX_V7_LPBIG_OFFBIG_CFLAGS: {result:?}"
    );
}

#[test]
fn cs_posix_v7_lpbig_offbig_ldflags() {
    let result = confstr(sysconf::_CS_POSIX_V7_LPBIG_OFFBIG_LDFLAGS, None);
    assert!(
        result.is_ok() || result == Err(Errno::EINVAL),
        "_CS_POSIX_V7_LPBIG_OFFBIG_LDFLAGS: {result:?}"
    );
}

#[test]
fn cs_posix_v7_lpbig_offbig_libs() {
    let result = confstr(sysconf::_CS_POSIX_V7_LPBIG_OFFBIG_LIBS, None);
    assert!(
        result.is_ok() || result == Err(Errno::EINVAL),
        "_CS_POSIX_V7_LPBIG_OFFBIG_LIBS: {result:?}"
    );
}

#[test]
fn cs_posix_v7_lpbig_offbig_lintflags() {
    let result = confstr(sysconf::_CS_POSIX_V7_LPBIG_OFFBIG_LINTFLAGS, None);
    assert!(
        result.is_ok() || result == Err(Errno::EINVAL),
        "_CS_POSIX_V7_LPBIG_OFFBIG_LINTFLAGS: {result:?}"
    );
}

#[test]
fn cs_posix_v7_threads_cflags() {
    let result = confstr(sysconf::_CS_POSIX_V7_THREADS_CFLAGS, None);
    assert!(
        result.is_ok() || result == Err(Errno::EINVAL),
        "_CS_POSIX_V7_THREADS_CFLAGS: {result:?}"
    );
}

#[test]
fn cs_posix_v7_threads_ldflags() {
    let result = confstr(sysconf::_CS_POSIX_V7_THREADS_LDFLAGS, None);
    assert!(
        result.is_ok() || result == Err(Errno::EINVAL),
        "_CS_POSIX_V7_THREADS_LDFLAGS: {result:?}"
    );
}

#[test]
fn cs_posix_v7_width_restricted_envs() {
    let result = confstr(sysconf::_CS_POSIX_V7_WIDTH_RESTRICTED_ENVS, None);
    assert!(
        result.is_ok() || result == Err(Errno::EINVAL),
        "_CS_POSIX_V7_WIDTH_RESTRICTED_ENVS: {result:?}"
    );
}

#[test]
fn cs_v6_env() {
    let result = confstr(sysconf::_CS_V6_ENV, None);
    assert!(
        result.is_ok() || result == Err(Errno::EINVAL),
        "_CS_V6_ENV: {result:?}"
    );
}

#[test]
fn cs_v7_env() {
    let result = confstr(sysconf::_CS_V7_ENV, None);
    assert!(
        result.is_ok() || result == Err(Errno::EINVAL),
        "_CS_V7_ENV: {result:?}"
    );
}

#[test]
fn cs_path_is_the_raven_single_root() {
    let mut buffer = [0u8; 32];
    let needed = confstr(sysconf::_CS_PATH, Some(&mut buffer)).expect("path");
    assert_eq!(needed, b"/bin:/usr/bin".len() + 1);
    assert_eq!(&buffer[..needed], b"/bin:/usr/bin\0");
}

#[test]
fn short_buffer_truncates_and_reports_the_size() {
    let mut buffer = [0u8; 4];
    let needed = confstr(sysconf::_CS_PATH, Some(&mut buffer)).expect("path");
    assert_eq!(needed, 14);
    assert_eq!(&buffer, b"/bin");
}

#[test]
fn no_value_returns_zero() {
    assert_eq!(
        confstr(sysconf::_CS_POSIX_V6_ILP32_OFF32_CFLAGS, None),
        Ok(0)
    );
}

#[test]
fn width_restricted_envs_are_lp64() {
    let mut buffer = [0u8; 128];
    confstr(
        sysconf::_CS_POSIX_V6_WIDTH_RESTRICTED_ENVS,
        Some(&mut buffer),
    )
    .expect("v6");
    assert!(buffer.starts_with(b"POSIX_V6_LP64_OFF64"));
    confstr(
        sysconf::_CS_POSIX_V7_WIDTH_RESTRICTED_ENVS,
        Some(&mut buffer),
    )
    .expect("v7");
    assert!(buffer.starts_with(b"POSIX_V7_LP64_OFF64"));
}

#[test]
fn threads_ldflags_is_empty() {
    let mut buffer = [0u8; 4];
    let needed =
        confstr(sysconf::_CS_POSIX_V7_THREADS_LDFLAGS, Some(&mut buffer)).expect("ldflags");
    assert_eq!(needed, 1);
    assert_eq!(buffer[0], 0);
}

#[test]
fn unknown_is_einval() {
    assert_eq!(confstr(-1, None), Err(Errno::EINVAL));
}
