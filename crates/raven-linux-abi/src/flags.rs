// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// Linux UAPI flag values, transcribed from linux-raw-sys 0.12.1 (Apache-2.0 OR MIT).
// Common values are identical on x86_64, aarch64 and riscv64; architecture submodules
// carry the values that differ (open flags, MAP_/PROT_ extras, signal stack sizes and
// auxiliary vector keys).

//! Linux UAPI flags, constants and option numbers.
//!
//! Each submodule mirrors one Linux UAPI prefix. The `x86_64`, `aarch64` and `riscv64`
//! submodules exist only where the architectures differ.

/// `O_*` file status flags for `open`, `openat`, `openat2` and `fcntl(F_SETFL)`.
pub mod open {
    pub const O_ACCMODE: u32 = 3;
    pub const O_APPEND: u32 = 1024;
    pub const O_CLOEXEC: u32 = 524288;
    pub const O_CREAT: u32 = 64;
    pub const O_DSYNC: u32 = 4096;
    pub const O_EXCL: u32 = 128;
    pub const O_NDELAY: u32 = 2048;
    pub const O_NOATIME: u32 = 262144;
    pub const O_NOCTTY: u32 = 256;
    pub const O_NONBLOCK: u32 = 2048;
    pub const O_PATH: u32 = 2097152;
    pub const O_RDONLY: u32 = 0;
    pub const O_RDWR: u32 = 2;
    pub const O_SYNC: u32 = 1052672;
    pub const O_TRUNC: u32 = 512;
    pub const O_WRONLY: u32 = 1;

    /// x86-64 values for the flags that differ between architectures.
    pub mod x86_64 {
        pub const O_DIRECT: u32 = 16384;
        pub const O_DIRECTORY: u32 = 65536;
        pub const O_LARGEFILE: u32 = 32768;
        pub const O_NOFOLLOW: u32 = 131072;
        pub const O_TMPFILE: u32 = 4259840;
    }

    /// aarch64 values for the flags that differ between architectures.
    pub mod aarch64 {
        pub const O_DIRECT: u32 = 65536;
        pub const O_DIRECTORY: u32 = 16384;
        pub const O_LARGEFILE: u32 = 131072;
        pub const O_NOFOLLOW: u32 = 32768;
        pub const O_TMPFILE: u32 = 4210688;
    }

    /// riscv64 values for the flags that differ between architectures.
    pub mod riscv64 {
        pub const O_DIRECT: u32 = 16384;
        pub const O_DIRECTORY: u32 = 65536;
        pub const O_LARGEFILE: u32 = 32768;
        pub const O_NOFOLLOW: u32 = 131072;
        pub const O_TMPFILE: u32 = 4259840;
    }
}

/// `AT_*` flags used by the `*at` syscall family (not the auxiliary vector).
pub mod at_fd {
    pub const AT_EACCESS: u32 = 512;
    pub const AT_EMPTY_PATH: u32 = 4096;
    pub const AT_EXECVE_CHECK: u32 = 65536;
    pub const AT_FDCWD: i32 = -100;
    pub const AT_HANDLE_CONNECTABLE: u32 = 2;
    pub const AT_HANDLE_FID: u32 = 512;
    pub const AT_HANDLE_MNT_ID_UNIQUE: u32 = 1;
    pub const AT_NO_AUTOMOUNT: u32 = 2048;
    pub const AT_RECURSIVE: u32 = 32768;
    pub const AT_REMOVEDIR: u32 = 512;
    pub const AT_RENAME_EXCHANGE: u32 = 2;
    pub const AT_RENAME_NOREPLACE: u32 = 1;
    pub const AT_RENAME_WHITEOUT: u32 = 4;
    pub const AT_STATX_DONT_SYNC: u32 = 16384;
    pub const AT_STATX_FORCE_SYNC: u32 = 8192;
    pub const AT_STATX_SYNC_AS_STAT: u32 = 0;
    pub const AT_STATX_SYNC_TYPE: u32 = 24576;
    pub const AT_SYMLINK_FOLLOW: u32 = 1024;
    pub const AT_SYMLINK_NOFOLLOW: u32 = 256;
}

/// `AT_*` keys passed in the initial process auxiliary vector.
pub mod auxv {
    pub const AT_BASE: u32 = 7;
    pub const AT_BASE_PLATFORM: u32 = 24;
    pub const AT_CLKTCK: u32 = 17;
    pub const AT_EGID: u32 = 14;
    pub const AT_ENTRY: u32 = 9;
    pub const AT_EUID: u32 = 12;
    pub const AT_EXECFD: u32 = 2;
    pub const AT_EXECFN: u32 = 31;
    pub const AT_FLAGS: u32 = 8;
    pub const AT_GID: u32 = 13;
    pub const AT_HWCAP: u32 = 16;
    pub const AT_HWCAP2: u32 = 26;
    pub const AT_HWCAP3: u32 = 29;
    pub const AT_HWCAP4: u32 = 30;
    pub const AT_IGNORE: u32 = 1;
    pub const AT_MINSIGSTKSZ: u32 = 51;
    pub const AT_NOTELF: u32 = 10;
    pub const AT_NULL: u32 = 0;
    pub const AT_PAGESZ: u32 = 6;
    pub const AT_PHDR: u32 = 3;
    pub const AT_PHENT: u32 = 4;
    pub const AT_PHNUM: u32 = 5;
    pub const AT_PLATFORM: u32 = 15;
    pub const AT_RANDOM: u32 = 25;
    pub const AT_RSEQ_ALIGN: u32 = 28;
    pub const AT_RSEQ_FEATURE_SIZE: u32 = 27;
    pub const AT_SECURE: u32 = 23;
    pub const AT_UID: u32 = 11;

