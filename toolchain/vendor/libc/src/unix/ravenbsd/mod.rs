// SPDX-License-Identifier: MIT
// Copyright (c) 2014 The Rust Project Developers

use crate::pid_t;
use crate::prelude::*;

pub type wchar_t = i32;

// The kernel signal.h layout contract: handler at 0, the 128-byte mask at 8, flags at
// 136 and the restorer at 144 (libc-ravenlibc-abi Sigaction). Array lengths are the
// macro-free static assertion: a mismatched layout fails to compile.
const _: [(); 152] = [(); core::mem::size_of::<sigaction>()];
const _: [(); 0] = [(); core::mem::offset_of!(sigaction, sa_sigaction)];
const _: [(); 8] = [(); core::mem::offset_of!(sigaction, sa_mask)];
const _: [(); 136] = [(); core::mem::offset_of!(sigaction, sa_flags)];
const _: [(); 144] = [(); core::mem::offset_of!(sigaction, sa_restorer)];

pub type blkcnt_t = c_longlong;
pub type blksize_t = i32;
pub type clock_t = c_long;
pub type clockid_t = c_int;
pub type dev_t = c_ulonglong;
pub type fsblkcnt_t = c_ulong;
pub type fsfilcnt_t = c_ulong;
pub type ino_t = c_ulonglong;
pub type mode_t = u32;
pub type nfds_t = c_ulong;
pub type nlink_t = u32;
pub type off_t = c_longlong;
pub type pthread_t = *mut c_void;
// Must be usize due to library/std/sys_common/thread_local.rs,
// should technically be *mut c_void
pub type pthread_key_t = usize;
pub type rlim_t = c_ulonglong;
pub type sa_family_t = u16;
pub type sem_t = *mut c_void;
pub type socklen_t = u32;
pub type speed_t = u32;
pub type suseconds_t = c_longlong;
pub type tcflag_t = u32;
pub type time_t = c_longlong;
pub type id_t = c_uint;
pub type nl_item = c_int;
pub type timer_t = *mut c_void;
pub type uid_t = u32;
pub type gid_t = u32;

extern_ty! {
    pub enum timezone {}
}

s! {
    #[repr(C)]
    pub struct utsname {
        pub sysname: [c_char; UTSLENGTH],
        pub nodename: [c_char; UTSLENGTH],
        pub release: [c_char; UTSLENGTH],
        pub version: [c_char; UTSLENGTH],
        pub machine: [c_char; UTSLENGTH],
        pub domainname: [c_char; UTSLENGTH],
    }

    pub struct sigset_t {
        __val: [c_ulong; 16],
    }

    pub struct dirent {
        pub d_ino: crate::ino_t,
        pub d_off: off_t,
        pub d_reclen: c_ushort,
        pub d_type: c_uchar,
        pub d_name: [c_char; 256],
    }

    pub struct sockaddr_un {
        pub sun_family: crate::sa_family_t,
        pub sun_path: [c_char; 108],
    }

    pub struct sockaddr_storage {
        pub ss_family: crate::sa_family_t,
        __ss_padding: Padding<[u8; 128 - size_of::<sa_family_t>() - size_of::<c_ulong>()]>,
        __ss_align: c_ulong,
    }

    pub struct addrinfo {
        pub ai_flags: c_int,
        pub ai_family: c_int,
        pub ai_socktype: c_int,
        pub ai_protocol: c_int,
        pub ai_addrlen: size_t,
        pub ai_canonname: *mut c_char,
        pub ai_addr: *mut crate::sockaddr,
        pub ai_next: *mut crate::addrinfo,
    }

    pub struct Dl_info {
        pub dli_fname: *const c_char,
        pub dli_fbase: *mut c_void,
        pub dli_sname: *const c_char,
        pub dli_saddr: *mut c_void,
    }

    pub struct epoll_event {
        pub events: u32,
        pub u64: u64,
        pub _pad: u64,
    }

    pub struct fd_set {
        fds_bits: [c_ulong; crate::FD_SETSIZE as usize / ULONG_SIZE],
    }

    pub struct in_addr {
        pub s_addr: crate::in_addr_t,
    }

    pub struct ip_mreq {
        pub imr_multiaddr: crate::in_addr,
        pub imr_interface: crate::in_addr,
    }

    pub struct lconv {
        pub currency_symbol: *const c_char,
        pub decimal_point: *const c_char,
        pub frac_digits: c_char,
        pub grouping: *const c_char,
        pub int_curr_symbol: *const c_char,
        pub int_frac_digits: c_char,
        pub mon_decimal_point: *const c_char,
        pub mon_grouping: *const c_char,
        pub mon_thousands_sep: *const c_char,
        pub negative_sign: *const c_char,
        pub n_cs_precedes: c_char,
        pub n_sep_by_space: c_char,
        pub n_sign_posn: c_char,
        pub positive_sign: *const c_char,
        pub p_cs_precedes: c_char,
        pub p_sep_by_space: c_char,
        pub p_sign_posn: c_char,
        pub thousands_sep: *const c_char,
    }

    pub struct msghdr {
        pub msg_name: *mut c_void,
        pub msg_namelen: crate::socklen_t,
        pub msg_iov: *mut crate::iovec,
        pub msg_iovlen: size_t,
        pub msg_control: *mut c_void,
        pub msg_controllen: size_t,
        pub msg_flags: c_int,
    }

    pub struct cmsghdr {
        pub cmsg_len: size_t,
        pub cmsg_level: c_int,
        pub cmsg_type: c_int,
    }

    pub struct passwd {
        pub pw_name: *mut c_char,
        pub pw_passwd: *mut c_char,
        pub pw_uid: crate::uid_t,
        pub pw_gid: crate::gid_t,
        pub pw_gecos: *mut c_char,
        pub pw_dir: *mut c_char,
        pub pw_shell: *mut c_char,
    }

    // FIXME(1.0): This should not implement `PartialEq`
    #[allow(unpredictable_function_pointer_comparisons)]
    pub struct sigaction {
        pub sa_sigaction: crate::sighandler_t,
        pub sa_mask: crate::sigset_t,
        pub sa_flags: c_int,
        pub sa_restorer: Option<extern "C" fn()>,
    }

    pub struct siginfo_t {
        pub si_signo: c_int,
        pub si_errno: c_int,
        pub si_code: c_int,
        _pad: Padding<[c_int; 29]>,
        _align: [usize; 0],
    }

    pub struct sockaddr {
        pub sa_family: crate::sa_family_t,
        pub sa_data: [c_char; 14],
    }

    pub struct sockaddr_in {
        pub sin_family: crate::sa_family_t,
        pub sin_port: crate::in_port_t,
        pub sin_addr: crate::in_addr,
        pub sin_zero: [c_char; 8],
    }

    pub struct sockaddr_in6 {
        pub sin6_family: crate::sa_family_t,
        pub sin6_port: crate::in_port_t,
        pub sin6_flowinfo: u32,
        pub sin6_addr: crate::in6_addr,
        pub sin6_scope_id: u32,
    }

    pub struct stat {
        pub st_dev: crate::dev_t,
        pub st_ino: crate::ino_t,
        pub st_mode: mode_t,
        pub st_nlink: crate::nlink_t,
        pub st_uid: crate::uid_t,
        pub st_gid: crate::gid_t,
        pub st_rdev: crate::dev_t,
        pub __pad1: u64,
        pub st_size: off_t,
        pub st_blksize: crate::blksize_t,
        pub __pad2: i32,
        pub st_blocks: crate::blkcnt_t,
        pub st_atime: crate::time_t,
        pub st_atime_nsec: c_long,
        pub st_mtime: crate::time_t,
        pub st_mtime_nsec: c_long,
        pub st_ctime: crate::time_t,
        pub st_ctime_nsec: c_long,
        pub __unused: [u32; 2],
    }

    pub struct statvfs {
        pub f_bsize: c_ulong,
        pub f_frsize: c_ulong,
        pub f_blocks: crate::fsblkcnt_t,
        pub f_bfree: crate::fsblkcnt_t,
        pub f_bavail: crate::fsblkcnt_t,
        pub f_files: crate::fsfilcnt_t,
        pub f_ffree: crate::fsfilcnt_t,
        pub f_favail: crate::fsfilcnt_t,
        pub f_fsid: c_ulong,
        pub f_flag: c_ulong,
        pub f_namemax: c_ulong,
        pub f_type: u32,
        pub __reserved: [i32; 5],
    }

    pub struct termios {
        pub c_iflag: crate::tcflag_t,
        pub c_oflag: crate::tcflag_t,
        pub c_cflag: crate::tcflag_t,
        pub c_lflag: crate::tcflag_t,
        pub c_line: crate::cc_t,
        pub c_cc: [crate::cc_t; crate::NCCS],
        pub c_ispeed: crate::speed_t,
        pub c_ospeed: crate::speed_t,
    }

    pub struct tm {
        pub tm_sec: c_int,
        pub tm_min: c_int,
        pub tm_hour: c_int,
        pub tm_mday: c_int,
        pub tm_mon: c_int,
        pub tm_year: c_int,
        pub tm_wday: c_int,
        pub tm_yday: c_int,
        pub tm_isdst: c_int,
        pub tm_gmtoff: c_long,
        pub tm_zone: *const c_char,
    }

    pub struct ucred {
        pub pid: crate::pid_t,
        pub uid: uid_t,
        pub gid: gid_t,
    }

    #[cfg_attr(target_pointer_width = "32", repr(C, align(4)))]
    #[cfg_attr(target_pointer_width = "64", repr(C, align(8)))]
    pub struct pthread_attr_t {
        bytes: [u8; _PTHREAD_ATTR_SIZE],
    }
    #[repr(C)]
    #[repr(align(8))]
    pub struct pthread_barrier_t {
        bytes: [u8; _PTHREAD_BARRIER_SIZE],
    }
    #[repr(C)]
    #[repr(align(4))]
    pub struct pthread_barrierattr_t {
        bytes: [u8; _PTHREAD_BARRIERATTR_SIZE],
    }
    #[repr(C)]
    #[repr(align(8))]
    pub struct pthread_mutex_t {
        bytes: [u8; _PTHREAD_MUTEX_SIZE],
    }
    #[repr(C)]
    #[repr(align(8))]
    pub struct pthread_rwlock_t {
        bytes: [u8; _PTHREAD_RWLOCK_SIZE],
    }
    #[repr(C)]
    #[repr(align(4))]
    pub struct pthread_mutexattr_t {
        bytes: [u8; _PTHREAD_MUTEXATTR_SIZE],
    }
    #[repr(C)]
    #[repr(align(4))]
    pub struct pthread_rwlockattr_t {
        bytes: [u8; _PTHREAD_RWLOCKATTR_SIZE],
    }
    #[repr(C)]
    #[repr(align(8))]
    pub struct pthread_cond_t {
        bytes: [u8; _PTHREAD_COND_SIZE],
    }
    #[repr(C)]
    #[repr(align(4))]
    pub struct pthread_condattr_t {
        bytes: [u8; _PTHREAD_CONDATTR_SIZE],
    }
    #[repr(C)]
    #[repr(align(4))]
    pub struct pthread_once_t {
        bytes: [u8; _PTHREAD_ONCE_SIZE],
    }
    #[repr(C)]
    #[repr(align(4))]
    pub struct pthread_spinlock_t {
        bytes: [u8; _PTHREAD_SPINLOCK_SIZE],
    }

    pub struct rlimit {
        pub rlim_cur: crate::rlim_t,
        pub rlim_max: crate::rlim_t,
    }

    pub struct itimerspec {
        pub it_interval: crate::timespec,
        pub it_value: crate::timespec,
    }

    pub struct flock {
        pub l_type: c_short,
        pub l_whence: c_short,
        pub l_start: off_t,
        pub l_len: off_t,
        pub l_pid: crate::pid_t,
    }

    pub struct fsid_t {
        pub __val: [c_int; 2],
    }

    pub struct statfs {
        pub f_type: c_ulong,
        pub f_bsize: c_ulong,
        pub f_blocks: crate::fsblkcnt_t,
        pub f_bfree: crate::fsblkcnt_t,
        pub f_bavail: crate::fsblkcnt_t,
        pub f_files: crate::fsfilcnt_t,
        pub f_ffree: crate::fsfilcnt_t,
        pub f_fsid: crate::fsid_t,
        pub f_namelen: c_ulong,
        pub f_frsize: c_ulong,
        pub f_flags: c_ulong,
        pub f_spare: [c_ulong; 4],
    }

    pub struct sigevent {
        pub sigev_value: crate::sigval,
        pub sigev_signo: c_int,
        pub sigev_notify: c_int,
        pub sigev_notify_thread_id: c_int,
        #[cfg(target_pointer_width = "64")]
        __unused1: Padding<[c_int; 11]>,
        #[cfg(target_pointer_width = "32")]
        __unused1: Padding<[c_int; 12]>,
    }
}
const _PTHREAD_ATTR_SIZE: usize = 56;
const _PTHREAD_RWLOCKATTR_SIZE: usize = 8;
const _PTHREAD_RWLOCK_SIZE: usize = 56;
const _PTHREAD_BARRIER_SIZE: usize = 32;
const _PTHREAD_BARRIERATTR_SIZE: usize = 4;
const _PTHREAD_CONDATTR_SIZE: usize = 4;
const _PTHREAD_COND_SIZE: usize = 48;
const _PTHREAD_MUTEX_SIZE: usize = 40;
const _PTHREAD_MUTEXATTR_SIZE: usize = 4;
const _PTHREAD_ONCE_SIZE: usize = 4;
const _PTHREAD_SPINLOCK_SIZE: usize = 4;

#[repr(C)]
#[derive(Copy, Clone)]
struct sifields_sigchld {
    pub si_pid: crate::pid_t,
    pub si_uid: crate::uid_t,
    pub si_status: c_int,
    pub si_utime: c_long,
    pub si_stime: c_long,
}

#[repr(C)]
union sifields {
    _align_pointer: *mut c_void,
    sigchld: sifields_sigchld,
}

#[repr(C)]
struct siginfo_f {
    _siginfo_base: [c_int; 3],
    sifields: sifields,
}

impl siginfo_t {
    unsafe fn sifields(&self) -> &sifields {
        unsafe { &(*(self as *const siginfo_t as *const siginfo_f)).sifields }
    }

    pub unsafe fn si_pid(&self) -> crate::pid_t {
        unsafe { self.sifields().sigchld.si_pid }
    }

    pub unsafe fn si_uid(&self) -> crate::uid_t {
        unsafe { self.sifields().sigchld.si_uid }
    }

    pub unsafe fn si_status(&self) -> c_int {
        unsafe { self.sifields().sigchld.si_status }
    }

    pub unsafe fn si_utime(&self) -> c_long {
        unsafe { self.sifields().sigchld.si_utime }
    }

    pub unsafe fn si_stime(&self) -> c_long {
        unsafe { self.sifields().sigchld.si_stime }
    }
}

pub const UTSLENGTH: usize = 65;

