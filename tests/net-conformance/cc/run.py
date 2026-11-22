#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""K289 CUBIC/RACK packet-level scenarios over the K253 raw-peer harness.

The guest runs one bulk sender (``/bin/tcpcc``).  The host peer controls ACKs
and the receive window, so the scenarios observe startup burst growth, a loss
and fast retransmit, the post-recovery additive growth, and stale-ACK
reordering without trusting the sender's internal congestion-control state.
"""

from __future__ import annotations

import argparse
import hashlib
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
RAVEN_GUEST = harness.RAVEN_GUEST
RAVEN_HOST = harness.RAVEN_HOST
RAVEN_GUEST_MAC = harness.RAVEN_GUEST_MAC
CC_PORT = 23091
TOTAL = 256 * 1024


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def read_burst(peer, state, local, next_seq, window=65535, idle=0.08, timeout=3.0):
    """Collect one in-order burst, ACK it at the end, and return its byte count."""
    received = 0
    end = next_seq
    frames = 0
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        parsed = peer.recv(idle)
        if parsed is None:
            break
        if parsed["flags"] & (harness.TCP_RST | harness.TCP_FIN):
            break
        if not parsed["payload"]:
            continue
        frames += 1
        if parsed["seq"] == end:
            received += len(parsed["payload"])
            end += len(parsed["payload"])
        elif parsed["seq"] < end < parsed["seq"] + len(parsed["payload"]):
            offset = end - parsed["seq"]
            received += len(parsed["payload"]) - offset
            end += len(parsed["payload"]) - offset
    if received:
        harness.ack(peer, state, local, CC_PORT, end, window)
    return received, end, frames


def slow_start(peer, local):
    state = harness.handshake(peer, local, CC_PORT, 65535, window_scale=7)
    first_data = peer.wait(3.0, lambda p: bool(p["payload"]))
    if first_data is None:
        return False, "no initial bulk data"
    first = len(first_data["payload"])
    next_seq = first_data["seq"] + first
    harness.ack(peer, state, local, CC_PORT, next_seq, 65535)
    bursts = [first]
    zeros = 0
    for _ in range(10):
        got, next_seq, _ = read_burst(peer, state, local, next_seq, idle=0.15, timeout=4.0)
        if got == 0:
            zeros += 1
            harness.ack(peer, state, local, CC_PORT, next_seq, 65535)
            if zeros >= 2 or sum(bursts) >= 100_000:
                break
            continue
        zeros = 0
        bursts.append(got)
        if sum(bursts) >= 100_000:
            break
    harness.reset(peer, state, local, CC_PORT, next_seq)
    grew = len(bursts) >= 2 and max(bursts) > bursts[0] * 2
    return grew, f"bursts={bursts} bytes={sum(bursts)}"


def loss_recovery(peer, local):
    state = harness.handshake(peer, local, CC_PORT, 65535, window_scale=7)
    first = peer.wait(3.0, lambda p: bool(p["payload"]))
    if first is None:
        return False, "no initial bulk data"
    next_seq = first["seq"]
    # ACK the first three segments, then skip one and send duplicate ACKs for the
    # following data. The sender must retransmit the skipped sequence/bytes.
    acked = 0
    missing = None
    while acked < 3:
        parsed = peer.wait(2.0, lambda p: bool(p["payload"]))
        if parsed is None:
            return False, "lost the initial burst"
        next_seq = parsed["seq"] + len(parsed["payload"])
        harness.ack(peer, state, local, CC_PORT, next_seq, 65535)
        acked += 1
    parsed = peer.wait(2.0, lambda p: bool(p["payload"]))
    if parsed is None:
        return False, "no segment to leave unacknowledged"
    missing = parsed["seq"]
    missing_len = len(parsed["payload"])
    for _ in range(3):
        parsed = peer.wait(1.0, lambda p: bool(p["payload"]) and p["seq"] > missing)
        if parsed is None:
            break
        harness.ack(peer, state, local, CC_PORT, missing, 65535)
    retry = peer.wait(3.0, lambda p: bool(p["payload"]) and p["seq"] == missing)
    ok = retry is not None and len(retry["payload"]) == missing_len
    if retry is not None:
        harness.ack(peer, state, local, CC_PORT, missing + missing_len, 65535)
    harness.reset(peer, state, local, CC_PORT, missing + missing_len)
    return ok, f"missing={missing_len}B retransmit={'yes' if retry else 'no'}"


def congestion_avoidance(peer, local):
    state = harness.handshake(peer, local, CC_PORT, 65535, window_scale=7)
    first = peer.wait(3.0, lambda p: bool(p["payload"]))
    if first is None:
        return False, "no initial bulk data"
    next_seq = first["seq"]
    # ACK three segments, leave the fourth unacknowledged, then duplicate-ACK
    # the following data. This is the same loss pattern as loss-recovery.
    acked = 0
    while acked < 3:
        parsed = peer.wait(2.0, lambda p: bool(p["payload"]))
        if parsed is None:
            return False, "lost the initial burst"
        next_seq = parsed["seq"] + len(parsed["payload"])
        harness.ack(peer, state, local, CC_PORT, next_seq, 65535)
        acked += 1
    parsed = peer.wait(2.0, lambda p: bool(p["payload"]))
    if parsed is None:
        return False, "no segment to leave unacknowledged"
    missing = parsed["seq"]
    missing_len = len(parsed["payload"])
    highest = missing + missing_len
    for _ in range(3):
        parsed = peer.wait(1.0, lambda p: bool(p["payload"]) and p["seq"] > missing)
        if parsed is None:
            break
        highest = max(highest, parsed["seq"] + len(parsed["payload"]))
        harness.ack(peer, state, local, CC_PORT, missing, 65535)
    retry = peer.wait(3.0, lambda p: bool(p["payload"]) and p["seq"] == missing)
    if retry is None:
        return False, "no recovery retransmit"
    next_seq = max(highest, missing + len(retry["payload"]))
    harness.ack(peer, state, local, CC_PORT, next_seq, 65535)
    # After reduction, collect two bursts. CUBIC/NewReno must grow additively,
    # not double the post-recovery flight every RTT.
    first_burst, next_seq, _ = read_burst(peer, state, local, next_seq, timeout=4.0)
    second_burst, next_seq, _ = read_burst(peer, state, local, next_seq, timeout=4.0)
    harness.reset(peer, state, local, CC_PORT, next_seq)
    additive = first_burst > 0 and second_burst >= first_burst and second_burst < first_burst * 2
    return additive, f"post-recovery bursts={first_burst},{second_burst} additive-limit={'yes' if additive else 'no'}"


def reordering(peer, local):
    state = harness.handshake(peer, local, CC_PORT, 65535, window_scale=7)
    first = peer.wait(3.0, lambda p: bool(p["payload"]))
    if first is None:
        return False, "no initial bulk data"
    next_seq = first["seq"] + len(first["payload"])
    harness.ack(peer, state, local, CC_PORT, next_seq, 65535)
    # A stale ACK below snd_una must not cause a spurious retransmit.
    harness.ack(peer, state, local, CC_PORT, state["server_isn"] + 1, 65535)
    harness.ack(peer, state, local, CC_PORT, next_seq, 65535)
    spurious = peer.wait(0.6, lambda p: bool(p["payload"]) and p["seq"] < next_seq)
    harness.reset(peer, state, local, CC_PORT, next_seq)
    return spurious is None, f"stale-ack=ignored spurious-segment={'yes' if spurious else 'no'}"


SCENARIOS = {
    "slow-start": slow_start,
    "congestion-avoidance": congestion_avoidance,
    "loss-recovery": loss_recovery,
    "reordering": reordering,
}


def run(scenario: str, image: Path, pcap: Path, serial: Path, tap: str) -> tuple[bool, str]:
    qemu = harness.qemu_boot(image, serial, tap)
    try:
        if not harness.wait_serial(serial, b"cc: listening port=23091", 150):
            tail = serial.read_bytes()[-2000:] if serial.is_file() else b""
            return False, f"guest did not reach cc listener; serial-tail={tail[-400:]!r}"
        time.sleep(1.0)
        peer = harness.RawPeer(tap, RAVEN_HOST, RAVEN_GUEST, RAVEN_GUEST_MAC, pcap)
        try:
            return SCENARIOS[scenario](peer, 40000 + random.randrange(1000))
        finally:
            peer.close()
    finally:
        try:
            os.killpg(qemu.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        qemu.wait()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--image", type=Path, default=ROOT / "build/fixtures/cc/image.img")
    parser.add_argument("--scenario", default="slow-start", choices=tuple(SCENARIOS))
    parser.add_argument("--pcap-dir", type=Path, default=Path("/tmp/opencode/k289-pcaps"))
    parser.add_argument("--serial", type=Path, default=Path("/tmp/opencode/k289-serial.log"))
    parser.add_argument("--tap", default=os.environ.get("NETCONF_TAP", "nctap0"))
    args = parser.parse_args()
    if not args.image.is_file():
        print(f"cc-conformance: image not found: {args.image}", file=sys.stderr)
        return 2
    args.pcap_dir.mkdir(parents=True, exist_ok=True)
    args.serial.parent.mkdir(parents=True, exist_ok=True)
    pcap = args.pcap_dir / f"{args.scenario}.pcap"
    started = time.monotonic()
    ok, detail = run(args.scenario, args.image, pcap, args.serial, args.tap)
    elapsed = time.monotonic() - started
    print(f"cc-conformance: {args.scenario} {'pass' if ok else 'fail'} ({elapsed:.2f}s) {detail}")
    print(f"cc-conformance: pcap={pcap} sha256={sha256(pcap)}")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
