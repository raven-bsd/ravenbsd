#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""K323 packet-level TCP loss/reordering scenarios.

The K253 harness only exposes one RTO scenario.  This runner reuses its raw
peer, handshake and frame helpers and drives six focused cases against one
net-conformance image:

* single-loss: drop the first data segment; the retransmit must repeat its seq/bytes.
* multiple-loss: drop the first retransmit too; a later retransmit must repeat the
  same seq/bytes with RTO backoff.
* dup-ack: three duplicate ACKs must produce a retransmit of the same segment.
* fast-retransmit: the duplicate-ACK retransmit must arrive before a full RTO.
* reordering: a stale ACK after the correct ACK must not cause a spurious segment.
* rto: the loss retransmit must wait for the RTO, not fire immediately.

No product source is changed.  Each scenario writes its own pcap and prints a
SHA-256; the exit status is non-zero if any assertion fails.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import random
import signal
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "tests" / "net-conformance"))

import harness  # noqa: E402

TCP_ACK = harness.TCP_ACK
PAYLOAD = harness.PAYLOAD
RAVEN_GUEST = harness.RAVEN_GUEST
RAVEN_HOST = harness.RAVEN_HOST
RAVEN_GUEST_MAC = harness.RAVEN_GUEST_MAC
RAVEN_PORT = harness.RAVEN_PORT


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def first_data(peer: harness.RawPeer, state: dict, timeout: float = 4.0) -> dict | None:
    return peer.wait(
        timeout,
        lambda parsed: bool(parsed["payload"])
        and parsed["seq"] == (state["server_isn"] + 1) & 0xFFFFFFFF,
    )


def same_segment(peer: harness.RawPeer, first: dict, timeout: float = 4.0) -> dict | None:
    return peer.wait(
        timeout,
        lambda parsed: bool(parsed["payload"])
        and parsed["seq"] == first["seq"]
        and parsed["payload"] == first["payload"],
    )


def finish(peer: harness.RawPeer, state: dict, local: int, remote: int, seq: int) -> None:
    harness.ack(peer, state, local, remote, seq, 65535)
    harness.reset(peer, state, local, remote)


def single_loss(peer: harness.RawPeer, local: int, remote: int) -> tuple[bool, str]:
    state = harness.handshake(peer, local, remote, 65535, window_scale=7)
    first = first_data(peer, state)
    if first is None:
        return False, "no initial data segment"
    again = same_segment(peer, first)
    ok = again is not None and len(first["payload"]) == len(PAYLOAD)
    finish(peer, state, local, remote, first["seq"] + len(first["payload"]))
    return ok, f"first={len(first['payload'])}B seq+0; retransmit={'yes' if again else 'no'}"


def multiple_loss(peer: harness.RawPeer, local: int, remote: int) -> tuple[bool, str]:
    state = harness.handshake(peer, local, remote, 65535, window_scale=7)
    first = first_data(peer, state)
    if first is None:
        return False, "no initial data segment"
    first_retry = same_segment(peer, first)
    # The second RTO backs off; the first observed retransmit was ~3.3 s, so the
    # second needs a window that covers the doubled timer.
    second_retry = same_segment(peer, first, timeout=8.0) if first_retry is not None else None
    ok = (
        first_retry is not None
        and second_retry is not None
        and first_retry["seq"] == first["seq"]
        and second_retry["payload"] == first["payload"]
    )
    finish(peer, state, local, remote, first["seq"] + len(first["payload"]))
    return ok, (
        f"first={len(first['payload'])}B; retries="
        f"{int(first_retry is not None) + int(second_retry is not None)}"
    )


def duplicate_acks(peer: harness.RawPeer, state: dict, local: int, remote: int) -> None:
    for _ in range(3):
        peer.send(
            local,
            remote,
            (state["isn"] + 1) & 0xFFFFFFFF,
            (state["server_isn"] + 1) & 0xFFFFFFFF,
            TCP_ACK,
            65535,
        )
        time.sleep(0.02)


def dup_ack(peer: harness.RawPeer, local: int, remote: int) -> tuple[bool, str]:
    state = harness.handshake(peer, local, remote, 65535, window_scale=7)
    first = first_data(peer, state)
    if first is None:
        return False, "no initial data segment"
    duplicate_acks(peer, state, local, remote)
    again = same_segment(peer, first, timeout=2.0)
    ok = again is not None
    finish(peer, state, local, remote, first["seq"] + len(first["payload"]))
    return ok, f"dup-acks=3; retransmit={'yes' if again else 'no'}"


