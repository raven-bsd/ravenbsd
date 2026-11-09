// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Every x86-64 struct matches `linux-raw-sys` in size, alignment and every field offset.

use raven_linux_abi::structs::x86_64 as ours;
use std::mem::{align_of, offset_of, size_of};

macro_rules! check {
    ($ours:ty, $theirs:ty, { $($field:ident => $their:expr),+ $(,)? }) => {{
        assert_eq!(size_of::<$ours>(), size_of::<$theirs>(), "size");
        assert_eq!(align_of::<$ours>(), align_of::<$theirs>(), "alignment");
        $(assert_eq!(offset_of!($ours, $field), $their, concat!("offset of ", stringify!($field)));)+
    }};
}

#[test]
fn timespec() {
    check!(ours::Timespec, linux_raw_sys::general::timespec, {
        tv_sec => offset_of!(linux_raw_sys::general::timespec, tv_sec),
        tv_nsec => offset_of!(linux_raw_sys::general::timespec, tv_nsec),
    });
}

#[test]
fn timeval() {
    check!(ours::Timeval, linux_raw_sys::general::timeval, {
        tv_sec => offset_of!(linux_raw_sys::general::timeval, tv_sec),
        tv_usec => offset_of!(linux_raw_sys::general::timeval, tv_usec),
    });
}

#[test]
fn stat() {
    check!(ours::Stat, linux_raw_sys::general::stat, {
        st_dev => offset_of!(linux_raw_sys::general::stat, st_dev),
        st_ino => offset_of!(linux_raw_sys::general::stat, st_ino),
        st_nlink => offset_of!(linux_raw_sys::general::stat, st_nlink),
        st_mode => offset_of!(linux_raw_sys::general::stat, st_mode),
        st_uid => offset_of!(linux_raw_sys::general::stat, st_uid),
        st_gid => offset_of!(linux_raw_sys::general::stat, st_gid),
        __pad0 => offset_of!(linux_raw_sys::general::stat, __pad0),
        st_rdev => offset_of!(linux_raw_sys::general::stat, st_rdev),
        st_size => offset_of!(linux_raw_sys::general::stat, st_size),
        st_blksize => offset_of!(linux_raw_sys::general::stat, st_blksize),
        st_blocks => offset_of!(linux_raw_sys::general::stat, st_blocks),
        st_atime => offset_of!(linux_raw_sys::general::stat, st_atime),
        st_atime_nsec => offset_of!(linux_raw_sys::general::stat, st_atime_nsec),
        st_mtime => offset_of!(linux_raw_sys::general::stat, st_mtime),
        st_mtime_nsec => offset_of!(linux_raw_sys::general::stat, st_mtime_nsec),
        st_ctime => offset_of!(linux_raw_sys::general::stat, st_ctime),
        st_ctime_nsec => offset_of!(linux_raw_sys::general::stat, st_ctime_nsec),
        __unused => offset_of!(linux_raw_sys::general::stat, __unused),
    });
}

