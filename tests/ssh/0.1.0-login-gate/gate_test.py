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
        serial = "login: root\npassword:\nraven# " + gate.PROBE_ABSENT
        stage = gate.classify(
            serial=serial,
            sshd_probe=gate.PROBE_ABSENT,
            client_rc=None,
            client_stdout="",
            client_stderr="",
        )
        self.assertEqual(stage, gate.SSHD_MISSING)

    def test_incomplete_sshd_installation_is_its_own_stage(self) -> None:
        serial = "raven# " + gate.PROBE_INCOMPLETE
        stage = gate.classify(
            serial=serial,
            sshd_probe=gate.PROBE_INCOMPLETE,
            client_rc=None,
            client_stdout="",
            client_stderr="",
        )
        self.assertEqual(stage, gate.SSHD_INCOMPLETE)

    def test_connection_refused_is_not_listening(self) -> None:
        stage = gate.classify(
            serial="raven# ",
            sshd_probe=gate.PROBE_PRESENT,
            client_rc=255,
            client_stdout="",
            client_stderr="ssh: connect to host 127.0.0.1 port 2222: Connection refused",
        )
        self.assertEqual(stage, gate.SSHD_NOT_LISTENING)

    def test_permission_denied_is_authentication(self) -> None:
        stage = gate.classify(
            serial="raven# ",
            sshd_probe=gate.PROBE_PRESENT,
            client_rc=255,
            client_stdout="",
            client_stderr="raven@127.0.0.1: Permission denied (password).",
        )
        self.assertEqual(stage, gate.AUTH)

    def test_kex_failure_is_host_key_or_handshake(self) -> None:
        stage = gate.classify(
            serial="raven# ",
            sshd_probe=gate.PROBE_PRESENT,
            client_rc=255,
            client_stdout="",
            client_stderr="kex_exchange_identification: Connection closed by remote host",
        )
        self.assertEqual(stage, gate.HANDSHAKE)

    def test_pty_allocation_failure_is_pty(self) -> None:
        stage = gate.classify(
            serial="raven# ",
            sshd_probe=gate.PROBE_PRESENT,
            client_rc=0,
            client_stdout=MARKER + "\n",
            client_stderr="PTY allocation request failed on channel 0\n",
        )
        self.assertEqual(stage, gate.PTY)

    def test_marker_without_clean_close_is_a_session_failure(self) -> None:
        stage = gate.classify(
            serial="raven# ",
            sshd_probe=gate.PROBE_PRESENT,
            client_rc=0,
            client_stdout=MARKER + "\n",
            client_stderr="",
        )
        self.assertEqual(stage, gate.SESSION)

    def test_marker_without_the_freed_channel_is_a_session_failure(self) -> None:
        stage = gate.classify(
            serial="raven# ",
            sshd_probe=gate.PROBE_PRESENT,
            client_rc=0,
            client_stdout=MARKER + "\n",
            client_stderr="debug1: Exit status 0\n",
        )
        self.assertEqual(stage, gate.SESSION)

    def test_marker_with_clean_close_passes(self) -> None:
        stage = gate.classify(
            serial="raven# ",
            sshd_probe=gate.PROBE_PRESENT,
            client_rc=0,
            client_stdout=MARKER + "\n",
            client_stderr=(
                "debug1: client_input_channel_req: channel 0 rtype exit-status reply 0\n"
                "debug1: channel 0: free: client-session, nchannels 1\n"
                "debug1: Exit status 0\n"
            ),
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


class SshdStateTest(unittest.TestCase):
    def test_boot_report_without_sshd_row_is_missing(self) -> None:
        serial = "000.00  netd   ok\n000.01  getty  ok\nlogin: "
        self.assertEqual(gate.sshd_state_from_serial(serial), gate.PROBE_ABSENT)

    def test_boot_report_with_failed_sshd_row_is_not_listening(self) -> None:
        serial = "000.00  sshd   failed\n000.01  getty  ok\n"
        self.assertEqual(gate.sshd_state_from_serial(serial), gate.PROBE_NOT_LISTENING)

    def test_boot_report_with_ok_sshd_row_is_present(self) -> None:
        serial = "000.00  sshd   \x1b[32mok\x1b[0m\n"
        self.assertEqual(gate.sshd_state_from_serial(serial), gate.PROBE_PRESENT)


if __name__ == "__main__":
    unittest.main()