    /// x86-64-only auxiliary vector keys.
    pub mod x86_64 {
        pub const AT_SYSINFO_EHDR: u32 = 33;
    }

    /// riscv64-only auxiliary vector keys (hwprobe cache information).
    pub mod riscv64 {
        pub const AT_L1D_CACHEGEOMETRY: u32 = 43;
        pub const AT_L1D_CACHESIZE: u32 = 42;
        pub const AT_L1I_CACHEGEOMETRY: u32 = 41;
        pub const AT_L1I_CACHESIZE: u32 = 40;
        pub const AT_L2_CACHEGEOMETRY: u32 = 45;
        pub const AT_L2_CACHESIZE: u32 = 44;
        pub const AT_L3_CACHEGEOMETRY: u32 = 47;
        pub const AT_L3_CACHESIZE: u32 = 46;
    }
}

/// `PROT_*` protection bits and `MAP_*` mapping flags for `mmap` and `mprotect`.
pub mod mmap {
    pub const MAP_ANONYMOUS: u32 = 32;
    pub const MAP_DENYWRITE: u32 = 2048;
    pub const MAP_DROPPABLE: u32 = 8;
    pub const MAP_EXECUTABLE: u32 = 4096;
    pub const MAP_FILE: u32 = 0;
    pub const MAP_FIXED: u32 = 16;
    pub const MAP_FIXED_NOREPLACE: u32 = 1048576;
    pub const MAP_GROWSDOWN: u32 = 256;
    pub const MAP_HUGETLB: u32 = 262144;
    pub const MAP_HUGE_16GB: u32 = 2281701376;
    pub const MAP_HUGE_16KB: u32 = 939524096;
    pub const MAP_HUGE_16MB: u32 = 1610612736;
    pub const MAP_HUGE_1GB: u32 = 2013265920;
    pub const MAP_HUGE_1MB: u32 = 1342177280;
    pub const MAP_HUGE_256MB: u32 = 1879048192;
    pub const MAP_HUGE_2GB: u32 = 2080374784;
    pub const MAP_HUGE_2MB: u32 = 1409286144;
    pub const MAP_HUGE_32MB: u32 = 1677721600;
    pub const MAP_HUGE_512KB: u32 = 1275068416;
    pub const MAP_HUGE_512MB: u32 = 1946157056;
    pub const MAP_HUGE_64KB: u32 = 1073741824;
    pub const MAP_HUGE_8MB: u32 = 1543503872;
    pub const MAP_HUGE_MASK: u32 = 63;
    pub const MAP_HUGE_SHIFT: u32 = 26;
    pub const MAP_LOCKED: u32 = 8192;
    pub const MAP_NONBLOCK: u32 = 65536;
    pub const MAP_NORESERVE: u32 = 16384;
    pub const MAP_POPULATE: u32 = 32768;
    pub const MAP_PRIVATE: u32 = 2;
    pub const MAP_SHARED: u32 = 1;
    pub const MAP_SHARED_VALIDATE: u32 = 3;
    pub const MAP_STACK: u32 = 131072;
    pub const MAP_SYNC: u32 = 524288;
    pub const MAP_TYPE: u32 = 15;
    pub const MAP_UNINITIALIZED: u32 = 67108864;
    pub const PROT_EXEC: u32 = 4;
    pub const PROT_GROWSDOWN: u32 = 16777216;
    pub const PROT_GROWSUP: u32 = 33554432;
    pub const PROT_NONE: u32 = 0;
    pub const PROT_READ: u32 = 1;
    pub const PROT_SEM: u32 = 8;
    pub const PROT_WRITE: u32 = 2;

    /// x86-64-only `MAP_*` flags.
    pub mod x86_64 {
        pub const MAP_32BIT: u32 = 64;
        pub const MAP_ABOVE4G: u32 = 128;
    }

    /// aarch64-only `PROT_*` flags.
    pub mod aarch64 {
        pub const PROT_BTI: u32 = 16;
        pub const PROT_MTE: u32 = 32;
    }
}