// intentionally not public, only used for fd_set
cfg_if! {
    if #[cfg(target_pointer_width = "32")] {
        const ULONG_SIZE: usize = 32;
    } else if #[cfg(target_pointer_width = "64")] {
        const ULONG_SIZE: usize = 64;
    } else {
        // Unknown target_pointer_width
    }
}

// limits.h
pub const PATH_MAX: c_int = 4096;

// fcntl.h

pub const AT_FDCWD: c_int = -100;
pub const AT_EACCESS: c_int = 512;
pub const AT_EMPTY_PATH: c_int = 4096;
pub const AT_NO_AUTOMOUNT: c_int = 2048;
pub const AT_RECURSIVE: c_int = 32768;
pub const AT_REMOVEDIR: c_int = 512;
pub const AT_RESOLVE_BENEATH: c_int = 65536;
pub const AT_STATX_DONT_SYNC: c_int = 16384;
pub const AT_STATX_FORCE_SYNC: c_int = 8192;
pub const AT_STATX_SYNC_AS_STAT: c_int = 0;
pub const AT_STATX_SYNC_TYPE: c_int = 24576;
pub const AT_SYMLINK_FOLLOW: c_int = 1024;
pub const AT_SYMLINK_NOFOLLOW: c_int = 256;

pub const F_ADD_SEALS: i32 = 1033;
pub const F_CANCELLK: i32 = 1029;
pub const F_DUPFD: i32 = 0;
pub const F_DUPFD_CLOEXEC: i32 = 1030;
pub const F_GETFD: i32 = 1;
pub const F_GETFL: i32 = 3;
pub const F_GETLEASE: i32 = 1025;
pub const F_GETLK: i32 = 5;
pub const F_GETLK64: i32 = F_GETLK;
pub const F_GETOWN: i32 = 9;
pub const F_GETOWNER_UIDS: i32 = 17;
pub const F_GETOWN_EX: i32 = 16;
pub const F_GETPIPE_SZ: i32 = 1032;
pub const F_GETSIG: i32 = 11;
pub const F_GET_FILE_RW_HINT: i32 = 1037;
pub const F_GET_RW_HINT: i32 = 1035;
pub const F_GET_SEALS: i32 = 1034;
pub const F_LOCK: i32 = 1;
pub const F_NOTIFY: i32 = 1026;
pub const F_OFD_GETLK: i32 = 36;
pub const F_OFD_SETLK: i32 = 37;
pub const F_OFD_SETLKW: i32 = 38;
pub const F_OK: i32 = 0;
pub const F_OWNER_GID: i32 = 2;
pub const F_OWNER_PGRP: i32 = 2;
pub const F_OWNER_PID: i32 = 1;
pub const F_OWNER_TID: i32 = 0;
pub const F_RDLCK: i32 = 0;
pub const F_SEAL_FUTURE_WRITE: i32 = 16;
pub const F_SEAL_GROW: i32 = 4;
pub const F_SEAL_SEAL: i32 = 1;
pub const F_SEAL_SHRINK: i32 = 2;
pub const F_SEAL_WRITE: i32 = 8;
pub const F_SETFD: i32 = 2;
pub const F_SETFL: i32 = 4;
pub const F_SETLEASE: i32 = 1024;
pub const F_SETLK: i32 = 6;
pub const F_SETLK64: i32 = F_SETLK;
pub const F_SETLKW: i32 = 7;
pub const F_SETLKW64: i32 = F_SETLKW;
pub const F_SETOWN: i32 = 8;
pub const F_SETOWN_EX: i32 = 15;
pub const F_SETPIPE_SZ: i32 = 1031;
pub const F_SETSIG: i32 = 10;
pub const F_SET_FILE_RW_HINT: i32 = 1038;
pub const F_SET_RW_HINT: i32 = 1036;
pub const F_TEST: i32 = 3;
pub const F_TLOCK: i32 = 2;
pub const F_ULOCK: i32 = 0;
pub const F_UNLCK: i32 = 2;
pub const F_WRLCK: i32 = 1;

pub const POSIX_FADV_NORMAL: c_int = 0;
pub const POSIX_FADV_RANDOM: c_int = 1;
pub const POSIX_FADV_SEQUENTIAL: c_int = 2;
pub const POSIX_FADV_WILLNEED: c_int = 3;
pub const POSIX_FADV_DONTNEED: c_int = 4;
pub const POSIX_FADV_NOREUSE: c_int = 5;
pub const FALLOC_FL_KEEP_SIZE: c_int = 0x01;
pub const FALLOC_FL_PUNCH_HOLE: c_int = 0x02;
pub const FALLOC_FL_NO_HIDE_STALE: c_int = 0x04;
pub const FALLOC_FL_COLLAPSE_RANGE: c_int = 0x08;
pub const FALLOC_FL_ZERO_RANGE: c_int = 0x10;
pub const FALLOC_FL_INSERT_RANGE: c_int = 0x20;
pub const FALLOC_FL_UNSHARE_RANGE: c_int = 0x40;

// FIXME(redox): relibc {
pub const RTLD_DEFAULT: *mut c_void = ptr::null_mut();
// }

// dlfcn.h
pub const RTLD_LAZY: c_int = 0x0001;
pub const RTLD_NOW: c_int = 0x0002;
pub const RTLD_NOLOAD: c_int = 0x0004;
pub const RTLD_GLOBAL: c_int = 0x0100;
pub const RTLD_LOCAL: c_int = 0x0000;

// errno.h
pub const EPERM: c_int = 1; /* Operation not permitted */
pub const ENOENT: c_int = 2; /* No such file or directory */
pub const ESRCH: c_int = 3; /* No such process */
pub const EINTR: c_int = 4; /* Interrupted system call */
pub const EIO: c_int = 5; /* I/O error */
pub const ENXIO: c_int = 6; /* No such device or address */
pub const E2BIG: c_int = 7; /* Argument list too long */
pub const ENOEXEC: c_int = 8; /* Exec format error */
pub const EBADF: c_int = 9; /* Bad file number */
pub const ECHILD: c_int = 10; /* No child processes */
pub const EAGAIN: c_int = 11; /* Try again */
pub const ENOMEM: c_int = 12; /* Out of memory */
pub const EACCES: c_int = 13; /* Permission denied */
pub const EFAULT: c_int = 14; /* Bad address */
pub const ENOTBLK: c_int = 15; /* Block device required */
pub const EBUSY: c_int = 16; /* Device or resource busy */
pub const EEXIST: c_int = 17; /* File exists */
pub const EXDEV: c_int = 18; /* Cross-device link */
pub const ENODEV: c_int = 19; /* No such device */
pub const ENOTDIR: c_int = 20; /* Not a directory */
pub const EISDIR: c_int = 21; /* Is a directory */
pub const EINVAL: c_int = 22; /* Invalid argument */
pub const ENFILE: c_int = 23; /* File table overflow */
pub const EMFILE: c_int = 24; /* Too many open files */
pub const ENOTTY: c_int = 25; /* Not a typewriter */
pub const ETXTBSY: c_int = 26; /* Text file busy */
pub const EFBIG: c_int = 27; /* File too large */
pub const ENOSPC: c_int = 28; /* No space left on device */
pub const ESPIPE: c_int = 29; /* Illegal seek */
pub const EROFS: c_int = 30; /* Read-only file system */
pub const EMLINK: c_int = 31; /* Too many links */
pub const EPIPE: c_int = 32; /* Broken pipe */
pub const EDOM: c_int = 33; /* Math argument out of domain of func */
pub const ERANGE: c_int = 34; /* Math result not representable */
pub const EDEADLK: c_int = 35; /* Resource deadlock would occur */
pub const ENAMETOOLONG: c_int = 36; /* File name too long */
pub const ENOLCK: c_int = 37; /* No record locks available */
pub const ENOSYS: c_int = 38; /* Function not implemented */
pub const ENOTEMPTY: c_int = 39; /* Directory not empty */
pub const ELOOP: c_int = 40; /* Too many symbolic links encountered */
pub const EWOULDBLOCK: c_int = EAGAIN; /* Operation would block */
pub const ENOMSG: c_int = 42; /* No message of desired type */
pub const EIDRM: c_int = 43; /* Identifier removed */
pub const ECHRNG: c_int = 44; /* Channel number out of range */
pub const EL2NSYNC: c_int = 45; /* Level 2 not synchronized */
pub const EL3HLT: c_int = 46; /* Level 3 halted */
pub const EL3RST: c_int = 47; /* Level 3 reset */
pub const ELNRNG: c_int = 48; /* Link number out of range */
pub const EUNATCH: c_int = 49; /* Protocol driver not attached */
pub const ENOCSI: c_int = 50; /* No CSI structure available */
pub const EL2HLT: c_int = 51; /* Level 2 halted */
pub const EBADE: c_int = 52; /* Invalid exchange */
pub const EBADR: c_int = 53; /* Invalid request descriptor */
pub const EXFULL: c_int = 54; /* Exchange full */
pub const ENOANO: c_int = 55; /* No anode */
pub const EBADRQC: c_int = 56; /* Invalid request code */
pub const EBADSLT: c_int = 57; /* Invalid slot */
pub const EDEADLOCK: c_int = EDEADLK; /* Resource deadlock would occur */
pub const EBFONT: c_int = 59; /* Bad font file format */
pub const ENOSTR: c_int = 60; /* Device not a stream */
pub const ENODATA: c_int = 61; /* No data available */
pub const ETIME: c_int = 62; /* Timer expired */
pub const ENOSR: c_int = 63; /* Out of streams resources */
pub const ENONET: c_int = 64; /* Machine is not on the network */
pub const ENOPKG: c_int = 65; /* Package not installed */
pub const EREMOTE: c_int = 66; /* Object is remote */
pub const ENOLINK: c_int = 67; /* Link has been severed */
pub const EADV: c_int = 68; /* Advertise error */
pub const ESRMNT: c_int = 69; /* Srmount error */
pub const ECOMM: c_int = 70; /* Communication error on send */
pub const EPROTO: c_int = 71; /* Protocol error */
pub const EMULTIHOP: c_int = 72; /* Multihop attempted */
pub const EDOTDOT: c_int = 73; /* RFS specific error */
pub const EBADMSG: c_int = 74; /* Not a data message */
pub const EOVERFLOW: c_int = 75; /* Value too large for defined data type */
pub const ENOTUNIQ: c_int = 76; /* Name not unique on network */
pub const EBADFD: c_int = 77; /* File descriptor in bad state */
pub const EREMCHG: c_int = 78; /* Remote address changed */
pub const ELIBACC: c_int = 79; /* Can not access a needed shared library */
pub const ELIBBAD: c_int = 80; /* Accessing a corrupted shared library */
pub const ELIBSCN: c_int = 81; /* .lib section in a.out corrupted */
/* Attempting to link in too many shared libraries */
pub const ELIBMAX: c_int = 82;
pub const ELIBEXEC: c_int = 83; /* Cannot exec a shared library directly */
pub const EILSEQ: c_int = 84; /* Illegal byte sequence */
/* Interrupted system call should be restarted */
pub const ERESTART: c_int = 85;
pub const ESTRPIPE: c_int = 86; /* Streams pipe error */
pub const EUSERS: c_int = 87; /* Too many users */
pub const ENOTSOCK: c_int = 88; /* Socket operation on non-socket */
pub const EDESTADDRREQ: c_int = 89; /* Destination address required */
pub const EMSGSIZE: c_int = 90; /* Message too long */
pub const EPROTOTYPE: c_int = 91; /* Protocol wrong type for socket */
pub const ENOPROTOOPT: c_int = 92; /* Protocol not available */
pub const EPROTONOSUPPORT: c_int = 93; /* Protocol not supported */
pub const ESOCKTNOSUPPORT: c_int = 94; /* Socket type not supported */
/* Operation not supported on transport endpoint */
pub const EOPNOTSUPP: c_int = 95;
pub const ENOTSUP: c_int = EOPNOTSUPP;
pub const EPFNOSUPPORT: c_int = 96; /* Protocol family not supported */
/* Address family not supported by protocol */
pub const EAFNOSUPPORT: c_int = 97;
pub const EADDRINUSE: c_int = 98; /* Address already in use */
pub const EADDRNOTAVAIL: c_int = 99; /* Cannot assign requested address */
pub const ENETDOWN: c_int = 100; /* Network is down */
pub const ENETUNREACH: c_int = 101; /* Network is unreachable */
/* Network dropped connection because of reset */
pub const ENETRESET: c_int = 102;
pub const ECONNABORTED: c_int = 103; /* Software caused connection abort */
pub const ECONNRESET: c_int = 104; /* Connection reset by peer */
pub const ENOBUFS: c_int = 105; /* No buffer space available */
pub const EISCONN: c_int = 106; /* Transport endpoint is already connected */
pub const ENOTCONN: c_int = 107; /* Transport endpoint is not connected */
/* Cannot send after transport endpoint shutdown */
pub const ESHUTDOWN: c_int = 108;
pub const ETOOMANYREFS: c_int = 109; /* Too many references: cannot splice */
pub const ETIMEDOUT: c_int = 110; /* Connection timed out */
pub const ECONNREFUSED: c_int = 111; /* Connection refused */
pub const EHOSTDOWN: c_int = 112; /* Host is down */
pub const EHOSTUNREACH: c_int = 113; /* No route to host */
pub const EALREADY: c_int = 114; /* Operation already in progress */
pub const EINPROGRESS: c_int = 115; /* Operation now in progress */
pub const ESTALE: c_int = 116; /* Stale NFS file handle */
pub const EUCLEAN: c_int = 117; /* Structure needs cleaning */
pub const ENOTNAM: c_int = 118; /* Not a XENIX named type file */
pub const ENAVAIL: c_int = 119; /* No XENIX semaphores available */
pub const EISNAM: c_int = 120; /* Is a named type file */
pub const EREMOTEIO: c_int = 121; /* Remote I/O error */
pub const EDQUOT: c_int = 122; /* Quota exceeded */
pub const ENOMEDIUM: c_int = 123; /* No medium found */
pub const EMEDIUMTYPE: c_int = 124; /* Wrong medium type */
pub const ECANCELED: c_int = 125; /* Operation Canceled */
pub const ENOKEY: c_int = 126; /* Required key not available */
pub const EKEYEXPIRED: c_int = 127; /* Key has expired */
pub const EKEYREVOKED: c_int = 128; /* Key has been revoked */
pub const EKEYREJECTED: c_int = 129; /* Key was rejected by service */
pub const EOWNERDEAD: c_int = 130; /* Owner died */
pub const ENOTRECOVERABLE: c_int = 131; /* State not recoverable */
pub const ERFKILL: c_int = 132; /* Operation not possible due to RF-kill */
pub const EHWPOISON: c_int = 133; /* Memory page has hardware error */