#[test]
fn statx() {
    check!(ours::Statx, linux_raw_sys::general::statx, {
        stx_mask => offset_of!(linux_raw_sys::general::statx, stx_mask),
        stx_blksize => offset_of!(linux_raw_sys::general::statx, stx_blksize),
        stx_attributes => offset_of!(linux_raw_sys::general::statx, stx_attributes),
        stx_nlink => offset_of!(linux_raw_sys::general::statx, stx_nlink),
        stx_uid => offset_of!(linux_raw_sys::general::statx, stx_uid),
        stx_gid => offset_of!(linux_raw_sys::general::statx, stx_gid),
        stx_mode => offset_of!(linux_raw_sys::general::statx, stx_mode),
        __spare0 => offset_of!(linux_raw_sys::general::statx, __spare0),
        stx_ino => offset_of!(linux_raw_sys::general::statx, stx_ino),
        stx_size => offset_of!(linux_raw_sys::general::statx, stx_size),
        stx_blocks => offset_of!(linux_raw_sys::general::statx, stx_blocks),
        stx_attributes_mask => offset_of!(linux_raw_sys::general::statx, stx_attributes_mask),
        stx_atime => offset_of!(linux_raw_sys::general::statx, stx_atime),
        stx_btime => offset_of!(linux_raw_sys::general::statx, stx_btime),
        stx_ctime => offset_of!(linux_raw_sys::general::statx, stx_ctime),
        stx_mtime => offset_of!(linux_raw_sys::general::statx, stx_mtime),
        stx_rdev_major => offset_of!(linux_raw_sys::general::statx, stx_rdev_major),
        stx_rdev_minor => offset_of!(linux_raw_sys::general::statx, stx_rdev_minor),
        stx_dev_major => offset_of!(linux_raw_sys::general::statx, stx_dev_major),
        stx_dev_minor => offset_of!(linux_raw_sys::general::statx, stx_dev_minor),
        stx_mnt_id => offset_of!(linux_raw_sys::general::statx, stx_mnt_id),
        stx_dio_mem_align => offset_of!(linux_raw_sys::general::statx, stx_dio_mem_align),
        stx_dio_offset_align => offset_of!(linux_raw_sys::general::statx, stx_dio_offset_align),
        stx_subvol => offset_of!(linux_raw_sys::general::statx, stx_subvol),
        stx_atomic_write_unit_min => offset_of!(linux_raw_sys::general::statx, stx_atomic_write_unit_min),
        stx_atomic_write_unit_max => offset_of!(linux_raw_sys::general::statx, stx_atomic_write_unit_max),
        stx_atomic_write_segments_max => offset_of!(linux_raw_sys::general::statx, stx_atomic_write_segments_max),
        stx_dio_read_offset_align => offset_of!(linux_raw_sys::general::statx, stx_dio_read_offset_align),
        stx_atomic_write_unit_max_opt => offset_of!(linux_raw_sys::general::statx, stx_atomic_write_unit_max_opt),
        __spare2 => offset_of!(linux_raw_sys::general::statx, __spare2),
        __spare3 => offset_of!(linux_raw_sys::general::statx, __spare3),
    });
}

#[test]
fn iovec() {
    check!(ours::Iovec, linux_raw_sys::general::iovec, {
        iov_base => offset_of!(linux_raw_sys::general::iovec, iov_base),
        iov_len => offset_of!(linux_raw_sys::general::iovec, iov_len),
    });
}

#[test]
fn utsname() {
    check!(ours::Utsname, linux_raw_sys::system::new_utsname, {
        sysname => offset_of!(linux_raw_sys::system::new_utsname, sysname),
        nodename => offset_of!(linux_raw_sys::system::new_utsname, nodename),
        release => offset_of!(linux_raw_sys::system::new_utsname, release),
        version => offset_of!(linux_raw_sys::system::new_utsname, version),
        machine => offset_of!(linux_raw_sys::system::new_utsname, machine),
        domainname => offset_of!(linux_raw_sys::system::new_utsname, domainname),
    });
}

#[test]
fn rlimit() {
    check!(ours::Rlimit, linux_raw_sys::general::rlimit, {
        rlim_cur => offset_of!(linux_raw_sys::general::rlimit, rlim_cur),
        rlim_max => offset_of!(linux_raw_sys::general::rlimit, rlim_max),
    });
}

#[test]
fn pollfd() {
    check!(ours::Pollfd, linux_raw_sys::general::pollfd, {
        fd => offset_of!(linux_raw_sys::general::pollfd, fd),
        events => offset_of!(linux_raw_sys::general::pollfd, events),
        revents => offset_of!(linux_raw_sys::general::pollfd, revents),
    });
}

#[test]
fn epoll_event() {
    check!(ours::EpollEvent, linux_raw_sys::general::epoll_event, {
        events => offset_of!(linux_raw_sys::general::epoll_event, events),
        data => offset_of!(linux_raw_sys::general::epoll_event, data),
    });
}

#[test]
fn dirent64() {
    check!(ours::Dirent64, linux_raw_sys::general::linux_dirent64, {
        d_ino => offset_of!(linux_raw_sys::general::linux_dirent64, d_ino),
        d_off => offset_of!(linux_raw_sys::general::linux_dirent64, d_off),
        d_reclen => offset_of!(linux_raw_sys::general::linux_dirent64, d_reclen),
        d_type => offset_of!(linux_raw_sys::general::linux_dirent64, d_type),
        d_name => offset_of!(linux_raw_sys::general::linux_dirent64, d_name),
    });
}