/// `CLONE_*` flags for `clone` and `clone3`. All values are widened to `u64`.
pub mod clone {
    pub const CLONE_ARGS_SIZE_VER0: u64 = 64;
    pub const CLONE_ARGS_SIZE_VER1: u64 = 80;
    pub const CLONE_ARGS_SIZE_VER2: u64 = 88;
    pub const CLONE_CHILD_CLEARTID: u64 = 2097152;
    pub const CLONE_CHILD_SETTID: u64 = 16777216;
    pub const CLONE_CLEAR_SIGHAND: u64 = 4294967296;
    pub const CLONE_DETACHED: u64 = 4194304;
    pub const CLONE_FILES: u64 = 1024;
    pub const CLONE_FS: u64 = 512;
    pub const CLONE_INTO_CGROUP: u64 = 8589934592;
    pub const CLONE_IO: u64 = 2147483648;
    pub const CLONE_NEWCGROUP: u64 = 33554432;
    pub const CLONE_NEWIPC: u64 = 134217728;
    pub const CLONE_NEWNET: u64 = 1073741824;
    pub const CLONE_NEWNS: u64 = 131072;
    pub const CLONE_NEWPID: u64 = 536870912;
    pub const CLONE_NEWTIME: u64 = 128;
    pub const CLONE_NEWUSER: u64 = 268435456;
    pub const CLONE_NEWUTS: u64 = 67108864;
    pub const CLONE_PARENT: u64 = 32768;
    pub const CLONE_PARENT_SETTID: u64 = 1048576;
    pub const CLONE_PIDFD: u64 = 4096;
    pub const CLONE_PTRACE: u64 = 8192;
    pub const CLONE_SETTLS: u64 = 524288;
    pub const CLONE_SIGHAND: u64 = 2048;
    pub const CLONE_SYSVSEM: u64 = 262144;
    pub const CLONE_THREAD: u64 = 65536;
    pub const CLONE_UNTRACED: u64 = 8388608;
    pub const CLONE_VFORK: u64 = 16384;
    pub const CLONE_VM: u64 = 256;
}

/// `SIG*` signal numbers, `sigprocmask` how-values and `sigevent` notification modes.
pub mod signal {
    pub const SIGABRT: u32 = 6;
    pub const SIGALRM: u32 = 14;
    pub const SIGBUS: u32 = 7;
    pub const SIGCHLD: u32 = 17;
    pub const SIGCONT: u32 = 18;
    pub const SIGEV_MAX_SIZE: u32 = 64;
    pub const SIGEV_NONE: u32 = 1;
    pub const SIGEV_SIGNAL: u32 = 0;
    pub const SIGEV_THREAD: u32 = 2;
    pub const SIGEV_THREAD_ID: u32 = 4;
    pub const SIGFPE: u32 = 8;
    pub const SIGHUP: u32 = 1;
    pub const SIGILL: u32 = 4;
    pub const SIGINT: u32 = 2;
    pub const SIGIO: u32 = 29;
    pub const SIGIOT: u32 = 6;
    pub const SIGKILL: u32 = 9;
    pub const SIGPIPE: u32 = 13;
    pub const SIGPOLL: u32 = 29;
    pub const SIGPROF: u32 = 27;
    pub const SIGPWR: u32 = 30;
    pub const SIGQUIT: u32 = 3;
    pub const SIGRTMIN: u32 = 32;
    pub const SIGSEGV: u32 = 11;
    pub const SIGSTKFLT: u32 = 16;
    pub const SIGSTOP: u32 = 19;
    pub const SIGSYS: u32 = 31;
    pub const SIGTERM: u32 = 15;
    pub const SIGTRAP: u32 = 5;
    pub const SIGTSTP: u32 = 20;
    pub const SIGTTIN: u32 = 21;
    pub const SIGTTOU: u32 = 22;
    pub const SIGUNUSED: u32 = 31;
    pub const SIGURG: u32 = 23;
    pub const SIGUSR1: u32 = 10;
    pub const SIGUSR2: u32 = 12;
    pub const SIGVTALRM: u32 = 26;
    pub const SIGWINCH: u32 = 28;
    pub const SIGXCPU: u32 = 24;
    pub const SIGXFSZ: u32 = 25;
    pub const SIG_BLOCK: u32 = 0;
    pub const SIG_SETMASK: u32 = 2;
    pub const SIG_UNBLOCK: u32 = 1;

    /// x86-64-only signal constants.
    pub mod x86_64 {
        pub const SIGSTKSZ: u32 = 8192;
    }

    /// aarch64 values that differ from x86-64.
    pub mod aarch64 {
        pub const SIGRTMAX: u32 = 64;
        pub const SIGSTKSZ: u32 = 16384;
    }

    /// riscv64 values that differ from x86-64.
    pub mod riscv64 {
        pub const SIGRTMAX: u32 = 64;
        pub const SIGSTKSZ: u32 = 8192;
    }
}

