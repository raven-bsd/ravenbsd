<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# RavenBSD libc delta required by the K26c dependency port (C64h)

This file records the exact `toolchain/vendor/libc` additions that the vendored
brush dependency graph needs. K26c deliberately does **not** change the vendored
libc source: the directive requires the delta to be listed for the reviewer and
the vendored version to stay untouched. These definitions are the named
follow-up **C64h** work.

The list was derived by applying the additions locally, compiling the target,
and then reverting them. With the local delta applied, `mio`, `socket2`,
`rustix`, `nix`, `uzers`, `rlimit` and `tokio` all compiled; the final link
stopped on the 19 ravenlibc symbols at the end of this file.

## Types and structs

```
termios2 (alias of termios)          __fsword_t (c_long)
off64_t = off_t                      ino64_t = ino_t
blkcnt64_t = blkcnt_t                rlim64_t = rlim_t
fsblkcnt64_t = fsblkcnt_t            fsfilcnt64_t = fsfilcnt_t
statfs64 = statfs                    statvfs64 = statvfs
sigval (union)                       sigevent
signalfd_siginfo                     sockaddr_vm
ip_mreqn                             ip_mreq_source
sock_filter                          sock_fprog
flock                                rlimit
fsid_t                               statfs
pollfd                               winsize
sysinfo
```

`siginfo_t` also needs the Linux accessors `si_pid`, `si_uid` and `si_status`
(the layout is already the 128-byte Linux one).

## Signals and wait

```
SIGRTMIN SIGRTMAX NSIG
P_ALL P_PID P_PGID P_PIDFD
CLD_EXITED CLD_KILLED CLD_DUMPED CLD_TRAPPED CLD_STOPPED CLD_CONTINUED
SIGEV_NONE SIGEV_SIGNAL
SYS_pidfd_open PIDFD_NONBLOCK
```

Functions: `waitid`.

## Socket / netinet

```
SOCK_RDM SOCK_DCCP AF_PACKET AF_VSOCK
IPPROTO_DCCP IPPROTO_MPTCP IPPROTO_SCTP IPPROTO_UDPLITE
SOL_IP SOL_IPV6 SOL_DCCP
SO_MARK SO_BINDTODEVICE SO_BINDTOIFINDEX SO_BUSY_POLL SO_INCOMING_CPU
SO_ATTACH_FILTER SO_DETACH_FILTER SO_COOKIE SO_ORIGINAL_DST
IP_TRANSPARENT IP_FREEBIND IP_MULTICAST_ALL
IPV6_TRANSPARENT IPV6_FREEBIND IPV6_MULTICAST_ALL IPV6_TCLASS
IP6T_SO_ORIGINAL_DST
TCP_CONGESTION TCP_CORK TCP_NOTSENT_LOWAT TCP_QUICKACK
TCP_THIN_LINEAR_TIMEOUTS TCP_USER_TIMEOUT
MSG_CONFIRM IP_HDRINCL IPV6_HDRINCL IPV6_RECVHOPLIMIT IPV6_RECVTCLASS
IP_ADD_SOURCE_MEMBERSHIP IP_DROP_SOURCE_MEMBERSHIP
DCCP_SOCKOPT_SERVICE DCCP_SOCKOPT_SERVER_TIMEWAIT DCCP_SOCKOPT_SEND_CSCOV
DCCP_SOCKOPT_RECV_CSCOV DCCP_SOCKOPT_QPOLICY_TXQLEN
DCCP_SOCKOPT_AVAILABLE_CCIDS DCCP_SOCKOPT_CCID DCCP_SOCKOPT_GET_CUR_MPS
DCCP_SOCKOPT_RX_CCID DCCP_SOCKOPT_TX_CCID
```

Functions: `accept4`, `eventfd`, `sendfile`, `statfs`, `fstatfs`, `statfs64`,
`fstatfs64`, `syscall`.

## Termios / ioctl