// fcntl.h
#[allow(dead_code)] // libc exposes the full constant set; std does not use every one yet.
pub const FD_CLOEXEC: c_int = 1;
pub const O_RDONLY: c_int = 0;
pub const O_WRONLY: c_int = 0o1;
pub const O_RDWR: c_int = 0o2;
pub const O_ACCMODE: c_int = 0o3 | O_PATH;
pub const O_NONBLOCK: c_int = 0o4000;
pub const O_NDELAY: c_int = O_NONBLOCK;
pub const O_APPEND: c_int = 0o2000;
pub const O_SHLOCK: c_int = 0x20000000;
pub const O_EXLOCK: c_int = 0x10000000;
pub const O_ASYNC: c_int = 0o20000;
pub const O_CLOEXEC: c_int = 0o2000000;
pub const O_CREAT: c_int = 0o100;
pub const O_TRUNC: c_int = 0o1000;
pub const O_EXCL: c_int = 0o200;
pub const O_DIRECTORY: c_int = 0o200000;
pub const O_PATH: c_int = 0o10000000;
pub const O_NOFOLLOW: c_int = 0o400000;
pub const O_NOCTTY: c_int = 0o400;
pub const O_DIRECT: c_int = 0o40000;
pub const O_DSYNC: c_int = 0o10000;
pub const O_LARGEFILE: c_int = 0;
pub const O_NOATIME: c_int = 0o1000000;
pub const O_RSYNC: c_int = O_SYNC;
pub const O_SYNC: c_int = 0o4010000;
pub const O_TMPFILE: c_int = 0o20200000;
pub const O_EXEC: c_int = O_PATH;
pub const O_SEARCH: c_int = O_PATH;
pub const O_RESOLVE_BENEATH: c_int = 0x40000000;
pub const O_TTY_INIT: c_int = 0;

pub const O_FSYNC: c_int = O_SYNC;

// locale.h
pub const LC_CTYPE: c_int = 0;
pub const LC_NUMERIC: c_int = 1;
pub const LC_TIME: c_int = 2;
pub const LC_COLLATE: c_int = 3;
pub const LC_MONETARY: c_int = 4;
pub const LC_MESSAGES: c_int = 5;
pub const LC_ALL: c_int = 6;

// netdb.h
pub const AI_PASSIVE: c_int = 0x0001;
pub const AI_CANONNAME: c_int = 0x0002;
pub const AI_NUMERICHOST: c_int = 0x0004;
pub const AI_V4MAPPED: c_int = 0x0008;
pub const AI_ALL: c_int = 0x0010;
pub const AI_ADDRCONFIG: c_int = 0x0020;
pub const AI_NUMERICSERV: c_int = 0x0400;
pub const EAI_BADFLAGS: c_int = -1;
pub const EAI_NONAME: c_int = -2;
pub const EAI_AGAIN: c_int = -3;
pub const EAI_FAIL: c_int = -4;
pub const EAI_NODATA: c_int = -5;
pub const EAI_FAMILY: c_int = -6;
pub const EAI_SOCKTYPE: c_int = -7;
pub const EAI_SERVICE: c_int = -8;
pub const EAI_ADDRFAMILY: c_int = -9;
pub const EAI_MEMORY: c_int = -10;
pub const EAI_SYSTEM: c_int = -11;
pub const EAI_OVERFLOW: c_int = -12;
pub const NI_MAXHOST: c_int = 1025;
pub const NI_MAXSERV: c_int = 32;
pub const NI_NUMERICHOST: c_int = 0x0001;
pub const NI_NUMERICSERV: c_int = 0x0002;
pub const NI_NOFQDN: c_int = 0x0004;
pub const NI_NAMEREQD: c_int = 0x0008;
pub const NI_DGRAM: c_int = 0x0010;

// netinet/in.h
// FIXME(redox): relibc {
pub const IP_TTL: c_int = 2;
pub const IPV6_UNICAST_HOPS: c_int = 16;
pub const IPV6_MULTICAST_IF: c_int = 17;
pub const IPV6_MULTICAST_HOPS: c_int = 18;
pub const IPV6_MULTICAST_LOOP: c_int = 19;
pub const IPV6_ADD_MEMBERSHIP: c_int = 20;
pub const IPV6_JOIN_GROUP: c_int = 20;
pub const IPV6_DROP_MEMBERSHIP: c_int = 21;
pub const IPV6_LEAVE_GROUP: c_int = 21;
pub const IPV6_V6ONLY: c_int = 26;
pub const IP_MULTICAST_IF: c_int = 32;
pub const IP_MULTICAST_TTL: c_int = 33;
pub const IP_MULTICAST_LOOP: c_int = 34;
pub const IP_ADD_MEMBERSHIP: c_int = 35;
pub const IP_DROP_MEMBERSHIP: c_int = 36;
pub const IP_TOS: c_int = 1;
pub const IP_RECVTOS: c_int = 13;
#[allow(dead_code)] // libc exposes the full constant set; std does not use every one yet.
pub const IPPROTO_IP: c_int = 0;
#[allow(dead_code)] // libc exposes the full constant set; std does not use every one yet.
pub const IPPROTO_ICMP: c_int = 1;
pub const IPPROTO_IGMP: c_int = 2;
#[allow(dead_code)] // libc exposes the full constant set; std does not use every one yet.
pub const IPPROTO_TCP: c_int = 6;
#[allow(dead_code)] // libc exposes the full constant set; std does not use every one yet.
pub const IPPROTO_UDP: c_int = 17;
#[allow(dead_code)] // libc exposes the full constant set; std does not use every one yet.
pub const IPPROTO_IPV6: c_int = 41;
#[allow(dead_code)] // libc exposes the full constant set; std does not use every one yet.
pub const IPPROTO_ICMPV6: c_int = 58;
pub const IPPROTO_PUP: c_int = 12;
pub const IPPROTO_IDP: c_int = 22;
pub const IPPROTO_RAW: c_int = 255;
pub const IPPROTO_MAX: c_int = 263;
// }

// netinet/tcp.h
pub const TCP_NODELAY: c_int = 1;
pub const TCP_MAXSEG: c_int = 2;
pub const TCP_KEEPIDLE: c_int = 4;
pub const TCP_KEEPINTVL: c_int = 5;
pub const TCP_KEEPCNT: c_int = 6;

// poll.h
pub const POLLIN: c_short = 0x001;
pub const POLLPRI: c_short = 0x002;
pub const POLLOUT: c_short = 0x004;
pub const POLLERR: c_short = 0x008;
pub const POLLHUP: c_short = 0x010;
pub const POLLNVAL: c_short = 0x020;
pub const POLLRDNORM: c_short = 0x040;
pub const POLLRDBAND: c_short = 0x080;
pub const POLLWRNORM: c_short = 0x100;
pub const POLLWRBAND: c_short = 0x200;

// pthread.h
pub const PTHREAD_MUTEX_DEFAULT: c_int = 0;
pub const PTHREAD_MUTEX_NORMAL: c_int = 0;
pub const PTHREAD_MUTEX_RECURSIVE: c_int = 1;
pub const PTHREAD_MUTEX_ERRORCHECK: c_int = 2;

pub const PTHREAD_MUTEX_STALLED: c_int = 0;
pub const PTHREAD_MUTEX_ROBUST: c_int = 1;

pub const PTHREAD_MUTEX_INITIALIZER: crate::pthread_mutex_t = crate::pthread_mutex_t {
    bytes: [0; _PTHREAD_MUTEX_SIZE],
};
pub const PTHREAD_COND_INITIALIZER: crate::pthread_cond_t = crate::pthread_cond_t {
    bytes: [0; _PTHREAD_COND_SIZE],
};
pub const PTHREAD_RWLOCK_INITIALIZER: crate::pthread_rwlock_t = crate::pthread_rwlock_t {
    bytes: [0; _PTHREAD_RWLOCK_SIZE],
};
pub const PTHREAD_STACK_MIN: size_t = 16384;

// signal.h
pub const SIG_BLOCK: c_int = 0;
pub const SIG_UNBLOCK: c_int = 1;
pub const SIG_SETMASK: c_int = 2;
pub const SIGHUP: c_int = 1;
pub const SIGINT: c_int = 2;
pub const SIGQUIT: c_int = 3;
pub const SIGILL: c_int = 4;
pub const SIGTRAP: c_int = 5;
pub const SIGABRT: c_int = 6;
pub const SIGBUS: c_int = 7;
pub const SIGFPE: c_int = 8;
pub const SIGKILL: c_int = 9;
pub const SIGUSR1: c_int = 10;
pub const SIGSEGV: c_int = 11;
pub const SIGUSR2: c_int = 12;
pub const SIGPIPE: c_int = 13;
pub const SIGALRM: c_int = 14;
pub const SIGTERM: c_int = 15;
pub const SIGSTKFLT: c_int = 16;
pub const SIGCHLD: c_int = 17;
pub const SIGCONT: c_int = 18;
pub const SIGSTOP: c_int = 19;
pub const SIGTSTP: c_int = 20;
pub const SIGTTIN: c_int = 21;
pub const SIGTTOU: c_int = 22;
pub const SIGURG: c_int = 23;
pub const SIGXCPU: c_int = 24;
pub const SIGXFSZ: c_int = 25;
pub const SIGVTALRM: c_int = 26;
pub const SIGPROF: c_int = 27;
pub const SIGWINCH: c_int = 28;
pub const SIGIO: c_int = 29;
pub const SIGPWR: c_int = 30;
pub const SIGSYS: c_int = 31;
pub const NSIG: c_int = 65;

pub const SA_NOCLDSTOP: c_int = 1;
pub const SA_NOCLDWAIT: c_int = 2;
pub const SA_SIGINFO: c_int = 4;
pub const SA_RESTORER: c_int = 0x0400_0000;
pub const SA_ONSTACK: c_int = 0x0800_0000;
pub const SA_RESTART: c_int = 0x1000_0000;
pub const SA_NODEFER: c_int = 0x4000_0000;
pub const SA_RESETHAND: c_int = 0x8000_0000;
pub const SA_NOMASK: c_int = SA_NODEFER;
pub const SA_ONESHOT: c_int = SA_RESETHAND;

// sys/file.h
pub const LOCK_SH: c_int = 1;
pub const LOCK_EX: c_int = 2;
pub const LOCK_NB: c_int = 4;
pub const LOCK_UN: c_int = 8;

// sys/epoll.h
pub const EPOLL_CLOEXEC: c_int = crate::O_CLOEXEC;
pub const EPOLL_CTL_ADD: c_int = 1;
pub const EPOLL_CTL_DEL: c_int = 2;
pub const EPOLL_CTL_MOD: c_int = 3;
pub const EPOLLIN: c_int = 0x001;
pub const EPOLLPRI: c_int = 0x002;
pub const EPOLLOUT: c_int = 0x004;
pub const EPOLLERR: c_int = 0x008;
pub const EPOLLHUP: c_int = 0x010;
pub const EPOLLNVAL: c_int = 0x020;
pub const EPOLLRDNORM: c_int = 0x040;
pub const EPOLLRDBAND: c_int = 0x080;
pub const EPOLLWRNORM: c_int = 0x100;
pub const EPOLLWRBAND: c_int = 0x200;
pub const EPOLLMSG: c_int = 0x400;
pub const EPOLLRDHUP: c_int = 0x2000;
pub const EPOLLEXCLUSIVE: c_int = 1 << 28;
pub const EPOLLWAKEUP: c_int = 1 << 29;
pub const EPOLLONESHOT: c_int = 1 << 30;
pub const EPOLLET: c_int = 1 << 31;

// sys/stat.h
pub const S_IFMT: crate::mode_t = 0o17_0000;
pub const S_IFDIR: crate::mode_t = 0o4_0000;
pub const S_IFCHR: crate::mode_t = 0o2_0000;
pub const S_IFBLK: crate::mode_t = 0o6_0000;
pub const S_IFREG: crate::mode_t = 0o10_0000;
pub const S_IFIFO: crate::mode_t = 0o1_0000;
pub const S_IFLNK: crate::mode_t = 0o12_0000;
pub const S_IFSOCK: crate::mode_t = 0o14_0000;
pub const S_IRWXU: crate::mode_t = 0o0700;
pub const S_IRUSR: crate::mode_t = 0o0400;
pub const S_IWUSR: crate::mode_t = 0o0200;
pub const S_IXUSR: crate::mode_t = 0o0100;
pub const S_IRWXG: crate::mode_t = 0o0070;
pub const S_IRGRP: crate::mode_t = 0o0040;
pub const S_IWGRP: crate::mode_t = 0o0020;
pub const S_IXGRP: crate::mode_t = 0o0010;
pub const S_IRWXO: crate::mode_t = 0o0007;
pub const S_IROTH: crate::mode_t = 0o0004;
pub const S_IWOTH: crate::mode_t = 0o0002;
pub const S_IXOTH: crate::mode_t = 0o0001;

pub const ST_RDONLY: c_ulong = 1;
pub const ST_NOSUID: c_ulong = 2;

// stdlib.h
pub const EXIT_SUCCESS: c_int = 0;
pub const EXIT_FAILURE: c_int = 1;

// sys/ioctl.h
// FIXME(redox): relibc {
pub const FIONREAD: c_ulong = 0x541B;
pub const FIONBIO: c_ulong = 0x5421;
pub const FIOCLEX: c_ulong = 0x5451;
// }
pub const TCGETS: c_ulong = 0x5401;
pub const TCSETS: c_ulong = 0x5402;
pub const TCFLSH: c_ulong = 0x540B;
pub const TIOCSCTTY: c_ulong = 0x540E;
pub const TIOCGPGRP: c_ulong = 0x540F;
pub const TIOCSPGRP: c_ulong = 0x5410;
pub const TIOCGWINSZ: c_ulong = 0x5413;
pub const TIOCSWINSZ: c_ulong = 0x5414;
pub const TIOCEXCL: c_ulong = 0x540C;
pub const TIOCNXCL: c_ulong = 0x540D;
pub const TIOCNOTTY: c_ulong = 0x5422;
pub const TCSBRK: c_ulong = 0x5409;
pub const TCXONC: c_ulong = 0x540A;

// sys/mman.h
pub const PROT_NONE: c_int = 0x0000;
pub const PROT_READ: c_int = 0x0001;
pub const PROT_WRITE: c_int = 0x0002;
pub const PROT_EXEC: c_int = 0x0004;

pub const MADV_NORMAL: c_int = 0;
pub const MADV_RANDOM: c_int = 1;
pub const MADV_SEQUENTIAL: c_int = 2;
pub const MADV_WILLNEED: c_int = 3;
pub const MADV_DONTNEED: c_int = 4;

pub const MAP_SHARED: c_int = 0x0001;
pub const MAP_PRIVATE: c_int = 0x0002;
pub const MAP_ANON: c_int = 0x0020;
pub const MAP_ANONYMOUS: c_int = MAP_ANON;
pub const MAP_FIXED: c_int = 0x0010;
pub const MAP_FAILED: *mut c_void = !0 as _;

pub const MS_ASYNC: c_int = 0x0001;
pub const MS_INVALIDATE: c_int = 0x0002;
pub const MS_SYNC: c_int = 0x0004;