/// `F_*` commands for `fcntl`.
pub mod fcntl {
    pub const F_ADD_SEALS: u32 = 1033;
    pub const F_CANCELLK: u32 = 1029;
    pub const F_CREATED_QUERY: u32 = 1028;
    pub const F_DUPFD: u32 = 0;
    pub const F_DUPFD_CLOEXEC: u32 = 1030;
    pub const F_DUPFD_QUERY: u32 = 1027;
    pub const F_EXLCK: u32 = 4;
    pub const F_GETFD: u32 = 1;
    pub const F_GETFL: u32 = 3;
    pub const F_GETLEASE: u32 = 1025;
    pub const F_GETLK: u32 = 5;
    pub const F_GETOWN: u32 = 9;
    pub const F_GETOWNER_UIDS: u32 = 17;
    pub const F_GETOWN_EX: u32 = 16;
    pub const F_GETPIPE_SZ: u32 = 1032;
    pub const F_GETSIG: u32 = 11;
    pub const F_GET_FILE_RW_HINT: u32 = 1037;
    pub const F_GET_RW_HINT: u32 = 1035;
    pub const F_GET_SEALS: u32 = 1034;
    pub const F_LINUX_SPECIFIC_BASE: u32 = 1024;
    pub const F_NOTIFY: u32 = 1026;
    pub const F_OFD_GETLK: u32 = 36;
    pub const F_OFD_SETLK: u32 = 37;
    pub const F_OFD_SETLKW: u32 = 38;
    pub const F_OK: u32 = 0;
    pub const F_OWNER_PGRP: u32 = 2;
    pub const F_OWNER_PID: u32 = 1;
    pub const F_OWNER_TID: u32 = 0;
    pub const F_RDLCK: u32 = 0;
    pub const F_SEAL_EXEC: u32 = 32;
    pub const F_SEAL_FUTURE_WRITE: u32 = 16;
    pub const F_SEAL_GROW: u32 = 4;
    pub const F_SEAL_SEAL: u32 = 1;
    pub const F_SEAL_SHRINK: u32 = 2;
    pub const F_SEAL_WRITE: u32 = 8;
    pub const F_SETFD: u32 = 2;
    pub const F_SETFL: u32 = 4;
    pub const F_SETLEASE: u32 = 1024;
    pub const F_SETLK: u32 = 6;
    pub const F_SETLKW: u32 = 7;
    pub const F_SETOWN: u32 = 8;
    pub const F_SETOWN_EX: u32 = 15;
    pub const F_SETPIPE_SZ: u32 = 1031;
    pub const F_SETSIG: u32 = 10;
    pub const F_SET_FILE_RW_HINT: u32 = 1038;
    pub const F_SET_RW_HINT: u32 = 1036;
    pub const F_SHLCK: u32 = 8;
    pub const F_UNLCK: u32 = 2;
    pub const F_WRLCK: u32 = 1;
}

/// `FUTEX_*` operations and commands for `futex`.
pub mod futex {
    pub const FUTEX_32: u32 = 2;
    pub const FUTEX_BITSET_MATCH_ANY: u32 = 4294967295;
    pub const FUTEX_CLOCK_REALTIME: u32 = 256;
    pub const FUTEX_CMD_MASK: i32 = -385;
    pub const FUTEX_CMP_REQUEUE: u32 = 4;
    pub const FUTEX_CMP_REQUEUE_PI: u32 = 12;
    pub const FUTEX_CMP_REQUEUE_PI_PRIVATE: u32 = 140;
    pub const FUTEX_CMP_REQUEUE_PRIVATE: u32 = 132;
    pub const FUTEX_FD: u32 = 2;
    pub const FUTEX_LOCK_PI: u32 = 6;
    pub const FUTEX_LOCK_PI2: u32 = 13;
    pub const FUTEX_LOCK_PI2_PRIVATE: u32 = 141;
    pub const FUTEX_LOCK_PI_PRIVATE: u32 = 134;
    pub const FUTEX_NO_NODE: i32 = -1;
    pub const FUTEX_OP_ADD: u32 = 1;
    pub const FUTEX_OP_ANDN: u32 = 3;
    pub const FUTEX_OP_CMP_EQ: u32 = 0;
    pub const FUTEX_OP_CMP_GE: u32 = 5;
    pub const FUTEX_OP_CMP_GT: u32 = 4;
    pub const FUTEX_OP_CMP_LE: u32 = 3;
    pub const FUTEX_OP_CMP_LT: u32 = 2;
    pub const FUTEX_OP_CMP_NE: u32 = 1;
    pub const FUTEX_OP_OPARG_SHIFT: u32 = 8;
    pub const FUTEX_OP_OR: u32 = 2;
    pub const FUTEX_OP_SET: u32 = 0;
    pub const FUTEX_OP_XOR: u32 = 4;
    pub const FUTEX_OWNER_DIED: u32 = 1073741824;
    pub const FUTEX_PRIVATE_FLAG: u32 = 128;
    pub const FUTEX_REQUEUE: u32 = 3;
    pub const FUTEX_REQUEUE_PRIVATE: u32 = 131;
    pub const FUTEX_TID_MASK: u32 = 1073741823;
    pub const FUTEX_TRYLOCK_PI: u32 = 8;
    pub const FUTEX_TRYLOCK_PI_PRIVATE: u32 = 136;
    pub const FUTEX_UNLOCK_PI: u32 = 7;
    pub const FUTEX_UNLOCK_PI_PRIVATE: u32 = 135;
    pub const FUTEX_WAIT: u32 = 0;
    pub const FUTEX_WAITERS: u32 = 2147483648;
    pub const FUTEX_WAITV_MAX: u32 = 128;
    pub const FUTEX_WAIT_BITSET: u32 = 9;
    pub const FUTEX_WAIT_BITSET_PRIVATE: u32 = 137;
    pub const FUTEX_WAIT_PRIVATE: u32 = 128;
    pub const FUTEX_WAIT_REQUEUE_PI: u32 = 11;
    pub const FUTEX_WAIT_REQUEUE_PI_PRIVATE: u32 = 139;
    pub const FUTEX_WAKE: u32 = 1;
    pub const FUTEX_WAKE_BITSET: u32 = 10;
    pub const FUTEX_WAKE_BITSET_PRIVATE: u32 = 138;
    pub const FUTEX_WAKE_OP: u32 = 5;
    pub const FUTEX_WAKE_OP_PRIVATE: u32 = 133;
    pub const FUTEX_WAKE_PRIVATE: u32 = 129;
}

