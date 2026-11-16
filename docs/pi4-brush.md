<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Pi 4 brush and aarch64 uutils staging (K26d)

Status: host build and staging OBSERVED. The Pi 4 release image builds with the aarch64 brush and
uutils binaries. Board results remain UNVERIFIED.

## 1. Build

The shared ravenbsd toolchain now carries the aarch64 std. In a fresh clone run:

```sh
cargo xtask libc-pack --arch aarch64
cargo xtask build brush --arch aarch64     # stages build/aarch64/brush
cargo xtask build uutils --arch aarch64    # stages build/aarch64/uutils/<tool>
```

Observed ELF headers:

```
build/aarch64/brush: ELF 64-bit LSB executable, ARM aarch64, statically linked, stripped
build/aarch64/uutils/ls: ELF 64-bit LSB executable, ARM aarch64, statically linked, stripped
```

`tests/brush/check-aarch64-staging.sh` parses the release fragments and checks every staged
`build/aarch64` source is an AArch64 ELF (`e_machine` 0xb7). It also rejects `build/brush` and
`build/x86_64/uutils` sources.

## 2. Release profile

`profiles/pi4-release.toml` adds the release userspace through `profiles/pi4-release.d/`:

* `20-userspace.toml`: shell services, first-party tools, netd/resolvd/ping/route/netstat, platd,
  `/bin/login`, the account files and the getty manifest;
* `30-brush.toml`: `/bin/brush` from `build/aarch64/brush`;
* `40-uutils.toml`: the 15-name aarch64 uutils slice under `/bin` (without `echo`, which the base
  profile already supplies);
* `10-pi4.toml`: the EMMC2/blockd staging declarations.

The profile loader permits only one level of `extends`. `pi4` already extends `base`, so
`pi4-release` extends `base` directly and carries the same Pi 4 boot cmdline and storage entries;
it cannot literally `extends = "pi4"` without changing the profile loader.

## 3. Pending packaging

* `raven-getty` builds the `getty` binary, but `xtask` stages user packages by package name
  (`build/aarch64/user/raven-getty`), so the image build fails to find the artifact. The getty
  manifest is staged disabled until the package/binary mapping is fixed; `/bin/login` and the
  account files are present.
* `cargo nextest run -p brush` from the repository root does not resolve the nested brush
  workspace; from `userland/brush` the `brush` package builds but has no tests (nextest "no tests
  to run"). The image build and `make check-fast` are the current gates.

## 4. Board

No Pi 4 board run was performed. The release login/brush/ping gate remains UNVERIFIED until the
getty packaging and board transcript are available.
