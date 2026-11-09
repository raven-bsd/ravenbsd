// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Flag values match `linux-raw-sys` and its per-architecture sources.
//!
//! Common values and the x86-64 modules are checked against the `linux-raw-sys` crate on this
//! host. Values that only exist for aarch64 or riscv64 cannot be instantiated on an x86-64
//! host, so those tests read the corresponding `linux-raw-sys` source files instead.

mod support;

use raven_linux_abi::flags;

#[test]
fn open_common_match_linux_raw_sys() {
    assert_eq!(flags::open::O_ACCMODE, linux_raw_sys::general::O_ACCMODE);
    assert_eq!(flags::open::O_APPEND, linux_raw_sys::general::O_APPEND);
    assert_eq!(flags::open::O_CLOEXEC, linux_raw_sys::general::O_CLOEXEC);
    assert_eq!(flags::open::O_CREAT, linux_raw_sys::general::O_CREAT);
    assert_eq!(flags::open::O_DSYNC, linux_raw_sys::general::O_DSYNC);
    assert_eq!(flags::open::O_EXCL, linux_raw_sys::general::O_EXCL);
    assert_eq!(flags::open::O_NDELAY, linux_raw_sys::general::O_NDELAY);
    assert_eq!(flags::open::O_NOATIME, linux_raw_sys::general::O_NOATIME);
    assert_eq!(flags::open::O_NOCTTY, linux_raw_sys::general::O_NOCTTY);
    assert_eq!(flags::open::O_NONBLOCK, linux_raw_sys::general::O_NONBLOCK);
    assert_eq!(flags::open::O_PATH, linux_raw_sys::general::O_PATH);
    assert_eq!(flags::open::O_RDONLY, linux_raw_sys::general::O_RDONLY);
    assert_eq!(flags::open::O_RDWR, linux_raw_sys::general::O_RDWR);
    assert_eq!(flags::open::O_SYNC, linux_raw_sys::general::O_SYNC);
    assert_eq!(flags::open::O_TRUNC, linux_raw_sys::general::O_TRUNC);
    assert_eq!(flags::open::O_WRONLY, linux_raw_sys::general::O_WRONLY);
}

#[test]
fn open_x86_64_matches_linux_raw_sys() {
    assert_eq!(
        flags::open::x86_64::O_DIRECT,
        linux_raw_sys::general::O_DIRECT
    );
    assert_eq!(
        flags::open::x86_64::O_LARGEFILE,
        linux_raw_sys::general::O_LARGEFILE
    );
    assert_eq!(
        flags::open::x86_64::O_DIRECTORY,
        linux_raw_sys::general::O_DIRECTORY
    );
    assert_eq!(
        flags::open::x86_64::O_NOFOLLOW,
        linux_raw_sys::general::O_NOFOLLOW
    );
    assert_eq!(
        flags::open::x86_64::O_TMPFILE,
        linux_raw_sys::general::O_TMPFILE
    );
}

#[test]
fn open_aarch64_matches_source() {
    let source = support::u32_consts("src/aarch64/general.rs", "O_");
    assert_eq!(flags::open::aarch64::O_DIRECT, source["O_DIRECT"]);
    assert_eq!(flags::open::aarch64::O_LARGEFILE, source["O_LARGEFILE"]);
    assert_eq!(flags::open::aarch64::O_DIRECTORY, source["O_DIRECTORY"]);
    assert_eq!(flags::open::aarch64::O_NOFOLLOW, source["O_NOFOLLOW"]);
    assert_eq!(flags::open::aarch64::O_TMPFILE, source["O_TMPFILE"]);
}

#[test]
fn open_riscv64_matches_source() {
    let source = support::u32_consts("src/riscv64/general.rs", "O_");
    assert_eq!(flags::open::riscv64::O_DIRECT, source["O_DIRECT"]);
    assert_eq!(flags::open::riscv64::O_LARGEFILE, source["O_LARGEFILE"]);
    assert_eq!(flags::open::riscv64::O_DIRECTORY, source["O_DIRECTORY"]);
    assert_eq!(flags::open::riscv64::O_NOFOLLOW, source["O_NOFOLLOW"]);
    assert_eq!(flags::open::riscv64::O_TMPFILE, source["O_TMPFILE"]);
}

#[test]
fn at_fd_match_linux_raw_sys() {
    assert_eq!(flags::at_fd::AT_EACCESS, linux_raw_sys::general::AT_EACCESS);
    assert_eq!(
        flags::at_fd::AT_EMPTY_PATH,
        linux_raw_sys::general::AT_EMPTY_PATH
    );
    assert_eq!(
        flags::at_fd::AT_EXECVE_CHECK,
        linux_raw_sys::general::AT_EXECVE_CHECK
    );
    assert_eq!(flags::at_fd::AT_FDCWD, linux_raw_sys::general::AT_FDCWD);
    assert_eq!(
        flags::at_fd::AT_HANDLE_CONNECTABLE,
        linux_raw_sys::general::AT_HANDLE_CONNECTABLE
    );
    assert_eq!(
        flags::at_fd::AT_HANDLE_FID,
        linux_raw_sys::general::AT_HANDLE_FID
    );
    assert_eq!(
        flags::at_fd::AT_HANDLE_MNT_ID_UNIQUE,
        linux_raw_sys::general::AT_HANDLE_MNT_ID_UNIQUE
    );
    assert_eq!(
        flags::at_fd::AT_NO_AUTOMOUNT,
        linux_raw_sys::general::AT_NO_AUTOMOUNT
    );
    assert_eq!(
        flags::at_fd::AT_RECURSIVE,
        linux_raw_sys::general::AT_RECURSIVE
    );
    assert_eq!(
        flags::at_fd::AT_REMOVEDIR,
        linux_raw_sys::general::AT_REMOVEDIR
    );
    assert_eq!(
        flags::at_fd::AT_RENAME_EXCHANGE,
        linux_raw_sys::general::AT_RENAME_EXCHANGE
    );
    assert_eq!(
        flags::at_fd::AT_RENAME_NOREPLACE,
        linux_raw_sys::general::AT_RENAME_NOREPLACE
    );
    assert_eq!(
        flags::at_fd::AT_RENAME_WHITEOUT,
        linux_raw_sys::general::AT_RENAME_WHITEOUT
    );
    assert_eq!(
        flags::at_fd::AT_STATX_DONT_SYNC,
        linux_raw_sys::general::AT_STATX_DONT_SYNC
    );
    assert_eq!(
        flags::at_fd::AT_STATX_FORCE_SYNC,
        linux_raw_sys::general::AT_STATX_FORCE_SYNC
    );
    assert_eq!(
        flags::at_fd::AT_STATX_SYNC_AS_STAT,
        linux_raw_sys::general::AT_STATX_SYNC_AS_STAT
    );
    assert_eq!(
        flags::at_fd::AT_STATX_SYNC_TYPE,
        linux_raw_sys::general::AT_STATX_SYNC_TYPE
    );
    assert_eq!(
        flags::at_fd::AT_SYMLINK_FOLLOW,
        linux_raw_sys::general::AT_SYMLINK_FOLLOW
    );
    assert_eq!(
        flags::at_fd::AT_SYMLINK_NOFOLLOW,
        linux_raw_sys::general::AT_SYMLINK_NOFOLLOW
    );
}

#[test]
fn auxv_common_match_linux_raw_sys() {
    assert_eq!(flags::auxv::AT_BASE, linux_raw_sys::auxvec::AT_BASE);
    assert_eq!(
        flags::auxv::AT_BASE_PLATFORM,
        linux_raw_sys::auxvec::AT_BASE_PLATFORM
    );
    assert_eq!(flags::auxv::AT_CLKTCK, linux_raw_sys::auxvec::AT_CLKTCK);
    assert_eq!(flags::auxv::AT_EGID, linux_raw_sys::auxvec::AT_EGID);
    assert_eq!(flags::auxv::AT_ENTRY, linux_raw_sys::auxvec::AT_ENTRY);
    assert_eq!(flags::auxv::AT_EUID, linux_raw_sys::auxvec::AT_EUID);
    assert_eq!(flags::auxv::AT_EXECFD, linux_raw_sys::auxvec::AT_EXECFD);
    assert_eq!(flags::auxv::AT_EXECFN, linux_raw_sys::auxvec::AT_EXECFN);
    assert_eq!(flags::auxv::AT_FLAGS, linux_raw_sys::auxvec::AT_FLAGS);
    assert_eq!(flags::auxv::AT_GID, linux_raw_sys::auxvec::AT_GID);
    assert_eq!(flags::auxv::AT_HWCAP, linux_raw_sys::auxvec::AT_HWCAP);
    assert_eq!(flags::auxv::AT_HWCAP2, linux_raw_sys::auxvec::AT_HWCAP2);
    assert_eq!(flags::auxv::AT_HWCAP3, linux_raw_sys::auxvec::AT_HWCAP3);
    assert_eq!(flags::auxv::AT_HWCAP4, linux_raw_sys::auxvec::AT_HWCAP4);
    assert_eq!(flags::auxv::AT_IGNORE, linux_raw_sys::auxvec::AT_IGNORE);
    assert_eq!(
        flags::auxv::AT_MINSIGSTKSZ,
        linux_raw_sys::auxvec::AT_MINSIGSTKSZ
    );
    assert_eq!(flags::auxv::AT_NOTELF, linux_raw_sys::auxvec::AT_NOTELF);
    assert_eq!(flags::auxv::AT_NULL, linux_raw_sys::auxvec::AT_NULL);
    assert_eq!(flags::auxv::AT_PAGESZ, linux_raw_sys::auxvec::AT_PAGESZ);
    assert_eq!(flags::auxv::AT_PHDR, linux_raw_sys::auxvec::AT_PHDR);
    assert_eq!(flags::auxv::AT_PHENT, linux_raw_sys::auxvec::AT_PHENT);
    assert_eq!(flags::auxv::AT_PHNUM, linux_raw_sys::auxvec::AT_PHNUM);
    assert_eq!(flags::auxv::AT_PLATFORM, linux_raw_sys::auxvec::AT_PLATFORM);
    assert_eq!(flags::auxv::AT_RANDOM, linux_raw_sys::auxvec::AT_RANDOM);
    assert_eq!(
        flags::auxv::AT_RSEQ_ALIGN,
        linux_raw_sys::auxvec::AT_RSEQ_ALIGN
    );
    assert_eq!(
        flags::auxv::AT_RSEQ_FEATURE_SIZE,
        linux_raw_sys::auxvec::AT_RSEQ_FEATURE_SIZE
    );
    assert_eq!(flags::auxv::AT_SECURE, linux_raw_sys::auxvec::AT_SECURE);
    assert_eq!(flags::auxv::AT_UID, linux_raw_sys::auxvec::AT_UID);
}

#[test]
fn auxv_x86_64_matches_linux_raw_sys() {
    assert_eq!(
        flags::auxv::x86_64::AT_SYSINFO_EHDR,
        linux_raw_sys::auxvec::AT_SYSINFO_EHDR
    );
}

