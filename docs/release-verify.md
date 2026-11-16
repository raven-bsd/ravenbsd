<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Independent clean-clone release verification (REL8)

Status: REL8 implementation. `scripts/release-verify.sh` clones `origin/master` into a fresh scratch
directory, checks the host tools, builds the release image twice around the release gate, compares
the two SHA-256 values and records the qtest transcript. It never modifies the verifier's clone or
product tree and never creates a tag.

## Usage

```sh
sh scripts/release-verify.sh /tmp/opencode/release-verify
```

The argument must be a scratch directory under `/tmp`; the script removes and recreates it. The
verifier's `origin/master` commit is read-only from the verifier clone.

## Flow

1. **Commit.** Read `origin/master` and its commit time from the verifier clone; print the commit.
2. **Clone.** `git clone --shared --no-checkout <verifier> <scratch>/clone`, then detached checkout
   of that exact commit. The clone shares objects with the verifier but has its own working tree
   and `build/`.
3. **Host tools.** `make -C <clone> setup SETUP_ARGS=--check` (`scripts/doctor.sh`). If required
   host tools are missing, print the `MISSING` lines and exit nonzero; the report records them.
4. **Release gate.** `make -C <clone> release-gate` (the full `cargo xtask check` plus the
   `release-login` QEMU qtest). This is the clean-tree proof: the qtest image build triggers the
   REL7 prerequisite builder for libc, brush and uutils in the fresh clone. Copy
   `build/qtest/release-login.log` to `<scratch>/release-login.log`.
5. **First release image.** `RAVEN_USER_RELEASE=1 SOURCE_DATE_EPOCH=<commit-time> make -C <clone>
   image ARCH=x86_64 PROFILE=release`; copy to `<scratch>/image-before.img` and hash it.
6. **Second release image.** Repeat the release image command; copy to
   `<scratch>/image-after.img` and hash it.
7. **Compare.** Print both SHA-256 values and `PASS`/`FAIL`. Exit 0 only when they are equal.

The two compared images are two builds of the **normal release image after the gate**. The QEMU
gate's per-case image is not used for the hash: the case overrides the boot command line with
`test-exit=1`, so its `boot.toml` intentionally differs from the release image. Building both
compared images after the gate also keeps the clone's build state (including the generated
`Symbols.list` tracked file) identical for the pair; a pre-gate image would embed the
clean-versus-dirty build id transition and differ. The pair uses the same release profile, release
user programs and `SOURCE_DATE_EPOCH` (the commit time), which makes the image content
deterministic. **OBSERVED** in `scripts/release-verify.sh`; the gate-first order is the REL8
contract's reproducibility pair.

## Why each choice

* **Release user programs.** `RAVEN_USER_RELEASE=1` builds the user binaries with the release
  profile. The release profile's debug shape trips a known sha2/LLVM debug-info failure; the
  release gate uses the same switch in `make release-gate`.
* **Commit-time epoch.** `SOURCE_DATE_EPOCH` fixes the FAT timestamps and is also the epoch the
  user builds derive from `HEAD`; without it two builds made at different wall-clock times would
  differ even with identical source.
* **Shared clone.** The clone is independent in working tree, `build/` and refs; sharing the
  read-only object store avoids a multi-hundred-megabyte network fetch. The check is of the
  `origin/master` commit, not of the verifier's working branch.
* **No tag.** The script only reads git and writes the scratch directory; it never tags, pushes or
  merges.

## Outputs

Inside the scratch directory:

| Path | Contents |
|---|---|
| `clone/` | detached clean clone at the verified commit |
| `image-before.img` | baseline release image |
| `image-after.img` | release image after the gate |
| `release-login.log` | release-login qtest serial transcript |
| `transcript.log` | full command log and the two SHA-256 lines |

The script prints:

```
release-verify: commit=<sha> time=<epoch>
release-verify: image1 sha256=<hash>
release-verify: image2 sha256=<hash>
release-verify: PASS commit=<sha> sha256=<hash>
```

## Scope and limits

* x86_64 QEMU release only. The Pi 4 release image and board evidence are REL3/REL4.
* The release gate's QEMU test needs the x86_64 OVMF firmware and KVM/QEMU tools; `make setup
  --check` reports missing required items before the build.
* The scratch clone needs the pinned Rust toolchain and the host C/clang/qemu tools; no crates
  are downloaded beyond what the existing lock and cache provide.
* The verifier is a clean-source/clean-build check, not a network isolation proof: the clone is
  local and shared, and cargo may use its normal registry cache.
* The release login credential is the development `root` / `raven` default; do not expose the
  release image or VM to an untrusted network.

## Acceptance

```sh
cargo nextest run -p raven-qtest
make check-fast
sh scripts/release-verify.sh /tmp/opencode/release-verify
```

The last command must exit 0 and print two equal SHA-256 values. The REL8 report carries the
commit, hashes and transcript path. **OBSERVED** in `tickets/kevin/reports/REL8.md`.
