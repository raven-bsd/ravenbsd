#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""Unit tests for the boot-time-login service-line parser."""

import importlib.util
from pathlib import Path


def load_module():
    path = Path(__file__).with_name("boot-time-login.py")
    spec = importlib.util.spec_from_file_location("boot_time_login", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def test_parse_service_start():
    module = load_module()
    assert module.parse_service_start("1795445681.300 randd: started") == "randd"
    assert module.parse_service_start("svcd: randd up") is None
    assert module.parse_service_start("random text") is None


def test_parse_service_up():
    module = load_module()
    assert module.parse_service_up("svcd: vfsd up") == "vfsd"
    assert module.parse_service_up("svcd: bootstrap plan: logd pcid") is None
    assert module.parse_service_up("1795445681.300 randd: started") is None


def test_parse_loader_mark():
    module = load_module()
    assert module.parse_loader_mark("loader: dtb 0x0+0x0 counter_freq=0") == "dtb"
    assert module.parse_loader_mark("loader: memory map: 26 regions") == "memory_map"
    assert module.parse_loader_mark("loader: loader image 0x1e010000+0x15000") == "image"
    assert module.parse_loader_mark("loader: page tables: building") == "paging_start"
    assert module.parse_loader_mark("loader: page tables: built") == "paging_end"
    assert (
        module.parse_loader_mark("loader: jumping to kernel entry 0xffffffff80200000")
        == "jump"
    )
    assert module.parse_loader_mark("RavenBSD kernel 0.1 (x86_64)") is None


if __name__ == "__main__":
    test_parse_service_start()
    test_parse_service_up()
    test_parse_loader_mark()
    print("boot-time-login_ut: PASS")