#[test]
fn sig_action() {
    check!(ours::SigAction, linux_raw_sys::general::sigaction, {
        sa_handler => offset_of!(linux_raw_sys::general::sigaction, sa_handler),
        sa_flags => offset_of!(linux_raw_sys::general::sigaction, sa_flags),
        sa_restorer => offset_of!(linux_raw_sys::general::sigaction, sa_restorer),
        sa_mask => offset_of!(linux_raw_sys::general::sigaction, sa_mask),
    });
}

#[test]
fn sockaddr_in() {
    check!(ours::SockaddrIn, linux_raw_sys::net::sockaddr_in, {
        sin_family => offset_of!(linux_raw_sys::net::sockaddr_in, sin_family),
        sin_port => offset_of!(linux_raw_sys::net::sockaddr_in, sin_port),
        sin_addr => offset_of!(linux_raw_sys::net::sockaddr_in, sin_addr),
        __pad => offset_of!(linux_raw_sys::net::sockaddr_in, __pad),
    });
}

#[test]
fn sockaddr_in6() {
    check!(ours::SockaddrIn6, linux_raw_sys::net::sockaddr_in6, {
        sin6_family => offset_of!(linux_raw_sys::net::sockaddr_in6, sin6_family),
        sin6_port => offset_of!(linux_raw_sys::net::sockaddr_in6, sin6_port),
        sin6_flowinfo => offset_of!(linux_raw_sys::net::sockaddr_in6, sin6_flowinfo),
        sin6_addr => offset_of!(linux_raw_sys::net::sockaddr_in6, sin6_addr),
        sin6_scope_id => offset_of!(linux_raw_sys::net::sockaddr_in6, sin6_scope_id),
    });
}

#[test]
fn sockaddr_un() {
    check!(ours::SockaddrUn, linux_raw_sys::net::sockaddr_un, {
        sun_family => offset_of!(linux_raw_sys::net::sockaddr_un, sun_family),
        sun_path => offset_of!(linux_raw_sys::net::sockaddr_un, sun_path),
    });
}

#[test]
fn msghdr() {
    check!(ours::Msghdr, linux_raw_sys::net::msghdr, {
        msg_name => offset_of!(linux_raw_sys::net::msghdr, msg_name),
        msg_namelen => offset_of!(linux_raw_sys::net::msghdr, msg_namelen),
        msg_iov => offset_of!(linux_raw_sys::net::msghdr, msg_iov),
        msg_iovlen => offset_of!(linux_raw_sys::net::msghdr, msg_iovlen),
        msg_control => offset_of!(linux_raw_sys::net::msghdr, msg_control),
        msg_controllen => offset_of!(linux_raw_sys::net::msghdr, msg_controllen),
        msg_flags => offset_of!(linux_raw_sys::net::msghdr, msg_flags),
    });
}

#[test]
fn cmsghdr() {
    check!(ours::Cmsghdr, linux_raw_sys::net::cmsghdr, {
        cmsg_len => offset_of!(linux_raw_sys::net::cmsghdr, cmsg_len),
        cmsg_level => offset_of!(linux_raw_sys::net::cmsghdr, cmsg_level),
        cmsg_type => offset_of!(linux_raw_sys::net::cmsghdr, cmsg_type),
    });
}