/// `EPOLL*` events and `epoll_ctl` operations.
pub mod epoll {
    pub const EPOLLERR: u32 = 8;
    pub const EPOLLET: u32 = 2147483648;
    pub const EPOLLEXCLUSIVE: u32 = 268435456;
    pub const EPOLLHUP: u32 = 16;
    pub const EPOLLIN: u32 = 1;
    pub const EPOLLMSG: u32 = 1024;
    pub const EPOLLNVAL: u32 = 32;
    pub const EPOLLONESHOT: u32 = 1073741824;
    pub const EPOLLOUT: u32 = 4;
    pub const EPOLLPRI: u32 = 2;
    pub const EPOLLRDBAND: u32 = 128;
    pub const EPOLLRDHUP: u32 = 8192;
    pub const EPOLLRDNORM: u32 = 64;
    pub const EPOLLWAKEUP: u32 = 536870912;
    pub const EPOLLWRBAND: u32 = 512;
    pub const EPOLLWRNORM: u32 = 256;
    pub const EPOLL_CLOEXEC: u32 = 524288;
    pub const EPOLL_CTL_ADD: u32 = 1;
    pub const EPOLL_CTL_DEL: u32 = 2;
    pub const EPOLL_CTL_MOD: u32 = 3;
    pub const EPOLL_IOC_TYPE: u32 = 138;
}

/// `CLOCK_*` clock identifiers for the time syscalls.
pub mod clock {
    pub const CLOCK_AUX: u32 = 16;
    pub const CLOCK_AUX_LAST: u32 = 23;
    pub const CLOCK_BOOTTIME: u32 = 7;
    pub const CLOCK_BOOTTIME_ALARM: u32 = 9;
    pub const CLOCK_MONOTONIC: u32 = 1;
    pub const CLOCK_MONOTONIC_COARSE: u32 = 6;
    pub const CLOCK_MONOTONIC_RAW: u32 = 4;
    pub const CLOCK_PROCESS_CPUTIME_ID: u32 = 2;
    pub const CLOCK_REALTIME: u32 = 0;
    pub const CLOCK_REALTIME_ALARM: u32 = 8;
    pub const CLOCK_REALTIME_COARSE: u32 = 5;
    pub const CLOCK_SGI_CYCLE: u32 = 10;
    pub const CLOCK_TAI: u32 = 11;
    pub const CLOCK_THREAD_CPUTIME_ID: u32 = 3;
}

/// `S_IF*` file type bits, `S_IS*` special bits and `S_I*` permission bits.
pub mod stat {
    pub const S_IFBLK: u32 = 24576;
    pub const S_IFCHR: u32 = 8192;
    pub const S_IFDIR: u32 = 16384;
    pub const S_IFIFO: u32 = 4096;
    pub const S_IFLNK: u32 = 40960;
    pub const S_IFMT: u32 = 61440;
    pub const S_IFREG: u32 = 32768;
    pub const S_IFSOCK: u32 = 49152;
    pub const S_IRGRP: u32 = 32;
    pub const S_IROTH: u32 = 4;
    pub const S_IRUSR: u32 = 256;
    pub const S_IRWXG: u32 = 56;
    pub const S_IRWXO: u32 = 7;
    pub const S_IRWXU: u32 = 448;
    pub const S_ISGID: u32 = 1024;
    pub const S_ISUID: u32 = 2048;
    pub const S_ISVTX: u32 = 512;
    pub const S_IWGRP: u32 = 16;
    pub const S_IWOTH: u32 = 2;
    pub const S_IWUSR: u32 = 128;
    pub const S_IXGRP: u32 = 8;
    pub const S_IXOTH: u32 = 1;
    pub const S_IXUSR: u32 = 64;
}

/// `POLL*` events for `poll` and `ppoll`.
pub mod poll {
    pub const POLLERR: u32 = 8;
    pub const POLLHUP: u32 = 16;
    pub const POLLIN: u32 = 1;
    pub const POLLMSG: u32 = 1024;
    pub const POLLNVAL: u32 = 32;
    pub const POLLOUT: u32 = 4;
    pub const POLLPRI: u32 = 2;
    pub const POLLRDBAND: u32 = 128;
    pub const POLLRDHUP: u32 = 8192;
    pub const POLLRDNORM: u32 = 64;
    pub const POLLREMOVE: u32 = 4096;
    pub const POLLWRBAND: u32 = 512;
    pub const POLLWRNORM: u32 = 256;
    pub const POLL_ERR: u32 = 4;
    pub const POLL_HUP: u32 = 6;
    pub const POLL_IN: u32 = 1;
    pub const POLL_MSG: u32 = 3;
    pub const POLL_OUT: u32 = 2;
    pub const POLL_PRI: u32 = 5;
}

/// `SOCK_*` socket types for `socket`.
pub mod sock {
    pub const SOCK_BUF_LOCK_MASK: u32 = 3;
    pub const SOCK_DGRAM: u32 = 2;
    pub const SOCK_RAW: u32 = 3;
    pub const SOCK_RCVBUF_LOCK: u32 = 2;
    pub const SOCK_RDM: u32 = 4;
    pub const SOCK_SEQPACKET: u32 = 5;
    pub const SOCK_SNDBUF_LOCK: u32 = 1;
    pub const SOCK_STREAM: u32 = 1;
    pub const SOCK_TXREHASH_DEFAULT: u32 = 255;
    pub const SOCK_TXREHASH_DISABLED: u32 = 0;
    pub const SOCK_TXREHASH_ENABLED: u32 = 1;
}