// sys/resource.h
pub const RLIM_INFINITY: rlim_t = !0;
pub const RLIM_SAVED_CUR: rlim_t = RLIM_INFINITY;
pub const RLIM_SAVED_MAX: rlim_t = RLIM_INFINITY;
pub const RLIMIT_CPU: c_int = 0;
pub const RLIMIT_FSIZE: c_int = 1;
pub const RLIMIT_DATA: c_int = 2;
pub const RLIMIT_STACK: c_int = 3;
pub const RLIMIT_CORE: c_int = 4;
pub const RLIMIT_RSS: c_int = 5;
pub const RLIMIT_NPROC: c_int = 6;
pub const RLIMIT_NOFILE: c_int = 7;
pub const RLIMIT_MEMLOCK: c_int = 8;
pub const RLIMIT_AS: c_int = 9;
pub const RLIMIT_LOCKS: c_int = 10;
pub const RLIMIT_SIGPENDING: c_int = 11;
pub const RLIMIT_MSGQUEUE: c_int = 12;
pub const RLIMIT_NICE: c_int = 13;
pub const RLIMIT_RTPRIO: c_int = 14;
pub const RLIMIT_RTTIME: c_int = 15;
pub const RLIMIT_NLIMITS: c_int = 16;
pub const RLIM_NLIMITS: c_int = RLIMIT_NLIMITS;

pub const RUSAGE_SELF: c_int = 0;
pub const RUSAGE_CHILDREN: c_int = -1;
pub const RUSAGE_BOTH: c_int = -2;
pub const RUSAGE_THREAD: c_int = 1;

// sys/select.h
pub const FD_SETSIZE: usize = 1024;

// sys/socket.h
pub const AF_INET: c_int = 2;
pub const AF_INET6: c_int = 10;
pub const AF_UNIX: c_int = 1;
pub const AF_UNSPEC: c_int = 0;
pub const PF_INET: c_int = 2;
pub const PF_INET6: c_int = 10;
pub const PF_UNIX: c_int = 1;
pub const PF_UNSPEC: c_int = 0;
pub const MSG_CTRUNC: c_int = 8;
pub const MSG_DONTROUTE: c_int = 4;
pub const MSG_EOR: c_int = 128;
pub const MSG_OOB: c_int = 1;
pub const MSG_PEEK: c_int = 2;
pub const MSG_TRUNC: c_int = 32;
pub const MSG_DONTWAIT: c_int = 64;
pub const MSG_WAITALL: c_int = 256;
pub const SCM_RIGHTS: c_int = 1;
pub const SHUT_RD: c_int = 0;
pub const SHUT_WR: c_int = 1;
pub const SHUT_RDWR: c_int = 2;
pub const SO_DEBUG: c_int = 1;
pub const SO_REUSEADDR: c_int = 2;
pub const SO_TYPE: c_int = 3;
pub const SO_ERROR: c_int = 4;
pub const SO_DONTROUTE: c_int = 5;
pub const SO_BROADCAST: c_int = 6;
pub const SO_SNDBUF: c_int = 7;
pub const SO_RCVBUF: c_int = 8;
pub const SO_KEEPALIVE: c_int = 9;
pub const SO_OOBINLINE: c_int = 10;
pub const SO_NO_CHECK: c_int = 11;
pub const SO_PRIORITY: c_int = 12;
pub const SO_LINGER: c_int = 13;
pub const SO_BSDCOMPAT: c_int = 14;
pub const SO_REUSEPORT: c_int = 15;
pub const SO_PASSCRED: c_int = 16;
pub const SO_PEERCRED: c_int = 17;
pub const SO_RCVLOWAT: c_int = 18;
pub const SO_SNDLOWAT: c_int = 19;
pub const SO_RCVTIMEO: c_int = 20;
pub const SO_SNDTIMEO: c_int = 21;
pub const SO_ACCEPTCONN: c_int = 30;
pub const SO_PEERSEC: c_int = 31;
pub const SO_SNDBUFFORCE: c_int = 32;
pub const SO_RCVBUFFORCE: c_int = 33;
pub const SO_PROTOCOL: c_int = 38;
pub const SO_DOMAIN: c_int = 39;
pub const SOCK_STREAM: c_int = 1;
pub const SOCK_DGRAM: c_int = 2;
pub const SOCK_RAW: c_int = 3;
pub const SOCK_NONBLOCK: c_int = 0o4_000;
pub const SOCK_CLOEXEC: c_int = 0o2_000_000;
pub const SOCK_SEQPACKET: c_int = 5;
pub const SOL_SOCKET: c_int = 1;
pub const SOMAXCONN: c_int = 4096;

// sys/termios.h
pub const VEOF: usize = 4;
pub const VEOL: usize = 11;
pub const VEOL2: usize = 16;
pub const VERASE: usize = 2;
pub const VWERASE: usize = 14;
pub const VKILL: usize = 3;
pub const VREPRINT: usize = 12;
pub const VSWTC: usize = 7;
pub const VINTR: usize = 0;
pub const VQUIT: usize = 1;
pub const VSUSP: usize = 10;
pub const VSTART: usize = 8;
pub const VSTOP: usize = 9;
pub const VLNEXT: usize = 15;
pub const VDISCARD: usize = 13;
pub const VMIN: usize = 6;
pub const VTIME: usize = 5;
pub const NCCS: usize = 32;

pub const IGNBRK: crate::tcflag_t = 0o000_001;
pub const BRKINT: crate::tcflag_t = 0o000_002;
pub const IGNPAR: crate::tcflag_t = 0o000_004;
pub const PARMRK: crate::tcflag_t = 0o000_010;
pub const INPCK: crate::tcflag_t = 0o000_020;
pub const ISTRIP: crate::tcflag_t = 0o000_040;
pub const INLCR: crate::tcflag_t = 0o000_100;
pub const IGNCR: crate::tcflag_t = 0o000_200;
pub const ICRNL: crate::tcflag_t = 0o000_400;
pub const IUCLC: crate::tcflag_t = 0o001_000;
pub const IXON: crate::tcflag_t = 0o002_000;
pub const IXANY: crate::tcflag_t = 0o004_000;
pub const IXOFF: crate::tcflag_t = 0o010_000;
pub const IMAXBEL: crate::tcflag_t = 0o020_000;
pub const IUTF8: crate::tcflag_t = 0o040_000;

pub const OPOST: crate::tcflag_t = 0o000_001;
pub const OLCUC: crate::tcflag_t = 0o000_002;
pub const ONLCR: crate::tcflag_t = 0o000_004;
pub const OCRNL: crate::tcflag_t = 0o000_010;
pub const ONOCR: crate::tcflag_t = 0o000_020;
pub const ONLRET: crate::tcflag_t = 0o000_040;
pub const OFILL: crate::tcflag_t = 0o000_100;
pub const OFDEL: crate::tcflag_t = 0o000_200;
pub const NLDLY: crate::tcflag_t = 0o000_400;
pub const NL0: crate::tcflag_t = 0o000_000;
pub const NL1: crate::tcflag_t = 0o000_400;
pub const CRDLY: crate::tcflag_t = 0o003_000;
pub const CR0: crate::tcflag_t = 0o000_000;
pub const CR1: crate::tcflag_t = 0o001_000;
pub const CR2: crate::tcflag_t = 0o002_000;
pub const CR3: crate::tcflag_t = 0o003_000;
pub const TABDLY: crate::tcflag_t = 0o014_000;
pub const TAB0: crate::tcflag_t = 0o000_000;
pub const TAB1: crate::tcflag_t = 0o004_000;
pub const TAB2: crate::tcflag_t = 0o010_000;
pub const TAB3: crate::tcflag_t = 0o014_000;
pub const BSDLY: crate::tcflag_t = 0o020_000;
pub const BS0: crate::tcflag_t = 0o000_000;
pub const BS1: crate::tcflag_t = 0o020_000;
pub const FFDLY: crate::tcflag_t = 0o100_000;
pub const FF0: crate::tcflag_t = 0o000_000;
pub const FF1: crate::tcflag_t = 0o100_000;
pub const VTDLY: crate::tcflag_t = 0o040_000;
pub const VT0: crate::tcflag_t = 0o000_000;
pub const VT1: crate::tcflag_t = 0o040_000;
pub const XTABS: crate::tcflag_t = 0o014_000;

pub const B0: speed_t = 0o000_000;
pub const B50: speed_t = 0o000_001;
pub const B75: speed_t = 0o000_002;
pub const B110: speed_t = 0o000_003;
pub const B134: speed_t = 0o000_004;
pub const B150: speed_t = 0o000_005;
pub const B200: speed_t = 0o000_006;
pub const B300: speed_t = 0o000_007;
pub const B600: speed_t = 0o000_010;
pub const B1200: speed_t = 0o000_011;
pub const B1800: speed_t = 0o000_012;
pub const B2400: speed_t = 0o000_013;
pub const B4800: speed_t = 0o000_014;
pub const B9600: speed_t = 0o000_015;
pub const B19200: speed_t = 0o000_016;
pub const B38400: speed_t = 0o000_017;
pub const B57600: speed_t = 0o010_001;
pub const B115200: speed_t = 0o010_002;
pub const B230400: speed_t = 0o010_003;
pub const B460800: speed_t = 0o010_004;
pub const B500000: speed_t = 0o010_005;
pub const B576000: speed_t = 0o010_006;
pub const B921600: speed_t = 0o010_007;
pub const B1000000: speed_t = 0o010_010;
pub const B1152000: speed_t = 0o010_011;
pub const B1500000: speed_t = 0o010_012;
pub const B2000000: speed_t = 0o010_013;
pub const B2500000: speed_t = 0o010_014;
pub const B3000000: speed_t = 0o010_015;
pub const B3500000: speed_t = 0o010_016;
pub const B4000000: speed_t = 0o010_017;

pub const CSIZE: crate::tcflag_t = 0o000_060;
pub const CS5: crate::tcflag_t = 0o000_000;
pub const CS6: crate::tcflag_t = 0o000_020;
pub const CS7: crate::tcflag_t = 0o000_040;
pub const CS8: crate::tcflag_t = 0o000_060;

pub const CSTOPB: crate::tcflag_t = 0o000_100;
pub const CREAD: crate::tcflag_t = 0o000_200;
pub const PARENB: crate::tcflag_t = 0o000_400;
pub const PARODD: crate::tcflag_t = 0o001_000;
pub const HUPCL: crate::tcflag_t = 0o002_000;
pub const CLOCAL: crate::tcflag_t = 0o004_000;
pub const CBAUD: crate::tcflag_t = 0o010_017;
pub const EXTA: speed_t = B19200;
pub const EXTB: speed_t = B38400;
pub const CBAUDEX: crate::tcflag_t = 0o010_000;
pub const CIBAUD: crate::tcflag_t = 0o020_036_000_00;
pub const CMSPAR: crate::tcflag_t = 0o100_000_000_00;
pub const CRTSCTS: crate::tcflag_t = 0o200_000_000_00;

pub const ISIG: crate::tcflag_t = 0o000_001;
pub const ICANON: crate::tcflag_t = 0o000_002;
pub const ECHO: crate::tcflag_t = 0o000_010;
pub const ECHOE: crate::tcflag_t = 0o000_020;
pub const ECHOK: crate::tcflag_t = 0o000_040;
pub const ECHONL: crate::tcflag_t = 0o000_100;
pub const NOFLSH: crate::tcflag_t = 0o000_200;
pub const TOSTOP: crate::tcflag_t = 0o000_400;
pub const IEXTEN: crate::tcflag_t = 0o100_000;
pub const XCASE: crate::tcflag_t = 0o000_004;
pub const ECHOCTL: crate::tcflag_t = 0o001_000;
pub const ECHOPRT: crate::tcflag_t = 0o002_000;
pub const ECHOKE: crate::tcflag_t = 0o004_000;
pub const FLUSHO: crate::tcflag_t = 0o010_000;
pub const PENDIN: crate::tcflag_t = 0o040_000;
pub const EXTPROC: crate::tcflag_t = 0o200_000;

pub const TCOOFF: c_int = 0;
pub const TCOON: c_int = 1;
pub const TCIOFF: c_int = 2;
pub const TCION: c_int = 3;
pub const TCIFLUSH: c_int = 0;
pub const TCOFLUSH: c_int = 1;
pub const TCIOFLUSH: c_int = 2;
pub const TCSANOW: c_int = 0;
pub const TCSADRAIN: c_int = 1;
pub const TCSAFLUSH: c_int = 2;

pub const _POSIX_VDISABLE: crate::cc_t = 0;


// signal.h sigevent values
pub const SIGEV_SIGNAL: c_int = 0;
pub const SIGEV_NONE: c_int = 1;
pub const SIGEV_THREAD: c_int = 2;
pub const SIGEV_THREAD_ID: c_int = 4;

// sys/wait.h
pub type idtype_t = c_int;
pub const P_ALL: idtype_t = 0;
pub const P_PID: idtype_t = 1;
pub const P_PGID: idtype_t = 2;
pub const WNOHANG: c_int = 1;
pub const WUNTRACED: c_int = 2;

pub const WSTOPPED: c_int = 2;
pub const WEXITED: c_int = 4;
pub const WCONTINUED: c_int = 8;
pub const WNOWAIT: c_int = 0x0100_0000;

pub const CLD_EXITED: c_int = 1;
pub const CLD_KILLED: c_int = 2;
pub const CLD_DUMPED: c_int = 3;
pub const CLD_TRAPPED: c_int = 4;
pub const CLD_STOPPED: c_int = 5;
pub const CLD_CONTINUED: c_int = 6;

pub const __WNOTHREAD: c_int = 0x2000_0000;
pub const __WALL: c_int = 0x4000_0000;
#[allow(overflowing_literals)]
pub const __WCLONE: c_int = 0x8000_0000;

// time.h
pub const UTIME_NOW: c_long = 0x3fffffff;
pub const UTIME_OMIT: c_long = 0x3ffffffe;
pub const PIPE_BUF: usize = 4096;
pub const CLOCK_REALTIME: c_int = 0;
pub const CLOCK_MONOTONIC: c_int = 1;
pub const CLOCK_PROCESS_CPUTIME_ID: crate::clockid_t = 2;
pub const CLOCK_THREAD_CPUTIME_ID: crate::clockid_t = 3;
pub const CLOCK_MONOTONIC_RAW: crate::clockid_t = 4;
pub const CLOCK_REALTIME_COARSE: crate::clockid_t = 5;
pub const CLOCK_MONOTONIC_COARSE: crate::clockid_t = 6;
pub const CLOCK_BOOTTIME: crate::clockid_t = 7;
pub const CLOCK_REALTIME_ALARM: crate::clockid_t = 8;
pub const CLOCK_BOOTTIME_ALARM: crate::clockid_t = 9;
pub const CLOCKS_PER_SEC: crate::clock_t = 1_000_000;

// unistd.h
// POSIX.1 {
pub const _SC_ARG_MAX: c_int = 0;
pub const _SC_CHILD_MAX: c_int = 1;
pub const _SC_CLK_TCK: c_int = 2;
pub const _SC_NGROUPS_MAX: c_int = 3;
pub const _SC_OPEN_MAX: c_int = 4;
pub const _SC_STREAM_MAX: c_int = 5;
pub const _SC_TZNAME_MAX: c_int = 6;
// ...
pub const _SC_VERSION: c_int = 29;
pub const _SC_PAGESIZE: c_int = 30;
pub const _SC_PAGE_SIZE: c_int = 30;
// ...
pub const _SC_RE_DUP_MAX: c_int = 44;