#[test]
fn auxv_riscv64_matches_source() {
    let source = support::u32_consts("src/riscv64/auxvec.rs", "AT_");
    assert_eq!(
        flags::auxv::riscv64::AT_L1D_CACHEGEOMETRY,
        source["AT_L1D_CACHEGEOMETRY"]
    );
    assert_eq!(
        flags::auxv::riscv64::AT_L1D_CACHESIZE,
        source["AT_L1D_CACHESIZE"]
    );
    assert_eq!(
        flags::auxv::riscv64::AT_L1I_CACHEGEOMETRY,
        source["AT_L1I_CACHEGEOMETRY"]
    );
    assert_eq!(
        flags::auxv::riscv64::AT_L1I_CACHESIZE,
        source["AT_L1I_CACHESIZE"]
    );
    assert_eq!(
        flags::auxv::riscv64::AT_L2_CACHEGEOMETRY,
        source["AT_L2_CACHEGEOMETRY"]
    );
    assert_eq!(
        flags::auxv::riscv64::AT_L2_CACHESIZE,
        source["AT_L2_CACHESIZE"]
    );
    assert_eq!(
        flags::auxv::riscv64::AT_L3_CACHEGEOMETRY,
        source["AT_L3_CACHEGEOMETRY"]
    );
    assert_eq!(
        flags::auxv::riscv64::AT_L3_CACHESIZE,
        source["AT_L3_CACHESIZE"]
    );
}

#[test]
fn mmap_common_match_linux_raw_sys() {
    assert_eq!(
        flags::mmap::MAP_ANONYMOUS,
        linux_raw_sys::general::MAP_ANONYMOUS
    );
    assert_eq!(
        flags::mmap::MAP_DENYWRITE,
        linux_raw_sys::general::MAP_DENYWRITE
    );
    assert_eq!(
        flags::mmap::MAP_DROPPABLE,
        linux_raw_sys::general::MAP_DROPPABLE
    );
    assert_eq!(
        flags::mmap::MAP_EXECUTABLE,
        linux_raw_sys::general::MAP_EXECUTABLE
    );
    assert_eq!(flags::mmap::MAP_FILE, linux_raw_sys::general::MAP_FILE);
    assert_eq!(flags::mmap::MAP_FIXED, linux_raw_sys::general::MAP_FIXED);
    assert_eq!(
        flags::mmap::MAP_FIXED_NOREPLACE,
        linux_raw_sys::general::MAP_FIXED_NOREPLACE
    );
    assert_eq!(
        flags::mmap::MAP_GROWSDOWN,
        linux_raw_sys::general::MAP_GROWSDOWN
    );
    assert_eq!(
        flags::mmap::MAP_HUGETLB,
        linux_raw_sys::general::MAP_HUGETLB
    );
    assert_eq!(
        flags::mmap::MAP_HUGE_16GB,
        linux_raw_sys::general::MAP_HUGE_16GB
    );
    assert_eq!(
        flags::mmap::MAP_HUGE_16KB,
        linux_raw_sys::general::MAP_HUGE_16KB
    );
    assert_eq!(
        flags::mmap::MAP_HUGE_16MB,
        linux_raw_sys::general::MAP_HUGE_16MB
    );
    assert_eq!(
        flags::mmap::MAP_HUGE_1GB,
        linux_raw_sys::general::MAP_HUGE_1GB
    );
    assert_eq!(
        flags::mmap::MAP_HUGE_1MB,
        linux_raw_sys::general::MAP_HUGE_1MB
    );
    assert_eq!(
        flags::mmap::MAP_HUGE_256MB,
        linux_raw_sys::general::MAP_HUGE_256MB
    );
    assert_eq!(
        flags::mmap::MAP_HUGE_2GB,
        linux_raw_sys::general::MAP_HUGE_2GB
    );
    assert_eq!(
        flags::mmap::MAP_HUGE_2MB,
        linux_raw_sys::general::MAP_HUGE_2MB
    );
    assert_eq!(
        flags::mmap::MAP_HUGE_32MB,
        linux_raw_sys::general::MAP_HUGE_32MB
    );
    assert_eq!(
        flags::mmap::MAP_HUGE_512KB,
        linux_raw_sys::general::MAP_HUGE_512KB
    );
    assert_eq!(
        flags::mmap::MAP_HUGE_512MB,
        linux_raw_sys::general::MAP_HUGE_512MB
    );
    assert_eq!(
        flags::mmap::MAP_HUGE_64KB,
        linux_raw_sys::general::MAP_HUGE_64KB
    );
    assert_eq!(
        flags::mmap::MAP_HUGE_8MB,
        linux_raw_sys::general::MAP_HUGE_8MB
    );
    assert_eq!(
        flags::mmap::MAP_HUGE_MASK,
        linux_raw_sys::general::MAP_HUGE_MASK
    );
    assert_eq!(
        flags::mmap::MAP_HUGE_SHIFT,
        linux_raw_sys::general::MAP_HUGE_SHIFT
    );
    assert_eq!(flags::mmap::MAP_LOCKED, linux_raw_sys::general::MAP_LOCKED);
    assert_eq!(
        flags::mmap::MAP_NONBLOCK,
        linux_raw_sys::general::MAP_NONBLOCK
    );
    assert_eq!(
        flags::mmap::MAP_NORESERVE,
        linux_raw_sys::general::MAP_NORESERVE
    );
    assert_eq!(
        flags::mmap::MAP_POPULATE,
        linux_raw_sys::general::MAP_POPULATE
    );
    assert_eq!(
        flags::mmap::MAP_PRIVATE,
        linux_raw_sys::general::MAP_PRIVATE
    );
    assert_eq!(flags::mmap::MAP_SHARED, linux_raw_sys::general::MAP_SHARED);
    assert_eq!(
        flags::mmap::MAP_SHARED_VALIDATE,
        linux_raw_sys::general::MAP_SHARED_VALIDATE
    );
    assert_eq!(flags::mmap::MAP_STACK, linux_raw_sys::general::MAP_STACK);
    assert_eq!(flags::mmap::MAP_SYNC, linux_raw_sys::general::MAP_SYNC);
    assert_eq!(flags::mmap::MAP_TYPE, linux_raw_sys::general::MAP_TYPE);
    assert_eq!(
        flags::mmap::MAP_UNINITIALIZED,
        linux_raw_sys::general::MAP_UNINITIALIZED
    );
    assert_eq!(flags::mmap::PROT_EXEC, linux_raw_sys::general::PROT_EXEC);
    assert_eq!(
        flags::mmap::PROT_GROWSDOWN,
        linux_raw_sys::general::PROT_GROWSDOWN
    );
    assert_eq!(
        flags::mmap::PROT_GROWSUP,
        linux_raw_sys::general::PROT_GROWSUP
    );
    assert_eq!(flags::mmap::PROT_NONE, linux_raw_sys::general::PROT_NONE);
    assert_eq!(flags::mmap::PROT_READ, linux_raw_sys::general::PROT_READ);
    assert_eq!(flags::mmap::PROT_SEM, linux_raw_sys::general::PROT_SEM);
    assert_eq!(flags::mmap::PROT_WRITE, linux_raw_sys::general::PROT_WRITE);
}

#[test]
fn mmap_x86_64_matches_linux_raw_sys() {
    assert_eq!(
        flags::mmap::x86_64::MAP_32BIT,
        linux_raw_sys::general::MAP_32BIT
    );
    assert_eq!(
        flags::mmap::x86_64::MAP_ABOVE4G,
        linux_raw_sys::general::MAP_ABOVE4G
    );
}

#[test]
fn mmap_aarch64_matches_source() {
    let source = support::u32_consts("src/aarch64/general.rs", "PROT_");
    assert_eq!(flags::mmap::aarch64::PROT_BTI, source["PROT_BTI"]);
    assert_eq!(flags::mmap::aarch64::PROT_MTE, source["PROT_MTE"]);
}

#[test]
fn clone_match_linux_raw_sys() {
    assert_eq!(
        flags::clone::CLONE_ARGS_SIZE_VER0,
        u64::from(linux_raw_sys::general::CLONE_ARGS_SIZE_VER0)
    );
    assert_eq!(
        flags::clone::CLONE_ARGS_SIZE_VER1,
        u64::from(linux_raw_sys::general::CLONE_ARGS_SIZE_VER1)
    );
    assert_eq!(
        flags::clone::CLONE_ARGS_SIZE_VER2,
        u64::from(linux_raw_sys::general::CLONE_ARGS_SIZE_VER2)
    );
    assert_eq!(
        flags::clone::CLONE_CHILD_CLEARTID,
        u64::from(linux_raw_sys::general::CLONE_CHILD_CLEARTID)
    );
    assert_eq!(
        flags::clone::CLONE_CHILD_SETTID,
        u64::from(linux_raw_sys::general::CLONE_CHILD_SETTID)
    );
    assert_eq!(
        flags::clone::CLONE_CLEAR_SIGHAND,
        linux_raw_sys::general::CLONE_CLEAR_SIGHAND
    );
    assert_eq!(
        flags::clone::CLONE_DETACHED,
        u64::from(linux_raw_sys::general::CLONE_DETACHED)
    );
    assert_eq!(
        flags::clone::CLONE_FILES,
        u64::from(linux_raw_sys::general::CLONE_FILES)
    );
    assert_eq!(
        flags::clone::CLONE_FS,
        u64::from(linux_raw_sys::general::CLONE_FS)
    );
    assert_eq!(
        flags::clone::CLONE_INTO_CGROUP,
        linux_raw_sys::general::CLONE_INTO_CGROUP
    );
    assert_eq!(
        flags::clone::CLONE_IO,
        u64::from(linux_raw_sys::general::CLONE_IO)
    );
    assert_eq!(
        flags::clone::CLONE_NEWCGROUP,
        u64::from(linux_raw_sys::general::CLONE_NEWCGROUP)
    );
    assert_eq!(
        flags::clone::CLONE_NEWIPC,
        u64::from(linux_raw_sys::general::CLONE_NEWIPC)
    );
    assert_eq!(
        flags::clone::CLONE_NEWNET,
        u64::from(linux_raw_sys::general::CLONE_NEWNET)
    );
    assert_eq!(
        flags::clone::CLONE_NEWNS,
        u64::from(linux_raw_sys::general::CLONE_NEWNS)
    );
    assert_eq!(
        flags::clone::CLONE_NEWPID,
        u64::from(linux_raw_sys::general::CLONE_NEWPID)
    );
    assert_eq!(
        flags::clone::CLONE_NEWTIME,
        u64::from(linux_raw_sys::general::CLONE_NEWTIME)
    );
    assert_eq!(
        flags::clone::CLONE_NEWUSER,
        u64::from(linux_raw_sys::general::CLONE_NEWUSER)
    );
    assert_eq!(
        flags::clone::CLONE_NEWUTS,
        u64::from(linux_raw_sys::general::CLONE_NEWUTS)
    );
    assert_eq!(
        flags::clone::CLONE_PARENT,
        u64::from(linux_raw_sys::general::CLONE_PARENT)
    );
    assert_eq!(
        flags::clone::CLONE_PARENT_SETTID,
        u64::from(linux_raw_sys::general::CLONE_PARENT_SETTID)
    );
    assert_eq!(
        flags::clone::CLONE_PIDFD,
        u64::from(linux_raw_sys::general::CLONE_PIDFD)
    );
    assert_eq!(
        flags::clone::CLONE_PTRACE,
        u64::from(linux_raw_sys::general::CLONE_PTRACE)
    );
    assert_eq!(
        flags::clone::CLONE_SETTLS,
        u64::from(linux_raw_sys::general::CLONE_SETTLS)
    );
    assert_eq!(
        flags::clone::CLONE_SIGHAND,
        u64::from(linux_raw_sys::general::CLONE_SIGHAND)
    );
    assert_eq!(
        flags::clone::CLONE_SYSVSEM,
        u64::from(linux_raw_sys::general::CLONE_SYSVSEM)
    );
    assert_eq!(
        flags::clone::CLONE_THREAD,
        u64::from(linux_raw_sys::general::CLONE_THREAD)
    );
    assert_eq!(
        flags::clone::CLONE_UNTRACED,
        u64::from(linux_raw_sys::general::CLONE_UNTRACED)
    );
    assert_eq!(
        flags::clone::CLONE_VFORK,
        u64::from(linux_raw_sys::general::CLONE_VFORK)
    );
    assert_eq!(
        flags::clone::CLONE_VM,
        u64::from(linux_raw_sys::general::CLONE_VM)
    );
}