/// `AF_*` address families for `socket`.
pub mod af {
    pub const AF_ALG: u32 = 38;
    pub const AF_APPLETALK: u32 = 5;
    pub const AF_ASH: u32 = 18;
    pub const AF_ATMPVC: u32 = 8;
    pub const AF_ATMSVC: u32 = 20;
    pub const AF_AX25: u32 = 3;
    pub const AF_BLUETOOTH: u32 = 31;
    pub const AF_BRIDGE: u32 = 7;
    pub const AF_CAIF: u32 = 37;
    pub const AF_CAN: u32 = 29;
    pub const AF_ECONET: u32 = 19;
    pub const AF_IEEE802154: u32 = 36;
    pub const AF_INET: u32 = 2;
    pub const AF_INET6: u32 = 10;
    pub const AF_IPX: u32 = 4;
    pub const AF_IRDA: u32 = 23;
    pub const AF_ISDN: u32 = 34;
    pub const AF_IUCV: u32 = 32;
    pub const AF_KCM: u32 = 41;
    pub const AF_KEY: u32 = 15;
    pub const AF_LLC: u32 = 26;
    pub const AF_MAX: u32 = 46;
    pub const AF_MCTP: u32 = 45;
    pub const AF_NETBEUI: u32 = 13;
    pub const AF_NETLINK: u32 = 16;
    pub const AF_NETROM: u32 = 6;
    pub const AF_NFC: u32 = 39;
    pub const AF_PACKET: u32 = 17;
    pub const AF_PHONET: u32 = 35;
    pub const AF_PPPOX: u32 = 24;
    pub const AF_QIPCRTR: u32 = 42;
    pub const AF_RDS: u32 = 21;
    pub const AF_ROSE: u32 = 11;
    pub const AF_RXRPC: u32 = 33;
    pub const AF_SECURITY: u32 = 14;
    pub const AF_SMC: u32 = 43;
    pub const AF_SNA: u32 = 22;
    pub const AF_TIPC: u32 = 30;
    pub const AF_UNIX: u32 = 1;
    pub const AF_UNSPEC: u32 = 0;
    pub const AF_VSOCK: u32 = 40;
    pub const AF_WANPIPE: u32 = 25;
    pub const AF_X25: u32 = 9;
    pub const AF_XDP: u32 = 44;
}

/// `SOL_*` protocol levels for `setsockopt` and `getsockopt`.
pub mod sol {
    pub const SOL_AAL: u32 = 265;
    pub const SOL_ALG: u32 = 279;
    pub const SOL_ATALK: u32 = 258;
    pub const SOL_ATM: u32 = 264;
    pub const SOL_AX25: u32 = 257;
    pub const SOL_BLUETOOTH: u32 = 274;
    pub const SOL_CAIF: u32 = 278;
    pub const SOL_DCCP: u32 = 269;
    pub const SOL_DECNET: u32 = 261;
    pub const SOL_ICMPV6: u32 = 58;
    pub const SOL_IP: u32 = 0;
    pub const SOL_IPV6: u32 = 41;
    pub const SOL_IPX: u32 = 256;
    pub const SOL_IRDA: u32 = 266;
    pub const SOL_IUCV: u32 = 277;
    pub const SOL_KCM: u32 = 281;
    pub const SOL_LLC: u32 = 268;
    pub const SOL_MCTP: u32 = 285;
    pub const SOL_MPTCP: u32 = 284;
    pub const SOL_NETBEUI: u32 = 267;
    pub const SOL_NETLINK: u32 = 270;
    pub const SOL_NETROM: u32 = 259;
    pub const SOL_NFC: u32 = 280;
    pub const SOL_PACKET: u32 = 263;
    pub const SOL_PNPIPE: u32 = 275;
    pub const SOL_PPPOL2TP: u32 = 273;
    pub const SOL_RAW: u32 = 255;
    pub const SOL_RDS: u32 = 276;
    pub const SOL_ROSE: u32 = 260;
    pub const SOL_RXRPC: u32 = 272;
    pub const SOL_SCTP: u32 = 132;
    pub const SOL_SMC: u32 = 286;
    pub const SOL_SOCKET: u32 = 1;
    pub const SOL_TCP: u32 = 6;
    pub const SOL_TIPC: u32 = 271;
    pub const SOL_TLS: u32 = 282;
    pub const SOL_UDP: u32 = 17;
    pub const SOL_UDPLITE: u32 = 136;
    pub const SOL_X25: u32 = 262;
    pub const SOL_XDP: u32 = 283;
}