pub const _SC_NPROCESSORS_CONF: c_int = 83;
pub const _SC_NPROCESSORS_ONLN: c_int = 84;

// ...
pub const _SC_GETGR_R_SIZE_MAX: c_int = 69;
pub const _SC_GETPW_R_SIZE_MAX: c_int = 70;
pub const _SC_LOGIN_NAME_MAX: c_int = 71;
pub const _SC_TTY_NAME_MAX: c_int = 72;
// ...
pub const _SC_SYMLOOP_MAX: c_int = 173;
// ...
pub const _SC_HOST_NAME_MAX: c_int = 180;


// langinfo.h
pub const ABMON_1: c_int = 0x2000E;
pub const ABMON_2: c_int = 0x2000F;
pub const ABMON_3: c_int = 0x20010;
pub const ABMON_4: c_int = 0x20011;
pub const ABMON_5: c_int = 0x20012;
pub const ABMON_6: c_int = 0x20013;
pub const ABMON_7: c_int = 0x20014;
pub const ABMON_8: c_int = 0x20015;
pub const ABMON_9: c_int = 0x20016;
pub const ABMON_10: c_int = 0x20017;
pub const ABMON_11: c_int = 0x20018;
pub const ABMON_12: c_int = 0x20019;
pub const AM_STR: c_int = 0x20026;
pub const PM_STR: c_int = 0x20027;
pub const D_T_FMT: c_int = 0x20028;
pub const D_FMT: c_int = 0x20029;
pub const T_FMT: c_int = 0x2002A;
pub const T_FMT_AMPM: c_int = 0x2002B;

// sysconf.h
pub const _SC_2_CHAR_TERM: c_int = 95;
pub const _SC_2_C_BIND: c_int = 47;
pub const _SC_2_C_DEV: c_int = 48;
pub const _SC_2_FORT_DEV: c_int = 49;
pub const _SC_2_FORT_RUN: c_int = 50;
pub const _SC_2_LOCALEDEF: c_int = 52;
pub const _SC_2_PBS: c_int = 168;
pub const _SC_2_PBS_ACCOUNTING: c_int = 169;
pub const _SC_2_PBS_CHECKPOINT: c_int = 175;
pub const _SC_2_PBS_LOCATE: c_int = 170;
pub const _SC_2_PBS_MESSAGE: c_int = 171;
pub const _SC_2_PBS_TRACK: c_int = 172;
pub const _SC_2_SW_DEV: c_int = 51;
pub const _SC_2_UPE: c_int = 97;
pub const _SC_2_VERSION: c_int = 46;
pub const _SC_ADVISORY_INFO: c_int = 132;
pub const _SC_AIO_LISTIO_MAX: c_int = 23;
pub const _SC_AIO_MAX: c_int = 24;
pub const _SC_AIO_PRIO_DELTA_MAX: c_int = 25;
pub const _SC_ASYNCHRONOUS_IO: c_int = 12;
pub const _SC_ATEXIT_MAX: c_int = 87;
pub const _SC_AVPHYS_PAGES: c_int = 86;
pub const _SC_BARRIERS: c_int = 133;
pub const _SC_BC_BASE_MAX: c_int = 36;
pub const _SC_BC_DIM_MAX: c_int = 37;
pub const _SC_BC_SCALE_MAX: c_int = 38;
pub const _SC_BC_STRING_MAX: c_int = 39;
pub const _SC_CLOCK_SELECTION: c_int = 137;
pub const _SC_COLL_WEIGHTS_MAX: c_int = 40;
pub const _SC_CPUTIME: c_int = 138;
pub const _SC_DELAYTIMER_MAX: c_int = 26;
pub const _SC_EXPR_NEST_MAX: c_int = 42;
pub const _SC_FSYNC: c_int = 15;
pub const _SC_IOV_MAX: c_int = 60;
pub const _SC_IPV6: c_int = 235;
pub const _SC_JOB_CONTROL: c_int = 7;
pub const _SC_LINE_MAX: c_int = 43;
pub const _SC_MAPPED_FILES: c_int = 16;
pub const _SC_MEMLOCK: c_int = 17;
pub const _SC_MEMLOCK_RANGE: c_int = 18;
pub const _SC_MEMORY_PROTECTION: c_int = 19;
pub const _SC_MESSAGE_PASSING: c_int = 20;
pub const _SC_MINSIGSTKSZ: c_int = 249;
pub const _SC_MONOTONIC_CLOCK: c_int = 149;
pub const _SC_MQ_OPEN_MAX: c_int = 27;
pub const _SC_MQ_PRIO_MAX: c_int = 28;
pub const _SC_NZERO: c_int = 109;
pub const _SC_PASS_MAX: c_int = 88;
pub const _SC_PHYS_PAGES: c_int = 85;
pub const _SC_PRIORITIZED_IO: c_int = 13;
pub const _SC_PRIORITY_SCHEDULING: c_int = 10;
pub const _SC_RAW_SOCKETS: c_int = 236;
pub const _SC_READER_WRITER_LOCKS: c_int = 153;
pub const _SC_REGEXP: c_int = 155;
pub const _SC_RTSIG_MAX: c_int = 31;
pub const _SC_SAVED_IDS: c_int = 8;
pub const _SC_SEMAPHORES: c_int = 21;
pub const _SC_SEM_NSEMS_MAX: c_int = 32;
pub const _SC_SEM_VALUE_MAX: c_int = 33;
pub const _SC_SHARED_MEMORY_OBJECTS: c_int = 22;
pub const _SC_SHELL: c_int = 157;
pub const _SC_SIGSTKSZ: c_int = 250;
pub const _SC_SPAWN: c_int = 159;
pub const _SC_SPIN_LOCKS: c_int = 154;
pub const _SC_SPORADIC_SERVER: c_int = 160;
pub const _SC_SS_REPL_MAX: c_int = 241;
pub const _SC_STREAMS: c_int = 174;
pub const _SC_SYNCHRONIZED_IO: c_int = 14;
pub const _SC_THREADS: c_int = 67;
pub const _SC_THREAD_ATTR_STACKADDR: c_int = 77;
pub const _SC_THREAD_ATTR_STACKSIZE: c_int = 78;
pub const _SC_THREAD_CPUTIME: c_int = 139;
pub const _SC_THREAD_DESTRUCTOR_ITERATIONS: c_int = 73;
pub const _SC_THREAD_KEYS_MAX: c_int = 74;
pub const _SC_THREAD_PRIORITY_SCHEDULING: c_int = 79;
pub const _SC_THREAD_PRIO_INHERIT: c_int = 80;
pub const _SC_THREAD_PRIO_PROTECT: c_int = 81;
pub const _SC_THREAD_PROCESS_SHARED: c_int = 82;
pub const _SC_THREAD_ROBUST_PRIO_INHERIT: c_int = 247;
pub const _SC_THREAD_ROBUST_PRIO_PROTECT: c_int = 248;
pub const _SC_THREAD_SAFE_FUNCTIONS: c_int = 68;
pub const _SC_THREAD_SPORADIC_SERVER: c_int = 161;
pub const _SC_THREAD_STACK_MIN: c_int = 75;
pub const _SC_THREAD_THREADS_MAX: c_int = 76;
pub const _SC_TIMEOUTS: c_int = 164;
pub const _SC_TIMERS: c_int = 11;
pub const _SC_TIMER_MAX: c_int = 35;
pub const _SC_TRACE: c_int = 181;
pub const _SC_TRACE_EVENT_FILTER: c_int = 182;
pub const _SC_TRACE_EVENT_NAME_MAX: c_int = 242;
pub const _SC_TRACE_INHERIT: c_int = 183;
pub const _SC_TRACE_LOG: c_int = 184;
pub const _SC_TRACE_NAME_MAX: c_int = 243;
pub const _SC_TRACE_SYS_MAX: c_int = 244;
pub const _SC_TRACE_USER_EVENT_MAX: c_int = 245;
pub const _SC_TYPED_MEMORY_OBJECTS: c_int = 165;
pub const _SC_UIO_MAXIOV: c_int = 60;
pub const _SC_V6_ILP32_OFF32: c_int = 176;
pub const _SC_V6_ILP32_OFFBIG: c_int = 177;
pub const _SC_V6_LP64_OFF64: c_int = 178;
pub const _SC_V6_LPBIG_OFFBIG: c_int = 179;
pub const _SC_V7_ILP32_OFF32: c_int = 237;
pub const _SC_V7_ILP32_OFFBIG: c_int = 238;
pub const _SC_V7_LP64_OFF64: c_int = 239;
pub const _SC_V7_LPBIG_OFFBIG: c_int = 240;
pub const _SC_XBS5_ILP32_OFF32: c_int = 125;
pub const _SC_XBS5_ILP32_OFFBIG: c_int = 126;
pub const _SC_XBS5_LP64_OFF64: c_int = 127;
pub const _SC_XBS5_LPBIG_OFFBIG: c_int = 128;
pub const _SC_XOPEN_CRYPT: c_int = 92;
pub const _SC_XOPEN_ENH_I18N: c_int = 93;
pub const _SC_XOPEN_LEGACY: c_int = 129;
pub const _SC_XOPEN_REALTIME: c_int = 130;
pub const _SC_XOPEN_REALTIME_THREADS: c_int = 131;
pub const _SC_XOPEN_SHM: c_int = 94;
pub const _SC_XOPEN_STREAMS: c_int = 246;
pub const _SC_XOPEN_UNIX: c_int = 91;
pub const _SC_XOPEN_VERSION: c_int = 89;
pub const _SC_XOPEN_XCU_VERSION: c_int = 90;
pub const _SC_XOPEN_XPG2: c_int = 98;
pub const _SC_XOPEN_XPG3: c_int = 99;
pub const _SC_XOPEN_XPG4: c_int = 100;
// ...
pub const _SC_SIGQUEUE_MAX: c_int = 34;
pub const _SC_REALTIME_SIGNALS: c_int = 9;
// } POSIX.1

// confstr
pub const _CS_PATH: c_int = 0;
pub const _CS_POSIX_V6_WIDTH_RESTRICTED_ENVS: c_int = 1;
pub const _CS_POSIX_V5_WIDTH_RESTRICTED_ENVS: c_int = 4;
pub const _CS_POSIX_V7_WIDTH_RESTRICTED_ENVS: c_int = 5;
pub const _CS_POSIX_V6_ILP32_OFF32_CFLAGS: c_int = 1116;
pub const _CS_POSIX_V6_ILP32_OFF32_LDFLAGS: c_int = 1117;
pub const _CS_POSIX_V6_ILP32_OFF32_LIBS: c_int = 1118;
pub const _CS_POSIX_V6_ILP32_OFF32_LINTFLAGS: c_int = 1119;
pub const _CS_POSIX_V6_ILP32_OFFBIG_CFLAGS: c_int = 1120;
pub const _CS_POSIX_V6_ILP32_OFFBIG_LDFLAGS: c_int = 1121;
pub const _CS_POSIX_V6_ILP32_OFFBIG_LIBS: c_int = 1122;
pub const _CS_POSIX_V6_ILP32_OFFBIG_LINTFLAGS: c_int = 1123;
pub const _CS_POSIX_V6_LP64_OFF64_CFLAGS: c_int = 1124;
pub const _CS_POSIX_V6_LP64_OFF64_LDFLAGS: c_int = 1125;
pub const _CS_POSIX_V6_LP64_OFF64_LIBS: c_int = 1126;
pub const _CS_POSIX_V6_LP64_OFF64_LINTFLAGS: c_int = 1127;
pub const _CS_POSIX_V6_LPBIG_OFFBIG_CFLAGS: c_int = 1128;
pub const _CS_POSIX_V6_LPBIG_OFFBIG_LDFLAGS: c_int = 1129;
pub const _CS_POSIX_V6_LPBIG_OFFBIG_LIBS: c_int = 1130;
pub const _CS_POSIX_V6_LPBIG_OFFBIG_LINTFLAGS: c_int = 1131;
pub const _CS_POSIX_V7_ILP32_OFF32_CFLAGS: c_int = 1132;
pub const _CS_POSIX_V7_ILP32_OFF32_LDFLAGS: c_int = 1133;
pub const _CS_POSIX_V7_ILP32_OFF32_LIBS: c_int = 1134;
pub const _CS_POSIX_V7_ILP32_OFF32_LINTFLAGS: c_int = 1135;
pub const _CS_POSIX_V7_ILP32_OFFBIG_CFLAGS: c_int = 1136;
pub const _CS_POSIX_V7_ILP32_OFFBIG_LDFLAGS: c_int = 1137;
pub const _CS_POSIX_V7_ILP32_OFFBIG_LIBS: c_int = 1138;
pub const _CS_POSIX_V7_ILP32_OFFBIG_LINTFLAGS: c_int = 1139;
pub const _CS_POSIX_V7_LP64_OFF64_CFLAGS: c_int = 1140;
pub const _CS_POSIX_V7_LP64_OFF64_LDFLAGS: c_int = 1141;
pub const _CS_POSIX_V7_LP64_OFF64_LIBS: c_int = 1142;
pub const _CS_POSIX_V7_LP64_OFF64_LINTFLAGS: c_int = 1143;
pub const _CS_POSIX_V7_LPBIG_OFFBIG_CFLAGS: c_int = 1144;
pub const _CS_POSIX_V7_LPBIG_OFFBIG_LDFLAGS: c_int = 1145;
pub const _CS_POSIX_V7_LPBIG_OFFBIG_LIBS: c_int = 1146;
pub const _CS_POSIX_V7_LPBIG_OFFBIG_LINTFLAGS: c_int = 1147;

pub const R_OK: c_int = 4;
pub const W_OK: c_int = 2;
pub const X_OK: c_int = 1;

// stdio.h
pub const BUFSIZ: c_uint = 1024;
pub const _IOFBF: c_int = 0;
pub const _IOLBF: c_int = 1;
pub const _IONBF: c_int = 2;
pub const SEEK_SET: c_int = 0;
pub const SEEK_CUR: c_int = 1;
pub const SEEK_END: c_int = 2;

pub const _PC_LINK_MAX: c_int = 0;
pub const _PC_MAX_CANON: c_int = 1;
pub const _PC_MAX_INPUT: c_int = 2;
pub const _PC_NAME_MAX: c_int = 3;
pub const _PC_PATH_MAX: c_int = 4;
pub const _PC_PIPE_BUF: c_int = 5;
pub const _PC_CHOWN_RESTRICTED: c_int = 6;
pub const _PC_NO_TRUNC: c_int = 7;
pub const _PC_VDISABLE: c_int = 8;
pub const _PC_SYNC_IO: c_int = 9;
pub const _PC_ASYNC_IO: c_int = 10;
pub const _PC_PRIO_IO: c_int = 11;
pub const _PC_SOCK_MAXBUF: c_int = 12;
pub const _PC_FILESIZEBITS: c_int = 13;
pub const _PC_REC_INCR_XFER_SIZE: c_int = 14;
pub const _PC_REC_MAX_XFER_SIZE: c_int = 15;
pub const _PC_REC_MIN_XFER_SIZE: c_int = 16;
pub const _PC_REC_XFER_ALIGN: c_int = 17;
pub const _PC_ALLOC_SIZE_MIN: c_int = 18;
pub const _PC_SYMLINK_MAX: c_int = 19;
pub const _PC_2_SYMLINKS: c_int = 20;

