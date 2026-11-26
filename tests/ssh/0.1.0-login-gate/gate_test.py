#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

"""Unit checks for the S85k SSH login gate classifier and client argv.

The gate is a test harness, so the failure modes are the contract: a console
login, a missing sshd, a refused password, a failed host key exchange and a
failed PTY request must never be reported as a successful SSH login.
"""

from __future__ import annotations

import unittest

import gate

MARKER = "S85K-SSH-LOGIN-OK"


class MarkerTest(unittest.TestCase):
    def test_exact_marker_is_a_pass(self) -> None:
        self.assertTrue(gate.marker_present(MARKER + "\n"))

    def test_console_transcript_is_not_a_marker(self) -> None:
        transcript = (
            "RavenBSD 0.1 (x86_64) (tty:console)\n"
            "login: root\n"
            "password:\n"
            "raven# echo S85K-SSH-LOGIN-OK\n"
            "S85K-SSH-LOGIN-OK\n"
        )
        self.assertFalse(gate.marker_present(transcript))

    def test_empty_output_is_not_a_marker(self) -> None:
        self.assertFalse(gate.marker_present(""))


class ClassifierTest(unittest.TestCase):
    def test_missing_sshd_probe_wins_over_the_serial_login(self) -> None:
        serial = "login: root\npassword:\nraven# " + gate.SSHD_ABSENT
        stage = gate.classify(
            serial=serial,
            sshd_probe=gate.SSHD_ABSENT,
            client_rc=None,
            client_stdout="",
            client_stderr="",
        )
        self.assertEqual(stage, gate.SSHD_MISSING)

    def test_incomplete_sshd_installation_is_its_own_stage(self) -> None:
        serial = "raven# " + gate.SSHD_INCOMPLETE
        stage = gate.classify(
            serial=serial,
            sshd_probe=gate.SSHD_INCOMPLETE,
            client_rc=None,
            client_stdout="",
            client_stderr="",
        )
        self.assertEqual(stage, gate.SSHD_INCOMPLETE)

    def test_connection_refused_is_not_listening(self) -> None:
        stage = gate.classify(
            serial="raven# ",
            sshd_probe=gate.SSHD_PRESENT,
            client_rc=255,
            client_stdout="",
            client_stderr="ssh: connect to host 127.0.0.1 port 2222: Connection refused",
        )
        self.assertEqual(stage, gate.SSHD_NOT_LISTENING)

    def test_permission_denied_is_authentication(self) -> None:
        stage = gate.classify(
            serial="raven# ",
            sshd_probe=gate.SSHD_PRESENT,
            client_rc=255,
            client_stdout="",
            client_stderr="raven@127.0.0.1: Permission denied (password).",
        )
        self.assertEqual(stage, gate.AUTH)

    def test_kex_failure_is_host_key_or_handshake(self) -> None:
        stage = gate.classify(
            serial="raven# ",
            sshd_probe=gate.SSHD_PRESENT,
            client_rc=255,
            client_stdout="",
            client_stderr="kex_exchange_identification: Connection closed by remote host",
        )
        self.assertEqual(stage, gate.HANDSHAKE)

    def test_pty_allocation_failure_is_pty(self) -> None:
        stage = gate.classify(
            serial="raven# ",
            sshd_probe=gate.SSHD_PRESENT,
            client_rc=0,
            client_stdout=MARKER + "\n",
            client_stderr="PTY allocation request failed on channel 0\n",
        )
        self.assertEqual(stage, gate.PTY)

    def test_marker_without_clean_close_is_a_session_failure(self) -> None:
        stage = gate.classify(
            serial="raven# ",
            sshd_probe=gate.SSHD_PRESENT,
            client_rc=0,
            client_stdout=MARKER + "\n",
            client_stderr="",
        )
        self.assertEqual(stage, gate.SESSION)

    def test_marker_with_clean_close_passes(self) -> None:
        stage = gate.classify(
            serial="raven# ",
            sshd_probe=gate.SSHD_PRESENT,
            client_rc=0,
            client_stdout=MARKER + "\n",
            client_stderr="Connection to 127.0.0.1 closed.\n",
        )
        self.assertIsNone(stage)


class ClientArgvTest(unittest.TestCase):
    def test_client_argv_targets_the_hostfwd_port_and_the_guest_user(self) -> None:
        argv = gate.client_argv(
            ssh="/usr/bin/ssh",
            port=2222,
            user="raven",
            known_hosts="/tmp/known_hosts",
            command="echo " + MARKER,
            allocate_pty=False,
        )
        joined = " ".join(argv)
        self.assertIn("raven@127.0.0.1", argv)
        self.assertIn("2222", argv)
        self.assertIn("PreferredAuthentications=password", joined)
        self.assertIn("PubkeyAuthentication=no", joined)
        self.assertEqual(argv[-1], "echo " + MARKER)

    def test_client_argv_requests_a_pty_only_for_the_pty_row(self) -> None:
        argv = gate.client_argv(
            ssh="/usr/bin/ssh",
            port=2222,
            user="raven",
            known_hosts="/tmp/known_hosts",
            command="pwd",
            allocate_pty=True,
        )
        self.assertIn("-tt", argv)
        self.assertNotIn("-T", argv)


class SshdProbeTest(unittest.TestCase):
    def test_probe_markers_map_to_the_staged_state(self) -> None:
        self.assertEqual(gate.sshd_probe_state("x\n" + gate.SSHD_ABSENT + "\n"), gate.SSHD_ABSENT)
        self.assertEqual(
            gate.sshd_probe_state("x\n" + gate.SSHD_INCOMPLETE + "\n"), gate.SSHD_INCOMPLETE
        )
        self.assertEqual(gate.sshd_probe_state("x\n" + gate.SSHD_PRESENT + "\n"), gate.SSHD_PRESENT)
        self.assertEqual(gate.sshd_probe_state("no marker"), gate.SSHD_UNKNOWN)


if __name__ == "__main__":
    unittest.main()
