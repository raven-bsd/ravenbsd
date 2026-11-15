# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Generate the Pi 5 DTB fixtures from the documented node shapes. The values are transcribed
# from the official firmware DTB (rpi-firmware/boot/bcm2712-rpi-5-b.dtb) and the
# NetBSD/Linux bcm2712 DTS sources; see README.md for the exact citations. The invalid fixtures
# use synthetic overlaps or malformed cell counts, never invented hardware addresses.
#
# Run from the repository root: python3 tests/fixtures/pi5/generate.py

from __future__ import annotations

import struct
from pathlib import Path

FDT_MAGIC = 0xD00D_FEED
FDT_BEGIN_NODE = 1
FDT_END_NODE = 2
FDT_PROP = 3
FDT_END = 9

# Values decoded from rpi-firmware/boot/bcm2712-rpi-5-b.dtb.
SOC_RANGES = [0x0, 0x10, 0x0, 0x8000_0000]
SOC_SERIAL_REG = [0x7D00_1000, 0x200]
SOC_GIC_REG = [
    0x7FFF_9000, 0x1000,
    0x7FFF_A000, 0x2000,
    0x7FFF_C000, 0x2000,
    0x7FFF_E000, 0x2000,
]
SOC_TIMER_REG = [0x7C00_3000, 0x1000]
PCIE_REG = [0x10, 0x120000, 0x0, 0x9310]
PCIE_RANGES = [
    0x0200_0000, 0x0, 0x0, 0x1F, 0x0, 0x0, 0xFFFF_FFFC,
    0x4300_0000, 0x4, 0x0, 0x1C, 0x0, 0x3, 0x0,
]
RP1_RANGES = [0xC0, 0x4000_0000, 0x0200_0000, 0x0, 0x0, 0x0, 0x41_0000]
RP1_SERIAL_REG = [0xC0, 0x4003_0000, 0x0, 0x100]
RP1_MAILBOX_REG = [0xC0, 0x4000_8000, 0x0, 0x4000]
RP1_CLOCKS_REG = [0xC0, 0x4001_8000, 0x0, 0x1_0038]
RP1_ETHERNET_REG = [0xC0, 0x4010_0000, 0x0, 0x4000]
RP1_GPIO_BANKS = [
    0xC0, 0x400D_0000, 0x0, 0xC000,
    0xC0, 0x400E_0000, 0x0, 0xC000,
    0xC0, 0x400F_0000, 0x0, 0xC000,
]
RP1_GPIO_INTERRUPTS = [0x0, 0x4, 0x1, 0x4, 0x2, 0x4]
RP1_GPIO_PINS = 0x36
RP1_GPIO_LINE_NAMES = [
    "ID_SDA", "ID_SCL", "GPIO2", "GPIO3", "GPIO4", "GPIO5", "GPIO6", "GPIO7",
    "GPIO8", "GPIO9", "GPIO10", "GPIO11", "GPIO12", "GPIO13", "GPIO14", "GPIO15",
    "GPIO16", "GPIO17", "GPIO18", "GPIO19", "GPIO20", "GPIO21", "GPIO22", "GPIO23",
    "GPIO24", "GPIO25", "GPIO26", "GPIO27", "PCIE_RP1_WAKE", "FAN_TACH",
    "HOST_SDA", "HOST_SCL", "ETH_RST_N", "-", "CD0_IO0_MICCLK",
    "CD0_IO0_MICDAT0", "RP1_PCIE_CLKREQ_N", "-", "CD0_SDA", "CD0_SCL", "CD1_SDA",
    "CD1_SCL", "USB_VBUS_EN", "USB_OC_N", "RP1_STAT_LED", "FAN_PWM",
    "CD1_IO0_MICCLK", "2712_WAKE", "CD1_IO1_MICDAT1", "EN_MAX_USB_CUR", "-", "-",
    "-", "-",
]
RP1_ASSIGNED_CLOCKS = [
    0x2, 0x0, 0x2, 0x1, 0x2, 0x3, 0x2, 0x9, 0x2, 0x10, 0x2, 0x4, 0x2, 0xA,
    0x2, 0xC, 0x2, 0x6, 0x2, 0xD, 0x2, 0x1F, 0x2, 0x20, 0x2, 0x1D,
]
RP1_ASSIGNED_RATES = [
    0x3B9A_CA00, 0x5B8D_8000, 0x0BEB_C200, 0x0773_5940, 0x0773_5940, 0x03A9_8000,
    0x0927_C000, 0x0BEB_C200, 0x05F5_E100, 0x02FA_F080, 0x000F_4240, 0x0BEB_C200,
    0x02FA_F080,
]
RP1_PIO_REG = [0xC0, 0x4017_8000, 0x0, 0x20]
RP1_USB0_REG = [0xC0, 0x4020_0000, 0x0, 0x10_0000]
RP1_USB1_REG = [0xC0, 0x4030_0000, 0x0, 0x10_0000]
RP1_SRAM_REG = [0xC0, 0x4040_0000, 0x0, 0x1_0000]
RP1_SRAM_RANGES = [0x0, 0xC0, 0x4040_0000, 0x1_0000]
RP1_SHMEM_REG = [0xFF00, 0x100]
RP1_GPIO_PHANDLE = 0x30
RP1_CLOCKS_PHANDLE = 0x2
RP1_MAILBOX_PHANDLE = 0x5B
RP1_SHMEM_PHANDLE = 0x5C
RP1_FIRMWARE_PHANDLE = 0x41


