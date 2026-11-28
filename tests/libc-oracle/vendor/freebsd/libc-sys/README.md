# FreeBSD kernel-facing libc-sys test input

This directory is the suite root named by `tests/libc-oracle/suites/freebsd-libc-sys.toml`
(`root = "vendor/freebsd/libc-sys"`). The suite's C bodies are deliberately not tracked here as
`.c` files: commit `afe2da3ed` embedded them byte-for-byte as test-only Rust text in
`tests/libc/atf/src/port_sources.rs`, so the structural C/C++ scaffold gate stays closed, and
`port_sources::stage` writes them at run time to
`<out>/port-sources/build/tests/libc-oracle/vendor/freebsd/libc-sys/`. `Builder::suite_root`
resolves the manifest root against that staged tree first, so the manifest root is the
historical, materialised path and is correct as written. No source was lost in that commit; a
`git cat-file` lookup for a `.c` path under this root returns 128 by design.

## Source map

| path | upstream (FreeBSD) | rev | licence | modified | sha256 (historical blob) |
|---|---|---|---|---|---|
| `tests/libc-oracle/vendor/freebsd/libc-sys/sys/h_macros.h` | `contrib/netbsd-tests/h_macros.h` | `133344405f532bd54ff3629a77d4c4e062f01fdb` | BSD-2-Clause | false | `6512c23cdf2344537170adf8f7bf052b77fec54d12fd2296382ec2c9effa05f4` |
| `tests/libc-oracle/vendor/freebsd/libc-sys/sys/t_dup.c` | `contrib/netbsd-tests/lib/libc/sys/t_dup.c` | `133344405f532bd54ff3629a77d4c4e062f01fdb` | BSD-2-Clause | false | `5a736f3a8e8ad2743bb2dca2622afd6693a4121e32e0a27e6b6dfd897c31b52f` |
| `tests/libc-oracle/vendor/freebsd/libc-sys/sys/t_getgroups.c` | `contrib/netbsd-tests/lib/libc/sys/t_getgroups.c` | `133344405f532bd54ff3629a77d4c4e062f01fdb` | BSD-2-Clause | false | `50f3b58e2de5b3728baad32ff2e493f462c8388b97fb2d4a8df3c665a9561e90` |
| `tests/libc-oracle/vendor/freebsd/libc-sys/sys/t_kill.c` | `contrib/netbsd-tests/lib/libc/sys/t_kill.c` | `133344405f532bd54ff3629a77d4c4e062f01fdb` | BSD-2-Clause | false | `7f461acb224bb49d3a614c2f8c9ab65dd7a3d96095b3049bea52c372cb9cfdae` |
| `tests/libc-oracle/vendor/freebsd/libc-sys/sys/t_link.c` | `contrib/netbsd-tests/lib/libc/sys/t_link.c` | `133344405f532bd54ff3629a77d4c4e062f01fdb` | BSD-2-Clause | false | `e4628385c25ad37dd3ad842f3352ddbf19441f67333cef900d80ff57c22df815` |
| `tests/libc-oracle/vendor/freebsd/libc-sys/sys/t_mkdir.c` | `contrib/netbsd-tests/lib/libc/sys/t_mkdir.c` | `133344405f532bd54ff3629a77d4c4e062f01fdb` | BSD-2-Clause | false | `2edca3e4bd3c8d19be64e96a93ec4fedaffdc455057f7385838c393f6d46f769` |
| `tests/libc-oracle/vendor/freebsd/libc-sys/sys/t_mmap.c` | `contrib/netbsd-tests/lib/libc/sys/t_mmap.c` | `133344405f532bd54ff3629a77d4c4e062f01fdb` | BSD-2-Clause | false | `2829a5e7dd6891fb98bcae6d24e943f3ac43e994378fb7e010ca33bb2ccaf75b` |
| `tests/libc-oracle/vendor/freebsd/libc-sys/sys/t_mprotect.c` | `contrib/netbsd-tests/lib/libc/sys/t_mprotect.c` | `133344405f532bd54ff3629a77d4c4e062f01fdb` | BSD-2-Clause | false | `05f98b33f5f8dff9a50a6f0e047e911fb0708f13e10f80c5800770da4c131940` |
| `tests/libc-oracle/vendor/freebsd/libc-sys/sys/t_pipe.c` | `contrib/netbsd-tests/lib/libc/sys/t_pipe.c` | `133344405f532bd54ff3629a77d4c4e062f01fdb` | BSD-2-Clause | false | `f5bf091b8f134b89440ef51380c1367653be5be1ce4b1f5acbbfe5eabd506b94` |
| `tests/libc-oracle/vendor/freebsd/libc-sys/sys/t_poll.c` | `contrib/netbsd-tests/lib/libc/sys/t_poll.c` | `133344405f532bd54ff3629a77d4c4e062f01fdb` | BSD-2-Clause | true | `80d4e927d2f2f3a8a1bd1fdd20bed9dc5f1402c7684189317614f2574a400980` |
| `tests/libc-oracle/vendor/freebsd/libc-sys/sys/t_select.c` | `contrib/netbsd-tests/lib/libc/sys/t_select.c` | `133344405f532bd54ff3629a77d4c4e062f01fdb` | BSD-2-Clause | false | `c06cc6845b9795b9189d1792f3f402910172ed861f7dd6714052c57a09c76e8a` |
| `tests/libc-oracle/vendor/freebsd/libc-sys/sys/t_socketpair.c` | `contrib/netbsd-tests/lib/libc/sys/t_socketpair.c` | `133344405f532bd54ff3629a77d4c4e062f01fdb` | BSD-2-Clause | false | `f521155baeae93ce150c89181afbd82ef06acec10a9f2a0f1e27116b98aa08c1` |
| `tests/libc-oracle/vendor/freebsd/libc-sys/sys/t_umask.c` | `contrib/netbsd-tests/lib/libc/sys/t_umask.c` | `133344405f532bd54ff3629a77d4c4e062f01fdb` | BSD-2-Clause | false | `17a128158754b4de6c0b68300e4eddef7ef2aa7292a87cf72f1d29c78037e9d0` |
| `tests/libc-oracle/vendor/freebsd/libc-sys/sys/t_unlink.c` | `contrib/netbsd-tests/lib/libc/sys/t_unlink.c` | `133344405f532bd54ff3629a77d4c4e062f01fdb` | BSD-2-Clause | false | `eb1e46e2c6d9d89ef5114c176256bd32c49096205088cca3eb3f46839a950621` |
| `tests/libc-oracle/vendor/freebsd/libc-sys/sys/t_wait.c` | `contrib/netbsd-tests/lib/libc/sys/t_wait.c` | `133344405f532bd54ff3629a77d4c4e062f01fdb` | BSD-2-Clause | true | `25ba8a77b16716d5aaaf5e9d5025659855055d453400fdf020a39350ddb87c47` |
| `tests/libc-oracle/vendor/freebsd/libc-sys/gen/execve/t_execve.c` | `contrib/netbsd-tests/lib/libc/gen/execve/t_execve.c` | `133344405f532bd54ff3629a77d4c4e062f01fdb` | BSD-2-Clause | false | `28eef6caef4409085d16a84977fd0386ce64a17f86d728574dd19816352e075f` |

Every staged exact body is verified against the historical Git blob SHA-256 above before the
single build transformation (`sys/t_select.c`: the prerequisite child exit-status check). The
upstream donor is FreeBSD `contrib/netbsd-tests/**` at `133344405f`, BSD-2-Clause, and the test
sources are never linked into a shipped artifact. `sys/h_macros.h` is an include, not a program
source; the other files are the suite's `sources` entries.

## Pollts

There is no `sys/t_pollts.c` under this FreeBSD vendor root and there never was. The pinned
FreeBSD `sys/t_poll.c` carries the `pollts` cases; the S750 port binds them to `ppoll` (same
prototype and contract) and records `modified = true` in `provenance.toml`. The separate NetBSD
donor body `tests/libc-oracle/vendor/netbsd/libc/sys/t_pollts.c` is tracked for the
`netbsd-libc` suite and is not this suite's source. A `git cat-file` lookup for a `.c` path under
this root returns 128 by design; the body is present in the embedded carrier and in the staged
tree.
