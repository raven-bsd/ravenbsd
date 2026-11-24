<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# K287 merged TCP conformance matrix

Release evidence for the merged FreeBSD TCP stack. The K253/K275 harness was run
unchanged against the image built from `origin/master` `0a641914`; the peer is
the raw `AF_PACKET` client and the guest is the K227 tap image. No harness or
product source was changed for this ticket.

* branch: `bob/K287`
* image: `build/qtest/net-conformance.img`
* image sha256: `c428e598739e5df7bbee55b9a257206ed7837c6eaf04d7dda64f409b223d2a19`
* KVM: `qemu-system-x86_64 -accel kvm` wrapper on `PATH`
* TCG: `qemu-system-x86_64 -accel tcg` wrapper on `PATH`

The external wrapper is required because the merged harness has no accelerator
option; the wrapper only inserts `-accel` and passes every other argument
through. It is not part of the product or the harness.

## Results

Both variants pass all five scenarios. The handshake and zero-window failures
that earlier tickets recorded do not reproduce on the merged stack: the no-WS
peer is no longer offered a window scale, and the accepted zero-window probe
byte is not replayed.

| Scenario | KVM | TCG | Wire detail |
|---|---|---|---|
| handshake | PASS | PASS | `SYN-ACK mss=1460 ws=3 sack=True; plain-peer ws=absent win=65535` |
| zero-window | PASS | PASS | `probe=1B seq=1; after-reopen=43B; total=44/44; frames=1; replayed-probe=no` |
| window-update | PASS | PASS | `delivered=44/44 frames=1` |
| retransmit | PASS | PASS | `first=44B seq+0; retransmit=yes` |
| fin-rst | PASS | PASS | `delivered=44/44; peer-close=FIN` |

Commands:

```text
timeout 600 sh scripts/net-conformance.sh --build-only
PATH=/tmp/opencode/k287/kvm:$PATH timeout 300 python3 tests/net-conformance/harness/run.py \
    --image build/qtest/net-conformance.img --label merged-kvm --commit 0a641914 \
    --results-root tests/net-conformance/results/merged
PATH=/tmp/opencode/k287/tcg:$PATH timeout 300 python3 tests/net-conformance/harness/run.py \
    --image build/qtest/net-conformance.img --label merged-tcg --commit 0a641914 \
    --results-root tests/net-conformance/results/merged
```

## Artifacts

Each variant directory contains the summary, the pcap, the guest serial log, the
command log and a `tcpdump` text transcript. Binary pcaps and serial logs stay
in the worktree (git-ignored per the K263 rule); the transcripts and hashes are
committed.

| Variant | pcap SHA-256 (handshake / zero-window) | Transcripts |
|---|---|---|
| `merged-kvm` | `05f46e3f89d9f6667472d82d42a7bb891630136bb26dcc520b5477dd99327504` / `e7d7fccb30d71f4d7eabb4796c31fd1c45d2ac01966e59b8b8b339842f67c387` | `results/merged/merged-kvm/*.txt` |
| `merged-tcg` | `16e57d39e234a04f6dc2880f11d81e94776712d7084d165bb99b32f8730fbd72` / `84d28b1276656434b110dba4dd7b4aaee174a0b06012a1b7ceeae315bd4a0409` | `results/merged/merged-tcg/*.txt` |

The complete list is in `tests/net-conformance/results/merged/SHA256SUMS`.

## Acceptance note

The ticket's build-bearing commands are
`timeout 300 sh scripts/net-conformance.sh --scenario <name> --target raven`.
The measured image build is 590.74 s, so both exact invocations hit the 300 s
cap while compiling. The same image was then exercised with `--no-build`:
handshake passed in 28.46 s and zero-window in 39.70 s. The cap was not raised.