```
TCGETS2 TCSETS2 TCSETSW2 TCSETSF2
FIONCLEX TIOCEXCL TIOCNXCL UIO_MAXIOV
CBAUD CBAUDEX BOTHER IBSHIFT CIBAUD
NLDLY NL0 NL1 CRDLY CR0 CR1 CR2 CR3
TABDLY TAB0 TAB1 TAB2 TAB3 XTABS
BSDLY BS0 BS1 FFDLY FF0 FF1 VTDLY VT0 VT1
CRTSCTS CMSPAR IMAXBEL IUTF8 IUCLC XCASE
```

## Filesystem / statfs / sysconf

```
XFS_SUPER_MAGIC and the other Linux *_SUPER_MAGIC values used by nix statfs
ST_RDONLY ST_NOSUID ST_NODEV ST_NOEXEC ST_SYNCHRONOUS ST_MANDLOCK
ST_NOATIME ST_NODIRATIME
POSIX_FADV_NORMAL POSIX_FADV_RANDOM POSIX_FADV_SEQUENTIAL
POSIX_FADV_WILLNEED POSIX_FADV_DONTNEED POSIX_FADV_NOREUSE
UTIME_NOW UTIME_OMIT
SFD_CLOEXEC SFD_NONBLOCK
MFD_CLOEXEC MFD_ALLOW_SEALING MFD_HUGETLB and the MFD_HUGE_* sizes
RWF_HIPRI RWF_DSYNC RWF_SYNC RWF_NOWAIT RWF_APPEND RWF_NOAPPEND
RWF_ATOMIC RWF_DONTCACHE
```

`_SC_*` values referenced by nix `sysconf`: the 60 constants for
`_SC_2_*`, `_SC_AIO_*`, `_SC_ASYNCHRONOUS_IO`, `_SC_BC_*`, `_SC_COLL_WEIGHTS_MAX`,
`_SC_DELAYTIMER_MAX`, `_SC_EXPR_NEST_MAX`, `_SC_FSYNC`, `_SC_IOV_MAX`,
`_SC_JOB_CONTROL`, `_SC_LINE_MAX`, `_SC_MAPPED_FILES`, `_SC_MEMLOCK*`,
`_SC_MEMORY_PROTECTION`, `_SC_MESSAGE_PASSING`, `_SC_MONOTONIC_CLOCK`,
`_SC_MQ_*`, `_SC_PHYS_PAGES`, `_SC_PRIORITIZED_IO`, `_SC_PRIORITY_SCHEDULING`,
`_SC_RTSIG_MAX`, `_SC_SAVED_IDS`, `_SC_SEMAPHORES`, `_SC_SEM_*`,
`_SC_SHARED_MEMORY_OBJECTS`, `_SC_SYNCHRONIZED_IO`, `_SC_THREAD*`,
`_SC_TIMERS`, `_SC_TIMER_MAX`, `_SC_XOPEN_*`.

Functions: `faccessat`, `mkfifoat`, `mknodat`, `posix_fadvise`,
`posix_fallocate`, `utimensat`, `futimens`, `ftruncate`, `truncate`,
`readlink`, `chdir`, `chroot`, `umask`, `confstr`, `gethostname`,
`ttyname_r`, `tcgetattr`, `tcsetattr`, `tcgetpgrp`, `tcsetpgrp`, `killpg`,
`cfmakeraw`, `pipe2`, `dup3`, `lseek64`, `sync`, `syncfs`, `fsync`,
`fdatasync`, `setfsuid`, `setfsgid`, `setgroups`, `initgroups`, `daemon`,
`fexecve`, `forkpty`, `ptsname_r`, `sigsuspend`, `sysinfo`.

## Final link gap observed with the delta applied

With the local delta applied, the brush binary linked until these ravenlibc
symbols were missing from `build/x86_64/libc.a`:

```
confstr gethostname errno_location pipe2 chroot chdir ftruncate readlink
syscall umask cfmakeraw __getrandom_v03_custom ttyname_r tcgetpgrp tcsetpgrp
killpg tcgetattr tcsetattr
```

These are the concrete C64h implementation items; the declarations alone are not
enough for the brush target.