#[test]
fn rusage() {
    check!(ours::Rusage, linux_raw_sys::general::rusage, {
        ru_utime => offset_of!(linux_raw_sys::general::rusage, ru_utime),
        ru_stime => offset_of!(linux_raw_sys::general::rusage, ru_stime),
        ru_maxrss => offset_of!(linux_raw_sys::general::rusage, ru_maxrss),
        ru_ixrss => offset_of!(linux_raw_sys::general::rusage, ru_ixrss),
        ru_idrss => offset_of!(linux_raw_sys::general::rusage, ru_idrss),
        ru_isrss => offset_of!(linux_raw_sys::general::rusage, ru_isrss),
        ru_minflt => offset_of!(linux_raw_sys::general::rusage, ru_minflt),
        ru_majflt => offset_of!(linux_raw_sys::general::rusage, ru_majflt),
        ru_nswap => offset_of!(linux_raw_sys::general::rusage, ru_nswap),
        ru_inblock => offset_of!(linux_raw_sys::general::rusage, ru_inblock),
        ru_oublock => offset_of!(linux_raw_sys::general::rusage, ru_oublock),
        ru_msgsnd => offset_of!(linux_raw_sys::general::rusage, ru_msgsnd),
        ru_msgrcv => offset_of!(linux_raw_sys::general::rusage, ru_msgrcv),
        ru_nsignals => offset_of!(linux_raw_sys::general::rusage, ru_nsignals),
        ru_nvcsw => offset_of!(linux_raw_sys::general::rusage, ru_nvcsw),
        ru_nivcsw => offset_of!(linux_raw_sys::general::rusage, ru_nivcsw),
    });
}

#[test]
fn sysinfo() {
    check!(ours::Sysinfo, linux_raw_sys::system::sysinfo, {
        uptime => offset_of!(linux_raw_sys::system::sysinfo, uptime),
        loads => offset_of!(linux_raw_sys::system::sysinfo, loads),
        totalram => offset_of!(linux_raw_sys::system::sysinfo, totalram),
        freeram => offset_of!(linux_raw_sys::system::sysinfo, freeram),
        sharedram => offset_of!(linux_raw_sys::system::sysinfo, sharedram),
        bufferram => offset_of!(linux_raw_sys::system::sysinfo, bufferram),
        totalswap => offset_of!(linux_raw_sys::system::sysinfo, totalswap),
        freeswap => offset_of!(linux_raw_sys::system::sysinfo, freeswap),
        procs => offset_of!(linux_raw_sys::system::sysinfo, procs),
        pad => offset_of!(linux_raw_sys::system::sysinfo, pad),
        totalhigh => offset_of!(linux_raw_sys::system::sysinfo, totalhigh),
        freehigh => offset_of!(linux_raw_sys::system::sysinfo, freehigh),
        mem_unit => offset_of!(linux_raw_sys::system::sysinfo, mem_unit),
        _f => offset_of!(linux_raw_sys::system::sysinfo, _f),
    });
}

#[test]
fn itimerspec() {
    check!(ours::Itimerspec, linux_raw_sys::general::itimerspec, {
        it_interval => offset_of!(linux_raw_sys::general::itimerspec, it_interval),
        it_value => offset_of!(linux_raw_sys::general::itimerspec, it_value),
    });
}

#[test]
fn sig_info() {
    type Theirs = linux_raw_sys::general::siginfo__bindgen_ty_1__bindgen_ty_1;
    assert_eq!(
        size_of::<ours::SigInfo>(),
        size_of::<linux_raw_sys::general::siginfo>()
    );
    assert_eq!(
        align_of::<ours::SigInfo>(),
        align_of::<linux_raw_sys::general::siginfo>()
    );
    assert_eq!(
        offset_of!(ours::SigInfo, si_signo),
        offset_of!(Theirs, si_signo)
    );
    assert_eq!(
        offset_of!(ours::SigInfo, si_errno),
        offset_of!(Theirs, si_errno)
    );
    assert_eq!(
        offset_of!(ours::SigInfo, si_code),
        offset_of!(Theirs, si_code)
    );
    assert_eq!(
        offset_of!(ours::SigInfo, _sifields),
        offset_of!(Theirs, _sifields)
    );
}

#[test]
fn sockaddr() {
    type Theirs = linux_raw_sys::net::__kernel_sockaddr_storage__bindgen_ty_1__bindgen_ty_1;
    assert_eq!(
        size_of::<ours::Sockaddr>(),
        size_of::<linux_raw_sys::net::sockaddr>()
    );
    assert_eq!(
        align_of::<ours::Sockaddr>(),
        align_of::<linux_raw_sys::net::sockaddr>()
    );
    assert_eq!(
        offset_of!(ours::Sockaddr, sa_family),
        offset_of!(Theirs, ss_family)
    );
    assert_eq!(
        offset_of!(ours::Sockaddr, sa_data),
        offset_of!(Theirs, __data)
    );
}
