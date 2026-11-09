// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Linux UAPI struct layouts.
//!
//! Two modules are always compiled, independent of the target: [`x86_64`] for the x86-64
//! layout and [`generic`] for the asm-generic layout used by aarch64 and riscv64. Both use
//! fixed-width integers and `usize` for pointer-sized fields, so they describe the 64-bit
//! ABI on every supported target.
//!
//! Every struct is `#[repr(C)]`, `Copy`, and carries compile-time assertions for its size,
//! alignment and the offset of every field.

macro_rules! layout {
    ($type:ty, size = $size:expr, align = $align:expr, $($field:ident = $offset:expr),+ $(,)?) => {
        const _: () = {
            assert!(core::mem::size_of::<$type>() == $size);
            assert!(core::mem::align_of::<$type>() == $align);
            $(assert!(core::mem::offset_of!($type, $field) == $offset);)+
        };
    };
}

/// The timestamp embedded four times in [`x86_64::Statx`] and [`generic::Statx`].
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StatxTimestamp {
    pub tv_sec: i64,
    pub tv_nsec: u32,
    pub __reserved: i32,
}

layout!(
    StatxTimestamp,
    size = 16,
    align = 8,
    tv_sec = 0,
    tv_nsec = 8,
    __reserved = 12
);

/// x86-64 Linux UAPI struct layouts.
pub mod x86_64 {
    /// `struct timespec`.
    #[repr(C)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Timespec {
        pub tv_sec: i64,
        pub tv_nsec: i64,
    }

    layout!(Timespec, size = 16, align = 8, tv_sec = 0, tv_nsec = 8);

    /// `struct timeval`.
    #[repr(C)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Timeval {
        pub tv_sec: i64,
        pub tv_usec: i64,
    }

    layout!(Timeval, size = 16, align = 8, tv_sec = 0, tv_usec = 8);

    /// `struct stat` (x86-64 layout; the asm-generic layout is [`super::generic::Stat`]).
    #[repr(C)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Stat {
        pub st_dev: u64,
        pub st_ino: u64,
        pub st_nlink: u64,
        pub st_mode: u32,
        pub st_uid: u32,
        pub st_gid: u32,
        pub __pad0: u32,
        pub st_rdev: u64,
        pub st_size: i64,
        pub st_blksize: i64,
        pub st_blocks: i64,
        pub st_atime: u64,
        pub st_atime_nsec: u64,
        pub st_mtime: u64,
        pub st_mtime_nsec: u64,
        pub st_ctime: u64,
        pub st_ctime_nsec: u64,
        pub __unused: [i64; 3],
    }

    layout!(
        Stat,
        size = 144,
        align = 8,
        st_dev = 0,
        st_ino = 8,
        st_nlink = 16,
        st_mode = 24,
        st_uid = 28,
        st_gid = 32,
        __pad0 = 36,
        st_rdev = 40,
        st_size = 48,
        st_blksize = 56,
        st_blocks = 64,
        st_atime = 72,
        st_atime_nsec = 80,
        st_mtime = 88,
        st_mtime_nsec = 96,
        st_ctime = 104,
        st_ctime_nsec = 112,
        __unused = 120
    );

    /// `struct statx`.
    #[repr(C)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Statx {
        pub stx_mask: u32,
        pub stx_blksize: u32,
        pub stx_attributes: u64,
        pub stx_nlink: u32,
        pub stx_uid: u32,
        pub stx_gid: u32,
        pub stx_mode: u16,
        pub __spare0: [u16; 1],
        pub stx_ino: u64,
        pub stx_size: u64,
        pub stx_blocks: u64,
        pub stx_attributes_mask: u64,
        pub stx_atime: super::StatxTimestamp,
        pub stx_btime: super::StatxTimestamp,
        pub stx_ctime: super::StatxTimestamp,
        pub stx_mtime: super::StatxTimestamp,
        pub stx_rdev_major: u32,
        pub stx_rdev_minor: u32,
        pub stx_dev_major: u32,
        pub stx_dev_minor: u32,
        pub stx_mnt_id: u64,
        pub stx_dio_mem_align: u32,
        pub stx_dio_offset_align: u32,
        pub stx_subvol: u64,
        pub stx_atomic_write_unit_min: u32,
        pub stx_atomic_write_unit_max: u32,
        pub stx_atomic_write_segments_max: u32,
        pub stx_dio_read_offset_align: u32,
        pub stx_atomic_write_unit_max_opt: u32,
        pub __spare2: [u32; 1],
        pub __spare3: [u64; 8],
    }

    layout!(
        Statx,
        size = 256,
        align = 8,
        stx_mask = 0,
        stx_blksize = 4,
        stx_attributes = 8,
        stx_nlink = 16,
        stx_uid = 20,
        stx_gid = 24,
        stx_mode = 28,
        __spare0 = 30,
        stx_ino = 32,
        stx_size = 40,
        stx_blocks = 48,
        stx_attributes_mask = 56,
        stx_atime = 64,
        stx_btime = 80,
        stx_ctime = 96,
        stx_mtime = 112,
        stx_rdev_major = 128,
        stx_rdev_minor = 132,
        stx_dev_major = 136,
        stx_dev_minor = 140,
        stx_mnt_id = 144,
        stx_dio_mem_align = 152,
        stx_dio_offset_align = 156,
        stx_subvol = 160,
        stx_atomic_write_unit_min = 168,
        stx_atomic_write_unit_max = 172,
        stx_atomic_write_segments_max = 176,
        stx_dio_read_offset_align = 180,
        stx_atomic_write_unit_max_opt = 184,
        __spare2 = 188,
        __spare3 = 192
    );

    /// `struct iovec`.
    #[repr(C)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Iovec {
        pub iov_base: usize,
        pub iov_len: usize,
    }

    layout!(Iovec, size = 16, align = 8, iov_base = 0, iov_len = 8);

    /// `struct new_utsname`.
    #[repr(C)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Utsname {
        pub sysname: [u8; 65],
        pub nodename: [u8; 65],
        pub release: [u8; 65],
        pub version: [u8; 65],
        pub machine: [u8; 65],
        pub domainname: [u8; 65],
    }

    layout!(
        Utsname,
        size = 390,
        align = 1,
        sysname = 0,
        nodename = 65,
        release = 130,
        version = 195,
        machine = 260,
        domainname = 325
    );

    /// `struct rlimit`.
    #[repr(C)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Rlimit {
        pub rlim_cur: u64,
        pub rlim_max: u64,
    }

    layout!(Rlimit, size = 16, align = 8, rlim_cur = 0, rlim_max = 8);

    /// `struct pollfd`.
    #[repr(C)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Pollfd {
        pub fd: i32,
        pub events: i16,
        pub revents: i16,
    }

    layout!(Pollfd, size = 8, align = 4, fd = 0, events = 4, revents = 6);

    /// `struct epoll_event` (packed on x86-64).
    #[repr(C, packed)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct EpollEvent {
        pub events: u32,
        pub data: u64,
    }

    layout!(EpollEvent, size = 12, align = 1, events = 0, data = 4);

    /// The fixed header of `struct linux_dirent64`; `d_name` is a trailing byte array.
    #[repr(C)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Dirent64 {
        pub d_ino: u64,
        pub d_off: i64,
        pub d_reclen: u16,
        pub d_type: u8,
        pub d_name: [u8; 0],
    }

    layout!(
        Dirent64,
        size = 24,
        align = 8,
        d_ino = 0,
        d_off = 8,
        d_reclen = 16,
        d_type = 18,
        d_name = 19
    );

    /// `struct sigaction` in the kernel layout.
    #[repr(C)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct SigAction {
        pub sa_handler: usize,
        pub sa_flags: u64,
        pub sa_restorer: usize,
        pub sa_mask: u64,
    }

    layout!(
        SigAction,
        size = 32,
        align = 8,
        sa_handler = 0,
        sa_flags = 8,
        sa_restorer = 16,
        sa_mask = 24
    );

    /// The leading fields and total size of `siginfo_t`.
    #[repr(C)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct SigInfo {
        pub si_signo: i32,
        pub si_errno: i32,
        pub si_code: i32,
        __pad: i32,
        pub _sifields: [u8; 112],
        __align: [u64; 0],
    }

    layout!(
        SigInfo,
        size = 128,
        align = 8,
        si_signo = 0,
        si_errno = 4,
        si_code = 8,
        __pad = 12,
        _sifields = 16,
        __align = 128
    );

    /// `struct sockaddr` storage (`__kernel_sockaddr_storage`), 128 bytes.
    #[repr(C)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Sockaddr {
        pub sa_family: u16,
        pub sa_data: [u8; 126],
        __align: [u64; 0],
    }

    layout!(
        Sockaddr,
        size = 128,
        align = 8,
        sa_family = 0,
        sa_data = 2,
        __align = 128
    );

    /// `struct sockaddr_in`.
    #[repr(C)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct SockaddrIn {
        pub sin_family: u16,
        pub sin_port: u16,
        pub sin_addr: u32,
        pub __pad: [u8; 8],
    }

    layout!(
        SockaddrIn,
        size = 16,
        align = 4,
        sin_family = 0,
        sin_port = 2,
        sin_addr = 4,
        __pad = 8
    );

    /// `struct sockaddr_in6`.
    #[repr(C)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct SockaddrIn6 {
        pub sin6_family: u16,
        pub sin6_port: u16,
        pub sin6_flowinfo: u32,
        pub sin6_addr: [u8; 16],
        pub sin6_scope_id: u32,
    }

    layout!(
        SockaddrIn6,
        size = 28,
        align = 4,
        sin6_family = 0,
        sin6_port = 2,
        sin6_flowinfo = 4,
        sin6_addr = 8,
        sin6_scope_id = 24
    );

    /// `struct sockaddr_un`.
    #[repr(C)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct SockaddrUn {
        pub sun_family: u16,
        pub sun_path: [u8; 108],
    }

    layout!(
        SockaddrUn,
        size = 110,
        align = 2,
        sun_family = 0,
        sun_path = 2
    );

    /// `struct msghdr`.
    #[repr(C)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Msghdr {
        pub msg_name: usize,
        pub msg_namelen: u32,
        pub msg_iov: usize,
        pub msg_iovlen: usize,
        pub msg_control: usize,
        pub msg_controllen: usize,
        pub msg_flags: u32,
    }

    layout!(
        Msghdr,
        size = 56,
        align = 8,
        msg_name = 0,
        msg_namelen = 8,
        msg_iov = 16,
        msg_iovlen = 24,
        msg_control = 32,
        msg_controllen = 40,
        msg_flags = 48
    );

    /// `struct cmsghdr`.
    #[repr(C)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Cmsghdr {
        pub cmsg_len: usize,
        pub cmsg_level: i32,
        pub cmsg_type: i32,
    }

    layout!(
        Cmsghdr,
        size = 16,
        align = 8,
        cmsg_len = 0,
        cmsg_level = 8,
        cmsg_type = 12
    );

    /// `struct rusage`.
    #[repr(C)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Rusage {
        pub ru_utime: Timeval,
        pub ru_stime: Timeval,
        pub ru_maxrss: i64,
        pub ru_ixrss: i64,
        pub ru_idrss: i64,
        pub ru_isrss: i64,
        pub ru_minflt: i64,
        pub ru_majflt: i64,
        pub ru_nswap: i64,
        pub ru_inblock: i64,
        pub ru_oublock: i64,
        pub ru_msgsnd: i64,
        pub ru_msgrcv: i64,
        pub ru_nsignals: i64,
        pub ru_nvcsw: i64,
        pub ru_nivcsw: i64,
    }

    layout!(
        Rusage,
        size = 144,
        align = 8,
        ru_utime = 0,
        ru_stime = 16,
        ru_maxrss = 32,
        ru_ixrss = 40,
        ru_idrss = 48,
        ru_isrss = 56,
        ru_minflt = 64,
        ru_majflt = 72,
        ru_nswap = 80,
        ru_inblock = 88,
        ru_oublock = 96,
        ru_msgsnd = 104,
        ru_msgrcv = 112,
        ru_nsignals = 120,
        ru_nvcsw = 128,
        ru_nivcsw = 136
    );

    /// `struct sysinfo`; `_f` is a trailing variable-length byte array.
    #[repr(C)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Sysinfo {
        pub uptime: i64,
        pub loads: [u64; 3],
        pub totalram: u64,
        pub freeram: u64,
        pub sharedram: u64,
        pub bufferram: u64,
        pub totalswap: u64,
        pub freeswap: u64,
        pub procs: u16,
        pub pad: u16,
        pub totalhigh: u64,
        pub freehigh: u64,
        pub mem_unit: u32,
        pub _f: [u8; 0],
    }

    layout!(
        Sysinfo,
        size = 112,
        align = 8,
        uptime = 0,
        loads = 8,
        totalram = 32,
        freeram = 40,
        sharedram = 48,
        bufferram = 56,
        totalswap = 64,
        freeswap = 72,
        procs = 80,
        pad = 82,
        totalhigh = 88,
        freehigh = 96,
        mem_unit = 104,
        _f = 108
    );

    /// `struct itimerspec`.
    #[repr(C)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Itimerspec {
        pub it_interval: Timespec,
        pub it_value: Timespec,
    }

    layout!(
        Itimerspec,
        size = 32,
        align = 8,
        it_interval = 0,
        it_value = 16
    );
}