/// `SO_*` socket options.
pub mod so {
    pub const SO_ACCEPTCONN: u32 = 30;
    pub const SO_ATTACH_BPF: u32 = 50;
    pub const SO_ATTACH_FILTER: u32 = 26;
    pub const SO_ATTACH_REUSEPORT_CBPF: u32 = 51;
    pub const SO_ATTACH_REUSEPORT_EBPF: u32 = 52;
    pub const SO_BINDTODEVICE: u32 = 25;
    pub const SO_BINDTOIFINDEX: u32 = 62;
    pub const SO_BPF_EXTENSIONS: u32 = 48;
    pub const SO_BROADCAST: u32 = 6;
    pub const SO_BSDCOMPAT: u32 = 14;
    pub const SO_BUF_LOCK: u32 = 72;
    pub const SO_BUSY_POLL: u32 = 46;
    pub const SO_BUSY_POLL_BUDGET: u32 = 70;
    pub const SO_CNX_ADVICE: u32 = 53;
    pub const SO_COOKIE: u32 = 57;
    pub const SO_DEBUG: u32 = 1;
    pub const SO_DETACH_BPF: u32 = 27;
    pub const SO_DETACH_FILTER: u32 = 27;
    pub const SO_DETACH_REUSEPORT_BPF: u32 = 68;
    pub const SO_DEVMEM_DMABUF: u32 = 79;
    pub const SO_DEVMEM_DONTNEED: u32 = 80;
    pub const SO_DEVMEM_LINEAR: u32 = 78;
    pub const SO_DOMAIN: u32 = 39;
    pub const SO_DONTROUTE: u32 = 5;
    pub const SO_ERROR: u32 = 4;
    pub const SO_GET_FILTER: u32 = 26;
    pub const SO_INCOMING_CPU: u32 = 49;
    pub const SO_INCOMING_NAPI_ID: u32 = 56;
    pub const SO_INQ: u32 = 84;
    pub const SO_KEEPALIVE: u32 = 9;
    pub const SO_LINGER: u32 = 13;
    pub const SO_LOCK_FILTER: u32 = 44;
    pub const SO_MARK: u32 = 36;
    pub const SO_MAX_PACING_RATE: u32 = 47;
    pub const SO_MEMINFO: u32 = 55;
    pub const SO_NETNS_COOKIE: u32 = 71;
    pub const SO_NOFCS: u32 = 43;
    pub const SO_NO_CHECK: u32 = 11;
    pub const SO_OOBINLINE: u32 = 10;
    pub const SO_ORIGINAL_DST: u32 = 80;
    pub const SO_PASSCRED: u32 = 16;
    pub const SO_PASSPIDFD: u32 = 76;
    pub const SO_PASSRIGHTS: u32 = 83;
    pub const SO_PASSSEC: u32 = 34;
    pub const SO_PEEK_OFF: u32 = 42;
    pub const SO_PEERCRED: u32 = 17;
    pub const SO_PEERGROUPS: u32 = 59;
    pub const SO_PEERNAME: u32 = 28;
    pub const SO_PEERPIDFD: u32 = 77;
    pub const SO_PEERSEC: u32 = 31;
    pub const SO_PREFER_BUSY_POLL: u32 = 69;
    pub const SO_PRIORITY: u32 = 12;
    pub const SO_PROTOCOL: u32 = 38;
    pub const SO_RCVBUF: u32 = 8;
    pub const SO_RCVBUFFORCE: u32 = 33;
    pub const SO_RCVLOWAT: u32 = 18;
    pub const SO_RCVMARK: u32 = 75;
    pub const SO_RCVPRIORITY: u32 = 82;
    pub const SO_RCVTIMEO: u32 = 20;
    pub const SO_RCVTIMEO_NEW: u32 = 66;
    pub const SO_RCVTIMEO_OLD: u32 = 20;
    pub const SO_RESERVE_MEM: u32 = 73;
    pub const SO_REUSEADDR: u32 = 2;
    pub const SO_REUSEPORT: u32 = 15;
    pub const SO_RXQ_OVFL: u32 = 40;
    pub const SO_SECURITY_AUTHENTICATION: u32 = 22;
    pub const SO_SECURITY_ENCRYPTION_NETWORK: u32 = 24;
    pub const SO_SECURITY_ENCRYPTION_TRANSPORT: u32 = 23;
    pub const SO_SELECT_ERR_QUEUE: u32 = 45;
    pub const SO_SNDBUF: u32 = 7;
    pub const SO_SNDBUFFORCE: u32 = 32;
    pub const SO_SNDLOWAT: u32 = 19;
    pub const SO_SNDTIMEO: u32 = 21;
    pub const SO_SNDTIMEO_NEW: u32 = 67;
    pub const SO_SNDTIMEO_OLD: u32 = 21;
    pub const SO_TIMESTAMP: u32 = 29;
    pub const SO_TIMESTAMPING: u32 = 37;
    pub const SO_TIMESTAMPING_NEW: u32 = 65;
    pub const SO_TIMESTAMPING_OLD: u32 = 37;
    pub const SO_TIMESTAMPNS: u32 = 35;
    pub const SO_TIMESTAMPNS_NEW: u32 = 64;
    pub const SO_TIMESTAMPNS_OLD: u32 = 35;
    pub const SO_TIMESTAMP_NEW: u32 = 63;
    pub const SO_TIMESTAMP_OLD: u32 = 29;
    pub const SO_TXREHASH: u32 = 74;
    pub const SO_TXTIME: u32 = 61;
    pub const SO_TYPE: u32 = 3;
    pub const SO_WIFI_STATUS: u32 = 41;
    pub const SO_ZEROCOPY: u32 = 60;
}