pub const PRIO_PROCESS: c_int = 0;
pub const PRIO_PGRP: c_int = 1;
pub const PRIO_USER: c_int = 2;

pub const RENAME_NOREPLACE: c_uint = 1;

f! {
    //sys/socket.h
    pub const fn CMSG_ALIGN(len: size_t) -> size_t {
        (len + size_of::<size_t>() - 1) & !(size_of::<size_t>() - 1)
    }
    pub const fn CMSG_LEN(length: c_uint) -> c_uint {
        (CMSG_ALIGN(size_of::<cmsghdr>()) + length as usize) as c_uint
    }
    pub const fn CMSG_SPACE(len: c_uint) -> c_uint {
        (CMSG_ALIGN(len as size_t) + CMSG_ALIGN(size_of::<cmsghdr>())) as c_uint
    }

    // wait.h
    pub fn FD_CLR(fd: c_int, set: *mut fd_set) -> () {
        let fd = fd as usize;
        let size = size_of_val(&(*set).fds_bits[0]) * 8;
        (*set).fds_bits[fd / size] &= !(1 << (fd % size));
        return;
    }

    pub fn FD_ISSET(fd: c_int, set: *const fd_set) -> bool {
        let fd = fd as usize;
        let size = size_of_val(&(*set).fds_bits[0]) * 8;
        return ((*set).fds_bits[fd / size] & (1 << (fd % size))) != 0;
    }

    pub fn FD_SET(fd: c_int, set: *mut fd_set) -> () {
        let fd = fd as usize;
        let size = size_of_val(&(*set).fds_bits[0]) * 8;
        (*set).fds_bits[fd / size] |= 1 << (fd % size);
        return;
    }

    pub fn FD_ZERO(set: *mut fd_set) -> () {
        for slot in (*set).fds_bits.iter_mut() {
            *slot = 0;
        }
    }
}

safe_f! {
    pub const fn WIFSTOPPED(status: c_int) -> bool {
        (status & 0xff) == 0x7f
    }

    pub const fn WSTOPSIG(status: c_int) -> c_int {
        (status >> 8) & 0xff
    }

    pub const fn WIFCONTINUED(status: c_int) -> bool {
        status == 0xffff
    }

    pub const fn WIFSIGNALED(status: c_int) -> bool {
        ((status & 0x7f) + 1) as i8 >= 2
    }

    pub const fn WTERMSIG(status: c_int) -> c_int {
        status & 0x7f
    }

    pub const fn WIFEXITED(status: c_int) -> bool {
        (status & 0x7f) == 0
    }

    pub const fn WEXITSTATUS(status: c_int) -> c_int {
        (status >> 8) & 0xff
    }

    pub const fn WCOREDUMP(status: c_int) -> bool {
        (status & 0x80) != 0
    }

    pub const fn makedev(major: c_uint, minor: c_uint) -> dev_t {
        let major = major as dev_t;
        let minor = minor as dev_t;
        let mut dev = 0;
        dev |= (major & 0x00000fff) << 8;
        dev |= (major & 0xfffff000) << 32;
        dev |= (minor & 0x000000ff) << 0;
        dev |= (minor & 0xffffff00) << 12;
        dev
    }

    pub const fn major(dev: dev_t) -> c_uint {
        let mut major = 0;
        major |= (dev & 0x00000000000fff00) >> 8;
        major |= (dev & 0xfffff00000000000) >> 32;
        major as c_uint
    }

    pub const fn minor(dev: dev_t) -> c_uint {
        let mut minor = 0;
        minor |= (dev & 0x00000000000000ff) >> 0;
        minor |= (dev & 0x00000ffffff00000) >> 12;
        minor as c_uint
    }
}

extern "C" {
    // errno.h
    pub fn __errno_location() -> *mut c_int;
    pub fn strerror_r(errnum: c_int, buf: *mut c_char, buflen: size_t) -> c_int;

    // langinfo.h
    pub fn nl_langinfo(item: nl_item) -> *mut c_char;

    // time.h timers
    pub fn timer_create(
        clockid: crate::clockid_t,
        sevp: *mut sigevent,
        timerid: *mut crate::timer_t,
    ) -> c_int;
    pub fn timer_delete(timerid: crate::timer_t) -> c_int;
    pub fn timer_settime(
        timerid: crate::timer_t,
        flags: c_int,
        new_value: *const itimerspec,
        old_value: *mut itimerspec,
    ) -> c_int;
    pub fn timer_gettime(timerid: crate::timer_t, curr_value: *mut itimerspec) -> c_int;
    pub fn timer_getoverrun(timerid: crate::timer_t) -> c_int;

    // signal.h
    pub fn sigsuspend(mask: *const sigset_t) -> c_int;

    // unistd.h
    pub fn daemon(nochdir: c_int, noclose: c_int) -> c_int;

    // grp.h
    pub fn setgroups(size: size_t, ptr: *const crate::gid_t) -> c_int;
    pub fn initgroups(user: *const c_char, group: crate::gid_t) -> c_int;

    // sys/stat.h
    pub fn mkfifoat(dirfd: c_int, pathname: *const c_char, mode: mode_t) -> c_int;

    // dirent.h
    pub fn dirfd(dirp: *mut crate::DIR) -> c_int;

    // unistd.h
    pub fn pipe2(fds: *mut c_int, flags: c_int) -> c_int;
    pub fn getdtablesize() -> c_int;
    pub fn getresgid(
        rgid: *mut crate::gid_t,
        egid: *mut crate::gid_t,
        sgid: *mut crate::gid_t,
    ) -> c_int;
    pub fn getresuid(
        ruid: *mut crate::uid_t,
        euid: *mut crate::uid_t,
        suid: *mut crate::uid_t,
    ) -> c_int;
    pub fn setresgid(rgid: crate::gid_t, egid: crate::gid_t, sgid: crate::gid_t) -> c_int;
    pub fn setresuid(ruid: crate::uid_t, euid: crate::uid_t, suid: crate::uid_t) -> c_int;

    // grp.h
    pub fn getgrent() -> *mut crate::group;
    pub fn setgrent();
    pub fn endgrent();
    pub fn getgrgid(gid: crate::gid_t) -> *mut crate::group;
    pub fn getgrgid_r(
        gid: crate::gid_t,
        grp: *mut crate::group,
        buf: *mut c_char,
        buflen: size_t,
        result: *mut *mut crate::group,
    ) -> c_int;
    pub fn getgrnam(name: *const c_char) -> *mut crate::group;
    pub fn getgrnam_r(
        name: *const c_char,
        grp: *mut crate::group,
        buf: *mut c_char,
        buflen: size_t,
        result: *mut *mut crate::group,
    ) -> c_int;
    pub fn getgrouplist(
        user: *const c_char,
        group: crate::gid_t,
        groups: *mut crate::gid_t,
        ngroups: *mut c_int,
    ) -> c_int;

    // malloc.h
    pub fn memalign(align: size_t, size: size_t) -> *mut c_void;

    // netdb.h
    pub fn getnameinfo(
        addr: *const crate::sockaddr,
        addrlen: crate::socklen_t,
        host: *mut c_char,
        hostlen: crate::socklen_t,
        serv: *mut c_char,
        servlen: crate::socklen_t,
        flags: c_int,
    ) -> c_int;

    // pthread.h
    pub fn pthread_atfork(
        prepare: Option<unsafe extern "C" fn()>,
        parent: Option<unsafe extern "C" fn()>,
        child: Option<unsafe extern "C" fn()>,
    ) -> c_int;
    pub fn pthread_create(
        tid: *mut crate::pthread_t,
        attr: *const crate::pthread_attr_t,
        start: extern "C" fn(*mut c_void) -> *mut c_void,
        arg: *mut c_void,
    ) -> c_int;
    pub fn pthread_condattr_setclock(
        attr: *mut pthread_condattr_t,
        clock_id: crate::clockid_t,
    ) -> c_int;

    //pty.h
    pub fn openpty(
        amaster: *mut c_int,
        aslave: *mut c_int,
        name: *mut c_char,
        termp: *const termios,
        winp: *const crate::winsize,
    ) -> c_int;

    // pwd.h
    pub fn getpwent() -> *mut passwd;
    pub fn setpwent();
    pub fn endpwent();
    pub fn getpwnam_r(
        name: *const c_char,
        pwd: *mut passwd,
        buf: *mut c_char,
        buflen: size_t,
        result: *mut *mut passwd,
    ) -> c_int;
    pub fn getpwuid_r(
        uid: crate::uid_t,
        pwd: *mut passwd,
        buf: *mut c_char,
        buflen: size_t,
        result: *mut *mut passwd,
    ) -> c_int;

    // semaphore.h
    pub fn sem_destroy(sem: *mut sem_t) -> c_int;
    pub fn sem_init(sem: *mut sem_t, pshared: c_int, value: c_uint) -> c_int;
    pub fn sem_clockwait(
        sem: *mut sem_t,
        clock_id: clockid_t,
        abstime: *const crate::timespec,
    ) -> c_int;
    pub fn sem_timedwait(sem: *mut sem_t, abstime: *const crate::timespec) -> c_int;
    pub fn sem_getvalue(sem: *mut sem_t, sval: *mut c_int) -> c_int;

    // signal.h
    pub fn pthread_sigmask(
        how: c_int,
        set: *const crate::sigset_t,
        oldset: *mut crate::sigset_t,
    ) -> c_int;
    pub fn pthread_cancel(thread: crate::pthread_t) -> c_int;
    pub fn pthread_kill(thread: crate::pthread_t, sig: c_int) -> c_int;
    pub fn sigtimedwait(
        set: *const sigset_t,
        sig: *mut siginfo_t,
        timeout: *const crate::timespec,
    ) -> c_int;
    pub fn sigwait(set: *const sigset_t, sig: *mut c_int) -> c_int;

    // stdlib.h
    pub fn getsubopt(
        optionp: *mut *mut c_char,
        tokens: *const *mut c_char,
        valuep: *mut *mut c_char,
    ) -> c_int;
    pub fn mkostemp(template: *mut c_char, flags: c_int) -> c_int;
    pub fn mkostemps(template: *mut c_char, suffixlen: c_int, flags: c_int) -> c_int;
    pub fn reallocarray(ptr: *mut c_void, nmemb: size_t, size: size_t) -> *mut c_void;

    // stdio.h
    pub fn renameat2(
        olddirfd: c_int,
        oldpath: *const c_char,
        newdirfd: c_int,
        newpath: *const c_char,
        flags: c_uint,
    ) -> c_int;

    // string.h
    pub fn explicit_bzero(p: *mut c_void, len: size_t);
    pub fn strlcat(dst: *mut c_char, src: *const c_char, siz: size_t) -> size_t;
    pub fn strlcpy(dst: *mut c_char, src: *const c_char, siz: size_t) -> size_t;

    // sys/epoll.h
    pub fn epoll_create(size: c_int) -> c_int;
    pub fn epoll_create1(flags: c_int) -> c_int;
    pub fn epoll_wait(
        epfd: c_int,
        events: *mut crate::epoll_event,
        maxevents: c_int,
        timeout: c_int,
    ) -> c_int;
    pub fn epoll_ctl(epfd: c_int, op: c_int, fd: c_int, event: *mut crate::epoll_event) -> c_int;

    // sys/ioctl.h
    pub fn ioctl(fd: c_int, request: c_ulong, ...) -> c_int;

    // sys/mman.h
    pub fn madvise(addr: *mut c_void, len: size_t, advice: c_int) -> c_int;
    pub fn msync(addr: *mut c_void, len: size_t, flags: c_int) -> c_int;
    pub fn mprotect(addr: *mut c_void, len: size_t, prot: c_int) -> c_int;
    pub fn shm_open(name: *const c_char, oflag: c_int, mode: mode_t) -> c_int;
    pub fn shm_unlink(name: *const c_char) -> c_int;

    // sys/resource.h
    pub fn getpriority(which: c_int, who: crate::id_t) -> c_int;
    pub fn setpriority(which: c_int, who: crate::id_t, prio: c_int) -> c_int;
    pub fn getrlimit(resource: c_int, rlim: *mut crate::rlimit) -> c_int;
    pub fn setrlimit(resource: c_int, rlim: *const crate::rlimit) -> c_int;

    // sys/socket.h
    pub fn CMSG_DATA(cmsg: *const cmsghdr) -> *mut c_uchar;
    pub fn CMSG_FIRSTHDR(mhdr: *const msghdr) -> *mut cmsghdr;
    pub fn CMSG_NXTHDR(mhdr: *const msghdr, cmsg: *const cmsghdr) -> *mut cmsghdr;
    pub fn bind(
        socket: c_int,
        address: *const crate::sockaddr,
        address_len: crate::socklen_t,
    ) -> c_int;
    pub fn recvfrom(
        socket: c_int,
        buf: *mut c_void,
        len: size_t,
        flags: c_int,
        addr: *mut crate::sockaddr,
        addrlen: *mut crate::socklen_t,
    ) -> ssize_t;
    pub fn recvmsg(socket: c_int, msg: *mut msghdr, flags: c_int) -> ssize_t;
    pub fn sendmsg(socket: c_int, msg: *const msghdr, flags: c_int) -> ssize_t;

    // sys/stat.h
    pub fn futimens(fd: c_int, times: *const crate::timespec) -> c_int;

    // sys/uio.h
    pub fn preadv(fd: c_int, iov: *const crate::iovec, iovcnt: c_int, offset: off_t) -> ssize_t;
    pub fn pwritev(fd: c_int, iov: *const crate::iovec, iovcnt: c_int, offset: off_t) -> ssize_t;
    pub fn readv(fd: c_int, iov: *const crate::iovec, iovcnt: c_int) -> ssize_t;
    pub fn writev(fd: c_int, iov: *const crate::iovec, iovcnt: c_int) -> ssize_t;

    // sys/utsname.h
    pub fn uname(utsname: *mut utsname) -> c_int;

    // time.h
    pub fn gettimeofday(tp: *mut crate::timeval, tz: *mut crate::timezone) -> c_int;
    pub fn clock_gettime(clk_id: crate::clockid_t, tp: *mut crate::timespec) -> c_int;
    pub fn strftime(
        s: *mut c_char,
        max: size_t,
        format: *const c_char,
        tm: *const crate::tm,
    ) -> size_t;

    // sys/statfs.h
    pub fn statfs(path: *const c_char, buf: *mut statfs) -> c_int;
    pub fn fstatfs(fd: c_int, buf: *mut statfs) -> c_int;

    // sys/stat.h and fcntl.h extras
    pub fn lutimes(filename: *const c_char, times: *const crate::timeval) -> c_int;
    pub fn utimensat(
        dirfd: c_int,
        pathname: *const c_char,
        times: *const crate::timespec,
        flags: c_int,
    ) -> c_int;
    pub fn mknodat(dirfd: c_int, pathname: *const c_char, mode: mode_t, dev: crate::dev_t)
        -> c_int;
    pub fn faccessat(dirfd: c_int, pathname: *const c_char, mode: c_int, flags: c_int) -> c_int;
    pub fn dup3(oldfd: c_int, newfd: c_int, flags: c_int) -> c_int;
    pub fn fdatasync(fd: c_int) -> c_int;
    pub fn posix_fadvise(fd: c_int, offset: off_t, len: off_t, advice: c_int) -> c_int;
    pub fn posix_fallocate(fd: c_int, offset: off_t, len: off_t) -> c_int;
    pub fn flock(fd: c_int, operation: c_int) -> c_int;
    pub fn sync();
    pub fn sethostname(name: *const c_char, len: size_t) -> c_int;
    pub fn setdomainname(name: *const c_char, len: size_t) -> c_int;

    // time.h extras
    pub fn clock_getres(clk_id: crate::clockid_t, tp: *mut crate::timespec) -> c_int;
    pub fn clock_settime(clk_id: crate::clockid_t, tp: *const crate::timespec) -> c_int;

    // sys/wait.h
    pub fn waitid(idtype: idtype_t, id: id_t, infop: *mut siginfo_t, options: c_int) -> c_int;

    // dirent.h
    pub fn seekdir(dirp: *mut crate::DIR, loc: c_long);

    // utmp.h
    pub fn login_tty(fd: c_int) -> c_int;
}