class Node:
    def __init__(self, name: str, props=None, children=None):
        self.name = name
        self.props = props or []
        self.children = children or []


def text(name: str, value: str):
    return (name, "str", value)


def text_list(name: str, values):
    return (name, "strings", list(values))


def cells(name: str, values):
    return (name, "cells", list(values))


def flag(name: str):
    return (name, "empty", None)


def encode_prop(kind: str, value) -> bytes:
    if kind == "str":
        return value.encode() + b"\0"
    if kind == "strings":
        return b"".join(item.encode() + b"\0" for item in value)
    if kind == "cells":
        return b"".join(struct.pack(">I", item) for item in value)
    if kind == "empty":
        return b""
    raise ValueError(f"unknown property kind {kind}")


def dts_prop(kind: str, value) -> str:
    if kind == "str":
        return f'"{value}"'
    if kind == "strings":
        return ", ".join(f'"{item}"' for item in value)
    if kind == "cells":
        return "<" + " ".join(f"0x{item:x}" for item in value) + ">"
    if kind == "empty":
        return ""
    raise ValueError(f"unknown property kind {kind}")


def build_blob(root: Node) -> bytes:
    strings = bytearray()
    offsets: dict[str, int] = {}

    def string_offset(name: str) -> int:
        if name not in offsets:
            offsets[name] = len(strings)
            strings.extend(name.encode() + b"\0")
        return offsets[name]

    def emit(node: Node) -> bytes:
        out = bytearray(struct.pack(">I", FDT_BEGIN_NODE))
        out.extend(node.name.encode() + b"\0")
        out.extend(b"\0" * ((4 - len(out) % 4) % 4))
        for name, kind, value in node.props:
            encoded = encode_prop(kind, value)
            out.extend(struct.pack(">III", FDT_PROP, len(encoded), string_offset(name)))
            out.extend(encoded)
            out.extend(b"\0" * ((4 - len(encoded) % 4) % 4))
        for child in node.children:
            out.extend(emit(child))
        out.extend(struct.pack(">I", FDT_END_NODE))
        return bytes(out)

    structure = emit(root) + struct.pack(">I", FDT_END)
    off_rsv = 40
    off_struct = off_rsv + 16
    off_strings = off_struct + len(structure)
    total = off_strings + len(strings)
    total = (total + 3) & ~3
    header = struct.pack(
        ">10I",
        FDT_MAGIC,
        total,
        off_struct,
        off_strings,
        off_rsv,
        17,
        16,
        0,
        len(strings),
        len(structure),
    )
    blob = bytearray(header)
    blob.extend(struct.pack(">QQ", 0, 0))
    blob.extend(structure)
    blob.extend(strings)
    blob.extend(b"\0" * (total - len(blob)))
    return bytes(blob)


def dts(root: Node) -> str:
    lines = [
        "// SPDX-License-Identifier: BSD-3-Clause",
        "// Generated by tests/fixtures/pi5/generate.py; values from the official Pi 5 DTB.",
        "/dts-v1/;",
        "",
    ]

    def emit(node: Node, indent: int) -> None:
        name = node.name or "/"
        lines.append(" " * indent + f"{name} {{")
        for prop_name, kind, value in node.props:
            rendered = dts_prop(kind, value)
            suffix = f" = {rendered}" if rendered else ""
            lines.append(" " * (indent + 4) + f"{prop_name}{suffix};")
        for child in node.children:
            emit(child, indent + 4)
        lines.append(" " * indent + "};")

    emit(root, 0)
    return "\n".join(lines) + "\n"