#[test]
fn signal_common_match_linux_raw_sys() {
    assert_eq!(flags::signal::SIGABRT, linux_raw_sys::general::SIGABRT);
    assert_eq!(flags::signal::SIGALRM, linux_raw_sys::general::SIGALRM);
    assert_eq!(flags::signal::SIGBUS, linux_raw_sys::general::SIGBUS);
    assert_eq!(flags::signal::SIGCHLD, linux_raw_sys::general::SIGCHLD);
    assert_eq!(flags::signal::SIGCONT, linux_raw_sys::general::SIGCONT);
    assert_eq!(
        flags::signal::SIGEV_MAX_SIZE,
        linux_raw_sys::general::SIGEV_MAX_SIZE
    );
    assert_eq!(
        flags::signal::SIGEV_NONE,
        linux_raw_sys::general::SIGEV_NONE
    );
    assert_eq!(
        flags::signal::SIGEV_SIGNAL,
        linux_raw_sys::general::SIGEV_SIGNAL
    );
    assert_eq!(
        flags::signal::SIGEV_THREAD,
        linux_raw_sys::general::SIGEV_THREAD
    );
    assert_eq!(
        flags::signal::SIGEV_THREAD_ID,
        linux_raw_sys::general::SIGEV_THREAD_ID
    );
    assert_eq!(flags::signal::SIGFPE, linux_raw_sys::general::SIGFPE);
    assert_eq!(flags::signal::SIGHUP, linux_raw_sys::general::SIGHUP);
    assert_eq!(flags::signal::SIGILL, linux_raw_sys::general::SIGILL);
    assert_eq!(flags::signal::SIGINT, linux_raw_sys::general::SIGINT);
    assert_eq!(flags::signal::SIGIO, linux_raw_sys::general::SIGIO);
    assert_eq!(flags::signal::SIGIOT, linux_raw_sys::general::SIGIOT);
    assert_eq!(flags::signal::SIGKILL, linux_raw_sys::general::SIGKILL);
    assert_eq!(flags::signal::SIGPIPE, linux_raw_sys::general::SIGPIPE);
    assert_eq!(flags::signal::SIGPOLL, linux_raw_sys::general::SIGPOLL);
    assert_eq!(flags::signal::SIGPROF, linux_raw_sys::general::SIGPROF);
    assert_eq!(flags::signal::SIGPWR, linux_raw_sys::general::SIGPWR);
    assert_eq!(flags::signal::SIGQUIT, linux_raw_sys::general::SIGQUIT);
    assert_eq!(flags::signal::SIGRTMIN, linux_raw_sys::general::SIGRTMIN);
    assert_eq!(flags::signal::SIGSEGV, linux_raw_sys::general::SIGSEGV);
    assert_eq!(flags::signal::SIGSTKFLT, linux_raw_sys::general::SIGSTKFLT);
    assert_eq!(flags::signal::SIGSTOP, linux_raw_sys::general::SIGSTOP);
    assert_eq!(flags::signal::SIGSYS, linux_raw_sys::general::SIGSYS);
    assert_eq!(flags::signal::SIGTERM, linux_raw_sys::general::SIGTERM);
    assert_eq!(flags::signal::SIGTRAP, linux_raw_sys::general::SIGTRAP);
    assert_eq!(flags::signal::SIGTSTP, linux_raw_sys::general::SIGTSTP);
    assert_eq!(flags::signal::SIGTTIN, linux_raw_sys::general::SIGTTIN);
    assert_eq!(flags::signal::SIGTTOU, linux_raw_sys::general::SIGTTOU);
    assert_eq!(flags::signal::SIGUNUSED, linux_raw_sys::general::SIGUNUSED);
    assert_eq!(flags::signal::SIGURG, linux_raw_sys::general::SIGURG);
    assert_eq!(flags::signal::SIGUSR1, linux_raw_sys::general::SIGUSR1);
    assert_eq!(flags::signal::SIGUSR2, linux_raw_sys::general::SIGUSR2);
    assert_eq!(flags::signal::SIGVTALRM, linux_raw_sys::general::SIGVTALRM);
    assert_eq!(flags::signal::SIGWINCH, linux_raw_sys::general::SIGWINCH);
    assert_eq!(flags::signal::SIGXCPU, linux_raw_sys::general::SIGXCPU);
    assert_eq!(flags::signal::SIGXFSZ, linux_raw_sys::general::SIGXFSZ);
    assert_eq!(flags::signal::SIG_BLOCK, linux_raw_sys::general::SIG_BLOCK);
    assert_eq!(
        flags::signal::SIG_SETMASK,
        linux_raw_sys::general::SIG_SETMASK
    );
    assert_eq!(
        flags::signal::SIG_UNBLOCK,
        linux_raw_sys::general::SIG_UNBLOCK
    );
}

#[test]
fn signal_x86_64_matches_linux_raw_sys() {
    assert_eq!(
        flags::signal::x86_64::SIGSTKSZ,
        linux_raw_sys::general::SIGSTKSZ
    );
}

#[test]
fn signal_aarch64_matches_source() {
    let source = support::u32_consts("src/aarch64/general.rs", "SIG");
    assert_eq!(flags::signal::aarch64::SIGSTKSZ, source["SIGSTKSZ"]);
    assert_eq!(flags::signal::aarch64::SIGRTMAX, source["SIGRTMAX"]);
}

#[test]
fn signal_riscv64_matches_source() {
    let source = support::u32_consts("src/riscv64/general.rs", "SIG");
    assert_eq!(flags::signal::riscv64::SIGSTKSZ, source["SIGSTKSZ"]);
    assert_eq!(flags::signal::riscv64::SIGRTMAX, source["SIGRTMAX"]);
}

#[test]
fn fcntl_match_linux_raw_sys() {
    assert_eq!(
        flags::fcntl::F_ADD_SEALS,
        linux_raw_sys::general::F_ADD_SEALS
    );
    assert_eq!(flags::fcntl::F_CANCELLK, linux_raw_sys::general::F_CANCELLK);
    assert_eq!(
        flags::fcntl::F_CREATED_QUERY,
        linux_raw_sys::general::F_CREATED_QUERY
    );
    assert_eq!(flags::fcntl::F_DUPFD, linux_raw_sys::general::F_DUPFD);
    assert_eq!(
        flags::fcntl::F_DUPFD_CLOEXEC,
        linux_raw_sys::general::F_DUPFD_CLOEXEC
    );
    assert_eq!(
        flags::fcntl::F_DUPFD_QUERY,
        linux_raw_sys::general::F_DUPFD_QUERY
    );
    assert_eq!(flags::fcntl::F_EXLCK, linux_raw_sys::general::F_EXLCK);
    assert_eq!(flags::fcntl::F_GETFD, linux_raw_sys::general::F_GETFD);
    assert_eq!(flags::fcntl::F_GETFL, linux_raw_sys::general::F_GETFL);
    assert_eq!(flags::fcntl::F_GETLEASE, linux_raw_sys::general::F_GETLEASE);
    assert_eq!(flags::fcntl::F_GETLK, linux_raw_sys::general::F_GETLK);
    assert_eq!(flags::fcntl::F_GETOWN, linux_raw_sys::general::F_GETOWN);
    assert_eq!(
        flags::fcntl::F_GETOWNER_UIDS,
        linux_raw_sys::general::F_GETOWNER_UIDS
    );
    assert_eq!(
        flags::fcntl::F_GETOWN_EX,
        linux_raw_sys::general::F_GETOWN_EX
    );
    assert_eq!(
        flags::fcntl::F_GETPIPE_SZ,
        linux_raw_sys::general::F_GETPIPE_SZ
    );
    assert_eq!(flags::fcntl::F_GETSIG, linux_raw_sys::general::F_GETSIG);
    assert_eq!(
        flags::fcntl::F_GET_FILE_RW_HINT,
        linux_raw_sys::general::F_GET_FILE_RW_HINT
    );
    assert_eq!(
        flags::fcntl::F_GET_RW_HINT,
        linux_raw_sys::general::F_GET_RW_HINT
    );
    assert_eq!(
        flags::fcntl::F_GET_SEALS,
        linux_raw_sys::general::F_GET_SEALS
    );
    assert_eq!(
        flags::fcntl::F_LINUX_SPECIFIC_BASE,
        linux_raw_sys::general::F_LINUX_SPECIFIC_BASE
    );
    assert_eq!(flags::fcntl::F_NOTIFY, linux_raw_sys::general::F_NOTIFY);
    assert_eq!(
        flags::fcntl::F_OFD_GETLK,
        linux_raw_sys::general::F_OFD_GETLK
    );
    assert_eq!(
        flags::fcntl::F_OFD_SETLK,
        linux_raw_sys::general::F_OFD_SETLK
    );
    assert_eq!(
        flags::fcntl::F_OFD_SETLKW,
        linux_raw_sys::general::F_OFD_SETLKW
    );
    assert_eq!(flags::fcntl::F_OK, linux_raw_sys::general::F_OK);
    assert_eq!(
        flags::fcntl::F_OWNER_PGRP,
        linux_raw_sys::general::F_OWNER_PGRP
    );
    assert_eq!(
        flags::fcntl::F_OWNER_PID,
        linux_raw_sys::general::F_OWNER_PID
    );
    assert_eq!(
        flags::fcntl::F_OWNER_TID,
        linux_raw_sys::general::F_OWNER_TID
    );
    assert_eq!(flags::fcntl::F_RDLCK, linux_raw_sys::general::F_RDLCK);
    assert_eq!(
        flags::fcntl::F_SEAL_EXEC,
        linux_raw_sys::general::F_SEAL_EXEC
    );
    assert_eq!(
        flags::fcntl::F_SEAL_FUTURE_WRITE,
        linux_raw_sys::general::F_SEAL_FUTURE_WRITE
    );
    assert_eq!(
        flags::fcntl::F_SEAL_GROW,
        linux_raw_sys::general::F_SEAL_GROW
    );
    assert_eq!(
        flags::fcntl::F_SEAL_SEAL,
        linux_raw_sys::general::F_SEAL_SEAL
    );
    assert_eq!(
        flags::fcntl::F_SEAL_SHRINK,
        linux_raw_sys::general::F_SEAL_SHRINK
    );
    assert_eq!(
        flags::fcntl::F_SEAL_WRITE,
        linux_raw_sys::general::F_SEAL_WRITE
    );
    assert_eq!(flags::fcntl::F_SETFD, linux_raw_sys::general::F_SETFD);
    assert_eq!(flags::fcntl::F_SETFL, linux_raw_sys::general::F_SETFL);
    assert_eq!(flags::fcntl::F_SETLEASE, linux_raw_sys::general::F_SETLEASE);
    assert_eq!(flags::fcntl::F_SETLK, linux_raw_sys::general::F_SETLK);
    assert_eq!(flags::fcntl::F_SETLKW, linux_raw_sys::general::F_SETLKW);
    assert_eq!(flags::fcntl::F_SETOWN, linux_raw_sys::general::F_SETOWN);
    assert_eq!(
        flags::fcntl::F_SETOWN_EX,
        linux_raw_sys::general::F_SETOWN_EX
    );
    assert_eq!(
        flags::fcntl::F_SETPIPE_SZ,
        linux_raw_sys::general::F_SETPIPE_SZ
    );
    assert_eq!(flags::fcntl::F_SETSIG, linux_raw_sys::general::F_SETSIG);
    assert_eq!(
        flags::fcntl::F_SET_FILE_RW_HINT,
        linux_raw_sys::general::F_SET_FILE_RW_HINT
    );
    assert_eq!(
        flags::fcntl::F_SET_RW_HINT,
        linux_raw_sys::general::F_SET_RW_HINT
    );
    assert_eq!(flags::fcntl::F_SHLCK, linux_raw_sys::general::F_SHLCK);
    assert_eq!(flags::fcntl::F_UNLCK, linux_raw_sys::general::F_UNLCK);
    assert_eq!(flags::fcntl::F_WRLCK, linux_raw_sys::general::F_WRLCK);
}