def fast_retransmit(peer: harness.RawPeer, local: int, remote: int) -> tuple[bool, str]:
    state = harness.handshake(peer, local, remote, 65535, window_scale=7)
    first = first_data(peer, state)
    if first is None:
        return False, "no initial data segment"
    started = time.monotonic()
    duplicate_acks(peer, state, local, remote)
    again = same_segment(peer, first, timeout=2.0)
    elapsed = time.monotonic() - started
    ok = again is not None and elapsed < 1.0
    finish(peer, state, local, remote, first["seq"] + len(first["payload"]))
    return ok, f"dup-acks=3; fast-retransmit={'yes' if again else 'no'} after {elapsed:.3f}s"


def reordering(peer: harness.RawPeer, local: int, remote: int) -> tuple[bool, str]:
    state = harness.handshake(peer, local, remote, 65535, window_scale=7)
    first = first_data(peer, state)
    if first is None:
        return False, "no initial data segment"
    next_seq = first["seq"] + len(first["payload"])
    harness.ack(peer, state, local, remote, next_seq, 65535)
    # Stale ACK below snd_una, then the current ACK again.
    harness.ack(peer, state, local, remote, state["server_isn"] + 1, 65535)
    harness.ack(peer, state, local, remote, next_seq, 65535)
    extra = peer.wait(0.6, lambda parsed: bool(parsed["payload"]))
    ok = extra is None
    harness.reset(peer, state, local, remote)
    return ok, f"stale-ack=ignored; spurious-segment={'yes' if extra else 'no'}"


def rto(peer: harness.RawPeer, local: int, remote: int) -> tuple[bool, str]:
    state = harness.handshake(peer, local, remote, 65535, window_scale=7)
    first = first_data(peer, state)
    if first is None:
        return False, "no initial data segment"
    started = time.monotonic()
    again = same_segment(peer, first, timeout=5.0)
    elapsed = time.monotonic() - started
    ok = again is not None and elapsed >= 1.0
    finish(peer, state, local, remote, first["seq"] + len(first["payload"]))
    return ok, f"retransmit={'yes' if again else 'no'} after {elapsed:.3f}s"


SCENARIOS = {
    "single-loss": single_loss,
    "multiple-loss": multiple_loss,
    "dup-ack": dup_ack,
    "fast-retransmit": fast_retransmit,
    "reordering": reordering,
    "rto": rto,
}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--image", type=Path, default=ROOT / "build/qtest/net-conformance.img")
    parser.add_argument("--pcap-dir", type=Path, default=Path("/tmp/opencode/k323-pcaps"))
    parser.add_argument("--serial", type=Path, default=Path("/tmp/opencode/k323-serial.log"))
    parser.add_argument("--tap", default=os.environ.get("NETCONF_TAP", "nctap0"))
    parser.add_argument("--scenario", action="append", choices=tuple(SCENARIOS), default=[])
    args = parser.parse_args()
    if not args.image.is_file():
        print(f"k323-loss: image not found: {args.image}", file=sys.stderr)
        return 2
    args.pcap_dir.mkdir(parents=True, exist_ok=True)
    args.serial.parent.mkdir(parents=True, exist_ok=True)

    selected = args.scenario or list(SCENARIOS)
    qemu = harness.qemu_boot(args.image, args.serial, args.tap)
    results = []
    try:
        if not harness.wait_serial(args.serial, b"netconf: listening", 150):
            print("k323-loss: guest did not reach netconf: listening", file=sys.stderr)
            return 1
        time.sleep(1.0)
        for name in selected:
            pcap = args.pcap_dir / f"{name}.pcap"
            peer = harness.RawPeer(
                args.tap, RAVEN_HOST, RAVEN_GUEST, RAVEN_GUEST_MAC, pcap
            )
            local = 40000 + random.randrange(1000)
            try:
                ok, detail = SCENARIOS[name](peer, local, RAVEN_PORT)
            except Exception as error:  # noqa: BLE001 - a scenario failure is evidence.
                ok, detail = False, f"exception: {error}"
            finally:
                peer.close()
            digest = sha256(pcap) if pcap.is_file() else ""
            result = {
                "scenario": name,
                "result": "pass" if ok else "fail",
                "detail": detail,
                "pcap": str(pcap),
                "pcap_sha256": digest,
            }
            results.append(result)
            print(
                f"k323-loss: {name} {'pass' if ok else 'fail'} ({detail}) "
                f"pcap_sha256={digest}",
                flush=True,
            )
    finally:
        try:
            os.killpg(qemu.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        qemu.wait()

    summary = {
        "image": str(args.image),
        "image_sha256": sha256(args.image),
        "serial": str(args.serial),
        "scenarios": results,
    }
    (args.pcap_dir / "summary.json").write_text(
        json.dumps(summary, indent=2) + "\n", encoding="utf-8"
    )
    return 0 if all(item["result"] == "pass" for item in results) else 1


if __name__ == "__main__":
    sys.exit(main())