def soc_node(uart_status: str, include_gic: bool) -> Node:
    children = [
        Node(
            "serial@7d001000",
            [
                text_list("compatible", ["arm,pl011-axi", "arm,pl011", "arm,primecell"]),
                cells("reg", SOC_SERIAL_REG),
                cells("interrupts", [0x0, 0x79, 0x4]),
                text("status", uart_status),
            ],
        )
    ]
    if include_gic:
        children.append(
            Node(
                "interrupt-controller@7fff9000",
                [
                    text_list("compatible", ["arm,gic-400"]),
                    cells("reg", SOC_GIC_REG),
                    flag("interrupt-controller"),
                    cells("#interrupt-cells", [0x3]),
                ],
            )
        )
    children.append(
        Node(
            "timer@7c003000",
            [
                text_list("compatible", ["brcm,bcm2835-system-timer"]),
                cells("reg", SOC_TIMER_REG),
                cells("interrupts", [0x0, 0x40, 0x4, 0x0, 0x41, 0x4, 0x0, 0x42, 0x4, 0x0, 0x43, 0x4]),
                cells("clock-frequency", [1_000_000]),
            ],
        )
    )
    return Node(
        "soc@107c000000",
        [
            text_list("compatible", ["simple-bus"]),
            cells("#address-cells", [0x1]),
            cells("#size-cells", [0x1]),
            cells("ranges", SOC_RANGES),
        ],
        children,
    )


def rp1_gpio_node(pins: int = RP1_GPIO_PINS) -> Node:
    return Node(
        "gpio@d0000",
        [
            text_list("compatible", ["raspberrypi,rp1-gpio"]),
            cells("reg", RP1_GPIO_BANKS),
            cells("interrupts", RP1_GPIO_INTERRUPTS),
            flag("gpio-controller"),
            cells("#gpio-cells", [0x2]),
            flag("interrupt-controller"),
            cells("#interrupt-cells", [0x2]),
            cells("gpio-ranges", [RP1_GPIO_PHANDLE, 0x0, 0x0, pins]),
            text_list("gpio-line-names", RP1_GPIO_LINE_NAMES),
            text("status", "okay"),
            cells("phandle", [RP1_GPIO_PHANDLE]),
        ],
        [
            Node(
                "rp1_uart0_14_15",
                [],
                [
                    Node(
                        "pin_txd",
                        [
                            text("function", "uart0"),
                            text_list("pins", ["gpio14"]),
                            flag("bias-disable"),
                        ],
                    ),
                    Node(
                        "pin_rxd",
                        [
                            text("function", "uart0"),
                            text_list("pins", ["gpio15"]),
                            flag("bias-pull-up"),
                        ],
                    ),
                ],
            ),
            Node(
                "rp1_uart0_ctsrts_16_17",
                [],
                [
                    Node(
                        "pin_cts",
                        [
                            text("function", "uart0"),
                            text_list("pins", ["gpio16"]),
                            flag("bias-pull-up"),
                        ],
                    ),
                    Node(
                        "pin_rts",
                        [
                            text("function", "uart0"),
                            text_list("pins", ["gpio17"]),
                            flag("bias-disable"),
                        ],
                    ),
                ],
            ),
            Node(
                "rp1_i2c0_0_1",
                [],
                [
                    Node(
                        "pin_sda",
                        [
                            text("function", "i2c0"),
                            text_list("pins", ["gpio0"]),
                            flag("bias-pull-up"),
                        ],
                    ),
                    Node(
                        "pin_scl",
                        [
                            text("function", "i2c0"),
                            text_list("pins", ["gpio1"]),
                            flag("bias-pull-up"),
                        ],
                    ),
                ],
            ),
        ],
    )