#[test]
fn futex_match_linux_raw_sys() {
    assert_eq!(flags::futex::FUTEX_32, linux_raw_sys::general::FUTEX_32);
    assert_eq!(
        flags::futex::FUTEX_BITSET_MATCH_ANY,
        linux_raw_sys::general::FUTEX_BITSET_MATCH_ANY
    );
    assert_eq!(
        flags::futex::FUTEX_CLOCK_REALTIME,
        linux_raw_sys::general::FUTEX_CLOCK_REALTIME
    );
    assert_eq!(
        flags::futex::FUTEX_CMD_MASK,
        linux_raw_sys::general::FUTEX_CMD_MASK
    );
    assert_eq!(
        flags::futex::FUTEX_CMP_REQUEUE,
        linux_raw_sys::general::FUTEX_CMP_REQUEUE
    );
    assert_eq!(
        flags::futex::FUTEX_CMP_REQUEUE_PI,
        linux_raw_sys::general::FUTEX_CMP_REQUEUE_PI
    );
    assert_eq!(
        flags::futex::FUTEX_CMP_REQUEUE_PI_PRIVATE,
        linux_raw_sys::general::FUTEX_CMP_REQUEUE_PI_PRIVATE
    );
    assert_eq!(
        flags::futex::FUTEX_CMP_REQUEUE_PRIVATE,
        linux_raw_sys::general::FUTEX_CMP_REQUEUE_PRIVATE
    );
    assert_eq!(flags::futex::FUTEX_FD, linux_raw_sys::general::FUTEX_FD);
    assert_eq!(
        flags::futex::FUTEX_LOCK_PI,
        linux_raw_sys::general::FUTEX_LOCK_PI
    );
    assert_eq!(
        flags::futex::FUTEX_LOCK_PI2,
        linux_raw_sys::general::FUTEX_LOCK_PI2
    );
    assert_eq!(
        flags::futex::FUTEX_LOCK_PI2_PRIVATE,
        linux_raw_sys::general::FUTEX_LOCK_PI2_PRIVATE
    );
    assert_eq!(
        flags::futex::FUTEX_LOCK_PI_PRIVATE,
        linux_raw_sys::general::FUTEX_LOCK_PI_PRIVATE
    );
    assert_eq!(
        flags::futex::FUTEX_NO_NODE,
        linux_raw_sys::general::FUTEX_NO_NODE
    );
    assert_eq!(
        flags::futex::FUTEX_OP_ADD,
        linux_raw_sys::general::FUTEX_OP_ADD
    );
    assert_eq!(
        flags::futex::FUTEX_OP_ANDN,
        linux_raw_sys::general::FUTEX_OP_ANDN
    );
    assert_eq!(
        flags::futex::FUTEX_OP_CMP_EQ,
        linux_raw_sys::general::FUTEX_OP_CMP_EQ
    );
    assert_eq!(
        flags::futex::FUTEX_OP_CMP_GE,
        linux_raw_sys::general::FUTEX_OP_CMP_GE
    );
    assert_eq!(
        flags::futex::FUTEX_OP_CMP_GT,
        linux_raw_sys::general::FUTEX_OP_CMP_GT
    );
    assert_eq!(
        flags::futex::FUTEX_OP_CMP_LE,
        linux_raw_sys::general::FUTEX_OP_CMP_LE
    );
    assert_eq!(
        flags::futex::FUTEX_OP_CMP_LT,
        linux_raw_sys::general::FUTEX_OP_CMP_LT
    );
    assert_eq!(
        flags::futex::FUTEX_OP_CMP_NE,
        linux_raw_sys::general::FUTEX_OP_CMP_NE
    );
    assert_eq!(
        flags::futex::FUTEX_OP_OPARG_SHIFT,
        linux_raw_sys::general::FUTEX_OP_OPARG_SHIFT
    );
    assert_eq!(
        flags::futex::FUTEX_OP_OR,
        linux_raw_sys::general::FUTEX_OP_OR
    );
    assert_eq!(
        flags::futex::FUTEX_OP_SET,
        linux_raw_sys::general::FUTEX_OP_SET
    );
    assert_eq!(
        flags::futex::FUTEX_OP_XOR,
        linux_raw_sys::general::FUTEX_OP_XOR
    );
    assert_eq!(
        flags::futex::FUTEX_OWNER_DIED,
        linux_raw_sys::general::FUTEX_OWNER_DIED
    );
    assert_eq!(
        flags::futex::FUTEX_PRIVATE_FLAG,
        linux_raw_sys::general::FUTEX_PRIVATE_FLAG
    );
    assert_eq!(
        flags::futex::FUTEX_REQUEUE,
        linux_raw_sys::general::FUTEX_REQUEUE
    );
    assert_eq!(
        flags::futex::FUTEX_REQUEUE_PRIVATE,
        linux_raw_sys::general::FUTEX_REQUEUE_PRIVATE
    );
    assert_eq!(
        flags::futex::FUTEX_TID_MASK,
        linux_raw_sys::general::FUTEX_TID_MASK
    );
    assert_eq!(
        flags::futex::FUTEX_TRYLOCK_PI,
        linux_raw_sys::general::FUTEX_TRYLOCK_PI
    );
    assert_eq!(
        flags::futex::FUTEX_TRYLOCK_PI_PRIVATE,
        linux_raw_sys::general::FUTEX_TRYLOCK_PI_PRIVATE
    );
    assert_eq!(
        flags::futex::FUTEX_UNLOCK_PI,
        linux_raw_sys::general::FUTEX_UNLOCK_PI
    );
    assert_eq!(
        flags::futex::FUTEX_UNLOCK_PI_PRIVATE,
        linux_raw_sys::general::FUTEX_UNLOCK_PI_PRIVATE
    );
    assert_eq!(flags::futex::FUTEX_WAIT, linux_raw_sys::general::FUTEX_WAIT);
    assert_eq!(
        flags::futex::FUTEX_WAITERS,
        linux_raw_sys::general::FUTEX_WAITERS
    );
    assert_eq!(
        flags::futex::FUTEX_WAITV_MAX,
        linux_raw_sys::general::FUTEX_WAITV_MAX
    );
    assert_eq!(
        flags::futex::FUTEX_WAIT_BITSET,
        linux_raw_sys::general::FUTEX_WAIT_BITSET
    );
    assert_eq!(
        flags::futex::FUTEX_WAIT_BITSET_PRIVATE,
        linux_raw_sys::general::FUTEX_WAIT_BITSET_PRIVATE
    );
    assert_eq!(
        flags::futex::FUTEX_WAIT_PRIVATE,
        linux_raw_sys::general::FUTEX_WAIT_PRIVATE
    );
    assert_eq!(
        flags::futex::FUTEX_WAIT_REQUEUE_PI,
        linux_raw_sys::general::FUTEX_WAIT_REQUEUE_PI
    );
    assert_eq!(
        flags::futex::FUTEX_WAIT_REQUEUE_PI_PRIVATE,
        linux_raw_sys::general::FUTEX_WAIT_REQUEUE_PI_PRIVATE
    );
    assert_eq!(flags::futex::FUTEX_WAKE, linux_raw_sys::general::FUTEX_WAKE);
    assert_eq!(
        flags::futex::FUTEX_WAKE_BITSET,
        linux_raw_sys::general::FUTEX_WAKE_BITSET
    );
    assert_eq!(
        flags::futex::FUTEX_WAKE_BITSET_PRIVATE,
        linux_raw_sys::general::FUTEX_WAKE_BITSET_PRIVATE
    );
    assert_eq!(
        flags::futex::FUTEX_WAKE_OP,
        linux_raw_sys::general::FUTEX_WAKE_OP
    );
    assert_eq!(
        flags::futex::FUTEX_WAKE_OP_PRIVATE,
        linux_raw_sys::general::FUTEX_WAKE_OP_PRIVATE
    );
    assert_eq!(
        flags::futex::FUTEX_WAKE_PRIVATE,
        linux_raw_sys::general::FUTEX_WAKE_PRIVATE
    );
}

