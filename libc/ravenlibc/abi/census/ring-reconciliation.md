<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# C33b ring reconciliation

Every one of the 332 assignment rows that C33a dropped is listed here with the
ring it carried before C33a, the ring computed by the completed census, the final
ring, and the source of the decision. `ring_override` is present in
`assignments.toml` only when the final ring differs from the computed ring.

| decision | rows |
|---|---|
| restored with `ring_override` | 260 |
| restored at the computed ring | 0 |
| restored, seed corrected to doc/computed | 64 |
| dropped as non-symbols / no donor name | 8 |

| name | seeded | computed | final | decision | source |
|---|---|---|---|---|---|
| `AF_NETLINK` | 3 | 1 | 3 | OVERRIDE | doc 02 L26: ring 3; constants exist but calls return EAFNOSUPPORT (doc 12 1.1). |
| `AF_PACKET` | 3 | 1 | 3 | OVERRIDE | doc 02 L26: ring 3; constants exist but calls return EAFNOSUPPORT (doc 12 1.1). |
| `LC_GLOBAL_LOCALE` | 0 | 1 | 0 | OVERRIDE | doc 02 P15 (locale.h): ring 0 macro/constant; the census standard index has no tag. |
| `MB_CUR_MAX` | 1 | 0 | 0 | COMPUTED | computed ring retained; a ring-0 standard tag promotes the row (rule 2.2). |
| `NOEXPR` | 0 | 1 | 0 | OVERRIDE | doc 02 X76 (langinfo.h): ring 0 macro/constant; the census standard index has no tag. |
| `YESEXPR` | 0 | 1 | 0 | OVERRIDE | doc 02 X76 (langinfo.h): ring 0 macro/constant; the census standard index has no tag. |
| `__assert` | 2 | 1 | 2 | OVERRIDE | doc 02 B21: BSD __assert alias conflicts with bionic Android assert at the same name (rule 2.5), ring 2 bsd. |
| `__assert2` | 2 | 1 | 2 | OVERRIDE | doc 02 B21: BSD __assert2 alias conflicts with bionic Android assert at the same name (rule 2.5), ring 2 bsd. |
| `__assert_fail` | 0 | 2 | 2 | COMPUTED | doc 02 I01 says ring 1 (glibc, musl), but both are the Linux family; rule 1 requires two families, so ring 2 linux. |
| `__ctype_get_mb_cur_max` | 1 | 2 | 2 | COMPUTED | computed ring retained; the completed census finds one family (rule 2.3 demotion). |
| `__dso_handle` | 1 | 2 | 1 | OVERRIDE | doc 02 Z02: C++ Itanium ABI contract name, ring 1 (linker-provided; no census cell). |
| `__environ` | 0 | 2 | 2 | COMPUTED | doc 02 Z03 says the underscore forms are ring 1, but glibc and musl are one family; rule 1 gives ring 2 linux. |
| `__fread_chk` | 1 | 2 | 2 | COMPUTED | computed ring retained; the completed census finds one family (rule 2.3 demotion). |
| `__getcwd_chk` | 1 | 2 | 2 | COMPUTED | computed ring retained; the completed census finds one family (rule 2.3 demotion). |
| `__libc_single_threaded` | 1 | 2 | 2 | COMPUTED | computed ring retained; the completed census finds one family (rule 2.3 demotion). |
| `__pread_chk` | 1 | 2 | 2 | COMPUTED | computed ring retained; the completed census finds one family (rule 2.3 demotion). |
| `__read_chk` | 1 | 2 | 2 | COMPUTED | computed ring retained; the completed census finds one family (rule 2.3 demotion). |
| `__readlink_chk` | 1 | 2 | 2 | COMPUTED | computed ring retained; the completed census finds one family (rule 2.3 demotion). |
| `__recv_chk` | 1 | 2 | 2 | COMPUTED | doc 02 Z06 _chk family; __recv_chk is glibc/bionic-only at the pin (census cell linux), so ring 2 linux. |
| `_ctype_` | 3 | 1 | 3 | OVERRIDE | doc 02 Z10: no exposed ctype table (doc 14). |
| `_environ` | 0 | 2 | 2 | COMPUTED | doc 02 Z03 says the underscore forms are ring 1, but only illumos exports _environ; rule 1 gives ring 2 solaris. |
| `_tolower_tab_` | 3 | 1 | 3 | OVERRIDE | doc 02 Z10: no exposed ctype table (doc 14). |
| `_uc` | 0 | 3 | dropped | DROP | dropped: stdbit suffix notation, not a standalone symbol name; the exported ABI names are stdc_*_uc/_us/_ui/_ul/_ull. |
| `_ui` | 0 | 3 | dropped | DROP | dropped: stdbit suffix notation, not a standalone symbol name; the exported ABI names are stdc_*_uc/_us/_ui/_ul/_ull. |
| `_ul` | 0 | 3 | dropped | DROP | dropped: stdbit suffix notation, not a standalone symbol name; the exported ABI names are stdc_*_uc/_us/_ui/_ul/_ull. |
| `_ull` | 0 | 3 | dropped | DROP | dropped: stdbit suffix notation, not a standalone symbol name; the exported ABI names are stdc_*_uc/_us/_ui/_ul/_ull. |
| `_us` | 0 | 3 | dropped | DROP | dropped: stdbit suffix notation, not a standalone symbol name; the exported ABI names are stdc_*_uc/_us/_ui/_ul/_ull. |
| `acospi` | 0 | 2 | 0 | OVERRIDE | doc 02 I10 (math.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `adjtime` | 0 | 1 | 0 | OVERRIDE | doc 02 X63 (sys/time.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `arch_prctl` | 3 | 2 | 3 | OVERRIDE | doc 02 L24: ring 3, not exported (doc 10 3.8). |
| `asinpi` | 0 | 2 | 0 | OVERRIDE | doc 02 I10 (math.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `at_quick_exit` | 0 | 1 | 0 | OVERRIDE | doc 02 I17 (stdlib.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `atan2pi` | 0 | 2 | 0 | OVERRIDE | doc 02 I10 (math.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `atanpi` | 0 | 2 | 0 | OVERRIDE | doc 02 I10 (math.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `atof_l` | 1 | 2 | 2 | COMPUTED | computed ring retained; the completed census finds one family (rule 2.3 demotion). |
| `basename` | 0 | 1 | 0 | OVERRIDE | doc 02 P14 (libgen.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `bgets` | 3 | 2 | 3 | OVERRIDE | doc 02 S08: SysV libgen extras SKIP (doc 07 section 1). |
| `bufsplit` | 3 | 2 | 3 | OVERRIDE | doc 02 S08: SysV libgen extras SKIP (doc 07 section 1). |
| `c16rtomb` | 0 | 1 | 0 | OVERRIDE | doc 02 I23 (uchar.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `c32rtomb` | 0 | 1 | 0 | OVERRIDE | doc 02 I23 (uchar.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `c8rtomb` | 0 | 1 | 0 | OVERRIDE | doc 02 I23 (uchar.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `cabs` | 0 | 1 | 0 | OVERRIDE | doc 02 I02 (complex.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `cacos` | 0 | 1 | 0 | OVERRIDE | doc 02 I02 (complex.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `cacosh` | 0 | 1 | 0 | OVERRIDE | doc 02 I02 (complex.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `caddr_t` | 1 | 2 | 1 | OVERRIDE | doc 02 B26: BSD typedef under __BSD_VISIBLE, ring 1 (header-only; no census cell). |
| `canonicalize_file_name` | 2 | 1 | 1 | COMPUTED | computed ring retained; the completed census finds two or more families (rule 2.1 promotion). |
| `cap_enter` | 3 | 2 | 3 | OVERRIDE | doc 02 B19: ring 3 (doc 15). |
| `cap_getmode` | 3 | 2 | 3 | OVERRIDE | doc 02 B19: ring 3 (doc 15). |
| `cap_rights_clear` | 3 | 2 | 3 | OVERRIDE | doc 02 B19: ring 3 (doc 15). |
| `cap_rights_init` | 3 | 2 | 3 | OVERRIDE | doc 02 B19: ring 3 (doc 15). |
| `cap_rights_is_set` | 3 | 2 | 3 | OVERRIDE | doc 02 B19: ring 3 (doc 15). |
| `cap_rights_limit` | 3 | 2 | 3 | OVERRIDE | doc 02 B19: ring 3 (doc 15). |
| `cap_rights_set` | 3 | 2 | 3 | OVERRIDE | doc 02 B19: ring 3 (doc 15). |
| `cap_sandboxed` | 3 | 2 | 3 | OVERRIDE | doc 02 B19: ring 3 (doc 15). |
| `capget` | 3 | 2 | 3 | OVERRIDE | doc 02 section 5 R14: absent; Linux binaries use the Linux personality, not libc. |
| `capset` | 3 | 2 | 3 | OVERRIDE | doc 02 section 5 R14: absent; Linux binaries use the Linux personality, not libc. |
| `carg` | 0 | 1 | 0 | OVERRIDE | doc 02 I02 (complex.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `casin` | 0 | 1 | 0 | OVERRIDE | doc 02 I02 (complex.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `casinh` | 0 | 1 | 0 | OVERRIDE | doc 02 I02 (complex.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `catan` | 0 | 1 | 0 | OVERRIDE | doc 02 I02 (complex.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `catanh` | 0 | 1 | 0 | OVERRIDE | doc 02 I02 (complex.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `catclose` | 0 | 1 | 0 | OVERRIDE | doc 02 X78 (nl_types.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `catgets` | 0 | 1 | 0 | OVERRIDE | doc 02 X78 (nl_types.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `ccos` | 0 | 1 | 0 | OVERRIDE | doc 02 I02 (complex.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `ccosh` | 0 | 1 | 0 | OVERRIDE | doc 02 I02 (complex.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `cexp` | 0 | 1 | 0 | OVERRIDE | doc 02 I02 (complex.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `cfmakeraw` | 0 | 1 | 1 | COMPUTED | computed ring retained; the completed census finds two or more families (rule 2.1 promotion). |
| `cfsetspeed` | 0 | 1 | 1 | COMPUTED | computed ring retained; the completed census finds two or more families (rule 2.1 promotion). |
| `chflags` | 3 | 1 | 3 | OVERRIDE | doc 02 B17: ring 3 stub ENOSYS; UF_*/SF_* hidden. |
| `cimag` | 0 | 1 | 0 | OVERRIDE | doc 02 I02 (complex.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `clock_getcpuclockid` | 0 | 1 | 0 | OVERRIDE | doc 02 X63 (sys/time.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `clock_nanosleep` | 0 | 1 | 0 | OVERRIDE | doc 02 X63 (sys/time.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `clog` | 0 | 1 | 0 | OVERRIDE | doc 02 I02 (complex.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `closelog` | 0 | 1 | 0 | OVERRIDE | doc 02 P48 (syslog.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `compoundn` | 0 | 2 | 0 | OVERRIDE | doc 02 I10 (math.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `conj` | 0 | 1 | 0 | OVERRIDE | doc 02 I02 (complex.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `copylist` | 3 | 2 | 3 | OVERRIDE | doc 02 S08: SysV libgen extras SKIP (doc 07 section 1). |
| `cospi` | 0 | 1 | 0 | OVERRIDE | doc 02 I10 (math.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `cpow` | 0 | 1 | 0 | OVERRIDE | doc 02 I02 (complex.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `cproj` | 0 | 1 | 0 | OVERRIDE | doc 02 I02 (complex.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `creal` | 0 | 1 | 0 | OVERRIDE | doc 02 I02 (complex.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `csin` | 0 | 1 | 0 | OVERRIDE | doc 02 I02 (complex.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `csinh` | 0 | 1 | 0 | OVERRIDE | doc 02 I02 (complex.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `csqrt` | 0 | 1 | 0 | OVERRIDE | doc 02 I02 (complex.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `ctan` | 0 | 1 | 0 | OVERRIDE | doc 02 I02 (complex.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `ctanh` | 0 | 1 | 0 | OVERRIDE | doc 02 I02 (complex.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `daylight` | 1 | 0 | 0 | COMPUTED | computed ring retained; a ring-0 standard tag promotes the row (rule 2.2). |
| `dirname` | 0 | 1 | 0 | OVERRIDE | doc 02 P14 (libgen.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `door_call` | 3 | 2 | 3 | OVERRIDE | doc 02 section 5 R16: Solaris-only, absent. |
| `door_create` | 3 | 2 | 3 | OVERRIDE | doc 02 section 5 R16: Solaris-only, absent. |
| `door_return` | 3 | 2 | 3 | OVERRIDE | doc 02 section 5 R16: Solaris-only, absent. |
| `door_server_create` | 3 | 2 | 3 | OVERRIDE | doc 02 section 5 R16: Solaris-only, absent. |
| `ecvt_r` | 1 | 2 | 2 | COMPUTED | computed ring retained; the completed census finds one family (rule 2.3 demotion). |
| `err_set_exit` | 1 | 2 | 2 | COMPUTED | computed ring retained; the completed census finds one family (rule 2.3 demotion). |
| `err_set_file` | 1 | 2 | 2 | COMPUTED | computed ring retained; the completed census finds one family (rule 2.3 demotion). |
| `exp10l` | 1 | 2 | 2 | COMPUTED | computed ring retained; the completed census finds one family (rule 2.3 demotion). |
| `exp10m1` | 0 | 2 | 0 | OVERRIDE | doc 02 I10 (math.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `exp2m1` | 0 | 2 | 0 | OVERRIDE | doc 02 I10 (math.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `fanotify_init` | 3 | 2 | 3 | OVERRIDE | doc 02 section 5 R10: absent, no primitive. |
| `fanotify_mark` | 3 | 2 | 3 | OVERRIDE | doc 02 section 5 R10: absent, no primitive. |
| `fchdir` | 0 | 1 | 0 | OVERRIDE | doc 02 P50 (unistd.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `fchflags` | 3 | 1 | 3 | OVERRIDE | doc 02 B17: ring 3 stub ENOSYS; UF_*/SF_* hidden. |
| `fchmod` | 0 | 1 | 0 | OVERRIDE | doc 02 P40 (sys/stat.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `fchmodat` | 0 | 1 | 0 | OVERRIDE | doc 02 P40 (sys/stat.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `fchown` | 0 | 1 | 0 | OVERRIDE | doc 02 P50 (unistd.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `fchownat` | 0 | 1 | 0 | OVERRIDE | doc 02 P50 (unistd.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `fcvt_r` | 1 | 2 | 2 | COMPUTED | computed ring retained; the completed census finds one family (rule 2.3 demotion). |
| `fflush_unlocked` | 1 | 0 | 0 | COMPUTED | computed ring retained; a ring-0 standard tag promotes the row (rule 2.2). |
| `fgetc_unlocked` | 1 | 0 | 0 | COMPUTED | computed ring retained; a ring-0 standard tag promotes the row (rule 2.2). |
| `fgets_unlocked` | 1 | 2 | 2 | COMPUTED | computed ring retained; the completed census finds one family (rule 2.3 demotion). |
| `fmaximum_mag_num` | 0 | 2 | 0 | OVERRIDE | doc 02 I10 (math.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `fmaxmag` | 0 | 2 | 0 | OVERRIDE | doc 02 I10 (math.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `fminimum_mag_num` | 0 | 2 | 0 | OVERRIDE | doc 02 I10 (math.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `fminmag` | 0 | 2 | 0 | OVERRIDE | doc 02 I10 (math.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `fmtmsg` | 0 | 1 | 0 | OVERRIDE | doc 02 P07 (fmtmsg.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `fopencookie` | 2 | 1 | 1 | COMPUTED | computed ring retained; the completed census finds two or more families (rule 2.1 promotion). |
| `fpgetmask` | 2 | 1 | 1 | COMPUTED | computed ring retained; the completed census finds two or more families (rule 2.1 promotion). |
| `fpgetround` | 2 | 1 | 1 | COMPUTED | computed ring retained; the completed census finds two or more families (rule 2.1 promotion). |
| `fpgetsticky` | 2 | 1 | 1 | COMPUTED | computed ring retained; the completed census finds two or more families (rule 2.1 promotion). |
| `fpsetmask` | 2 | 1 | 1 | COMPUTED | computed ring retained; the completed census finds two or more families (rule 2.1 promotion). |
| `fpsetround` | 2 | 1 | 1 | COMPUTED | computed ring retained; the completed census finds two or more families (rule 2.1 promotion). |
| `fpsetsticky` | 2 | 1 | 1 | COMPUTED | computed ring retained; the completed census finds two or more families (rule 2.1 promotion). |
| `fputc_unlocked` | 1 | 0 | 0 | COMPUTED | computed ring retained; a ring-0 standard tag promotes the row (rule 2.2). |
| `fromfp` | 0 | 2 | 0 | OVERRIDE | doc 02 I10 (math.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `fromfpx` | 0 | 2 | 0 | OVERRIDE | doc 02 I10 (math.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `fstatat64` | 2 | 1 | 1 | COMPUTED | computed ring retained; the completed census finds two or more families (rule 2.1 promotion). |
| `fstatfs64` | 1 | 2 | 2 | COMPUTED | computed ring retained; the completed census finds one family (rule 2.3 demotion). |
| `fstatvfs` | 0 | 1 | 0 | OVERRIDE | doc 02 P41 (sys/statvfs.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `ftok` | 0 | 1 | 0 | OVERRIDE | doc 02 P35 (sys/ipc.h sys/msg.h sys/sem.h sys/shm.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `ftruncate64` | 2 | 0 | 0 | COMPUTED | computed ring retained; a ring-0 standard tag promotes the row (rule 2.2). |
| `gethostname` | 0 | 1 | 0 | OVERRIDE | doc 02 P50 (unistd.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `getitimer` | 0 | 1 | 0 | OVERRIDE | doc 02 P42 (sys/time.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `getppriv` | 3 | 2 | 3 | OVERRIDE | doc 02 section 5 R16: Solaris-only, absent. |
| `getxattr` | 2 | 1 | 1 | COMPUTED | computed ring retained; the completed census finds two or more families (rule 2.1 promotion). |
| `gmatch` | 3 | 2 | 3 | OVERRIDE | doc 02 S08: SysV libgen extras SKIP (doc 07 section 1). |
| `gnu_dev_major` | 1 | 2 | 2 | COMPUTED | doc 02 X79 note: gnu_dev_* linkable names are ring 2 linux; glibc misc/Versions exports them. |
| `gnu_dev_makedev` | 1 | 2 | 2 | COMPUTED | doc 02 X79 note: gnu_dev_* linkable names are ring 2 linux; glibc misc/Versions exports them. |
| `gnu_dev_minor` | 1 | 2 | 2 | COMPUTED | doc 02 X79 note: gnu_dev_* linkable names are ring 2 linux; glibc misc/Versions exports them. |
| `gnu_get_libc_release` | 3 | 2 | 3 | OVERRIDE | doc 02 section 5 R13: absent; identity macros stay clean. |
| `gnu_get_libc_version` | 3 | 2 | 3 | OVERRIDE | doc 02 section 5 R13: absent; identity macros stay clean. |
| `grantpt` | 0 | 1 | 0 | OVERRIDE | doc 02 P32 (stdlib.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `hcreate` | 0 | 1 | 0 | OVERRIDE | doc 02 P26 (search.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `hdestroy` | 0 | 1 | 0 | OVERRIDE | doc 02 P26 (search.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `hsearch` | 0 | 1 | 0 | OVERRIDE | doc 02 P26 (search.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `iconv` | 0 | 1 | 0 | OVERRIDE | doc 02 P12 (iconv.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `iconv_close` | 0 | 1 | 0 | OVERRIDE | doc 02 P12 (iconv.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `iconv_open` | 0 | 1 | 0 | OVERRIDE | doc 02 P12 (iconv.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `if_freenameindex` | 0 | 1 | 0 | OVERRIDE | doc 02 P18 (net/if.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `if_indextoname` | 0 | 1 | 0 | OVERRIDE | doc 02 P18 (net/if.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `if_nameindex` | 0 | 1 | 0 | OVERRIDE | doc 02 P18 (net/if.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `if_nametoindex` | 0 | 1 | 0 | OVERRIDE | doc 02 P18 (net/if.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `isinf` | 0 | 1 | 1 | COMPUTED | computed ring retained; the completed census finds two or more families (rule 2.1 promotion). |
| `killpg` | 0 | 1 | 0 | OVERRIDE | doc 02 P29 (signal.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `lchflags` | 3 | 2 | 3 | OVERRIDE | doc 02 B17: ring 3 stub ENOSYS; UF_*/SF_* hidden. |
| `lchown` | 0 | 1 | 0 | OVERRIDE | doc 02 P50 (unistd.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `link_map` | 1 | 2 | 1 | OVERRIDE | doc 02 X83: loader link-map type, ring 1 (header-only; no census cell). |
| `listxattr` | 2 | 1 | 1 | COMPUTED | computed ring retained; the completed census finds two or more families (rule 2.1 promotion). |
| `llogb` | 0 | 2 | 0 | OVERRIDE | doc 02 I10 (math.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `llrint` | 0 | 1 | 0 | OVERRIDE | doc 02 I09 (math.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `llround` | 0 | 1 | 0 | OVERRIDE | doc 02 I09 (math.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `lockf` | 0 | 1 | 0 | OVERRIDE | doc 02 P50 (unistd.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `log10p1` | 0 | 2 | 0 | OVERRIDE | doc 02 I10 (math.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `log2p1` | 0 | 2 | 0 | OVERRIDE | doc 02 I10 (math.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `logb` | 0 | 1 | 0 | OVERRIDE | doc 02 I09 (math.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `logp1` | 0 | 2 | 0 | OVERRIDE | doc 02 I10 (math.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `lrint` | 0 | 1 | 0 | OVERRIDE | doc 02 I09 (math.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `lround` | 0 | 1 | 0 | OVERRIDE | doc 02 I09 (math.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `madvise` | 1 | 0 | 0 | COMPUTED | computed ring retained; a ring-0 standard tag promotes the row (rule 2.2). |
| `mallinfo` | 2 | 1 | 2 | OVERRIDE | doc 02 X69: glibc allocator API; illumos mallinfo uses the Solaris allocator contract (rule 2.5), ring 2 linux. |
| `mallopt` | 2 | 1 | 2 | OVERRIDE | doc 02 X69: glibc allocator API; illumos mallopt commands use the Solaris allocator contract (rule 2.5), ring 2 linux. |
| `mbrtoc16` | 0 | 1 | 0 | OVERRIDE | doc 02 I23 (uchar.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `mbrtoc32` | 0 | 1 | 0 | OVERRIDE | doc 02 I23 (uchar.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `mbrtoc8` | 0 | 1 | 0 | OVERRIDE | doc 02 I23 (uchar.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `mimmutable` | 3 | 2 | 3 | OVERRIDE | doc 02 B15: ring 3 stub ENOSYS. |
| `minherit` | 3 | 2 | 3 | OVERRIDE | doc 02 B15: ring 3 stub ENOSYS. |
| `mkdtemp` | 0 | 1 | 0 | OVERRIDE | doc 02 P32 (stdlib.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `mkfifoat` | 0 | 1 | 0 | OVERRIDE | doc 02 P40 (sys/stat.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `mknod` | 0 | 1 | 0 | OVERRIDE | doc 02 P40 (sys/stat.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `mknodat` | 0 | 1 | 0 | OVERRIDE | doc 02 P40 (sys/stat.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `mkstemp` | 0 | 1 | 0 | OVERRIDE | doc 02 P32 (stdlib.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `mktemp` | 0 | 1 | 0 | OVERRIDE | doc 02 P32 (stdlib.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `mlock2` | 1 | 2 | 2 | COMPUTED | computed ring retained; the completed census finds one family (rule 2.3 demotion). |
| `mq_close` | 0 | 1 | 0 | OVERRIDE | doc 02 P17 (mqueue.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `mq_getattr` | 0 | 1 | 0 | OVERRIDE | doc 02 P17 (mqueue.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `mq_notify` | 0 | 1 | 0 | OVERRIDE | doc 02 P17 (mqueue.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `mq_open` | 0 | 1 | 0 | OVERRIDE | doc 02 P17 (mqueue.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `mq_receive` | 0 | 1 | 0 | OVERRIDE | doc 02 P17 (mqueue.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `mq_send` | 0 | 1 | 0 | OVERRIDE | doc 02 P17 (mqueue.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `mq_setattr` | 0 | 1 | 0 | OVERRIDE | doc 02 P17 (mqueue.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `mq_timedreceive` | 0 | 1 | 0 | OVERRIDE | doc 02 P17 (mqueue.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `mq_timedsend` | 0 | 1 | 0 | OVERRIDE | doc 02 P17 (mqueue.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `mq_unlink` | 0 | 1 | 0 | OVERRIDE | doc 02 P17 (mqueue.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `msgctl` | 0 | 1 | 0 | OVERRIDE | doc 02 P35 (sys/ipc.h sys/msg.h sys/sem.h sys/shm.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `msgget` | 0 | 1 | 0 | OVERRIDE | doc 02 P35 (sys/ipc.h sys/msg.h sys/sem.h sys/shm.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `msgrcv` | 0 | 1 | 0 | OVERRIDE | doc 02 P35 (sys/ipc.h sys/msg.h sys/sem.h sys/shm.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `msgsnd` | 0 | 1 | 0 | OVERRIDE | doc 02 P35 (sys/ipc.h sys/msg.h sys/sem.h sys/shm.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `nan` | 0 | 1 | 0 | OVERRIDE | doc 02 I09 (math.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `nearbyint` | 0 | 1 | 0 | OVERRIDE | doc 02 I09 (math.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `nextafter` | 0 | 1 | 0 | OVERRIDE | doc 02 I09 (math.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `nexttoward` | 0 | 1 | 0 | OVERRIDE | doc 02 I09 (math.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `ns_name_compress` | 2 | 1 | 2 | OVERRIDE | doc 02 X42: Linux BIND spelling; FreeBSD exports __ns_name_* and macro-renames this name, so the unqualified ABI is Linux (rule 2.5), ring 2 linux. |
| `ns_name_ntop` | 2 | 1 | 2 | OVERRIDE | doc 02 X42: Linux BIND spelling; FreeBSD macro-renames this name to __ns_name_ntop, so the unqualified ABI is Linux (rule 2.5), ring 2 linux. |
| `ns_name_pton` | 2 | 1 | 2 | OVERRIDE | doc 02 X42: Linux BIND spelling; FreeBSD macro-renames this name to __ns_name_pton, so the unqualified ABI is Linux (rule 2.5), ring 2 linux. |
| `ns_name_uncompress` | 2 | 1 | 2 | OVERRIDE | doc 02 X42: Linux BIND spelling; FreeBSD macro-renames this name to __ns_name_uncompress, so the unqualified ABI is Linux (rule 2.5), ring 2 linux. |
| `open_wmemstream` | 0 | 1 | 0 | OVERRIDE | doc 02 P31 (stdio.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `openlog` | 0 | 1 | 0 | OVERRIDE | doc 02 P48 (syslog.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `p2close` | 3 | 2 | 3 | OVERRIDE | doc 02 S08: SysV libgen extras SKIP (doc 07 section 1). |
| `p2open` | 3 | 2 | 3 | OVERRIDE | doc 02 S08: SysV libgen extras SKIP (doc 07 section 1). |
| `pipe2` | 0 | 1 | 0 | OVERRIDE | doc 02 P50 (unistd.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `posix_fadvise` | 0 | 1 | 0 | OVERRIDE | doc 02 P06 (fcntl.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `posix_fallocate` | 0 | 1 | 0 | OVERRIDE | doc 02 P06 (fcntl.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `posix_madvise` | 0 | 1 | 0 | OVERRIDE | doc 02 P36 (sys/mman.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `pown` | 0 | 2 | 0 | OVERRIDE | doc 02 I10 (math.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `powr` | 0 | 2 | 0 | OVERRIDE | doc 02 I10 (math.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `preadv2` | 1 | 2 | 2 | COMPUTED | computed ring retained; the completed census finds one family (rule 2.3 demotion). |
| `processor_bind` | 3 | 2 | 3 | OVERRIDE | doc 02 section 5 R16: Solaris-only, absent. |
| `pselect` | 0 | 1 | 0 | OVERRIDE | doc 02 P38 (sys/select.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `pset_create` | 3 | 1 | 3 | OVERRIDE | doc 02 section 5 R16: Solaris-only, absent. |
| `psiginfo` | 0 | 1 | 0 | OVERRIDE | doc 02 P29 (signal.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `psignal` | 0 | 1 | 0 | OVERRIDE | doc 02 P29 (signal.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `pthread_attr_get_np` | 2 | 1 | 1 | COMPUTED | computed ring retained; the completed census finds two or more families (rule 2.1 promotion). |
| `pthread_attr_setdefault_np` | 1 | 3 | dropped | DROP | dropped: no donor defines this name; the real glibc/BSD spellings are pthread_setattr_default_np/pthread_getattr_default_np. |
| `ptrace` | 3 | 1 | 3 | OVERRIDE | doc 02 section 5 R14: absent; Linux binaries use the Linux personality, not libc. |
| `ptsname` | 0 | 1 | 0 | OVERRIDE | doc 02 P32 (stdlib.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `putgrent` | 1 | 2 | 2 | COMPUTED | doc 02 X46: glibc nss/Versions exports putgrent and no BSD donor defines it, so ring 2 linux. |
| `pvalloc` | 1 | 2 | 2 | COMPUTED | computed ring retained; the completed census finds one family (rule 2.3 demotion). |
| `pwritev2` | 1 | 2 | 2 | COMPUTED | computed ring retained; the completed census finds one family (rule 2.3 demotion). |
| `quad_t` | 1 | 3 | 1 | OVERRIDE | doc 02 B26: BSD typedef under __BSD_VISIBLE, ring 1 (header-only; no census cell). |
| `quick_exit` | 0 | 1 | 0 | OVERRIDE | doc 02 I17 (stdlib.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `r_debug` | 1 | 3 | 1 | OVERRIDE | doc 02 X83: loader r_debug data, ring 1 (loader-provided; no census cell). |
| `radixsort` | 1 | 2 | 3 | OVERRIDE | doc 02 section 5 R02: absent, no consumer. |
| `readlink` | 0 | 1 | 0 | OVERRIDE | doc 02 P50 (unistd.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `readlinkat` | 0 | 1 | 0 | OVERRIDE | doc 02 P50 (unistd.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `readv` | 0 | 1 | 0 | OVERRIDE | doc 02 P44 (sys/uio.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `realpath` | 0 | 1 | 0 | OVERRIDE | doc 02 P32 (stdlib.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `reboot` | 2 | 1 | 2 | OVERRIDE | doc 02 L18: Linux reboot magic; the BSD form has a different signature and is not provided (rule 2.5), ring 2 linux. |
| `regcmp` | 3 | 2 | 3 | OVERRIDE | doc 02 S08: SysV libgen extras SKIP (doc 07 section 1). |
| `regex` | 3 | 2 | 3 | OVERRIDE | doc 02 S08: SysV libgen extras SKIP (doc 07 section 1). |
| `remainder` | 0 | 1 | 0 | OVERRIDE | doc 02 I09 (math.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `removexattr` | 2 | 1 | 1 | COMPUTED | computed ring retained; the completed census finds two or more families (rule 2.1 promotion). |
| `remquo` | 0 | 1 | 0 | OVERRIDE | doc 02 I09 (math.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `renameat2` | 2 | 1 | 1 | COMPUTED | computed ring retained; the completed census finds two or more families (rule 2.1 promotion). |
| `res_nclose` | 2 | 1 | 2 | OVERRIDE | doc 02 X42: Linux res_n* spelling; glibc resolv/Versions exports res_nquery and musl exports the family; the census symbol-list cell is missing, ring 2 linux. |
| `res_ninit` | 2 | 1 | 2 | OVERRIDE | doc 02 X42: Linux res_n* spelling; glibc resolv/Versions exports res_ninit and musl exports the family; the census symbol-list cell is missing, ring 2 linux. |
| `res_nmkquery` | 2 | 1 | 2 | OVERRIDE | doc 02 X42: Linux res_n* spelling; glibc resolv/Versions exports res_nmkquery and musl exports the family; the census symbol-list cell is missing, ring 2 linux. |
| `res_nquery` | 2 | 1 | 2 | OVERRIDE | doc 02 X42: Linux res_n* spelling; glibc resolv/Versions exports res_nquery and musl exports the family; the census symbol-list cell is missing, ring 2 linux. |
| `res_nquerydomain` | 2 | 1 | 2 | OVERRIDE | doc 02 X42: Linux res_n* spelling; glibc resolv/Versions exports res_nquerydomain and musl exports the family; the census symbol-list cell is missing, ring 2 linux. |
| `res_nsearch` | 2 | 1 | 2 | OVERRIDE | doc 02 X42: Linux res_n* spelling; glibc resolv/Versions exports res_nsearch and musl exports the family; the census symbol-list cell is missing, ring 2 linux. |
| `res_nsend` | 2 | 1 | 2 | OVERRIDE | doc 02 X42: Linux res_n* spelling; glibc resolv/Versions exports res_nsend and musl exports the family; the census symbol-list cell is missing, ring 2 linux. |
| `rootn` | 0 | 2 | 0 | OVERRIDE | doc 02 I10 (math.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `rounddown` | 1 | 2 | 2 | COMPUTED | doc 02 B28 says ring 1, but only FreeBSD/NetBSD (BSD family) define rounddown; rule 1 gives ring 2 bsd. |
| `scalbln` | 0 | 1 | 0 | OVERRIDE | doc 02 I09 (math.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `semctl` | 0 | 1 | 0 | OVERRIDE | doc 02 P35 (sys/ipc.h sys/msg.h sys/sem.h sys/shm.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `semget` | 0 | 1 | 0 | OVERRIDE | doc 02 P35 (sys/ipc.h sys/msg.h sys/sem.h sys/shm.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `semop` | 0 | 1 | 0 | OVERRIDE | doc 02 P35 (sys/ipc.h sys/msg.h sys/sem.h sys/shm.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `setitimer` | 0 | 1 | 0 | OVERRIDE | doc 02 P42 (sys/time.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `setlogmask` | 0 | 1 | 0 | OVERRIDE | doc 02 P48 (syslog.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `settimeofday` | 0 | 1 | 0 | OVERRIDE | doc 02 X63 (sys/time.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `setxattr` | 2 | 1 | 1 | COMPUTED | computed ring retained; the completed census finds two or more families (rule 2.1 promotion). |
| `shm_unlink` | 0 | 1 | 0 | OVERRIDE | doc 02 P36 (sys/mman.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `shmat` | 0 | 1 | 0 | OVERRIDE | doc 02 P35 (sys/ipc.h sys/msg.h sys/sem.h sys/shm.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `shmctl` | 0 | 1 | 0 | OVERRIDE | doc 02 P35 (sys/ipc.h sys/msg.h sys/sem.h sys/shm.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `shmdt` | 0 | 1 | 0 | OVERRIDE | doc 02 P35 (sys/ipc.h sys/msg.h sys/sem.h sys/shm.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `shmget` | 0 | 1 | 0 | OVERRIDE | doc 02 P35 (sys/ipc.h sys/msg.h sys/sem.h sys/shm.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `signgam` | 0 | 1 | 0 | OVERRIDE | doc 02 Z17 ((data)): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `sockaddr_un` | 0 | 3 | 0 | OVERRIDE | doc 02 P45: <sys/un.h> header-only type, ring 0. |
| `sradixsort` | 1 | 2 | 3 | OVERRIDE | doc 02 section 5 R02: absent, no consumer. |
| `srandom_deterministic` | 3 | 2 | 3 | OVERRIDE | doc 02 section 5 R03: BSD-only, no port needs them (doc 03 O-6). |
| `srandomdev` | 3 | 2 | 3 | OVERRIDE | doc 02 section 5 R03: BSD-only, no port needs them (doc 03 O-6). |
| `statfs64` | 1 | 2 | 2 | COMPUTED | computed ring retained; the completed census finds one family (rule 2.3 demotion). |
| `statvfs` | 0 | 1 | 0 | OVERRIDE | doc 02 P41 (sys/statvfs.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `stdc_bit_ceil` | 0 | 1 | 0 | OVERRIDE | doc 02 I15: C23 <stdbit.h> type-generic macro; the exported ABI names are the suffixed stdc_*_uc/_us/_ui/_ul/_ull forms (FreeBSD Symbol.map). |
| `stdc_bit_floor` | 0 | 1 | 0 | OVERRIDE | doc 02 I15: C23 <stdbit.h> type-generic macro; the exported ABI names are the suffixed stdc_*_uc/_us/_ui/_ul/_ull forms (FreeBSD Symbol.map). |
| `stdc_bit_width` | 0 | 1 | 0 | OVERRIDE | doc 02 I15: C23 <stdbit.h> type-generic macro; the exported ABI names are the suffixed stdc_*_uc/_us/_ui/_ul/_ull forms (FreeBSD Symbol.map). |
| `stdc_count_ones` | 0 | 1 | 0 | OVERRIDE | doc 02 I15: C23 <stdbit.h> type-generic macro; the exported ABI names are the suffixed stdc_*_uc/_us/_ui/_ul/_ull forms (FreeBSD Symbol.map). |
| `stdc_count_zeros` | 0 | 1 | 0 | OVERRIDE | doc 02 I15: C23 <stdbit.h> type-generic macro; the exported ABI names are the suffixed stdc_*_uc/_us/_ui/_ul/_ull forms (FreeBSD Symbol.map). |
| `stdc_first_leading_one` | 0 | 1 | 0 | OVERRIDE | doc 02 I15: C23 <stdbit.h> type-generic macro; the exported ABI names are the suffixed stdc_*_uc/_us/_ui/_ul/_ull forms (FreeBSD Symbol.map). |
| `stdc_first_leading_zero` | 0 | 1 | 0 | OVERRIDE | doc 02 I15: C23 <stdbit.h> type-generic macro; the exported ABI names are the suffixed stdc_*_uc/_us/_ui/_ul/_ull forms (FreeBSD Symbol.map). |
| `stdc_first_trailing_one` | 0 | 1 | 0 | OVERRIDE | doc 02 I15: C23 <stdbit.h> type-generic macro; the exported ABI names are the suffixed stdc_*_uc/_us/_ui/_ul/_ull forms (FreeBSD Symbol.map). |
| `stdc_first_trailing_zero` | 0 | 1 | 0 | OVERRIDE | doc 02 I15: C23 <stdbit.h> type-generic macro; the exported ABI names are the suffixed stdc_*_uc/_us/_ui/_ul/_ull forms (FreeBSD Symbol.map). |
| `stdc_has_single_bit` | 0 | 1 | 0 | OVERRIDE | doc 02 I15: C23 <stdbit.h> type-generic macro; the exported ABI names are the suffixed stdc_*_uc/_us/_ui/_ul/_ull forms (FreeBSD Symbol.map). |
| `stdc_leading_ones` | 0 | 1 | 0 | OVERRIDE | doc 02 I15: C23 <stdbit.h> type-generic macro; the exported ABI names are the suffixed stdc_*_uc/_us/_ui/_ul/_ull forms (FreeBSD Symbol.map). |
| `stdc_leading_zeros` | 0 | 1 | 0 | OVERRIDE | doc 02 I15: C23 <stdbit.h> type-generic macro; the exported ABI names are the suffixed stdc_*_uc/_us/_ui/_ul/_ull forms (FreeBSD Symbol.map). |
| `stdc_trailing_ones` | 0 | 1 | 0 | OVERRIDE | doc 02 I15: C23 <stdbit.h> type-generic macro; the exported ABI names are the suffixed stdc_*_uc/_us/_ui/_ul/_ull forms (FreeBSD Symbol.map). |
| `stdc_trailing_zeros` | 0 | 1 | 0 | OVERRIDE | doc 02 I15: C23 <stdbit.h> type-generic macro; the exported ABI names are the suffixed stdc_*_uc/_us/_ui/_ul/_ull forms (FreeBSD Symbol.map). |
| `strfmon` | 0 | 1 | 0 | OVERRIDE | doc 02 P16 (monetary.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `strfmon_l` | 0 | 1 | 0 | OVERRIDE | doc 02 P16 (monetary.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `strfromd` | 0 | 1 | 0 | OVERRIDE | doc 02 I18 (stdlib.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `strfromf` | 0 | 1 | 0 | OVERRIDE | doc 02 I18 (stdlib.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `strfroml` | 0 | 1 | 0 | OVERRIDE | doc 02 I18 (stdlib.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `strftime_z` | 1 | 2 | 2 | COMPUTED | doc 02 X61: NetBSD exports strftime_z via __weak_alias and no other family defines it, so ring 2 bsd. |
| `strptime_l` | 0 | 1 | 0 | OVERRIDE | doc 02 X62: strptime_l is the POSIX.1 _l form, ring 0; getdate is the ring-3 member. |
| `strtof` | 0 | 1 | 0 | OVERRIDE | doc 02 I17 (stdlib.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `struct` | 0 | 3 | dropped | DROP | dropped: parser artifact from the doc 02 P45 entry "struct sockaddr_un"; it is not an identifier. |
| `swprintf` | 0 | 1 | 0 | OVERRIDE | doc 02 I25 (wchar.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `symlink` | 0 | 1 | 0 | OVERRIDE | doc 02 P50 (unistd.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `symlinkat` | 0 | 1 | 0 | OVERRIDE | doc 02 P50 (unistd.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `syslog` | 0 | 1 | 0 | OVERRIDE | doc 02 P48 (syslog.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `tanpi` | 0 | 1 | 0 | OVERRIDE | doc 02 I10 (math.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `tcgetwinsize` | 0 | 1 | 0 | OVERRIDE | doc 02 P49 (termios.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `tcsetwinsize` | 0 | 1 | 0 | OVERRIDE | doc 02 P49 (termios.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `timer_create` | 0 | 1 | 0 | OVERRIDE | doc 02 X63 (sys/time.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `timer_delete` | 0 | 1 | 0 | OVERRIDE | doc 02 X63 (sys/time.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `timer_getoverrun` | 0 | 1 | 0 | OVERRIDE | doc 02 X63 (sys/time.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `timer_gettime` | 0 | 1 | 0 | OVERRIDE | doc 02 X63 (sys/time.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `timer_settime` | 0 | 1 | 0 | OVERRIDE | doc 02 X63 (sys/time.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `timespecclear` | 1 | 2 | 2 | COMPUTED | doc 02 X60 says ring 1, but only BSD donors define this macro; rule 1 gives ring 2 bsd. |
| `timespeccmp` | 1 | 2 | 2 | COMPUTED | doc 02 X60 says ring 1, but only BSD donors define this macro; rule 1 gives ring 2 bsd. |
| `timespecisset` | 1 | 2 | 2 | COMPUTED | doc 02 X60 says ring 1, but only BSD donors define this macro; rule 1 gives ring 2 bsd. |
| `timezone` | 1 | 0 | 0 | COMPUTED | computed ring retained; a ring-0 standard tag promotes the row (rule 2.2). |
| `truncate64` | 2 | 0 | 0 | COMPUTED | computed ring retained; a ring-0 standard tag promotes the row (rule 2.2). |
| `tzname` | 1 | 0 | 0 | COMPUTED | computed ring retained; a ring-0 standard tag promotes the row (rule 2.2). |
| `tzset` | 1 | 0 | 0 | COMPUTED | computed ring retained; a ring-0 standard tag promotes the row (rule 2.2). |
| `u_char` | 1 | 2 | 1 | OVERRIDE | doc 02 B26: BSD typedef under __BSD_VISIBLE, ring 1 (header-only; no census cell). |
| `u_int` | 1 | 3 | 1 | OVERRIDE | doc 02 B26: BSD typedef under __BSD_VISIBLE, ring 1 (header-only; no census cell). |
| `u_long` | 1 | 2 | 1 | OVERRIDE | doc 02 B26: BSD typedef under __BSD_VISIBLE, ring 1 (header-only; no census cell). |
| `u_short` | 1 | 3 | 1 | OVERRIDE | doc 02 B26: BSD typedef under __BSD_VISIBLE, ring 1 (header-only; no census cell). |
| `ualarm` | 0 | 1 | 1 | COMPUTED | computed ring retained; the completed census finds two or more families (rule 2.1 promotion). |
| `ufromfp` | 0 | 2 | 0 | OVERRIDE | doc 02 I10 (math.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `ufromfpx` | 0 | 2 | 0 | OVERRIDE | doc 02 I10 (math.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `ulimit` | 0 | 1 | 0 | OVERRIDE | doc 02 P50 (unistd.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `unlockpt` | 0 | 1 | 0 | OVERRIDE | doc 02 P32 (stdlib.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `usleep` | 0 | 1 | 0 | OVERRIDE | doc 02 P50 (unistd.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `utimes` | 0 | 1 | 0 | OVERRIDE | doc 02 P42 (sys/time.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `vswprintf` | 0 | 1 | 0 | OVERRIDE | doc 02 I25 (wchar.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `vsyslog` | 0 | 1 | 0 | OVERRIDE | doc 02 P48 (syslog.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `wait6` | 1 | 2 | 2 | COMPUTED | computed ring retained; the completed census finds one family (rule 2.3 demotion). |
| `wcstod` | 0 | 1 | 0 | OVERRIDE | doc 02 I24 (wchar.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `wcstof` | 0 | 1 | 0 | OVERRIDE | doc 02 I24 (wchar.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `wcstok_l` | 1 | 3 | dropped | DROP | dropped: no donor symbol list defines wcstok_l; doc 02 X27 lists it but the census cell is empty. |
| `wcstold` | 0 | 1 | 0 | OVERRIDE | doc 02 I24 (wchar.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |
| `writev` | 0 | 1 | 0 | OVERRIDE | doc 02 P44 (sys/uio.h): ring 0; the census standard index has no tag and rule 2.3 forbids demoting a ring-0 row. |

## Notes

* Ring 0 rows win over the census because doc 02 rule 2.3 forbids demoting a
  ring-0 row; the census standard field is derived from the glibc manual only and
  does not carry every ISO C23/POSIX.1-2024/XSI tag.
* Ring 3 rows win over the census because doc 02 section 5 records a deliberate
  absence; donor presence is not a shipping decision.
* Ring 2 rows win only where doc 02 documents conflicting semantics or a
  different spelling (rule 2.5): `__assert*`, `mallinfo/mallopt`, `ns_name_*`,
  `reboot`, and the glibc `res_n*` family.
* A 1 -> 2 or 2 -> 1 change follows rules 2.1/2.3 from the completed census;
  the doc 02 row is retained as the ownership source but its old family count
  is superseded.