def rp1_node(
    uart_status: str,
    mfd: bool = False,
    mfd_pins: int = RP1_GPIO_PINS,
    mfd_rates=None,
) -> Node:
    mfd_rates = mfd_rates or RP1_ASSIGNED_RATES
    mailbox_props = [
        text_list("compatible", ["raspberrypi,rp1-mbox"]),
        cells("reg", RP1_MAILBOX_REG),
        text("status", "okay"),
    ]
    clocks_props = [
        text_list("compatible", ["raspberrypi,rp1-clocks"]),
        cells("reg", RP1_CLOCKS_REG),
    ]
    children = [
        Node(
            "serial@30000",
            [
                text_list("compatible", ["arm,pl011-axi", "arm,pl011"]),
                cells("reg", RP1_SERIAL_REG),
                text("status", uart_status),
            ],
        ),
        Node("mailbox@8000", mailbox_props),
        Node("clocks@18000", clocks_props),
        Node(
            "ethernet@100000",
            [
                text_list("compatible", ["raspberrypi,rp1-gem"]),
                cells("reg", RP1_ETHERNET_REG),
                text("status", "okay"),
            ],
        ),
    ]
    if mfd:
        mailbox_props.append(cells("phandle", [RP1_MAILBOX_PHANDLE]))
        clocks_props.append(cells("#clock-cells", [0x1]))
        clocks_props.append(cells("phandle", [RP1_CLOCKS_PHANDLE]))
        clocks_props.append(cells("assigned-clocks", RP1_ASSIGNED_CLOCKS))
        clocks_props.append(cells("assigned-clock-rates", mfd_rates))
        children.extend(
            [
                rp1_gpio_node(mfd_pins),
                Node(
                    "pio@178000",
                    [
                        text_list("compatible", ["raspberrypi,rp1-pio"]),
                        cells("reg", RP1_PIO_REG),
                        text("status", "okay"),
                    ],
                ),
                Node(
                    "usb@200000",
                    [
                        text_list("compatible", ["brcm,bcm2835-usb"]),
                        cells("reg", RP1_USB0_REG),
                        text("status", "disabled"),
                    ],
                ),
                Node(
                    "usb@300000",
                    [
                        text_list("compatible", ["brcm,bcm2835-usb"]),
                        cells("reg", RP1_USB1_REG),
                        text("status", "disabled"),
                    ],
                ),
                Node(
                    "sram@400000",
                    [
                        cells("reg", RP1_SRAM_REG),
                        cells("#address-cells", [0x1]),
                        cells("#size-cells", [0x1]),
                        cells("ranges", RP1_SRAM_RANGES),
                    ],
                    [
                        Node(
                            "shmem@ff00",
                            [
                                text_list("compatible", ["raspberrypi,rp1-shmem"]),
                                cells("reg", RP1_SHMEM_REG),
                                cells("phandle", [RP1_SHMEM_PHANDLE]),
                            ],
                        )
                    ],
                ),
            ]
        )
    return Node(
        "rp1",
        [
            text_list("compatible", ["simple-bus"]),
            cells("#address-cells", [0x2]),
            cells("#size-cells", [0x2]),
            cells("ranges", RP1_RANGES),
        ],
        children,
    )


def pcie_node(rp1: Node | None, ranges) -> Node:
    children = [rp1] if rp1 is not None else []
    return Node(
        "pcie@1000120000",
        [
            text_list("compatible", ["brcm,bcm2712-pcie"]),
            cells("reg", PCIE_REG),
            text("device_type", "pci"),
            cells("#address-cells", [0x3]),
            cells("#size-cells", [0x2]),
            cells("ranges", ranges),
            text("status", "okay"),
        ],
        children,
    )