#[test]
fn epoll_match_linux_raw_sys() {
    assert_eq!(flags::epoll::EPOLLERR, linux_raw_sys::general::EPOLLERR);
    assert_eq!(flags::epoll::EPOLLET, linux_raw_sys::general::EPOLLET);
    assert_eq!(
        flags::epoll::EPOLLEXCLUSIVE,
        linux_raw_sys::general::EPOLLEXCLUSIVE
    );
    assert_eq!(flags::epoll::EPOLLHUP, linux_raw_sys::general::EPOLLHUP);
    assert_eq!(flags::epoll::EPOLLIN, linux_raw_sys::general::EPOLLIN);
    assert_eq!(flags::epoll::EPOLLMSG, linux_raw_sys::general::EPOLLMSG);
    assert_eq!(flags::epoll::EPOLLNVAL, linux_raw_sys::general::EPOLLNVAL);
    assert_eq!(
        flags::epoll::EPOLLONESHOT,
        linux_raw_sys::general::EPOLLONESHOT
    );
    assert_eq!(flags::epoll::EPOLLOUT, linux_raw_sys::general::EPOLLOUT);
    assert_eq!(flags::epoll::EPOLLPRI, linux_raw_sys::general::EPOLLPRI);
    assert_eq!(
        flags::epoll::EPOLLRDBAND,
        linux_raw_sys::general::EPOLLRDBAND
    );
    assert_eq!(flags::epoll::EPOLLRDHUP, linux_raw_sys::general::EPOLLRDHUP);
    assert_eq!(
        flags::epoll::EPOLLRDNORM,
        linux_raw_sys::general::EPOLLRDNORM
    );
    assert_eq!(
        flags::epoll::EPOLLWAKEUP,
        linux_raw_sys::general::EPOLLWAKEUP
    );
    assert_eq!(
        flags::epoll::EPOLLWRBAND,
        linux_raw_sys::general::EPOLLWRBAND
    );
    assert_eq!(
        flags::epoll::EPOLLWRNORM,
        linux_raw_sys::general::EPOLLWRNORM
    );
    assert_eq!(
        flags::epoll::EPOLL_CLOEXEC,
        linux_raw_sys::general::EPOLL_CLOEXEC
    );
    assert_eq!(
        flags::epoll::EPOLL_CTL_ADD,
        linux_raw_sys::general::EPOLL_CTL_ADD
    );
    assert_eq!(
        flags::epoll::EPOLL_CTL_DEL,
        linux_raw_sys::general::EPOLL_CTL_DEL
    );
    assert_eq!(
        flags::epoll::EPOLL_CTL_MOD,
        linux_raw_sys::general::EPOLL_CTL_MOD
    );
    assert_eq!(
        flags::epoll::EPOLL_IOC_TYPE,
        linux_raw_sys::general::EPOLL_IOC_TYPE
    );
}

#[test]
fn clock_match_linux_raw_sys() {
    assert_eq!(flags::clock::CLOCK_AUX, linux_raw_sys::general::CLOCK_AUX);
    assert_eq!(
        flags::clock::CLOCK_AUX_LAST,
        linux_raw_sys::general::CLOCK_AUX_LAST
    );
    assert_eq!(
        flags::clock::CLOCK_BOOTTIME,
        linux_raw_sys::general::CLOCK_BOOTTIME
    );
    assert_eq!(
        flags::clock::CLOCK_BOOTTIME_ALARM,
        linux_raw_sys::general::CLOCK_BOOTTIME_ALARM
    );
    assert_eq!(
        flags::clock::CLOCK_MONOTONIC,
        linux_raw_sys::general::CLOCK_MONOTONIC
    );
    assert_eq!(
        flags::clock::CLOCK_MONOTONIC_COARSE,
        linux_raw_sys::general::CLOCK_MONOTONIC_COARSE
    );
    assert_eq!(
        flags::clock::CLOCK_MONOTONIC_RAW,
        linux_raw_sys::general::CLOCK_MONOTONIC_RAW
    );
    assert_eq!(
        flags::clock::CLOCK_PROCESS_CPUTIME_ID,
        linux_raw_sys::general::CLOCK_PROCESS_CPUTIME_ID
    );
    assert_eq!(
        flags::clock::CLOCK_REALTIME,
        linux_raw_sys::general::CLOCK_REALTIME
    );
    assert_eq!(
        flags::clock::CLOCK_REALTIME_ALARM,
        linux_raw_sys::general::CLOCK_REALTIME_ALARM
    );
    assert_eq!(
        flags::clock::CLOCK_REALTIME_COARSE,
        linux_raw_sys::general::CLOCK_REALTIME_COARSE
    );
    assert_eq!(
        flags::clock::CLOCK_SGI_CYCLE,
        linux_raw_sys::general::CLOCK_SGI_CYCLE
    );
    assert_eq!(flags::clock::CLOCK_TAI, linux_raw_sys::general::CLOCK_TAI);
    assert_eq!(
        flags::clock::CLOCK_THREAD_CPUTIME_ID,
        linux_raw_sys::general::CLOCK_THREAD_CPUTIME_ID
    );
}

#[test]
fn poll_match_linux_raw_sys() {
    assert_eq!(flags::poll::POLLERR, linux_raw_sys::general::POLLERR);
    assert_eq!(flags::poll::POLLHUP, linux_raw_sys::general::POLLHUP);
    assert_eq!(flags::poll::POLLIN, linux_raw_sys::general::POLLIN);
    assert_eq!(flags::poll::POLLMSG, linux_raw_sys::general::POLLMSG);
    assert_eq!(flags::poll::POLLNVAL, linux_raw_sys::general::POLLNVAL);
    assert_eq!(flags::poll::POLLOUT, linux_raw_sys::general::POLLOUT);
    assert_eq!(flags::poll::POLLPRI, linux_raw_sys::general::POLLPRI);
    assert_eq!(flags::poll::POLLRDBAND, linux_raw_sys::general::POLLRDBAND);
    assert_eq!(flags::poll::POLLRDHUP, linux_raw_sys::general::POLLRDHUP);
    assert_eq!(flags::poll::POLLRDNORM, linux_raw_sys::general::POLLRDNORM);
    assert_eq!(flags::poll::POLLREMOVE, linux_raw_sys::general::POLLREMOVE);
    assert_eq!(flags::poll::POLLWRBAND, linux_raw_sys::general::POLLWRBAND);
    assert_eq!(flags::poll::POLLWRNORM, linux_raw_sys::general::POLLWRNORM);
    assert_eq!(flags::poll::POLL_ERR, linux_raw_sys::general::POLL_ERR);
    assert_eq!(flags::poll::POLL_HUP, linux_raw_sys::general::POLL_HUP);
    assert_eq!(flags::poll::POLL_IN, linux_raw_sys::general::POLL_IN);
    assert_eq!(flags::poll::POLL_MSG, linux_raw_sys::general::POLL_MSG);
    assert_eq!(flags::poll::POLL_OUT, linux_raw_sys::general::POLL_OUT);
    assert_eq!(flags::poll::POLL_PRI, linux_raw_sys::general::POLL_PRI);
}

#[test]
fn stat_match_linux_raw_sys() {
    assert_eq!(flags::stat::S_IFBLK, linux_raw_sys::general::S_IFBLK);
    assert_eq!(flags::stat::S_IFCHR, linux_raw_sys::general::S_IFCHR);
    assert_eq!(flags::stat::S_IFDIR, linux_raw_sys::general::S_IFDIR);
    assert_eq!(flags::stat::S_IFIFO, linux_raw_sys::general::S_IFIFO);
    assert_eq!(flags::stat::S_IFLNK, linux_raw_sys::general::S_IFLNK);
    assert_eq!(flags::stat::S_IFMT, linux_raw_sys::general::S_IFMT);
    assert_eq!(flags::stat::S_IFREG, linux_raw_sys::general::S_IFREG);
    assert_eq!(flags::stat::S_IFSOCK, linux_raw_sys::general::S_IFSOCK);
    assert_eq!(flags::stat::S_IRGRP, linux_raw_sys::general::S_IRGRP);
    assert_eq!(flags::stat::S_IROTH, linux_raw_sys::general::S_IROTH);
    assert_eq!(flags::stat::S_IRUSR, linux_raw_sys::general::S_IRUSR);
    assert_eq!(flags::stat::S_IRWXG, linux_raw_sys::general::S_IRWXG);
    assert_eq!(flags::stat::S_IRWXO, linux_raw_sys::general::S_IRWXO);
    assert_eq!(flags::stat::S_IRWXU, linux_raw_sys::general::S_IRWXU);
    assert_eq!(flags::stat::S_ISGID, linux_raw_sys::general::S_ISGID);
    assert_eq!(flags::stat::S_ISUID, linux_raw_sys::general::S_ISUID);
    assert_eq!(flags::stat::S_ISVTX, linux_raw_sys::general::S_ISVTX);
    assert_eq!(flags::stat::S_IWGRP, linux_raw_sys::general::S_IWGRP);
    assert_eq!(flags::stat::S_IWOTH, linux_raw_sys::general::S_IWOTH);
    assert_eq!(flags::stat::S_IWUSR, linux_raw_sys::general::S_IWUSR);
    assert_eq!(flags::stat::S_IXGRP, linux_raw_sys::general::S_IXGRP);
    assert_eq!(flags::stat::S_IXOTH, linux_raw_sys::general::S_IXOTH);
    assert_eq!(flags::stat::S_IXUSR, linux_raw_sys::general::S_IXUSR);
}

#[test]
fn sock_match_linux_raw_sys() {
    assert_eq!(
        flags::sock::SOCK_BUF_LOCK_MASK,
        linux_raw_sys::net::SOCK_BUF_LOCK_MASK
    );
    assert_eq!(flags::sock::SOCK_DGRAM, linux_raw_sys::net::SOCK_DGRAM);
    assert_eq!(flags::sock::SOCK_RAW, linux_raw_sys::net::SOCK_RAW);
    assert_eq!(
        flags::sock::SOCK_RCVBUF_LOCK,
        linux_raw_sys::net::SOCK_RCVBUF_LOCK
    );
    assert_eq!(flags::sock::SOCK_RDM, linux_raw_sys::net::SOCK_RDM);
    assert_eq!(
        flags::sock::SOCK_SEQPACKET,
        linux_raw_sys::net::SOCK_SEQPACKET
    );
    assert_eq!(
        flags::sock::SOCK_SNDBUF_LOCK,
        linux_raw_sys::net::SOCK_SNDBUF_LOCK
    );
    assert_eq!(flags::sock::SOCK_STREAM, linux_raw_sys::net::SOCK_STREAM);
    assert_eq!(
        flags::sock::SOCK_TXREHASH_DEFAULT,
        linux_raw_sys::net::SOCK_TXREHASH_DEFAULT
    );
    assert_eq!(
        flags::sock::SOCK_TXREHASH_DISABLED,
        linux_raw_sys::net::SOCK_TXREHASH_DISABLED
    );
    assert_eq!(
        flags::sock::SOCK_TXREHASH_ENABLED,
        linux_raw_sys::net::SOCK_TXREHASH_ENABLED
    );
}