// ---------------------------------------------------------------------------
// C64h: Linux-compatible surface required by the brush dependency graph.
//
// The types and constants follow the Raven ABI headers under
// `libc/ravenlibc/include`; the Linux-only values that the ABI does not name
// (DCCP, `SO_ORIGINAL_DST`, the MFD huge-page sizes and RWF flags) are the
// documented Linux UAPI facts. The dependency crates select their Linux
// branches for `target_os = "ravenbsd"`, so the names must exist here.


pub type __fsword_t = c_long;
pub type off64_t = off_t;
pub type ino64_t = ino_t;
pub type blkcnt64_t = blkcnt_t;
pub type rlim64_t = rlim_t;
pub type fsblkcnt64_t = fsblkcnt_t;
pub type fsfilcnt64_t = fsfilcnt_t;
pub type statfs64 = statfs;
pub type statvfs64 = statvfs;
pub type termios2 = termios;

s! {


    pub struct pollfd {
        pub fd: c_int,
        pub events: c_short,
        pub revents: c_short,
    }



    pub struct signalfd_siginfo {
        pub ssi_signo: u32,
        pub ssi_errno: i32,
        pub ssi_code: i32,
        pub ssi_pid: u32,
        pub ssi_uid: u32,
        pub ssi_fd: i32,
        pub ssi_tid: u32,
        pub ssi_band: u32,
        pub ssi_overrun: u32,
        pub ssi_trapno: u32,
        pub ssi_status: i32,
        pub ssi_int: i32,
        pub ssi_ptr: u64,
        pub ssi_utime: u64,
        pub ssi_stime: u64,
        pub ssi_addr: u64,
        pub ssi_addr_lsb: u16,
        _pad2: Padding<u16>,
        pub ssi_syscall: i32,
        pub ssi_call_addr: u64,
        pub ssi_arch: u32,
        _pad: Padding<[u8; 28]>,
    }

    pub struct sockaddr_vm {
        pub svm_family: crate::sa_family_t,
        pub svm_reserved1: c_ushort,
        pub svm_port: c_uint,
        pub svm_cid: c_uint,
        pub svm_zero: [u8; 4],
    }

    pub struct ip_mreqn {
        pub imr_multiaddr: crate::in_addr,
        pub imr_address: crate::in_addr,
        pub imr_ifindex: c_int,
    }

    pub struct ip_mreq_source {
        pub imr_multiaddr: crate::in_addr,
        pub imr_interface: crate::in_addr,
        pub imr_sourceaddr: crate::in_addr,
    }

    pub struct sock_filter {
        pub code: c_ushort,
        pub jt: c_uchar,
        pub jf: c_uchar,
        pub k: c_uint,
    }

    pub struct sock_fprog {
        pub len: c_ushort,
        pub filter: *mut crate::sock_filter,
    }

    pub struct sysinfo {
        pub uptime: i64,
        pub loads: [u64; 3],
        pub totalram: u64,
        pub freeram: u64,
        pub sharedram: u64,
        pub bufferram: u64,
        pub totalswap: u64,
        pub freeswap: u64,
        pub procs: c_ushort,
        pub pad: c_ushort,
        pub totalhigh: u64,
        pub freehigh: u64,
        pub mem_unit: c_uint,
        pub _f: [c_char; 0],
    }
}


// sys/socket.h and netinet/in.h (Raven ABI).
pub const SOCK_RDM: c_int = 4;
pub const SOCK_DCCP: c_int = 6;
pub const AF_PACKET: c_int = 17;
pub const AF_VSOCK: c_int = 40;
pub const IPPROTO_DCCP: c_int = 33;
pub const IPPROTO_MPTCP: c_int = 262;
pub const IPPROTO_SCTP: c_int = 132;
pub const IPPROTO_UDPLITE: c_int = 136;
pub const SOL_IP: c_int = 0;
pub const SOL_IPV6: c_int = 41;
pub const SOL_DCCP: c_int = 269;
pub const SO_MARK: c_int = 36;
pub const SO_BINDTODEVICE: c_int = 25;
pub const SO_BINDTOIFINDEX: c_int = 62;
pub const SO_BUSY_POLL: c_int = 46;
pub const SO_INCOMING_CPU: c_int = 49;
pub const SO_ATTACH_FILTER: c_int = 26;
pub const SO_DETACH_FILTER: c_int = 27;
pub const SO_COOKIE: c_int = 57;
pub const SO_ORIGINAL_DST: c_int = 80;
pub const IP_TRANSPARENT: c_int = 19;
pub const IP_FREEBIND: c_int = 15;
pub const IP_MULTICAST_ALL: c_int = 49;
pub const IPV6_TRANSPARENT: c_int = 75;
pub const IPV6_FREEBIND: c_int = 78;
pub const IPV6_MULTICAST_ALL: c_int = 29;
pub const IPV6_TCLASS: c_int = 67;
pub const IP6T_SO_ORIGINAL_DST: c_int = 80;
pub const TCP_CONGESTION: c_int = 13;
pub const TCP_CORK: c_int = 3;
pub const TCP_NOTSENT_LOWAT: c_int = 25;
pub const TCP_QUICKACK: c_int = 12;
pub const TCP_THIN_LINEAR_TIMEOUTS: c_int = 16;
pub const TCP_USER_TIMEOUT: c_int = 18;
pub const MSG_CONFIRM: c_int = 2048;
pub const IP_HDRINCL: c_int = 3;
pub const IPV6_HDRINCL: c_int = 36;
pub const IPV6_RECVHOPLIMIT: c_int = 51;
pub const IPV6_RECVTCLASS: c_int = 66;
pub const IP_ADD_SOURCE_MEMBERSHIP: c_int = 39;
pub const IP_DROP_SOURCE_MEMBERSHIP: c_int = 40;

pub const DCCP_SOCKOPT_SERVICE: c_int = 2;
pub const DCCP_SOCKOPT_SERVER_TIMEWAIT: c_int = 6;
pub const DCCP_SOCKOPT_SEND_CSCOV: c_int = 10;
pub const DCCP_SOCKOPT_RECV_CSCOV: c_int = 11;
pub const DCCP_SOCKOPT_QPOLICY_TXQLEN: c_int = 17;
pub const DCCP_SOCKOPT_AVAILABLE_CCIDS: c_int = 12;
pub const DCCP_SOCKOPT_CCID: c_int = 13;
pub const DCCP_SOCKOPT_GET_CUR_MPS: c_int = 5;
pub const DCCP_SOCKOPT_RX_CCID: c_int = 15;
pub const DCCP_SOCKOPT_TX_CCID: c_int = 14;

// sys/termios.h and sys/ioctl.h (Raven ABI).
pub const TCGETS2: c_ulong = 0x802c542a;
pub const TCSETS2: c_ulong = 0x402c542b;
pub const TCSETSW2: c_ulong = 0x402c542c;
pub const TCSETSF2: c_ulong = 0x402c542d;
pub const FIONCLEX: c_ulong = 0x5450;
pub const UIO_MAXIOV: usize = 1024;
pub const BOTHER: crate::speed_t = 0o010_000;
pub const IBSHIFT: crate::tcflag_t = 16;

// sys/statvfs.h, sys/uio.h, sys/mman.h, sys/signalfd.h.
pub const ST_NODEV: c_ulong = 4;
pub const ST_NOEXEC: c_ulong = 8;
pub const ST_SYNCHRONOUS: c_ulong = 16;
pub const ST_MANDLOCK: c_ulong = 64;
pub const ST_NOATIME: c_ulong = 1024;
pub const ST_NODIRATIME: c_ulong = 2048;
pub const SFD_CLOEXEC: c_int = 0o200_0000;
pub const SFD_NONBLOCK: c_int = 0o4_000;
pub const MFD_CLOEXEC: c_uint = 0x1;
pub const MFD_ALLOW_SEALING: c_uint = 0x2;
pub const MFD_HUGETLB: c_uint = 0x4;
pub const RWF_HIPRI: c_int = 0x0000_0001;
pub const RWF_DSYNC: c_int = 0x0000_0002;
pub const RWF_SYNC: c_int = 0x0000_0004;
pub const RWF_NOWAIT: c_int = 0x0000_0008;
pub const RWF_APPEND: c_int = 0x0000_0010;
pub const RWF_NOAPPEND: c_int = 0x0000_0020;
pub const RWF_ATOMIC: c_int = 0x0000_0040;
pub const RWF_DONTCACHE: c_int = 0x0000_0080;

// bits/signum.h and sys/wait.h.
pub const SIGRTMIN: c_int = 35;
pub const SIGRTMAX: c_int = 64;
pub const P_PIDFD: idtype_t = 3;
pub const SYS_pidfd_open: c_long = 434;
pub const PIDFD_NONBLOCK: c_int = 0o4_000;

extern "C" {
    // sys/socket.h
    pub fn accept4(
        socket: c_int,
        address: *mut crate::sockaddr,
        address_len: *mut crate::socklen_t,
        flags: c_int,
    ) -> c_int;
    pub fn eventfd(count: c_uint, flags: c_int) -> c_int;
    pub fn sendfile(out_fd: c_int, in_fd: c_int, offset: *mut off_t, count: size_t) -> ssize_t;

    // sys/statfs.h
    pub fn statfs64(path: *const c_char, sz: size_t, buf: *mut crate::statfs64) -> c_int;
    pub fn fstatfs64(fd: c_int, sz: size_t, buf: *mut crate::statfs64) -> c_int;

    // unistd.h / sys/syscall.h
    pub fn syscall(num: c_long, ...) -> c_long;
    pub fn lseek64(fd: c_int, offset: off64_t, whence: c_int) -> off64_t;
    pub fn syncfs(fd: c_int) -> c_int;
    pub fn fsync(fd: c_int) -> c_int;
    pub fn setfsuid(uid: crate::uid_t) -> c_int;
    pub fn setfsgid(gid: crate::gid_t) -> c_int;
    pub fn fexecve(
        fd: c_int,
        argv: *const *const c_char,
        envp: *const *const c_char,
    ) -> c_int;
    pub fn forkpty(
        amaster: *mut c_int,
        name: *mut c_char,
        termp: *const crate::termios,
        winp: *const crate::winsize,
    ) -> crate::pid_t;
    pub fn ptsname_r(fd: c_int, buf: *mut c_char, buflen: size_t) -> c_int;
    pub fn sysinfo(info: *mut crate::sysinfo) -> c_int;
    pub fn confstr(name: c_int, buf: *mut c_char, len: size_t) -> size_t;
    pub fn gethostname(name: *mut c_char, len: size_t) -> c_int;
    pub fn ttyname_r(fd: c_int, buf: *mut c_char, buflen: size_t) -> c_int;
    pub fn tcgetattr(fd: c_int, termios_p: *mut crate::termios) -> c_int;
    pub fn tcsetattr(
        fd: c_int,
        optional_actions: c_int,
        termios_p: *const crate::termios,
    ) -> c_int;
    pub fn tcgetpgrp(fd: c_int) -> crate::pid_t;
    pub fn tcsetpgrp(fd: c_int, pgrp: crate::pid_t) -> c_int;
    pub fn killpg(pgrp: crate::pid_t, sig: c_int) -> c_int;
    pub fn cfmakeraw(termios_p: *mut crate::termios);
    pub fn truncate(path: *const c_char, length: off_t) -> c_int;
    pub fn readlink(path: *const c_char, buf: *mut c_char, bufsiz: size_t) -> ssize_t;
    pub fn chdir(path: *const c_char) -> c_int;
    pub fn chroot(path: *const c_char) -> c_int;
    pub fn umask(mask: crate::mode_t) -> crate::mode_t;
    pub fn ppoll(
        fds: *mut crate::pollfd,
        nfds: crate::nfds_t,
        timeout: *const crate::timespec,
        sigmask: *const crate::sigset_t,
    ) -> c_int;
    pub fn signalfd(fd: c_int, mask: *const crate::sigset_t, flags: c_int) -> c_int;
}

// bits/sysconf.h values used by nix.