/// The asm-generic Linux UAPI struct layouts (aarch64 and riscv64).
pub mod generic {
    /// `struct timespec`.
    #[repr(C)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Timespec {
        pub tv_sec: i64,
        pub tv_nsec: i64,
    }

    layout!(Timespec, size = 16, align = 8, tv_sec = 0, tv_nsec = 8);

    /// `struct timeval`.
    #[repr(C)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Timeval {
        pub tv_sec: i64,
        pub tv_usec: i64,
    }

    layout!(Timeval, size = 16, align = 8, tv_sec = 0, tv_usec = 8);

    /// `struct stat` (asm-generic layout; x86-64 is [`super::x86_64::Stat`]).
    #[repr(C)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Stat {
        pub st_dev: u64,
        pub st_ino: u64,
        pub st_mode: u32,
        pub st_nlink: u32,
        pub st_uid: u32,
        pub st_gid: u32,
        pub st_rdev: u64,
        pub __pad1: u64,
        pub st_size: i64,
        pub st_blksize: i32,
        pub __pad2: i32,
        pub st_blocks: i64,
        pub st_atime: i64,
        pub st_atime_nsec: u64,
        pub st_mtime: i64,
        pub st_mtime_nsec: u64,
        pub st_ctime: i64,
        pub st_ctime_nsec: u64,
        pub __unused4: u32,
        pub __unused5: u32,
    }

    layout!(
        Stat,
        size = 128,
        align = 8,
        st_dev = 0,
        st_ino = 8,
        st_mode = 16,
        st_nlink = 20,
        st_uid = 24,
        st_gid = 28,
        st_rdev = 32,
        __pad1 = 40,
        st_size = 48,
        st_blksize = 56,
        __pad2 = 60,
        st_blocks = 64,
        st_atime = 72,
        st_atime_nsec = 80,
        st_mtime = 88,
        st_mtime_nsec = 96,
        st_ctime = 104,
        st_ctime_nsec = 112,
        __unused4 = 120,
        __unused5 = 124
    );

    /// `struct statx` (identical layout on both supported architectures).
    #[repr(C)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Statx {
        pub stx_mask: u32,
        pub stx_blksize: u32,
        pub stx_attributes: u64,
        pub stx_nlink: u32,
        pub stx_uid: u32,
        pub stx_gid: u32,
        pub stx_mode: u16,
        pub __spare0: [u16; 1],
        pub stx_ino: u64,
        pub stx_size: u64,
        pub stx_blocks: u64,
        pub stx_attributes_mask: u64,
        pub stx_atime: super::StatxTimestamp,
        pub stx_btime: super::StatxTimestamp,
        pub stx_ctime: super::StatxTimestamp,
        pub stx_mtime: super::StatxTimestamp,
        pub stx_rdev_major: u32,
        pub stx_rdev_minor: u32,
        pub stx_dev_major: u32,
        pub stx_dev_minor: u32,
        pub stx_mnt_id: u64,
        pub stx_dio_mem_align: u32,
        pub stx_dio_offset_align: u32,
        pub stx_subvol: u64,
        pub stx_atomic_write_unit_min: u32,
        pub stx_atomic_write_unit_max: u32,
        pub stx_atomic_write_segments_max: u32,
        pub stx_dio_read_offset_align: u32,
        pub stx_atomic_write_unit_max_opt: u32,
        pub __spare2: [u32; 1],
        pub __spare3: [u64; 8],
    }

    layout!(
        Statx,
        size = 256,
        align = 8,
        stx_mask = 0,
        stx_blksize = 4,
        stx_attributes = 8,
        stx_nlink = 16,
        stx_uid = 20,
        stx_gid = 24,
        stx_mode = 28,
        __spare0 = 30,
        stx_ino = 32,
        stx_size = 40,
        stx_blocks = 48,
        stx_attributes_mask = 56,
        stx_atime = 64,
        stx_btime = 80,
        stx_ctime = 96,
        stx_mtime = 112,
        stx_rdev_major = 128,
        stx_rdev_minor = 132,
        stx_dev_major = 136,
        stx_dev_minor = 140,
        stx_mnt_id = 144,
        stx_dio_mem_align = 152,
        stx_dio_offset_align = 156,
        stx_subvol = 160,
        stx_atomic_write_unit_min = 168,
        stx_atomic_write_unit_max = 172,
        stx_atomic_write_segments_max = 176,
        stx_dio_read_offset_align = 180,
        stx_atomic_write_unit_max_opt = 184,
        __spare2 = 188,
        __spare3 = 192
    );

    /// `struct iovec`.
    #[repr(C)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Iovec {
        pub iov_base: usize,
        pub iov_len: usize,
    }

    layout!(Iovec, size = 16, align = 8, iov_base = 0, iov_len = 8);

    /// `struct new_utsname`.
    #[repr(C)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Utsname {
        pub sysname: [u8; 65],
        pub nodename: [u8; 65],
        pub release: [u8; 65],
        pub version: [u8; 65],
        pub machine: [u8; 65],
        pub domainname: [u8; 65],
    }

    layout!(
        Utsname,
        size = 390,
        align = 1,
        sysname = 0,
        nodename = 65,
        release = 130,
        version = 195,
        machine = 260,
        domainname = 325
    );

    /// `struct rlimit`.
    #[repr(C)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Rlimit {
        pub rlim_cur: u64,
        pub rlim_max: u64,
    }

    layout!(Rlimit, size = 16, align = 8, rlim_cur = 0, rlim_max = 8);

    /// `struct pollfd`.
    #[repr(C)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Pollfd {
        pub fd: i32,
        pub events: i16,
        pub revents: i16,
    }

    layout!(Pollfd, size = 8, align = 4, fd = 0, events = 4, revents = 6);

    /// `struct epoll_event` (naturally aligned in the asm-generic layout).
    #[repr(C)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct EpollEvent {
        pub events: u32,
        pub data: u64,
    }

    layout!(EpollEvent, size = 16, align = 8, events = 0, data = 8);

    /// The fixed header of `struct linux_dirent64`; `d_name` is a trailing byte array.
    #[repr(C)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Dirent64 {
        pub d_ino: u64,
        pub d_off: i64,
        pub d_reclen: u16,
        pub d_type: u8,
        pub d_name: [u8; 0],
    }

    layout!(
        Dirent64,
        size = 24,
        align = 8,
        d_ino = 0,
        d_off = 8,
        d_reclen = 16,
        d_type = 18,
        d_name = 19
    );

    /// `struct sigaction` in the kernel layout.
    #[repr(C)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct SigAction {
        pub sa_handler: usize,
        pub sa_flags: u64,
        pub sa_restorer: usize,
        pub sa_mask: u64,
    }

    layout!(
        SigAction,
        size = 32,
        align = 8,
        sa_handler = 0,
        sa_flags = 8,
        sa_restorer = 16,
        sa_mask = 24
    );

    /// The leading fields and total size of `siginfo_t`.
    #[repr(C)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct SigInfo {
        pub si_signo: i32,
        pub si_errno: i32,
        pub si_code: i32,
        __pad: i32,
        pub _sifields: [u8; 112],
        __align: [u64; 0],
    }

    layout!(
        SigInfo,
        size = 128,
        align = 8,
        si_signo = 0,
        si_errno = 4,
        si_code = 8,
        __pad = 12,
        _sifields = 16,
        __align = 128
    );

    /// `struct sockaddr` storage (`__kernel_sockaddr_storage`), 128 bytes.
    #[repr(C)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Sockaddr {
        pub sa_family: u16,
        pub sa_data: [u8; 126],
        __align: [u64; 0],
    }

    layout!(
        Sockaddr,
        size = 128,
        align = 8,
        sa_family = 0,
        sa_data = 2,
        __align = 128
    );

    /// `struct sockaddr_in`.
    #[repr(C)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct SockaddrIn {
        pub sin_family: u16,
        pub sin_port: u16,
        pub sin_addr: u32,
        pub __pad: [u8; 8],
    }

    layout!(
        SockaddrIn,
        size = 16,
        align = 4,
        sin_family = 0,
        sin_port = 2,
        sin_addr = 4,
        __pad = 8
    );

    /// `struct sockaddr_in6`.
    #[repr(C)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct SockaddrIn6 {
        pub sin6_family: u16,
        pub sin6_port: u16,
        pub sin6_flowinfo: u32,
        pub sin6_addr: [u8; 16],
        pub sin6_scope_id: u32,
    }

    layout!(
        SockaddrIn6,
        size = 28,
        align = 4,
        sin6_family = 0,
        sin6_port = 2,
        sin6_flowinfo = 4,
        sin6_addr = 8,
        sin6_scope_id = 24
    );

    /// `struct sockaddr_un`.
    #[repr(C)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct SockaddrUn {
        pub sun_family: u16,
        pub sun_path: [u8; 108],
    }

    layout!(
        SockaddrUn,
        size = 110,
        align = 2,
        sun_family = 0,
        sun_path = 2
    );

    /// `struct msghdr`.
    #[repr(C)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Msghdr {
        pub msg_name: usize,
        pub msg_namelen: u32,
        pub msg_iov: usize,
        pub msg_iovlen: usize,
        pub msg_control: usize,
        pub msg_controllen: usize,
        pub msg_flags: u32,
    }

    layout!(
        Msghdr,
        size = 56,
        align = 8,
        msg_name = 0,
        msg_namelen = 8,
        msg_iov = 16,
        msg_iovlen = 24,
        msg_control = 32,
        msg_controllen = 40,
        msg_flags = 48
    );

    /// `struct cmsghdr`.
    #[repr(C)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Cmsghdr {
        pub cmsg_len: usize,
        pub cmsg_level: i32,
        pub cmsg_type: i32,
    }

    layout!(
        Cmsghdr,
        size = 16,
        align = 8,
        cmsg_len = 0,
        cmsg_level = 8,
        cmsg_type = 12
    );

    /// `struct rusage`.
    #[repr(C)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Rusage {
        pub ru_utime: Timeval,
        pub ru_stime: Timeval,
        pub ru_maxrss: i64,
        pub ru_ixrss: i64,
        pub ru_idrss: i64,
        pub ru_isrss: i64,
        pub ru_minflt: i64,
        pub ru_majflt: i64,
        pub ru_nswap: i64,
        pub ru_inblock: i64,
        pub ru_oublock: i64,
        pub ru_msgsnd: i64,
        pub ru_msgrcv: i64,
        pub ru_nsignals: i64,
        pub ru_nvcsw: i64,
        pub ru_nivcsw: i64,
    }

    layout!(
        Rusage,
        size = 144,
        align = 8,
        ru_utime = 0,
        ru_stime = 16,
        ru_maxrss = 32,
        ru_ixrss = 40,
        ru_idrss = 48,
        ru_isrss = 56,
        ru_minflt = 64,
        ru_majflt = 72,
        ru_nswap = 80,
        ru_inblock = 88,
        ru_oublock = 96,
        ru_msgsnd = 104,
        ru_msgrcv = 112,
        ru_nsignals = 120,
        ru_nvcsw = 128,
        ru_nivcsw = 136
    );

    /// `struct sysinfo`; `_f` is a trailing variable-length byte array.
    #[repr(C)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Sysinfo {
        pub uptime: i64,
        pub loads: [u64; 3],
        pub totalram: u64,
        pub freeram: u64,
        pub sharedram: u64,
        pub bufferram: u64,
        pub totalswap: u64,
        pub freeswap: u64,
        pub procs: u16,
        pub pad: u16,
        pub totalhigh: u64,
        pub freehigh: u64,
        pub mem_unit: u32,
        pub _f: [u8; 0],
    }

    layout!(
        Sysinfo,
        size = 112,
        align = 8,
        uptime = 0,
        loads = 8,
        totalram = 32,
        freeram = 40,
        sharedram = 48,
        bufferram = 56,
        totalswap = 64,
        freeswap = 72,
        procs = 80,
        pad = 82,
        totalhigh = 88,
        freehigh = 96,
        mem_unit = 104,
        _f = 108
    );

    /// `struct itimerspec`.
    #[repr(C)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Itimerspec {
        pub it_interval: Timespec,
        pub it_value: Timespec,
    }

    layout!(
        Itimerspec,
        size = 32,
        align = 8,
        it_interval = 0,
        it_value = 16
    );
}
