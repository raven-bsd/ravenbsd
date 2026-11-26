<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->
# Pathname conformance (S732)

Rows through the real `vfsd` and `ramfsd` on `/tmp`.

The long-path and component rows use the filesystem control wire (`mkdir`, `stat`), where the
path travels in the request buffer and the filesystem wire carries up to 4095 bytes:

* `long-path` - a 302-byte, three-component path created with `mkdir` and confirmed with `stat`
  (the old filesystem field was 255 bytes).
* `component-255` - a 255-byte component is a directory; a 256-byte component returns
  `ENAMETOOLONG`.
* `symlink-suffix` - opening `link/<remaining>` reaches the link target plus the unprocessed
  suffix.
* `absolute-symlink` - a link to an absolute path resolves under the process root.
* `symlink-loop` - `a -> b -> a` returns `ELOOP`.
* `chroot-confinement` - after `chroot("/tmp/paths/root")`, `/inside` opens and `/outside` is
  `ENOENT`.

The `open(2)` rows use short paths: the S11 `Open` packet is still `[u8; 256]`
(`kernel/src/ipc/protocol.rs::MAX_PATH`). A longer `open` returns `ENAMETOOLONG` from the kernel
before it reaches `vfsd`, so the open transport is not claimed at 4095 here. Widening it is a
kernel packet migration outside this topic's ownership.

Controlled mutations, run before the normal section and before the final chroot row:

* `suffix-dropped` believes the components after a link are lost, so it expects the suffix open
  to fail; the correct product opens it, making the row red.
* `length-255-only` believes a 302-byte path is still rejected; the correct product stats it,
  making the row red.
* `component-limit-ignored` believes a 256-byte component is accepted; the correct product
  returns `ENAMETOOLONG`, making the row red.
* `chroot-not-applied` believes the old namespace is still reachable after chroot; the correct
  product returns `ENOENT`, making the row red.

Run with the shared harness:

```sh
python3 tests/conformance/harness/runner.py --topic tests/conformance/paths \
    --service paths-conformance --profile libc-test
```

The host runner owns the exit mapping; a red row is a row result, not a QEMU timeout. The UFS
variant of these rows is not booted by the shared runner: it cannot attach the UFS root disk
(`qemu_extra` in `tests/qtest/cases/ufs-root.toml`), which is outside this topic's ownership. The
UFS side is covered by `fsd-ufs`'s own image tests, including a non-UTF-8 name round trip.