#[test]
fn af_match_linux_raw_sys() {
    assert_eq!(flags::af::AF_ALG, linux_raw_sys::net::AF_ALG);
    assert_eq!(flags::af::AF_APPLETALK, linux_raw_sys::net::AF_APPLETALK);
    assert_eq!(flags::af::AF_ASH, linux_raw_sys::net::AF_ASH);
    assert_eq!(flags::af::AF_ATMPVC, linux_raw_sys::net::AF_ATMPVC);
    assert_eq!(flags::af::AF_ATMSVC, linux_raw_sys::net::AF_ATMSVC);
    assert_eq!(flags::af::AF_AX25, linux_raw_sys::net::AF_AX25);
    assert_eq!(flags::af::AF_BLUETOOTH, linux_raw_sys::net::AF_BLUETOOTH);
    assert_eq!(flags::af::AF_BRIDGE, linux_raw_sys::net::AF_BRIDGE);
    assert_eq!(flags::af::AF_CAIF, linux_raw_sys::net::AF_CAIF);
    assert_eq!(flags::af::AF_CAN, linux_raw_sys::net::AF_CAN);
    assert_eq!(flags::af::AF_ECONET, linux_raw_sys::net::AF_ECONET);
    assert_eq!(flags::af::AF_IEEE802154, linux_raw_sys::net::AF_IEEE802154);
    assert_eq!(flags::af::AF_INET, linux_raw_sys::net::AF_INET);
    assert_eq!(flags::af::AF_INET6, linux_raw_sys::net::AF_INET6);
    assert_eq!(flags::af::AF_IPX, linux_raw_sys::net::AF_IPX);
    assert_eq!(flags::af::AF_IRDA, linux_raw_sys::net::AF_IRDA);
    assert_eq!(flags::af::AF_ISDN, linux_raw_sys::net::AF_ISDN);
    assert_eq!(flags::af::AF_IUCV, linux_raw_sys::net::AF_IUCV);
    assert_eq!(flags::af::AF_KCM, linux_raw_sys::net::AF_KCM);
    assert_eq!(flags::af::AF_KEY, linux_raw_sys::net::AF_KEY);
    assert_eq!(flags::af::AF_LLC, linux_raw_sys::net::AF_LLC);
    assert_eq!(flags::af::AF_MAX, linux_raw_sys::net::AF_MAX);
    assert_eq!(flags::af::AF_MCTP, linux_raw_sys::net::AF_MCTP);
    assert_eq!(flags::af::AF_NETBEUI, linux_raw_sys::net::AF_NETBEUI);
    assert_eq!(flags::af::AF_NETLINK, linux_raw_sys::net::AF_NETLINK);
    assert_eq!(flags::af::AF_NETROM, linux_raw_sys::net::AF_NETROM);
    assert_eq!(flags::af::AF_NFC, linux_raw_sys::net::AF_NFC);
    assert_eq!(flags::af::AF_PACKET, linux_raw_sys::net::AF_PACKET);
    assert_eq!(flags::af::AF_PHONET, linux_raw_sys::net::AF_PHONET);
    assert_eq!(flags::af::AF_PPPOX, linux_raw_sys::net::AF_PPPOX);
    assert_eq!(flags::af::AF_QIPCRTR, linux_raw_sys::net::AF_QIPCRTR);
    assert_eq!(flags::af::AF_RDS, linux_raw_sys::net::AF_RDS);
    assert_eq!(flags::af::AF_ROSE, linux_raw_sys::net::AF_ROSE);
    assert_eq!(flags::af::AF_RXRPC, linux_raw_sys::net::AF_RXRPC);
    assert_eq!(flags::af::AF_SECURITY, linux_raw_sys::net::AF_SECURITY);
    assert_eq!(flags::af::AF_SMC, linux_raw_sys::net::AF_SMC);
    assert_eq!(flags::af::AF_SNA, linux_raw_sys::net::AF_SNA);
    assert_eq!(flags::af::AF_TIPC, linux_raw_sys::net::AF_TIPC);
    assert_eq!(flags::af::AF_UNIX, linux_raw_sys::net::AF_UNIX);
    assert_eq!(flags::af::AF_UNSPEC, linux_raw_sys::net::AF_UNSPEC);
    assert_eq!(flags::af::AF_VSOCK, linux_raw_sys::net::AF_VSOCK);
    assert_eq!(flags::af::AF_WANPIPE, linux_raw_sys::net::AF_WANPIPE);
    assert_eq!(flags::af::AF_X25, linux_raw_sys::net::AF_X25);
    assert_eq!(flags::af::AF_XDP, linux_raw_sys::net::AF_XDP);
}

#[test]
fn sol_match_linux_raw_sys() {
    assert_eq!(flags::sol::SOL_AAL, linux_raw_sys::net::SOL_AAL);
    assert_eq!(flags::sol::SOL_ALG, linux_raw_sys::net::SOL_ALG);
    assert_eq!(flags::sol::SOL_ATALK, linux_raw_sys::net::SOL_ATALK);
    assert_eq!(flags::sol::SOL_ATM, linux_raw_sys::net::SOL_ATM);
    assert_eq!(flags::sol::SOL_AX25, linux_raw_sys::net::SOL_AX25);
    assert_eq!(flags::sol::SOL_BLUETOOTH, linux_raw_sys::net::SOL_BLUETOOTH);
    assert_eq!(flags::sol::SOL_CAIF, linux_raw_sys::net::SOL_CAIF);
    assert_eq!(flags::sol::SOL_DCCP, linux_raw_sys::net::SOL_DCCP);
    assert_eq!(flags::sol::SOL_DECNET, linux_raw_sys::net::SOL_DECNET);
    assert_eq!(flags::sol::SOL_ICMPV6, linux_raw_sys::net::SOL_ICMPV6);
    assert_eq!(flags::sol::SOL_IP, linux_raw_sys::net::SOL_IP);
    assert_eq!(flags::sol::SOL_IPV6, linux_raw_sys::net::SOL_IPV6);
    assert_eq!(flags::sol::SOL_IPX, linux_raw_sys::net::SOL_IPX);
    assert_eq!(flags::sol::SOL_IRDA, linux_raw_sys::net::SOL_IRDA);
    assert_eq!(flags::sol::SOL_IUCV, linux_raw_sys::net::SOL_IUCV);
    assert_eq!(flags::sol::SOL_KCM, linux_raw_sys::net::SOL_KCM);
    assert_eq!(flags::sol::SOL_LLC, linux_raw_sys::net::SOL_LLC);
    assert_eq!(flags::sol::SOL_MCTP, linux_raw_sys::net::SOL_MCTP);
    assert_eq!(flags::sol::SOL_MPTCP, linux_raw_sys::net::SOL_MPTCP);
    assert_eq!(flags::sol::SOL_NETBEUI, linux_raw_sys::net::SOL_NETBEUI);
    assert_eq!(flags::sol::SOL_NETLINK, linux_raw_sys::net::SOL_NETLINK);
    assert_eq!(flags::sol::SOL_NETROM, linux_raw_sys::net::SOL_NETROM);
    assert_eq!(flags::sol::SOL_NFC, linux_raw_sys::net::SOL_NFC);
    assert_eq!(flags::sol::SOL_PACKET, linux_raw_sys::net::SOL_PACKET);
    assert_eq!(flags::sol::SOL_PNPIPE, linux_raw_sys::net::SOL_PNPIPE);
    assert_eq!(flags::sol::SOL_PPPOL2TP, linux_raw_sys::net::SOL_PPPOL2TP);
    assert_eq!(flags::sol::SOL_RAW, linux_raw_sys::net::SOL_RAW);
    assert_eq!(flags::sol::SOL_RDS, linux_raw_sys::net::SOL_RDS);
    assert_eq!(flags::sol::SOL_ROSE, linux_raw_sys::net::SOL_ROSE);
    assert_eq!(flags::sol::SOL_RXRPC, linux_raw_sys::net::SOL_RXRPC);
    assert_eq!(flags::sol::SOL_SCTP, linux_raw_sys::net::SOL_SCTP);
    assert_eq!(flags::sol::SOL_SMC, linux_raw_sys::net::SOL_SMC);
    assert_eq!(flags::sol::SOL_SOCKET, linux_raw_sys::net::SOL_SOCKET);
    assert_eq!(flags::sol::SOL_TCP, linux_raw_sys::net::SOL_TCP);
    assert_eq!(flags::sol::SOL_TIPC, linux_raw_sys::net::SOL_TIPC);
    assert_eq!(flags::sol::SOL_TLS, linux_raw_sys::net::SOL_TLS);
    assert_eq!(flags::sol::SOL_UDP, linux_raw_sys::net::SOL_UDP);
    assert_eq!(flags::sol::SOL_UDPLITE, linux_raw_sys::net::SOL_UDPLITE);
    assert_eq!(flags::sol::SOL_X25, linux_raw_sys::net::SOL_X25);
    assert_eq!(flags::sol::SOL_XDP, linux_raw_sys::net::SOL_XDP);
}

