#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

"""Unit tests for the S85n 0.1.0 network and TCP gate.

These tests pin the gate's own contract before the QEMU cells run: the QEMU
argument vector, the mutation inputs, the exact payload acceptance rule and the
stage classifier. They never boot a guest.
"""

from __future__ import annotations

import unittest

import gate


class QemuContractTest(unittest.TestCase):
    def test_qemu_argv_carries_virtio_net_and_hostfwd(self) -> None:
        argv = gate.qemu_argv(
            arch="x86_64",
            accel="tcg",
            image=gate.Path("/tmp/image.img"),
            firmware_code=gate.Path("/tmp/code.fd"),
            firmware_vars=gate.Path("/tmp/vars.fd"),
            serial=gate.Path("/tmp/serial.log"),
            host_port=24085,
            guest_port=gate.PROBE_PORT,
        )
        joined = " ".join(argv)
        self.assertIn("virtio-net-pci,netdev=n0,disable-legacy=on", joined)
        self.assertIn(
            f"user,id=n0,hostfwd=tcp:127.0.0.1:24085-:{gate.PROBE_PORT}", joined
        )
        self.assertIn("file:/tmp/serial.log", joined)
        self.assertNotIn("-enable-kvm", joined)

    def test_kvm_admits_the_host_accelerator(self) -> None:
        argv = gate.qemu_argv(
            arch="x86_64",
            accel="kvm",
            image=gate.Path("/tmp/image.img"),
            firmware_code=gate.Path("/tmp/code.fd"),
            firmware_vars=gate.Path("/tmp/vars.fd"),
            serial=gate.Path("/tmp/serial.log"),
            host_port=24085,
            guest_port=gate.PROBE_PORT,
        )
        self.assertEqual(argv[argv.index("-cpu") + 1], "host")
        self.assertIn("-enable-kvm", argv)


class MutationInputTest(unittest.TestCase):
    def test_no_mutation_keeps_the_probe_port(self) -> None:
        self.assertEqual(gate.mutation_guest_port("none", gate.PROBE_PORT), gate.PROBE_PORT)

    def test_closed_port_mutation_forwards_elsewhere(self) -> None:
        target = gate.mutation_guest_port("closed-port", gate.PROBE_PORT)
        self.assertNotEqual(target, gate.PROBE_PORT)

    def test_drop_payload_mutation_truncates_the_payload(self) -> None:
        self.assertEqual(
            gate.mutation_payload("drop-payload", gate.PAYLOAD), gate.PAYLOAD[:-1]
        )

    def test_no_mutation_keeps_the_payload(self) -> None:
        self.assertEqual(gate.mutation_payload("none", gate.PAYLOAD), gate.PAYLOAD)

    def test_unknown_mutation_is_rejected(self) -> None:
        with self.assertRaises(ValueError):
            gate.mutation_payload("invented", gate.PAYLOAD)


class PayloadTest(unittest.TestCase):
    def test_exact_echo_passes(self) -> None:
        self.assertTrue(gate.echo_matches(gate.PAYLOAD, gate.PAYLOAD))

    def test_short_echo_fails(self) -> None:
        self.assertFalse(gate.echo_matches(gate.PAYLOAD, gate.PAYLOAD[:-1]))

    def test_changed_echo_fails(self) -> None:
        self.assertFalse(gate.echo_matches(gate.PAYLOAD, gate.PAYLOAD[:-1] + b"X"))


class ClassifierTest(unittest.TestCase):
    def serial(self) -> str:
        return "\n".join(
            [
                "net-gate: interface eth0 10.0.2.15/24 up",
                f"net-gate: listening tcp 10.0.2.15:{gate.PROBE_PORT}",
                f"net-gate: echo received {len(gate.PAYLOAD)} bytes payload="
                + gate.PAYLOAD.decode(),
                f"net-gate: echo sent {len(gate.PAYLOAD)} bytes",
                f"net-gate: host ack {gate.ACK.decode()}",
                "net-gate: done",
            ]
        )

    def host(self) -> str:
        return "\n".join(
            [
                "net-gate-host: connected 127.0.0.1:24085",
                f"net-gate-host: sent {len(gate.PAYLOAD)} bytes",
                f"net-gate-host: echo received {len(gate.PAYLOAD)} bytes payload="
                + gate.PAYLOAD.decode(),
                f"net-gate-host: ack sent {gate.ACK.decode()}",
                "net-gate-host: done",
            ]
        )

    def test_complete_round_trip_passes(self) -> None:
        self.assertIsNone(
            gate.classify(serial=self.serial(), host=self.host(), qemu_exit=33)
        )

    def test_panic_is_named(self) -> None:
        self.assertEqual(
            gate.classify(
                serial=self.serial() + "\nKernel panic: boom",
                host=self.host(),
                qemu_exit=1,
            ),
            gate.PANIC,
        )

    def test_missing_interface_is_named(self) -> None:
        serial = self.serial().replace("net-gate: interface eth0 10.0.2.15/24 up\n", "")
        self.assertEqual(
            gate.classify(serial=serial, host=self.host(), qemu_exit=33),
            gate.INTERFACE,
        )

    def test_missing_listener_is_named(self) -> None:
        serial = self.serial().replace(
            f"net-gate: listening tcp 10.0.2.15:{gate.PROBE_PORT}\n", ""
        )
        self.assertEqual(
            gate.classify(serial=serial, host="", qemu_exit=33), gate.LISTENER
        )

    def test_refused_connect_is_named(self) -> None:
        host = "net-gate-host: echo read failed: Connection reset by peer"
        self.assertEqual(
            gate.classify(serial=self.serial(), host=host, qemu_exit=1), gate.CONNECT
        )

    def test_missing_echo_is_named(self) -> None:
        host = self.host().replace(
            f"net-gate-host: echo received {len(gate.PAYLOAD)} bytes payload="
            + gate.PAYLOAD.decode()
            + "\n",
            "",
        )
        self.assertEqual(
            gate.classify(serial=self.serial(), host=host, qemu_exit=1), gate.ECHO
        )

    def test_nonzero_qemu_exit_is_named(self) -> None:
        self.assertEqual(
            gate.classify(serial=self.serial(), host=self.host(), qemu_exit=1),
            gate.SESSION,
        )


class ArgumentTest(unittest.TestCase):
    def test_defaults(self) -> None:
        options = gate.parse_args([])
        self.assertEqual(options.arch, "x86_64")
        self.assertEqual(options.accel, "kvm")
        self.assertEqual(options.mutation, "none")

    def test_unknown_accel_is_rejected(self) -> None:
        with self.assertRaises(SystemExit):
            gate.parse_args(["--accel", "s390x"])

    def test_unknown_arch_is_rejected(self) -> None:
        with self.assertRaises(SystemExit):
            gate.parse_args(["--arch", "aarch64"])


if __name__ == "__main__":
    unittest.main(verbosity=2)
