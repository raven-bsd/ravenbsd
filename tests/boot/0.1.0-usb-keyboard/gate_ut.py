#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

"""Unit tests for the S85p USB keyboard console gate harness.

Run: python3 tests/boot/0.1.0-usb-keyboard/gate_ut.py
"""

import unittest

import gate


class KeyNameTests(unittest.TestCase):
    def test_lowercase_letters_and_digits_pass_through(self):
        self.assertEqual(gate.key_name("r"), "r")
        self.assertEqual(gate.key_name("8"), "8")

    def test_space_and_hyphen_use_qemu_key_names(self):
        self.assertEqual(gate.key_name(" "), "spc")
        self.assertEqual(gate.key_name("-"), "minus")

    def test_uppercase_letters_require_shift(self):
        self.assertEqual(gate.key_name("S"), "shift-s")
        self.assertEqual(gate.key_name("P"), "shift-p")

    def test_unsupported_character_is_rejected(self):
        with self.assertRaises(ValueError):
            gate.key_name("ä")

    def test_key_sequence_terminates_with_enter(self):
        self.assertEqual(
            gate.key_sequence("root"),
            ["r", "o", "o", "t", "ret"],
        )
        self.assertEqual(
            gate.key_sequence("echo s85p-usb-ok"),
            [
                "e", "c", "h", "o", "spc",
                "s", "8", "5", "p", "minus",
                "u", "s", "b", "minus", "o", "k",
                "ret",
            ],
        )


class QmpCommandTests(unittest.TestCase):
    def test_input_event_is_routed_to_the_usb_display(self):
        self.assertEqual(
            gate.input_event_command("ret", True),
            {
                "execute": "input-send-event",
                "arguments": {
                    "device": "video0",
                    "events": [
                        {
                            "type": "key",
                            "data": {
                                "key": {"type": "qcode", "data": "ret"},
                                "down": True,
                            },
                        }
                    ],
                },
            },
        )

    def test_input_event_carries_the_release(self):
        command = gate.input_event_command("r", False)
        self.assertFalse(command["arguments"]["events"][0]["data"]["down"])


class StageTests(unittest.TestCase):
    def test_console_transcript_stage(self):
        self.assertEqual(
            gate.console_stage("RavenBSD kernel 0.1 (x86_64)\nlogin: "),
            gate.BOOT,
        )
        self.assertEqual(
            gate.console_stage("login: \nPassword: "),
            gate.INPUT,
        )
        self.assertEqual(
            gate.console_stage("login: \nPassword: \nraven# "),
            gate.LOGIN,
        )
        self.assertEqual(
            gate.console_stage(
                "login: \nPassword: \nraven# \ns85p-usb-ok\nraven# "
            ),
            gate.MARKER,
        )

    def test_missing_login_prompt_is_boot(self):
        self.assertEqual(gate.console_stage("RavenBSD kernel 0.1\n"), gate.BOOT)

    def test_panic_wins_over_any_stage(self):
        self.assertEqual(
            gate.console_stage("login: \npanic: page fault\n"), gate.PANIC
        )

    def test_rust_panic_is_a_panic(self):
        self.assertEqual(
            gate.console_stage(
                "login: \nthread 'main' (1) panicked at src/main.rs:1:1:\n"
            ),
            gate.PANIC,
        )

    def test_exit_code_names_the_failed_stage(self):
        self.assertEqual(gate.EXIT_STAGE[gate.PANIC], 14)
        self.assertEqual(gate.EXIT_STAGE[gate.INPUT], 11)
        self.assertEqual(gate.EXIT_STAGE[gate.LOGIN], 12)
        self.assertEqual(gate.EXIT_STAGE[gate.MARKER], 13)
        self.assertEqual(gate.EXIT_STAGE[gate.IMAGE], 3)

    def test_artifact_suffix_names_the_cell_and_mutation(self):
        self.assertEqual(
            gate.artifact_suffix("x86_64", "kvm", "none"), "x86_64-kvm"
        )
        self.assertEqual(
            gate.artifact_suffix("x86_64", "tcg", "wrong-password"),
            "x86_64-tcg-wrong-password",
        )


class MutationTests(unittest.TestCase):
    def test_wrong_password_is_a_supported_mutation(self):
        self.assertEqual(gate.password_for("raven", "wrong-password"), "wrong")

    def test_normal_password_is_unchanged(self):
        self.assertEqual(gate.password_for("raven", "none"), "raven")

    def test_every_cell_attaches_the_routed_usb_keyboard(self):
        argv = gate.qemu_argv(
            "x86_64", "tcg", gate.Path("image.img"), gate.Path("vars.fd"),
            gate.Path("code.fd"), gate.Path("serial.log"), gate.Path("qmp.sock"),
        )
        self.assertIn("usb-kbd,display=video0", argv)
        self.assertIn("VGA,id=video0", argv)
        self.assertIn("qemu-xhci", argv)


if __name__ == "__main__":
    unittest.main()