/// `IPPROTO_*` protocol numbers.
pub mod ipproto {
    pub const IPPROTO_AGGFRAG: u32 = 144;
    pub const IPPROTO_AH: u32 = 51;
    pub const IPPROTO_BEETPH: u32 = 94;
    pub const IPPROTO_COMP: u32 = 108;
    pub const IPPROTO_DCCP: u32 = 33;
    pub const IPPROTO_EGP: u32 = 8;
    pub const IPPROTO_ENCAP: u32 = 98;
    pub const IPPROTO_ESP: u32 = 50;
    pub const IPPROTO_ETHERNET: u32 = 143;
    pub const IPPROTO_GRE: u32 = 47;
    pub const IPPROTO_ICMP: u32 = 1;
    pub const IPPROTO_IDP: u32 = 22;
    pub const IPPROTO_IGMP: u32 = 2;
    pub const IPPROTO_IP: u32 = 0;
    pub const IPPROTO_IPIP: u32 = 4;
    pub const IPPROTO_IPV6: u32 = 41;
    pub const IPPROTO_L2TP: u32 = 115;
    pub const IPPROTO_MAX: u32 = 263;
    pub const IPPROTO_MPLS: u32 = 137;
    pub const IPPROTO_MPTCP: u32 = 262;
    pub const IPPROTO_MTP: u32 = 92;
    pub const IPPROTO_PIM: u32 = 103;
    pub const IPPROTO_PUP: u32 = 12;
    pub const IPPROTO_RAW: u32 = 255;
    pub const IPPROTO_RSVP: u32 = 46;
    pub const IPPROTO_SCTP: u32 = 132;
    pub const IPPROTO_SMC: u32 = 256;
    pub const IPPROTO_TCP: u32 = 6;
    pub const IPPROTO_TP: u32 = 29;
    pub const IPPROTO_UDP: u32 = 17;
    pub const IPPROTO_UDPLITE: u32 = 136;
}

/// `TCP_*` TCP socket options.
pub mod tcp {
    pub const TCP_AO_ADD_KEY: u32 = 38;
    pub const TCP_AO_DEL_KEY: u32 = 39;
    pub const TCP_AO_GET_KEYS: u32 = 41;
    pub const TCP_AO_INFO: u32 = 40;
    pub const TCP_AO_KEYF_EXCLUDE_OPT: u32 = 2;
    pub const TCP_AO_KEYF_IFINDEX: u32 = 1;
    pub const TCP_AO_MAXKEYLEN: u32 = 80;
    pub const TCP_AO_REPAIR: u32 = 42;
    pub const TCP_CC_INFO: u32 = 26;
    pub const TCP_CM_INQ: u32 = 36;
    pub const TCP_CONGESTION: u32 = 13;
    pub const TCP_CORK: u32 = 3;
    pub const TCP_DEFER_ACCEPT: u32 = 9;
    pub const TCP_DELACK_MAX_US: u32 = 46;
    pub const TCP_FASTOPEN: u32 = 23;
    pub const TCP_FASTOPEN_CONNECT: u32 = 30;
    pub const TCP_FASTOPEN_KEY: u32 = 33;
    pub const TCP_FASTOPEN_NO_COOKIE: u32 = 34;
    pub const TCP_INFO: u32 = 11;
    pub const TCP_INQ: u32 = 36;
    pub const TCP_IS_MPTCP: u32 = 43;
    pub const TCP_KEEPCNT: u32 = 6;
    pub const TCP_KEEPIDLE: u32 = 4;
    pub const TCP_KEEPINTVL: u32 = 5;
    pub const TCP_LINGER2: u32 = 8;
    pub const TCP_MAXSEG: u32 = 2;
    pub const TCP_MD5SIG: u32 = 14;
    pub const TCP_MD5SIG_EXT: u32 = 32;
    pub const TCP_MD5SIG_FLAG_IFINDEX: u32 = 2;
    pub const TCP_MD5SIG_FLAG_PREFIX: u32 = 1;
    pub const TCP_MD5SIG_MAXKEYLEN: u32 = 80;
    pub const TCP_MSS_DEFAULT: u32 = 536;
    pub const TCP_MSS_DESIRED: u32 = 1220;
    pub const TCP_NODELAY: u32 = 1;
    pub const TCP_NOTSENT_LOWAT: u32 = 25;
    pub const TCP_QUEUE_SEQ: u32 = 21;
    pub const TCP_QUICKACK: u32 = 12;
    pub const TCP_RECEIVE_ZEROCOPY_FLAG_TLB_CLEAN_HINT: u32 = 1;
    pub const TCP_REPAIR: u32 = 19;
    pub const TCP_REPAIR_OFF: u32 = 0;
    pub const TCP_REPAIR_OFF_NO_WP: i32 = -1;
    pub const TCP_REPAIR_ON: u32 = 1;
    pub const TCP_REPAIR_OPTIONS: u32 = 22;
    pub const TCP_REPAIR_QUEUE: u32 = 20;
    pub const TCP_REPAIR_WINDOW: u32 = 29;
    pub const TCP_RTO_MAX_MS: u32 = 44;
    pub const TCP_RTO_MIN_US: u32 = 45;
    pub const TCP_SAVED_SYN: u32 = 28;
    pub const TCP_SAVE_SYN: u32 = 27;
    pub const TCP_SYNCNT: u32 = 7;
    pub const TCP_THIN_DUPACK: u32 = 17;
    pub const TCP_THIN_LINEAR_TIMEOUTS: u32 = 16;
    pub const TCP_TIMESTAMP: u32 = 24;
    pub const TCP_TX_DELAY: u32 = 37;
    pub const TCP_ULP: u32 = 31;
    pub const TCP_USER_TIMEOUT: u32 = 18;
    pub const TCP_WINDOW_CLAMP: u32 = 10;
    pub const TCP_ZEROCOPY_RECEIVE: u32 = 35;
}
