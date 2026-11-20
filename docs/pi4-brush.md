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

### 1a. aarch64 link layout (K169)

The aarch64 brush build now passes a real `-C link-arg=-Txtask/brush-aarch64.ld` and requires the
script to exist before cargo runs. `userland/linker/aarch64.ld` was evaluated and not used for
brush: it places `.tbss` after `.data` and does not name the dynamic metadata, so brush's TLS and
dynamic layout are not guaranteed. The dedicated script maps the program headers (PT_PHDR), keeps
`.tdata`/`.tbss` contiguous for PT_TLS, names `.dynamic`/`.dyn*`/`.rela.*`, and pads every
file-backed PT_LOAD tail to 4096. x86_64 keeps `xtask/brush-x86_64.ld`.

`cargo xtask build brush --arch <arch>` now runs the real `raven-elf` Strict planner on the staged
artifact (fail on `FileTailNotPageAligned`); `--check` re-verifies an existing artifact without
building. The staging script uses `--check` for the aarch64 artifact and the x86_64 control.

K169 verification (2026-11-19, base `ab651e7e`):

```
build/aarch64/brush sha256 2fa2d6e7fb262346d86687820342f0161fac5389818f0bb7fb6d18651654b4a0
build/x86_64/brush  sha256 ff4469b5f12374079cb2b990535b83f3aaacda9f29c7a2c18cff3255a342371e

build/aarch64/brush: Strict plan = Ok(1401)
build/x86_64/brush:  Strict plan = Ok(1651)
```

AArch64 LOAD table:

```
Type Offset   VirtAddr           FileSiz  MemSiz   Flg Align
LOAD 0x000000 0x0000000000400000 0x458000 0x458000 R E 0x10000
LOAD 0x458000 0x0000000000858000 0x117000 0x117000 R   0x10000
LOAD 0x56f000 0x000000000096f000 0x005000 0x005000 RW  0x10000
LOAD 0x574000 0x0000000000974000 0x000000 0x004020 RW  0x10000
TLS  0x56f000 0x000000000096f000 0x000308 0x000620 R   0x8
```

Every file-backed tail ends on a page (0x858000, 0x96f000, 0x974000); the last LOAD is BSS-only
(`filesz=0`). Entry `0x82c138` lies in the R E LOAD, and PT_PHDR/PT_TLS are present. The staging
check reports `PASS (16 aarch64 files)` and exercises both Strict plans.

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

## 4. Rev 41 verification

* The first aarch64 build reused a stale `build/cargo/brush-aarch64` from an older toolchain and
  failed with a std/crate mismatch in the brush dependency graph. After removing
  `build/cargo/brush-aarch64` (and the x86_64 cache for the control), the documented command
  passes:

  ```
  $ cargo xtask build brush --arch aarch64
      Finished `release` profile [optimized] target(s) in 2m 18s
  $ file build/aarch64/brush
  build/aarch64/brush: ELF 64-bit LSB executable, ARM aarch64, statically linked, stripped
  ```

* `tests/brush/check-aarch64-staging.sh` reports `PASS (16 aarch64 files)`: the release profile's
  brush plus the 15-name uutils slice all have `e_machine` 0xb7.
* `make check-fast` passes. The x86_64 libc archive was refreshed with
  `cargo xtask libc-pack --arch x86_64` to clear a stale archive-export mismatch before the run.
* The ticket's `cargo nextest run -p brush` spelling still cannot resolve the nested brush
  workspace from the root; from `userland/brush` the `brush` package has no tests. This is the
  exact K26b/brush test-harness follow-up; the image build and `make check-fast` are the gates.

## 5. Board

No Pi 4 board run was performed. The release login/brush/ping gate remains UNVERIFIED until the
getty packaging and board transcript are available.