#[test]
fn so_match_linux_raw_sys() {
    assert_eq!(flags::so::SO_ACCEPTCONN, linux_raw_sys::net::SO_ACCEPTCONN);
    assert_eq!(flags::so::SO_ATTACH_BPF, linux_raw_sys::net::SO_ATTACH_BPF);
    assert_eq!(
        flags::so::SO_ATTACH_FILTER,
        linux_raw_sys::net::SO_ATTACH_FILTER
    );
    assert_eq!(
        flags::so::SO_ATTACH_REUSEPORT_CBPF,
        linux_raw_sys::net::SO_ATTACH_REUSEPORT_CBPF
    );
    assert_eq!(
        flags::so::SO_ATTACH_REUSEPORT_EBPF,
        linux_raw_sys::net::SO_ATTACH_REUSEPORT_EBPF
    );
    assert_eq!(
        flags::so::SO_BINDTODEVICE,
        linux_raw_sys::net::SO_BINDTODEVICE
    );
    assert_eq!(
        flags::so::SO_BINDTOIFINDEX,
        linux_raw_sys::net::SO_BINDTOIFINDEX
    );
    assert_eq!(
        flags::so::SO_BPF_EXTENSIONS,
        linux_raw_sys::net::SO_BPF_EXTENSIONS
    );
    assert_eq!(flags::so::SO_BROADCAST, linux_raw_sys::net::SO_BROADCAST);
    assert_eq!(flags::so::SO_BSDCOMPAT, linux_raw_sys::net::SO_BSDCOMPAT);
    assert_eq!(flags::so::SO_BUF_LOCK, linux_raw_sys::net::SO_BUF_LOCK);
    assert_eq!(flags::so::SO_BUSY_POLL, linux_raw_sys::net::SO_BUSY_POLL);
    assert_eq!(
        flags::so::SO_BUSY_POLL_BUDGET,
        linux_raw_sys::net::SO_BUSY_POLL_BUDGET
    );
    assert_eq!(flags::so::SO_CNX_ADVICE, linux_raw_sys::net::SO_CNX_ADVICE);
    assert_eq!(flags::so::SO_COOKIE, linux_raw_sys::net::SO_COOKIE);
    assert_eq!(flags::so::SO_DEBUG, linux_raw_sys::net::SO_DEBUG);
    assert_eq!(flags::so::SO_DETACH_BPF, linux_raw_sys::net::SO_DETACH_BPF);
    assert_eq!(
        flags::so::SO_DETACH_FILTER,
        linux_raw_sys::net::SO_DETACH_FILTER
    );
    assert_eq!(
        flags::so::SO_DETACH_REUSEPORT_BPF,
        linux_raw_sys::net::SO_DETACH_REUSEPORT_BPF
    );
    assert_eq!(
        flags::so::SO_DEVMEM_DMABUF,
        linux_raw_sys::net::SO_DEVMEM_DMABUF
    );
    assert_eq!(
        flags::so::SO_DEVMEM_DONTNEED,
        linux_raw_sys::net::SO_DEVMEM_DONTNEED
    );
    assert_eq!(
        flags::so::SO_DEVMEM_LINEAR,
        linux_raw_sys::net::SO_DEVMEM_LINEAR
    );
    assert_eq!(flags::so::SO_DOMAIN, linux_raw_sys::net::SO_DOMAIN);
    assert_eq!(flags::so::SO_DONTROUTE, linux_raw_sys::net::SO_DONTROUTE);
    assert_eq!(flags::so::SO_ERROR, linux_raw_sys::net::SO_ERROR);
    assert_eq!(flags::so::SO_GET_FILTER, linux_raw_sys::net::SO_GET_FILTER);
    assert_eq!(
        flags::so::SO_INCOMING_CPU,
        linux_raw_sys::net::SO_INCOMING_CPU
    );
    assert_eq!(
        flags::so::SO_INCOMING_NAPI_ID,
        linux_raw_sys::net::SO_INCOMING_NAPI_ID
    );
    assert_eq!(flags::so::SO_INQ, linux_raw_sys::net::SO_INQ);
    assert_eq!(flags::so::SO_KEEPALIVE, linux_raw_sys::net::SO_KEEPALIVE);
    assert_eq!(flags::so::SO_LINGER, linux_raw_sys::net::SO_LINGER);
    assert_eq!(
        flags::so::SO_LOCK_FILTER,
        linux_raw_sys::net::SO_LOCK_FILTER
    );
    assert_eq!(flags::so::SO_MARK, linux_raw_sys::net::SO_MARK);
    assert_eq!(
        flags::so::SO_MAX_PACING_RATE,
        linux_raw_sys::net::SO_MAX_PACING_RATE
    );
    assert_eq!(flags::so::SO_MEMINFO, linux_raw_sys::net::SO_MEMINFO);
    assert_eq!(
        flags::so::SO_NETNS_COOKIE,
        linux_raw_sys::net::SO_NETNS_COOKIE
    );
    assert_eq!(flags::so::SO_NOFCS, linux_raw_sys::net::SO_NOFCS);
    assert_eq!(flags::so::SO_NO_CHECK, linux_raw_sys::net::SO_NO_CHECK);
    assert_eq!(flags::so::SO_OOBINLINE, linux_raw_sys::net::SO_OOBINLINE);
    assert_eq!(
        flags::so::SO_ORIGINAL_DST,
        linux_raw_sys::net::SO_ORIGINAL_DST
    );
    assert_eq!(flags::so::SO_PASSCRED, linux_raw_sys::net::SO_PASSCRED);
    assert_eq!(flags::so::SO_PASSPIDFD, linux_raw_sys::net::SO_PASSPIDFD);
    assert_eq!(flags::so::SO_PASSRIGHTS, linux_raw_sys::net::SO_PASSRIGHTS);
    assert_eq!(flags::so::SO_PASSSEC, linux_raw_sys::net::SO_PASSSEC);
    assert_eq!(flags::so::SO_PEEK_OFF, linux_raw_sys::net::SO_PEEK_OFF);
    assert_eq!(flags::so::SO_PEERCRED, linux_raw_sys::net::SO_PEERCRED);
    assert_eq!(flags::so::SO_PEERGROUPS, linux_raw_sys::net::SO_PEERGROUPS);
    assert_eq!(flags::so::SO_PEERNAME, linux_raw_sys::net::SO_PEERNAME);
    assert_eq!(flags::so::SO_PEERPIDFD, linux_raw_sys::net::SO_PEERPIDFD);
    assert_eq!(flags::so::SO_PEERSEC, linux_raw_sys::net::SO_PEERSEC);
    assert_eq!(
        flags::so::SO_PREFER_BUSY_POLL,
        linux_raw_sys::net::SO_PREFER_BUSY_POLL
    );
    assert_eq!(flags::so::SO_PRIORITY, linux_raw_sys::net::SO_PRIORITY);
    assert_eq!(flags::so::SO_PROTOCOL, linux_raw_sys::net::SO_PROTOCOL);
    assert_eq!(flags::so::SO_RCVBUF, linux_raw_sys::net::SO_RCVBUF);
    assert_eq!(
        flags::so::SO_RCVBUFFORCE,
        linux_raw_sys::net::SO_RCVBUFFORCE
    );
    assert_eq!(flags::so::SO_RCVLOWAT, linux_raw_sys::net::SO_RCVLOWAT);
    assert_eq!(flags::so::SO_RCVMARK, linux_raw_sys::net::SO_RCVMARK);
    assert_eq!(
        flags::so::SO_RCVPRIORITY,
        linux_raw_sys::net::SO_RCVPRIORITY
    );
    assert_eq!(flags::so::SO_RCVTIMEO, linux_raw_sys::net::SO_RCVTIMEO);
    assert_eq!(
        flags::so::SO_RCVTIMEO_NEW,
        linux_raw_sys::net::SO_RCVTIMEO_NEW
    );
    assert_eq!(
        flags::so::SO_RCVTIMEO_OLD,
        linux_raw_sys::net::SO_RCVTIMEO_OLD
    );
    assert_eq!(
        flags::so::SO_RESERVE_MEM,
        linux_raw_sys::net::SO_RESERVE_MEM
    );
    assert_eq!(flags::so::SO_REUSEADDR, linux_raw_sys::net::SO_REUSEADDR);
    assert_eq!(flags::so::SO_REUSEPORT, linux_raw_sys::net::SO_REUSEPORT);
    assert_eq!(flags::so::SO_RXQ_OVFL, linux_raw_sys::net::SO_RXQ_OVFL);
    assert_eq!(
        flags::so::SO_SECURITY_AUTHENTICATION,
        linux_raw_sys::net::SO_SECURITY_AUTHENTICATION
    );
    assert_eq!(
        flags::so::SO_SECURITY_ENCRYPTION_NETWORK,
        linux_raw_sys::net::SO_SECURITY_ENCRYPTION_NETWORK
    );
    assert_eq!(
        flags::so::SO_SECURITY_ENCRYPTION_TRANSPORT,
        linux_raw_sys::net::SO_SECURITY_ENCRYPTION_TRANSPORT
    );
    assert_eq!(
        flags::so::SO_SELECT_ERR_QUEUE,
        linux_raw_sys::net::SO_SELECT_ERR_QUEUE
    );
    assert_eq!(flags::so::SO_SNDBUF, linux_raw_sys::net::SO_SNDBUF);
    assert_eq!(
        flags::so::SO_SNDBUFFORCE,
        linux_raw_sys::net::SO_SNDBUFFORCE
    );
    assert_eq!(flags::so::SO_SNDLOWAT, linux_raw_sys::net::SO_SNDLOWAT);
    assert_eq!(flags::so::SO_SNDTIMEO, linux_raw_sys::net::SO_SNDTIMEO);
    assert_eq!(
        flags::so::SO_SNDTIMEO_NEW,
        linux_raw_sys::net::SO_SNDTIMEO_NEW
    );
    assert_eq!(
        flags::so::SO_SNDTIMEO_OLD,
        linux_raw_sys::net::SO_SNDTIMEO_OLD
    );
    assert_eq!(flags::so::SO_TIMESTAMP, linux_raw_sys::net::SO_TIMESTAMP);
    assert_eq!(
        flags::so::SO_TIMESTAMPING,
        linux_raw_sys::net::SO_TIMESTAMPING
    );
    assert_eq!(
        flags::so::SO_TIMESTAMPING_NEW,
        linux_raw_sys::net::SO_TIMESTAMPING_NEW
    );
    assert_eq!(
        flags::so::SO_TIMESTAMPING_OLD,
        linux_raw_sys::net::SO_TIMESTAMPING_OLD
    );
    assert_eq!(
        flags::so::SO_TIMESTAMPNS,
        linux_raw_sys::net::SO_TIMESTAMPNS
    );
    assert_eq!(
        flags::so::SO_TIMESTAMPNS_NEW,
        linux_raw_sys::net::SO_TIMESTAMPNS_NEW
    );
    assert_eq!(
        flags::so::SO_TIMESTAMPNS_OLD,
        linux_raw_sys::net::SO_TIMESTAMPNS_OLD
    );
    assert_eq!(
        flags::so::SO_TIMESTAMP_NEW,
        linux_raw_sys::net::SO_TIMESTAMP_NEW
    );
    assert_eq!(
        flags::so::SO_TIMESTAMP_OLD,
        linux_raw_sys::net::SO_TIMESTAMP_OLD
    );
    assert_eq!(flags::so::SO_TXREHASH, linux_raw_sys::net::SO_TXREHASH);
    assert_eq!(flags::so::SO_TXTIME, linux_raw_sys::net::SO_TXTIME);
    assert_eq!(flags::so::SO_TYPE, linux_raw_sys::net::SO_TYPE);
    assert_eq!(
        flags::so::SO_WIFI_STATUS,
        linux_raw_sys::net::SO_WIFI_STATUS
    );
    assert_eq!(flags::so::SO_ZEROCOPY, linux_raw_sys::net::SO_ZEROCOPY);
}

#[test]
fn ipproto_match_linux_raw_sys() {
    assert_eq!(
        flags::ipproto::IPPROTO_AGGFRAG,
        linux_raw_sys::net::_bindgen_ty_1::IPPROTO_AGGFRAG as u32
    );
    assert_eq!(
        flags::ipproto::IPPROTO_AH,
        linux_raw_sys::net::_bindgen_ty_1::IPPROTO_AH as u32
    );
    assert_eq!(
        flags::ipproto::IPPROTO_BEETPH,
        linux_raw_sys::net::_bindgen_ty_1::IPPROTO_BEETPH as u32
    );
    assert_eq!(
        flags::ipproto::IPPROTO_COMP,
        linux_raw_sys::net::_bindgen_ty_1::IPPROTO_COMP as u32
    );
    assert_eq!(
        flags::ipproto::IPPROTO_DCCP,
        linux_raw_sys::net::_bindgen_ty_1::IPPROTO_DCCP as u32
    );
    assert_eq!(
        flags::ipproto::IPPROTO_EGP,
        linux_raw_sys::net::_bindgen_ty_1::IPPROTO_EGP as u32
    );
    assert_eq!(
        flags::ipproto::IPPROTO_ENCAP,
        linux_raw_sys::net::_bindgen_ty_1::IPPROTO_ENCAP as u32
    );
    assert_eq!(
        flags::ipproto::IPPROTO_ESP,
        linux_raw_sys::net::_bindgen_ty_1::IPPROTO_ESP as u32
    );
    assert_eq!(
        flags::ipproto::IPPROTO_ETHERNET,
        linux_raw_sys::net::_bindgen_ty_1::IPPROTO_ETHERNET as u32
    );
    assert_eq!(
        flags::ipproto::IPPROTO_GRE,
        linux_raw_sys::net::_bindgen_ty_1::IPPROTO_GRE as u32
    );
    assert_eq!(
        flags::ipproto::IPPROTO_ICMP,
        linux_raw_sys::net::_bindgen_ty_1::IPPROTO_ICMP as u32
    );
    assert_eq!(
        flags::ipproto::IPPROTO_IDP,
        linux_raw_sys::net::_bindgen_ty_1::IPPROTO_IDP as u32
    );
    assert_eq!(
        flags::ipproto::IPPROTO_IGMP,
        linux_raw_sys::net::_bindgen_ty_1::IPPROTO_IGMP as u32
    );
    assert_eq!(
        flags::ipproto::IPPROTO_IP,
        linux_raw_sys::net::_bindgen_ty_1::IPPROTO_IP as u32
    );
    assert_eq!(
        flags::ipproto::IPPROTO_IPIP,
        linux_raw_sys::net::_bindgen_ty_1::IPPROTO_IPIP as u32
    );
    assert_eq!(
        flags::ipproto::IPPROTO_IPV6,
        linux_raw_sys::net::_bindgen_ty_1::IPPROTO_IPV6 as u32
    );
    assert_eq!(
        flags::ipproto::IPPROTO_L2TP,
        linux_raw_sys::net::_bindgen_ty_1::IPPROTO_L2TP as u32
    );
    assert_eq!(
        flags::ipproto::IPPROTO_MAX,
        linux_raw_sys::net::_bindgen_ty_1::IPPROTO_MAX as u32
    );
    assert_eq!(
        flags::ipproto::IPPROTO_MPLS,
        linux_raw_sys::net::_bindgen_ty_1::IPPROTO_MPLS as u32
    );
    assert_eq!(
        flags::ipproto::IPPROTO_MPTCP,
        linux_raw_sys::net::_bindgen_ty_1::IPPROTO_MPTCP as u32
    );
    assert_eq!(
        flags::ipproto::IPPROTO_MTP,
        linux_raw_sys::net::_bindgen_ty_1::IPPROTO_MTP as u32
    );
    assert_eq!(
        flags::ipproto::IPPROTO_PIM,
        linux_raw_sys::net::_bindgen_ty_1::IPPROTO_PIM as u32
    );
    assert_eq!(
        flags::ipproto::IPPROTO_PUP,
        linux_raw_sys::net::_bindgen_ty_1::IPPROTO_PUP as u32
    );
    assert_eq!(
        flags::ipproto::IPPROTO_RAW,
        linux_raw_sys::net::_bindgen_ty_1::IPPROTO_RAW as u32
    );
    assert_eq!(
        flags::ipproto::IPPROTO_RSVP,
        linux_raw_sys::net::_bindgen_ty_1::IPPROTO_RSVP as u32
    );
    assert_eq!(
        flags::ipproto::IPPROTO_SCTP,
        linux_raw_sys::net::_bindgen_ty_1::IPPROTO_SCTP as u32
    );
    assert_eq!(
        flags::ipproto::IPPROTO_SMC,
        linux_raw_sys::net::_bindgen_ty_1::IPPROTO_SMC as u32
    );
    assert_eq!(
        flags::ipproto::IPPROTO_TCP,
        linux_raw_sys::net::_bindgen_ty_1::IPPROTO_TCP as u32
    );
    assert_eq!(
        flags::ipproto::IPPROTO_TP,
        linux_raw_sys::net::_bindgen_ty_1::IPPROTO_TP as u32
    );
    assert_eq!(
        flags::ipproto::IPPROTO_UDP,
        linux_raw_sys::net::_bindgen_ty_1::IPPROTO_UDP as u32
    );
    assert_eq!(
        flags::ipproto::IPPROTO_UDPLITE,
        linux_raw_sys::net::_bindgen_ty_1::IPPROTO_UDPLITE as u32
    );
}