// Linux filesystem magic values used by nix `statfs` (UAPI facts).
pub const ADFS_SUPER_MAGIC: c_long = 0x0000adf5;
pub const AFFS_SUPER_MAGIC: c_long = 0x0000adff;
pub const AFS_SUPER_MAGIC: c_long = 0x5346414f;
pub const AUTOFS_SUPER_MAGIC: c_long = 0x0187;
pub const BPF_FS_MAGIC: c_long = 0xcafe4a11;
pub const BTRFS_SUPER_MAGIC: c_long = 0x9123683e;
pub const CGROUP2_SUPER_MAGIC: c_long = 0x63677270;
pub const CGROUP_SUPER_MAGIC: c_long = 0x27e0eb;
pub const CODA_SUPER_MAGIC: c_long = 0x73757245;
pub const CRAMFS_MAGIC: c_long = 0x28cd3d45;
pub const DEBUGFS_MAGIC: c_long = 0x64626720;
pub const DEVPTS_SUPER_MAGIC: c_long = 0x1cd1;
pub const ECRYPTFS_SUPER_MAGIC: c_long = 0xf15f;
pub const EFS_SUPER_MAGIC: c_long = 0x00414a53;
pub const EXT2_SUPER_MAGIC: c_long = 0x0000ef53;
pub const EXT3_SUPER_MAGIC: c_long = 0x0000ef53;
pub const EXT4_SUPER_MAGIC: c_long = 0x0000ef53;
pub const F2FS_SUPER_MAGIC: c_long = 0xf2f52010;
pub const FUSE_SUPER_MAGIC: c_long = 0x65735546;
pub const FUTEXFS_SUPER_MAGIC: c_long = 0xbad1dea;
pub const HOSTFS_SUPER_MAGIC: c_long = 0x00c0ffee;
pub const HPFS_SUPER_MAGIC: c_long = 0xf995e849;
pub const HUGETLBFS_MAGIC: c_long = 0x958458f6;
pub const ISOFS_SUPER_MAGIC: c_long = 0x00009660;
pub const JFFS2_SUPER_MAGIC: c_long = 0x000072b6;
pub const MINIX2_SUPER_MAGIC2: c_long = 0x00002478;
pub const MINIX2_SUPER_MAGIC: c_long = 0x00002468;
pub const MINIX3_SUPER_MAGIC: c_long = 0x4d5a;
pub const MINIX_SUPER_MAGIC2: c_long = 0x0000138f;
pub const MINIX_SUPER_MAGIC: c_long = 0x0000137f;
pub const MSDOS_SUPER_MAGIC: c_long = 0x00004d44;
pub const NCP_SUPER_MAGIC: c_long = 0x0000564c;
pub const NFS_SUPER_MAGIC: c_long = 0x00006969;
pub const NILFS_SUPER_MAGIC: c_long = 0x3434;
pub const NSFS_MAGIC: c_long = 0x6e736673;
pub const OCFS2_SUPER_MAGIC: c_long = 0x7461636f;
pub const OPENPROM_SUPER_MAGIC: c_long = 0x00009fa1;
pub const OVERLAYFS_SUPER_MAGIC: c_long = 0x794c7630;
pub const PROC_SUPER_MAGIC: c_long = 0x00009fa0;
pub const QNX4_SUPER_MAGIC: c_long = 0x0000002f;
pub const QNX6_SUPER_MAGIC: c_long = 0x68191122;
pub const RDTGROUP_SUPER_MAGIC: c_long = 0x7655821;
pub const REISERFS_SUPER_MAGIC: c_long = 0x52654973;
pub const SECURITYFS_MAGIC: c_long = 0x73636673;
pub const SELINUX_MAGIC: c_long = 0xf97cff8c;
pub const SMACK_MAGIC: c_long = 0x43415d53;
pub const SMB_SUPER_MAGIC: c_long = 0x0000517b;
pub const SYSFS_MAGIC: c_long = 0x62656572;
pub const TMPFS_MAGIC: c_long = 0x01021994;
pub const TRACEFS_MAGIC: c_long = 0x74726163;
pub const UDF_SUPER_MAGIC: c_long = 0x15013346;
pub const USBDEVICE_SUPER_MAGIC: c_long = 0x00009fa2;
pub const XENFS_SUPER_MAGIC: c_long = 0xabba1974;
pub const XFS_SUPER_MAGIC: c_long = 0x58465342;

// sys/mman.h huge-page flags (Linux UAPI).
pub const MFD_HUGE_64KB: c_uint = 0x4000_0000;
pub const MFD_HUGE_512KB: c_uint = 0x4c00_0000;
pub const MFD_HUGE_1MB: c_uint = 0x5000_0000;
pub const MFD_HUGE_2MB: c_uint = 0x5400_0000;
pub const MFD_HUGE_8MB: c_uint = 0x5c00_0000;
pub const MFD_HUGE_16MB: c_uint = 0x6000_0000;
pub const MFD_HUGE_32MB: c_uint = 0x6400_0000;
pub const MFD_HUGE_256MB: c_uint = 0x7000_0000;
pub const MFD_HUGE_512MB: c_uint = 0x7400_0000;
pub const MFD_HUGE_1GB: c_uint = 0x7800_0000;
pub const MFD_HUGE_2GB: c_uint = 0x7c00_0000;
pub const MFD_HUGE_16GB: c_uint = 0x8800_0000;
pub const MFD_HUGE_MASK: c_uint = 63;
pub const MFD_HUGE_SHIFT: c_uint = 26;

// sys/eventfd.h, errno and sys/sysinfo.h.
pub const EFD_CLOEXEC: c_int = 0o200_0000;
pub const EFD_NONBLOCK: c_int = 0o4_000;
pub const SI_LOAD_SHIFT: c_uint = 16;

// x86_64 syscall numbers used by the brush dependency graph.
pub const SYS_gettid: c_long = 186;
pub const SYS_memfd_create: c_long = 319;
pub const SYS_execveat: c_long = 322;
pub const SYS_pivot_root: c_long = 155;

// ---------------------------------------------------------------------------
// K26a merge: Linux-compatible surface needed by the current rustix/nix
// dependency graph after the C64h/P2 merge. The values are the documented
// Linux UAPI facts; the Raven ABI does not name these yet.

pub type stat64 = stat;
pub type dirent64 = dirent;
pub type loff_t = c_longlong;

// inotify.h
pub const IN_ACCESS: c_uint = 0x0000_0001;
pub const IN_MODIFY: c_uint = 0x0000_0002;
pub const IN_ATTRIB: c_uint = 0x0000_0004;
pub const IN_CLOSE_WRITE: c_uint = 0x0000_0008;
pub const IN_CLOSE_NOWRITE: c_uint = 0x0000_0010;
pub const IN_OPEN: c_uint = 0x0000_0020;
pub const IN_MOVED_FROM: c_uint = 0x0000_0040;
pub const IN_MOVED_TO: c_uint = 0x0000_0080;
pub const IN_CREATE: c_uint = 0x0000_0100;
pub const IN_DELETE: c_uint = 0x0000_0200;
pub const IN_DELETE_SELF: c_uint = 0x0000_0400;
pub const IN_MOVE_SELF: c_uint = 0x0000_0800;
pub const IN_UNMOUNT: c_uint = 0x0000_2000;
pub const IN_Q_OVERFLOW: c_uint = 0x0000_4000;
pub const IN_IGNORED: c_uint = 0x0000_8000;
pub const IN_ONLYDIR: c_uint = 0x0100_0000;
pub const IN_DONT_FOLLOW: c_uint = 0x0200_0000;
pub const IN_EXCL_UNLINK: c_uint = 0x0400_0000;
pub const IN_MASK_CREATE: c_uint = 0x1000_0000;
pub const IN_MASK_ADD: c_uint = 0x2000_0000;
pub const IN_ISDIR: c_uint = 0x4000_0000;
pub const IN_ONESHOT: c_uint = 0x8000_0000;
pub const IN_CLOEXEC: c_int = O_CLOEXEC;
pub const IN_NONBLOCK: c_int = O_NONBLOCK;
pub const IN_CLOSE: c_uint = IN_CLOSE_WRITE | IN_CLOSE_NOWRITE;
pub const IN_MOVE: c_uint = IN_MOVED_FROM | IN_MOVED_TO;
pub const IN_ALL_EVENTS: c_uint = 0x0000_0fff;

// fcntl.h seals and memfd_create(2) flags.
pub const F_SEAL_EXEC: c_int = 0x0020;
pub const MFD_EXEC: c_uint = 0x0010;
pub const MFD_NOEXEC_SEAL: c_uint = 0x0008;

// renameat2(2) flags.
pub const RENAME_EXCHANGE: c_uint = 2;
pub const RENAME_WHITEOUT: c_uint = 4;

// lseek(2) extensions.
pub const SEEK_DATA: c_int = 3;
pub const SEEK_HOLE: c_int = 4;

// statx(2).
pub const STATX_TYPE: c_uint = 0x0000_0001;
pub const STATX_MODE: c_uint = 0x0000_0002;
pub const STATX_NLINK: c_uint = 0x0000_0004;
pub const STATX_UID: c_uint = 0x0000_0008;
pub const STATX_GID: c_uint = 0x0000_0010;
pub const STATX_ATIME: c_uint = 0x0000_0020;
pub const STATX_MTIME: c_uint = 0x0000_0040;
pub const STATX_CTIME: c_uint = 0x0000_0080;
pub const STATX_INO: c_uint = 0x0000_0100;
pub const STATX_SIZE: c_uint = 0x0000_0200;
pub const STATX_BLOCKS: c_uint = 0x0000_0400;
pub const STATX_BASIC_STATS: c_uint = 0x0000_07ff;
pub const STATX_BTIME: c_uint = 0x0000_0800;
pub const STATX_MNT_ID: c_uint = 0x0000_1000;
pub const STATX_DIOALIGN: c_uint = 0x0000_2000;
pub const STATX_ALL: c_uint = 0x0000_0fff;
pub const STATX__RESERVED: c_uint = 0x8000_0000;
pub const STATX_ATTR_COMPRESSED: c_uint = 0x0000_0004;
pub const STATX_ATTR_IMMUTABLE: c_uint = 0x0000_0010;
pub const STATX_ATTR_APPEND: c_uint = 0x0000_0020;
pub const STATX_ATTR_NODUMP: c_uint = 0x0000_0040;
pub const STATX_ATTR_ENCRYPTED: c_uint = 0x0000_0800;
pub const STATX_ATTR_AUTOMOUNT: c_uint = 0x0000_1000;
pub const STATX_ATTR_MOUNT_ROOT: c_uint = 0x0000_2000;
pub const STATX_ATTR_VERITY: c_uint = 0x0010_0000;
pub const STATX_ATTR_DAX: c_uint = 0x0020_0000;

// xattr flags.
pub const XATTR_CREATE: c_int = 1;
pub const XATTR_REPLACE: c_int = 2;

// block device ioctls.
pub const BLKSSZGET: c_ulong = 0x1268;
pub const BLKPBSZGET: c_ulong = 0x127b;

// FICLONE.
pub const FICLONE: c_ulong = 0x4004_9409;

// x86_64 syscall numbers used by the current rustix/nix graph.
pub const SYS_getdents64: c_long = 217;
pub const SYS_fchmod: c_long = 91;
pub const SYS_fchown: c_long = 93;
pub const SYS_fchmodat: c_long = 268;
pub const SYS_renameat2: c_long = 316;
pub const SYS_syncfs: c_long = 306;
pub const SYS_copy_file_range: c_long = 326;
pub const SYS_statx: c_long = 332;

extern "C" {
    pub fn inotify_init1(flags: c_int) -> c_int;
    pub fn inotify_add_watch(fd: c_int, pathname: *const c_char, mask: c_uint) -> c_int;
    pub fn inotify_rm_watch(fd: c_int, wd: c_int) -> c_int;
    pub fn sendfile64(
        out_fd: c_int,
        in_fd: c_int,
        offset: *mut loff_t,
        count: size_t,
    ) -> ssize_t;
    pub fn getxattr(
        path: *const c_char,
        name: *const c_char,
        value: *mut c_void,
        size: size_t,
    ) -> ssize_t;
    pub fn lgetxattr(
        path: *const c_char,
        name: *const c_char,
        value: *mut c_void,
        size: size_t,
    ) -> ssize_t;
    pub fn fgetxattr(
        fd: c_int,
        name: *const c_char,
        value: *mut c_void,
        size: size_t,
    ) -> ssize_t;
    pub fn setxattr(
        path: *const c_char,
        name: *const c_char,
        value: *const c_void,
        size: size_t,
        flags: c_int,
    ) -> c_int;
    pub fn lsetxattr(
        path: *const c_char,
        name: *const c_char,
        value: *const c_void,
        size: size_t,
        flags: c_int,
    ) -> c_int;
    pub fn fsetxattr(
        fd: c_int,
        name: *const c_char,
        value: *const c_void,
        size: size_t,
        flags: c_int,
    ) -> c_int;
    pub fn listxattr(path: *const c_char, list: *mut c_char, size: size_t) -> ssize_t;
    pub fn llistxattr(path: *const c_char, list: *mut c_char, size: size_t) -> ssize_t;
    pub fn flistxattr(fd: c_int, list: *mut c_char, size: size_t) -> ssize_t;
    pub fn removexattr(path: *const c_char, name: *const c_char) -> c_int;
    pub fn lremovexattr(path: *const c_char, name: *const c_char) -> c_int;
    pub fn fremovexattr(fd: c_int, name: *const c_char) -> c_int;
}

s! {
    #[repr(C, align(8))]
    pub struct posix_spawnattr_t {
        pub __data: [u8; 336],
    }

    #[repr(C, align(8))]
    pub struct posix_spawn_file_actions_t {
        pub __data: [u8; 80],
    }
}

pub const POSIX_SPAWN_RESETIDS: c_short = 1;
pub const POSIX_SPAWN_SETPGROUP: c_short = 2;
pub const POSIX_SPAWN_SETSIGDEF: c_short = 4;
pub const POSIX_SPAWN_SETSIGMASK: c_short = 8;
pub const POSIX_SPAWN_SETSCHEDPARAM: c_short = 16;
pub const POSIX_SPAWN_SETSCHEDULER: c_short = 32;
pub const POSIX_SPAWN_SETSID: c_short = 128;

extern "C" {
    pub fn posix_spawn(
        pid: *mut pid_t,
        path: *const c_char,
        file_actions: *const posix_spawn_file_actions_t,
        attrp: *const posix_spawnattr_t,
        argv: *const *const c_char,
        envp: *const *const c_char,
    ) -> c_int;
    pub fn posix_spawnp(
        pid: *mut pid_t,
        file: *const c_char,
        file_actions: *const posix_spawn_file_actions_t,
        attrp: *const posix_spawnattr_t,
        argv: *const *const c_char,
        envp: *const *const c_char,
    ) -> c_int;
    pub fn posix_spawn_file_actions_init(
        file_actions: *mut posix_spawn_file_actions_t,
    ) -> c_int;
    pub fn posix_spawn_file_actions_destroy(
        file_actions: *mut posix_spawn_file_actions_t,
    ) -> c_int;
    pub fn posix_spawn_file_actions_adddup2(
        file_actions: *mut posix_spawn_file_actions_t,
        fd: c_int,
        newfd: c_int,
    ) -> c_int;
    pub fn posix_spawn_file_actions_addclose(
        file_actions: *mut posix_spawn_file_actions_t,
        fd: c_int,
    ) -> c_int;
    pub fn posix_spawn_file_actions_addchdir_np(
        file_actions: *mut posix_spawn_file_actions_t,
        path: *const c_char,
    ) -> c_int;

    pub fn posix_spawnattr_init(attr: *mut posix_spawnattr_t) -> c_int;
    pub fn posix_spawnattr_destroy(attr: *mut posix_spawnattr_t) -> c_int;
    pub fn posix_spawnattr_setpgroup(attr: *mut posix_spawnattr_t, pgroup: pid_t) -> c_int;
    pub fn posix_spawnattr_setsigdefault(
        attr: *mut posix_spawnattr_t,
        sigdefault: *const sigset_t,
    ) -> c_int;
    pub fn posix_spawnattr_setflags(attr: *mut posix_spawnattr_t, flags: c_short) -> c_int;
}
