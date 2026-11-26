#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""K263 independent Linux TCP baselines.

Each scenario drives the Linux kernel TCP stack on a veth through a raw peer and
writes a pcap plus a log under ``tests/net-conformance/results/linux``. The peer
side is user-space frames, so SYN options, ACK selection and SACK blocks are
controlled by the scenario; the kernel end is a normal Python socket.
"""

import argparse
import itertools
import os
import select
import struct
import subprocess
import sys
import time

import rawtcp as raw

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(
    os.path.dirname(os.path.abspath(__file__))))))
RESULTS = os.path.join(ROOT, "tests", "net-conformance", "results", "linux")
PYTHON = sys.executable
PORT_COUNTER = itertools.count(20000 + (os.getpid() * 7) % 20000)


def next_port():
    return next(PORT_COUNTER)


class Scenario:
    def __init__(self, name, port, mode, extra=None):
        self.name = name
        self.port = port
        self.mode = mode
        self.extra = extra or []


SCENARIOS = {
    "window-scale": Scenario("window-scale", 24001, "listener"),
    "window-scale-absent": Scenario("window-scale-absent", 24002, "listener"),
    "duplicate-ack": Scenario("duplicate-ack", 24003, "write"),
    "out-of-order-sack": Scenario("out-of-order-sack", 24004, "write"),
    "half-close": Scenario("half-close", 24005, "read"),
    "listener-backlog": Scenario("listener-backlog", 24006, "hold",
                                ["--backlog", "1", "--hold-ms", "10000"]),
    "ecn": Scenario("ecn", 24007, "listener"),
    "sack": Scenario("sack", 24008, "listener"),
    "cubic": Scenario("cubic", 24009, "listener"),
    "pmtu-blackhole": Scenario("pmtu-blackhole", 24010, "listener"),
}


class Log:
    def __init__(self, name):
        os.makedirs(RESULTS, exist_ok=True)
        self.path = os.path.join(RESULTS, f"{name}.log")
        self.handle = open(self.path, "w", encoding="utf-8")

    def write(self, text):
        print(text, flush=True)
        self.handle.write(text + "\n")
        self.handle.flush()

    def close(self):
        self.handle.close()


def start_endpoint(scenario, log):
    args = [PYTHON, os.path.join(os.path.dirname(__file__), "kernel_endpoint.py"),
            "--port", str(scenario.port), "--mode", scenario.mode] + scenario.extra
    proc = subprocess.Popen(args, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                            text=True, bufsize=1)
    ready = False
    deadline = time.monotonic() + 5
    while time.monotonic() < deadline:
        ready_list, _, _ = select.select([proc.stdout], [], [], 0.2)
        if not ready_list:
            if proc.poll() is not None:
                break
            continue
        line = proc.stdout.readline()
        if not line:
            break
        log.write(f"endpoint: {line.rstrip()}")
        if line.strip() == "READY":
            ready = True
            break
    if not ready:
        proc.kill()
        raise RuntimeError("kernel endpoint did not become ready")
    return proc


def stop_endpoint(proc, log):
    if proc is None:
        return
    if proc.poll() is None:
        proc.terminate()
        try:
            proc.wait(timeout=1)
        except subprocess.TimeoutExpired:
            proc.kill()
    if proc.stdout is not None:
        for line in proc.stdout:
            log.write(f"endpoint: {line.rstrip()}")


def synack_for(peer, port, src_port, with_wscale=True, sack=True, ecn=False):
    isn = int(time.time() * 1_000_000) & 0xFFFFFFFF
    options = raw.MSS
    if sack:
        options += raw.SACK_PERMITTED
    if with_wscale:
        options += raw.window_scale(7)
    flags = raw.SYN | (raw.ECE | raw.CWR if ecn else 0)
    peer.send(raw.PEER_IP, raw.HOST_IP, src_port, port, isn, 0, flags, 65535, options)
    synack = peer.recv(lambda p: p.sport == port and p.flags & raw.SYN and p.flags & raw.ACK, 3.0)
    if synack is None:
        return None, None
    peer.send(raw.PEER_IP, raw.HOST_IP, src_port, port, isn + 1, synack.seq + 1, raw.ACK, 65535)
    return isn, synack


def scenario_window_scale(peer, scenario, log, with_wscale):
    sport = next_port()
    isn, synack = synack_for(peer, scenario.port, sport, with_wscale=with_wscale)
    if synack is None:
        return False, "no SYN-ACK"
    log.write(f"SYN-ACK: {synack}")
    expected = with_wscale
    actual = synack.wscale is not None
    if actual != expected:
        return False, f"window scale option expected={expected} actual={actual}"
    if synack.mss is None or synack.mss > 1460:
        return False, f"unexpected MSS {synack.mss}"
    peer.send(raw.PEER_IP, raw.HOST_IP, sport, scenario.port, isn + 1,
              synack.seq + 1, raw.RST, 0)
    return True, f"ws={'offered' if with_wscale else 'absent'} mss={synack.mss}"


def scenario_backlog(peer, scenario, log):
    path = "/proc/sys/net/ipv4/tcp_syncookies"
    previous = None
    try:
        with open(path, encoding="ascii") as handle:
            previous = handle.read().strip()
        with open(path, "w", encoding="ascii") as handle:
            handle.write("0\n")
    except OSError as error:
        return False, f"cannot disable syncookies: {error}"
    synacks = 0
    for index in range(5):
        src_port = next_port()
        isn = int(time.time() * 1_000_000 + index) & 0xFFFFFFFF
        peer.send(raw.PEER_IP, raw.HOST_IP, src_port, scenario.port, isn, 0,
                  raw.SYN, 65535, raw.MSS + raw.SACK_PERMITTED)
        synack = peer.recv(lambda p, port=scenario.port, sport=src_port:
                           p.sport == port and p.dport == sport
                           and p.flags & raw.SYN and p.flags & raw.ACK, 0.4)
        if synack is not None:
            synacks += 1
            log.write(f"backlog SYN-ACK src={src_port} {synack}")
    if previous is not None:
        try:
            with open(path, "w", encoding="ascii") as handle:
                handle.write(previous + "\n")
        except OSError:
            pass
    log.write(f"backlog synacks={synacks}/5 (backlog 1, syncookies off)")
    if synacks < 1 or synacks > 3:
        return False, f"unexpected SYN-ACK count {synacks}/5 (backlog 1)"
    return True, f"synacks={synacks}/5, backlog drops observed"


def wait_data(peer, count, timeout):
    packets = []
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline and len(packets) < count:
        packet = peer.recv(lambda p: len(p.payload) > 0, min(0.2, deadline - time.monotonic()))
        if packet is not None:
            packets.append(packet)
    return packets


def scenario_duplicate_ack(peer, scenario, log):
    sport = next_port()
    isn, synack = synack_for(peer, scenario.port, sport)
    if synack is None:
        return False, "no SYN-ACK"
    packets = wait_data(peer, 1, 3.0)
    if not packets:
        return False, "no server data"
    first = packets[0]
    log.write(f"first data {first}")
    first_end = first.seq + len(first.payload)
    peer.send(raw.PEER_IP, raw.HOST_IP, sport, scenario.port, isn + 1,
              first_end, raw.ACK, 65535)
    for _ in range(3):
        peer.send(raw.PEER_IP, raw.HOST_IP, sport, scenario.port, isn + 1,
                  first_end, raw.ACK, 65535)
    retransmit = peer.recv(lambda p: len(p.payload) > 0 and p.seq == first_end, 2.0)
    if retransmit is None:
        return False, f"no retransmit at seq={first_end}"
    log.write(f"retransmit {retransmit}")
    return True, f"fast retransmit seq={first_end} len={len(retransmit.payload)}"


def scenario_out_of_order(peer, scenario, log):
    sport = next_port()
    isn, synack = synack_for(peer, scenario.port, sport, sack=True)
    if synack is None:
        return False, "no SYN-ACK"
    if not synack.sack:
        return False, "SACK not negotiated"
    packets = wait_data(peer, 3, 3.0)
    if len(packets) < 3:
        return False, f"only {len(packets)} data segments"
    packets.sort(key=lambda p: p.seq)
    first = packets[0]
    third = packets[-1]
    first_end = first.seq + len(first.payload)
    third_end = third.seq + len(third.payload)
    sack = raw.option(5, struct.pack("!II", third.seq, third_end))
    peer.send(raw.PEER_IP, raw.HOST_IP, sport, scenario.port, isn + 1,
              first_end, raw.ACK, 65535, sack)
    for _ in range(3):
        peer.send(raw.PEER_IP, raw.HOST_IP, sport, scenario.port, isn + 1,
                  first_end, raw.ACK, 65535, sack)
    retransmit = peer.recv(lambda p: len(p.payload) > 0 and p.seq == first_end, 2.0)
    if retransmit is None:
        return False, f"no retransmit for hole seq={first_end}"
    log.write(f"retransmit {retransmit}")
    return True, f"SACK hole {first_end} retransmitted len={len(retransmit.payload)}"


def scenario_half_close(peer, scenario, log):
    sport = next_port()
    isn, synack = synack_for(peer, scenario.port, sport)
    if synack is None:
        return False, "no SYN-ACK"
    peer.send(raw.PEER_IP, raw.HOST_IP, sport, scenario.port, isn + 1,
              synack.seq + 1, raw.FIN | raw.ACK, 65535)
    fin = peer.recv(lambda p: p.flags & raw.FIN, 2.0)
    if fin is None:
        return False, "server did not send FIN after peer FIN"
    log.write(f"server FIN {fin}")
    peer.send(raw.PEER_IP, raw.HOST_IP, sport, scenario.port, isn + 2, fin.seq + 1,
              raw.ACK, 65535)
    return True, f"half-close ACK+FIN seq={fin.seq}"


def scenario_ecn(peer, scenario, log):
    path = "/proc/sys/net/ipv4/tcp_ecn"
    try:
        with open(path, encoding="ascii") as handle:
            previous = handle.read().strip()
        with open(path, "w", encoding="ascii") as handle:
            handle.write("1\n")
    except OSError as error:
        return False, f"cannot enable tcp_ecn: {error}"
    try:
        _, synack = synack_for(peer, scenario.port, next_port(), ecn=True)
    finally:
        try:
            with open(path, "w", encoding="ascii") as handle:
                handle.write(previous + "\n")
        except OSError:
            pass
    if synack is None:
        return False, "no SYN-ACK"
    log.write(f"ECN SYN-ACK: {synack}")
    if not synack.flags & raw.ECE:
        return False, "SYN-ACK did not echo ECE"
    return True, "ECN negotiated (ECE on SYN-ACK)"


def scenario_sack(peer, scenario, log):
    _, synack = synack_for(peer, scenario.port, next_port(), sack=True)
    if synack is None:
        return False, "no SYN-ACK"
    log.write(f"SACK SYN-ACK: {synack}")
    if not synack.sack:
        return False, "SACK permitted not echoed"
    return True, "SACK permitted"


def scenario_cubic(peer, scenario, log):
    del peer, scenario
    try:
        with open("/proc/sys/net/ipv4/tcp_congestion_control", encoding="ascii") as handle:
            algorithm = handle.read().strip()
    except OSError as error:
        return False, f"cannot read congestion control: {error}"
    log.write(f"tcp_congestion_control={algorithm}")
    if algorithm != "cubic":
        return False, f"expected cubic, got {algorithm}"
    return True, "Linux default CC is cubic (capability baseline)"


def scenario_pmtu(peer, scenario, log):
    del peer, scenario
    reason = ("a deterministic PMTU black hole needs a router namespace with a reduced MTU "
              "and suppressed ICMP frag-needed; the single veth peer cannot hide the path MTU "
              "from the sender")
    log.write(f"SKIP {reason}")
    return None, reason


RUNNERS = {
    "window-scale": lambda p, s, l: scenario_window_scale(p, s, l, True),
    "window-scale-absent": lambda p, s, l: scenario_window_scale(p, s, l, False),
    "duplicate-ack": scenario_duplicate_ack,
    "out-of-order-sack": scenario_out_of_order,
    "half-close": scenario_half_close,
    "listener-backlog": scenario_backlog,
    "ecn": scenario_ecn,
    "sack": scenario_sack,
    "cubic": scenario_cubic,
    "pmtu-blackhole": scenario_pmtu,
}


def ensure_root():
    if os.geteuid() == 0:
        return
    os.execvp("sudo", ["sudo", "-n", sys.executable] + sys.argv)


def run_one(name, host_mac, peer_mac):
    scenario = SCENARIOS[name]
    log = Log(name)
    pcap_path = os.path.join(RESULTS, f"{name}.pcap")
    peer = None
    endpoint = None
    started = time.monotonic()
    try:
        endpoint = start_endpoint(scenario, log)
        peer = raw.RawPeer(host_mac, peer_mac, pcap_path)
        passed, detail = RUNNERS[name](peer, scenario, log)
        elapsed = time.monotonic() - started
        if passed is None:
            result = "SKIP"
        else:
            result = "PASS" if passed else "FAIL"
        log.write(f"RESULT {name} {result} {detail} elapsed={elapsed:.2f}s")
        log.write(f"PCAP {pcap_path}")
        return result, detail
    except Exception as error:
        log.write(f"RESULT {name} FAIL exception={error!r}")
        return "FAIL", repr(error)
    finally:
        if peer is not None:
            peer.close()
        stop_endpoint(endpoint, log)
        log.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--scenario", action="append", default=[])
    parser.add_argument("--list", action="store_true")
    args = parser.parse_args()
    if args.list:
        print("\n".join(SCENARIOS))
        return 0
    names = args.scenario or list(SCENARIOS)
    for name in names:
        if name not in SCENARIOS:
            print(f"unknown scenario {name}", file=sys.stderr)
            return 2
    ensure_root()
    host_mac, peer_mac = raw.setup_veth()
    results = {}
    try:
        for name in names:
            results[name] = run_one(name, host_mac, peer_mac)
    finally:
        raw.teardown_veth()
    failures = [name for name, (result, _) in results.items() if result == "FAIL"]
    for name in names:
        result, detail = results[name]
        print(f"{name}: {result} {detail}")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