#[test]
fn tcp_match_linux_raw_sys() {
    assert_eq!(
        flags::tcp::TCP_AO_ADD_KEY,
        linux_raw_sys::net::TCP_AO_ADD_KEY
    );
    assert_eq!(
        flags::tcp::TCP_AO_DEL_KEY,
        linux_raw_sys::net::TCP_AO_DEL_KEY
    );
    assert_eq!(
        flags::tcp::TCP_AO_GET_KEYS,
        linux_raw_sys::net::TCP_AO_GET_KEYS
    );
    assert_eq!(flags::tcp::TCP_AO_INFO, linux_raw_sys::net::TCP_AO_INFO);
    assert_eq!(
        flags::tcp::TCP_AO_KEYF_EXCLUDE_OPT,
        linux_raw_sys::net::TCP_AO_KEYF_EXCLUDE_OPT
    );
    assert_eq!(
        flags::tcp::TCP_AO_KEYF_IFINDEX,
        linux_raw_sys::net::TCP_AO_KEYF_IFINDEX
    );
    assert_eq!(
        flags::tcp::TCP_AO_MAXKEYLEN,
        linux_raw_sys::net::TCP_AO_MAXKEYLEN
    );
    assert_eq!(flags::tcp::TCP_AO_REPAIR, linux_raw_sys::net::TCP_AO_REPAIR);
    assert_eq!(flags::tcp::TCP_CC_INFO, linux_raw_sys::net::TCP_CC_INFO);
    assert_eq!(flags::tcp::TCP_CM_INQ, linux_raw_sys::net::TCP_CM_INQ);
    assert_eq!(
        flags::tcp::TCP_CONGESTION,
        linux_raw_sys::net::TCP_CONGESTION
    );
    assert_eq!(flags::tcp::TCP_CORK, linux_raw_sys::net::TCP_CORK);
    assert_eq!(
        flags::tcp::TCP_DEFER_ACCEPT,
        linux_raw_sys::net::TCP_DEFER_ACCEPT
    );
    assert_eq!(
        flags::tcp::TCP_DELACK_MAX_US,
        linux_raw_sys::net::TCP_DELACK_MAX_US
    );
    assert_eq!(flags::tcp::TCP_FASTOPEN, linux_raw_sys::net::TCP_FASTOPEN);
    assert_eq!(
        flags::tcp::TCP_FASTOPEN_CONNECT,
        linux_raw_sys::net::TCP_FASTOPEN_CONNECT
    );
    assert_eq!(
        flags::tcp::TCP_FASTOPEN_KEY,
        linux_raw_sys::net::TCP_FASTOPEN_KEY
    );
    assert_eq!(
        flags::tcp::TCP_FASTOPEN_NO_COOKIE,
        linux_raw_sys::net::TCP_FASTOPEN_NO_COOKIE
    );
    assert_eq!(flags::tcp::TCP_INFO, linux_raw_sys::net::TCP_INFO);
    assert_eq!(flags::tcp::TCP_INQ, linux_raw_sys::net::TCP_INQ);
    assert_eq!(flags::tcp::TCP_IS_MPTCP, linux_raw_sys::net::TCP_IS_MPTCP);
    assert_eq!(flags::tcp::TCP_KEEPCNT, linux_raw_sys::net::TCP_KEEPCNT);
    assert_eq!(flags::tcp::TCP_KEEPIDLE, linux_raw_sys::net::TCP_KEEPIDLE);
    assert_eq!(flags::tcp::TCP_KEEPINTVL, linux_raw_sys::net::TCP_KEEPINTVL);
    assert_eq!(flags::tcp::TCP_LINGER2, linux_raw_sys::net::TCP_LINGER2);
    assert_eq!(flags::tcp::TCP_MAXSEG, linux_raw_sys::net::TCP_MAXSEG);
    assert_eq!(flags::tcp::TCP_MD5SIG, linux_raw_sys::net::TCP_MD5SIG);
    assert_eq!(
        flags::tcp::TCP_MD5SIG_EXT,
        linux_raw_sys::net::TCP_MD5SIG_EXT
    );
    assert_eq!(
        flags::tcp::TCP_MD5SIG_FLAG_IFINDEX,
        linux_raw_sys::net::TCP_MD5SIG_FLAG_IFINDEX
    );
    assert_eq!(
        flags::tcp::TCP_MD5SIG_FLAG_PREFIX,
        linux_raw_sys::net::TCP_MD5SIG_FLAG_PREFIX
    );
    assert_eq!(
        flags::tcp::TCP_MD5SIG_MAXKEYLEN,
        linux_raw_sys::net::TCP_MD5SIG_MAXKEYLEN
    );
    assert_eq!(
        flags::tcp::TCP_MSS_DEFAULT,
        linux_raw_sys::net::TCP_MSS_DEFAULT
    );
    assert_eq!(
        flags::tcp::TCP_MSS_DESIRED,
        linux_raw_sys::net::TCP_MSS_DESIRED
    );
    assert_eq!(flags::tcp::TCP_NODELAY, linux_raw_sys::net::TCP_NODELAY);
    assert_eq!(
        flags::tcp::TCP_NOTSENT_LOWAT,
        linux_raw_sys::net::TCP_NOTSENT_LOWAT
    );
    assert_eq!(flags::tcp::TCP_QUEUE_SEQ, linux_raw_sys::net::TCP_QUEUE_SEQ);
    assert_eq!(flags::tcp::TCP_QUICKACK, linux_raw_sys::net::TCP_QUICKACK);
    assert_eq!(
        flags::tcp::TCP_RECEIVE_ZEROCOPY_FLAG_TLB_CLEAN_HINT,
        linux_raw_sys::net::TCP_RECEIVE_ZEROCOPY_FLAG_TLB_CLEAN_HINT
    );
    assert_eq!(flags::tcp::TCP_REPAIR, linux_raw_sys::net::TCP_REPAIR);
    assert_eq!(
        flags::tcp::TCP_REPAIR_OFF,
        linux_raw_sys::net::TCP_REPAIR_OFF
    );
    assert_eq!(
        flags::tcp::TCP_REPAIR_OFF_NO_WP,
        linux_raw_sys::net::TCP_REPAIR_OFF_NO_WP
    );
    assert_eq!(flags::tcp::TCP_REPAIR_ON, linux_raw_sys::net::TCP_REPAIR_ON);
    assert_eq!(
        flags::tcp::TCP_REPAIR_OPTIONS,
        linux_raw_sys::net::TCP_REPAIR_OPTIONS
    );
    assert_eq!(
        flags::tcp::TCP_REPAIR_QUEUE,
        linux_raw_sys::net::TCP_REPAIR_QUEUE
    );
    assert_eq!(
        flags::tcp::TCP_REPAIR_WINDOW,
        linux_raw_sys::net::TCP_REPAIR_WINDOW
    );
    assert_eq!(
        flags::tcp::TCP_RTO_MAX_MS,
        linux_raw_sys::net::TCP_RTO_MAX_MS
    );
    assert_eq!(
        flags::tcp::TCP_RTO_MIN_US,
        linux_raw_sys::net::TCP_RTO_MIN_US
    );
    assert_eq!(flags::tcp::TCP_SAVED_SYN, linux_raw_sys::net::TCP_SAVED_SYN);
    assert_eq!(flags::tcp::TCP_SAVE_SYN, linux_raw_sys::net::TCP_SAVE_SYN);
    assert_eq!(flags::tcp::TCP_SYNCNT, linux_raw_sys::net::TCP_SYNCNT);
    assert_eq!(
        flags::tcp::TCP_THIN_DUPACK,
        linux_raw_sys::net::TCP_THIN_DUPACK
    );
    assert_eq!(
        flags::tcp::TCP_THIN_LINEAR_TIMEOUTS,
        linux_raw_sys::net::TCP_THIN_LINEAR_TIMEOUTS
    );
    assert_eq!(flags::tcp::TCP_TIMESTAMP, linux_raw_sys::net::TCP_TIMESTAMP);
    assert_eq!(flags::tcp::TCP_TX_DELAY, linux_raw_sys::net::TCP_TX_DELAY);
    assert_eq!(flags::tcp::TCP_ULP, linux_raw_sys::net::TCP_ULP);
    assert_eq!(
        flags::tcp::TCP_USER_TIMEOUT,
        linux_raw_sys::net::TCP_USER_TIMEOUT
    );
    assert_eq!(
        flags::tcp::TCP_WINDOW_CLAMP,
        linux_raw_sys::net::TCP_WINDOW_CLAMP
    );
    assert_eq!(
        flags::tcp::TCP_ZEROCOPY_RECEIVE,
        linux_raw_sys::net::TCP_ZEROCOPY_RECEIVE
    );
}