def platform(
    *,
    include_gic: bool = True,
    include_rp1: bool = True,
    soc_uart: str = "okay",
    rp1_uart: str = "disabled",
    chosen_alias: str = "serial10",
    memory_nodes=None,
    include_reserved: bool = True,
    pcie_ranges=None,
    mfd: bool = False,
    mfd_pins: int = RP1_GPIO_PINS,
    mfd_rates=None,
) -> Node:
    memory_nodes = memory_nodes or [("memory@0", [0x0, 0x0, 0x0, 0x2800_0000])]
    root_children = [
        Node(
            "chosen",
            [
                text("stdout-path", f"{chosen_alias}:115200n8"),
                text("bootargs", "console=ttyAMA0 loglevel=info"),
            ],
        ),
        Node(
            "aliases",
            [
                text("serial10", "/soc@107c000000/serial@7d001000"),
                *(
                    [text("serial0", "/axi/pcie@1000120000/rp1/serial@30000")]
                    if include_rp1
                    else []
                ),
            ],
        ),
    ]
    for name, reg in memory_nodes:
        root_children.append(
            Node(name, [text("device_type", "memory"), cells("reg", reg)])
        )
    root_children.append(soc_node(soc_uart, include_gic))
    root_children.append(
        Node(
            "timer",
            [
                text_list("compatible", ["arm,armv8-timer"]),
                cells(
                    "interrupts",
                    [0x1, 0xD, 0xF08, 0x1, 0xE, 0xF08, 0x1, 0xB, 0xF08, 0x1, 0xA, 0xF08],
                ),
            ],
        )
    )
    if include_reserved:
        root_children.append(
            Node(
                "reserved-memory",
                [
                    cells("#address-cells", [0x2]),
                    cells("#size-cells", [0x2]),
                    cells("ranges", []),
                ],
                [
                    Node(
                        "atf@0",
                        [
                            cells("reg", [0x0, 0x0, 0x0, 0x80000]),
                            flag("no-map"),
                        ],
                    )
                ],
            )
        )
    if include_rp1:
        rp1 = rp1_node(rp1_uart, mfd=mfd, mfd_pins=mfd_pins, mfd_rates=mfd_rates)
        root_children.append(
            Node(
                "axi",
                [
                    text_list("compatible", ["simple-bus"]),
                    cells("#address-cells", [0x2]),
                    cells("#size-cells", [0x2]),
                ],
                [pcie_node(rp1, pcie_ranges or PCIE_RANGES)],
            )
        )
    if mfd:
        root_children.append(
            Node(
                "rp1_firmware",
                [
                    text_list("compatible", ["raspberrypi,rp1-firmware", "simple-mfd"]),
                    cells("mboxes", [RP1_MAILBOX_PHANDLE, 0x0]),
                    cells("shmem", [RP1_SHMEM_PHANDLE]),
                    cells("phandle", [RP1_FIRMWARE_PHANDLE]),
                ],
            )
        )
    root = Node(
        "",
        [
            text_list("compatible", ["raspberrypi,5-model-b", "brcm,bcm2712"]),
            text("model", "Raspberry Pi 5"),
            cells("#address-cells", [0x2]),
            cells("#size-cells", [0x2]),
        ],
        root_children,
    )
    return root


def write(name: str, root: Node, *, bad_magic: bool = False) -> None:
    directory = Path(__file__).parent
    blob = build_blob(root)
    if bad_magic:
        blob = b"\0\0\0\0" + blob[4:]
    (directory / f"{name}.dtb").write_bytes(blob)
    (directory / f"{name}.dts").write_text(dts(root), encoding="utf-8")
    print(f"wrote {name}.dtb and {name}.dts")


def main() -> None:
    write(
        "pi5-valid",
        platform(include_rp1=True, soc_uart="okay", rp1_uart="disabled"),
    )
    write(
        "pi5-minimal",
        platform(include_rp1=False, include_reserved=False),
    )
    write(
        "pi5-uart-rp1",
        platform(
            include_rp1=True,
            soc_uart="disabled",
            rp1_uart="okay",
            chosen_alias="serial0",
        ),
    )
    write(
        "pi5-memory-overlap",
        platform(
            memory_nodes=[
                ("memory@0", [0x0, 0x0, 0x0, 0x2800_0000]),
                ("memory@1", [0x0, 0x2000_0000, 0x0, 0x2000_0000]),
            ]
        ),
    )
    write(
        "pi5-memory-bad-cells",
        platform(memory_nodes=[("memory@0", [0x0, 0x0, 0x0])]),
    )
    write("pi5-gic-missing", platform(include_gic=False))
    write("pi5-bad-magic", platform(), bad_magic=True)
    write(
        "pi5-rp1-mfd",
        platform(include_rp1=True, soc_uart="okay", rp1_uart="disabled", mfd=True),
    )
    write(
        "pi5-rp1-mfd-bad-pins",
        platform(include_rp1=True, rp1_uart="disabled", mfd=True, mfd_pins=0),
    )
    write(
        "pi5-rp1-mfd-bad-clock",
        platform(
            include_rp1=True,
            rp1_uart="disabled",
            mfd=True,
            mfd_rates=RP1_ASSIGNED_RATES[:-1],
        ),
    )


if __name__ == "__main__":
    main()
