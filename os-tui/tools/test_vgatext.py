#!/usr/bin/env python3
"""Tests for vgatext — the VGA text-buffer reader used to verify TUI-OS screens.

The integration steps of docs/superpowers/plans/2026-09-24-tuios-desktop.md
depend on reading the 80x25 VGA text buffer back out of a running QEMU so a
screendump can be asserted on. These tests cover the pure half of that tool:
decoding the buffer and cleaning the monitor's replies.
"""

import json
import unittest

import vgatext


def cell(ch: int, attr: int = 0x07) -> bytes:
    """One VGA text cell: character byte then attribute byte."""
    return bytes([ch, attr])


def screen(rows, cols=80, attr=0x07):
    """Build a buffer from a list of strings, padded to a full screen."""
    buf = bytearray()
    for row in rows:
        line = row.ljust(cols)[:cols]
        for ch in line.encode("cp437"):
            buf += cell(ch, attr)
    while len(buf) < cols * 25 * 2:
        buf += cell(0x20, attr)
    return bytes(buf)


class DecodeScreen(unittest.TestCase):
    def test_reads_ascii_cells(self):
        data = screen(["Hola", "Adios"])
        self.assertEqual(vgatext.decode_screen(data)[:2], ["Hola", "Adios"])

    def test_strips_trailing_spaces(self):
        data = screen(["Hola          "])
        self.assertEqual(vgatext.decode_screen(data)[0], "Hola")

    def test_maps_cp437_glyphs(self):
        # The desktop's palette and box drawing come through as CP437 bytes.
        data = screen(["\u2588\u2591\u250c\u2500"])
        self.assertEqual(vgatext.decode_screen(data)[0], "\u2588\u2591\u250c\u2500")

    def test_rows_do_not_bleed_into_each_other(self):
        # A full 80-column row must not spill its last glyph onto the next one.
        data = screen(["x" * 80, "y" * 3])
        lines = vgatext.decode_screen(data)
        self.assertEqual(lines[0], "x" * 80)
        self.assertEqual(lines[1], "yyy")

    def test_short_buffer_pads_missing_rows(self):
        data = screen(["Hola"])[: 80 * 2]  # exactly one row of real content
        lines = vgatext.decode_screen(data)
        self.assertEqual(len(lines), 25)
        self.assertEqual(lines[0], "Hola")
        self.assertEqual(lines[24], "")

    def test_rejects_odd_length_buffer(self):
        # A truncated dump must fail loudly, not silently drop a byte.
        with self.assertRaises(ValueError):
            vgatext.decode_screen(b"\x00\x07\x41")


class AttrMap(unittest.TestCase):
    def test_reports_one_attribute_per_cell(self):
        data = bytearray()
        data += cell(ord("A"), 0x1F)  # white on blue
        data += cell(ord("B"), 0x0C)  # light red on black
        self.assertEqual(vgatext.attr_map(bytes(data), cols=80, rows=1), [0x1F, 0x0C])

    def test_counts_cells_not_bytes(self):
        # 80 columns is 160 bytes, and must yield 80 attributes, not 160.
        data = screen(["ab"], cols=80)[: 80 * 2]
        self.assertEqual(len(vgatext.attr_map(data, cols=80, rows=1)), 80)

    def test_splits_foreground_and_background(self):
        self.assertEqual(vgatext.split_attr(0x1F), (0x0F, 0x01))  # white on blue


class HmpCommand(unittest.TestCase):
    def test_pmemsave_quotes_the_filename(self):
        # Unquoted, HMP parses the leading '/' of a path as division and
        # answers "invalid char 't' in expression" instead of saving anything.
        cmd = vgatext.hmp_pmemsave(0xB8000, 4000, "/tmp/opencode/vga.bin")
        self.assertEqual(
            cmd, 'pmemsave 0xb8000 4000 "/tmp/opencode/vga.bin"'
        )

    def test_pmemsave_escapes_an_embedded_quote(self):
        cmd = vgatext.hmp_pmemsave(0xB8000, 16, '/tmp/we"ird.bin')
        self.assertEqual(cmd, 'pmemsave 0xb8000 16 "/tmp/we\\"ird.bin"')

    def test_send_key_uses_one_key_per_command(self):
        # QEMU's sendkey takes exactly one key per command; a burst is a loop.
        self.assertEqual(vgatext.hmp_sendkey("ret"), "sendkey ret")


class QmpMessage(unittest.TestCase):
    def test_builds_a_json_command(self):
        raw = vgatext.qmp_message("quit")
        self.assertEqual(json.loads(raw), {"execute": "quit"})

    def test_builds_a_command_with_arguments(self):
        raw = vgatext.qmp_message(
            "human-monitor-command", command_line="info status"
        )
        self.assertEqual(
            json.loads(raw),
            {
                "execute": "human-monitor-command",
                "arguments": {"command-line": "info status"},
            },
        )


class CleanResponse(unittest.TestCase):
    def test_strips_carriage_returns(self):
        self.assertEqual(vgatext.clean_response("hola\r\n"), "hola\n")

    def test_removes_ansi_escape_sequences(self):
        self.assertEqual(vgatext.clean_response("\x1b[Khola\x1b[0m\n"), "hola\n")

    def test_leaves_ordinary_text_alone(self):
        self.assertEqual(vgatext.clean_response("hola\n"), "hola\n")


if __name__ == "__main__":
    unittest.main()
