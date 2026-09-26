#!/usr/bin/env python3
"""Read the VGA 80x25 text buffer out of a running TUI-OS under QEMU.

The integration steps of docs/superpowers/plans/2026-09-24-tuios-desktop.md
assert on what the desktop actually painted. This asks QEMU's QMP interface to
``pmemsave`` the raw 4000 bytes at 0xB8000 to a file, then decodes them back
into text.

QMP rather than the HMP monitor: an HMP socket runs QEMU's readline, which
echoes every character back interleaved with the reply, so the output cannot be
parsed reliably. QMP is line-delimited JSON and comes back clean.

Usage:
    # boot headless with a QMP socket, then dump the screen
    qemu-system-x86_64 -m 32 -smp 2 -cpu core2duo -drive file=disk.img,format=raw \\
        -qmp unix:/tmp/tuios-qmp.sock,server,nowait -display none &
    sleep 7
    python3 tools/vgatext.py --socket /tmp/tuios-qmp.sock

    # type a command first, then dump (one key per sendkey)
    python3 tools/vgatext.py --socket /tmp/tuios-qmp.sock --keys "l s ret"

    # also mark cells whose background is not the default ink
    python3 tools/vgatext.py --socket /tmp/tuios-qmp.sock --colors
"""

import argparse
import json
import os
import re
import socket
import sys
import tempfile
import time

COLS = 80
ROWS = 25
VGA_BASE = 0xB8000

ANSI = re.compile(r"\x1b\[[0-9;?]*[ -/]*[@-~]")


def clean_response(raw):
    """Tidy an HMP return value: drop ANSI escapes and carriage returns."""
    if isinstance(raw, bytes):
        raw = raw.decode("latin-1")
    return ANSI.sub("", raw).replace("\r", "")


def qmp_message(execute, **arguments):
    """Build one QMP command line."""
    msg = {"execute": execute}
    if arguments:
        msg["arguments"] = {k.replace("_", "-"): v for k, v in arguments.items()}
    return json.dumps(msg)


def hmp_pmemsave(addr, size, path):
    """Build the HMP `pmemsave` command.

    The filename must be quoted: HMP reads a bare leading `/` as the start of a
    division expression and fails with "invalid char in expression" instead of
    writing the file.
    """
    escaped = path.replace("\\", "\\\\").replace('"', '\\"')
    return f'pmemsave 0x{addr:x} {size} "{escaped}"'


def hmp_sendkey(key):
    """Build one HMP `sendkey` command — QEMU accepts a single key per call."""
    return f"sendkey {key}"


# CP437's low range holds glyphs, not controls: the triangles, the arrows and
# the card suits. Python's `cp437` codec disagrees, treating 0x00-0x1F as C0
# control characters in both directions, so it can neither encode nor decode
# them. The dock's selection marker (0x10) and the title bar's breadcrumb
# (0x11) are in that range, and decoding them as controls renders them as
# invisible characters, which makes a correct kernel look broken. Hence this
# table, applied before the codec sees the row.
#
# Index 0 is the null cell, which has no glyph and is drawn blank. Getting the
# count wrong shifts every entry after the gap, so `test_vgatext` pins both the
# length and each cell.
CP437_LOW_RANGE = (
    " ☺☻♥♦♣♠•"  # 0x00-0x07
    "◘○◙♂♀♪♫☼"  # 0x08-0x0F
    "►◄↕‼¶§▬↨"  # 0x10-0x17
    "↑↓→←∟↔▲▼"  # 0x18-0x1F
)


def decode_cell(byte: int) -> str:
    """One character byte from the text buffer, as a character."""
    if byte < 0x20:
        return CP437_LOW_RANGE[byte]
    return bytes([byte]).decode("cp437", "replace")


def decode_screen(data, cols=COLS, rows=ROWS):
    """Decode a raw VGA text buffer into `rows` strings, trailing spaces cut."""
    if len(data) % 2:
        raise ValueError(
            f"odd-length VGA dump ({len(data)} bytes): the screen was read "
            "mid-write, retry the dump"
        )
    lines = []
    for row in range(rows):
        start = row * cols * 2
        chars = data[start : start + cols * 2 : 2]
        lines.append("".join(decode_cell(b) for b in chars).rstrip())
    return lines


def attr_map(data, cols=COLS, rows=ROWS):
    """One attribute byte per cell, row-major."""
    if len(data) % 2:
        raise ValueError(f"odd-length VGA dump ({len(data)} bytes)")
    return [data[i] for i in range(1, min(len(data), cols * rows * 2), 2)]


def split_attr(attr):
    """Split a VGA attribute byte into (foreground, background) indices."""
    return attr & 0x0F, (attr >> 4) & 0x07


class Qmp:
    """A line-delimited JSON client for QEMU's QMP interface."""

    def __init__(self, sock_path, timeout=15.0):
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.connect(sock_path)
        self.sock.settimeout(timeout)
        self.file = self.sock.makefile("rwb")
        self.greeting = self._read()
        self.cmd("qmp_capabilities")

    def _read(self):
        line = self.file.readline()
        if not line:
            raise ConnectionError("QMP closed the connection")
        return json.loads(line.decode())

    def cmd(self, execute, **arguments):
        """Run one QMP command and return its `return` value."""
        self.file.write(qmp_message(execute, **arguments).encode() + b"\n")
        self.file.flush()
        reply = self._read()
        if "error" in reply:
            raise RuntimeError(f"{execute} failed: {reply['error']}")
        if "return" in reply:
            return reply["return"]
        # Asynchronous events (RESET, SHUTDOWN) can arrive before the reply.
        while "return" not in reply and "error" not in reply:
            reply = self._read()
        if "error" in reply:
            raise RuntimeError(f"{execute} failed: {reply['error']}")
        return reply.get("return")

    def hmp(self, command_line):
        """Run an HMP command through QMP and return its cleaned output."""
        return clean_response(self.cmd("human-monitor-command", command_line=command_line))

    def close(self):
        try:
            self.cmd("quit")
        except (OSError, RuntimeError, ConnectionError):
            pass
        self.sock.close()


def read_screen(mon, base=VGA_BASE, cols=COLS, rows=ROWS):
    """Ask QEMU to save the text buffer to a temp file, then read it back."""
    fd, path = tempfile.mkstemp(prefix="tuios-vga-", suffix=".bin")
    os.close(fd)
    try:
        out = mon.hmp(hmp_pmemsave(base, cols * rows * 2, path)).strip()
        if "rror" in out or "nvalid" in out:  # pmemsave reports failures in prose
            raise RuntimeError(f"pmemsave failed: {out}")
        with open(path, "rb") as fh:
            data = fh.read()
    finally:
        os.unlink(path)
    if len(data) < cols * rows * 2:
        raise RuntimeError(f"pmemsave wrote {len(data)} bytes, expected {cols * rows * 2}")
    return data


def send_keys(mon, keys, delay=0.12):
    """Type each key with its own `sendkey` — QEMU takes one key per command."""
    for key in keys.split():
        mon.hmp(hmp_sendkey(key))
        time.sleep(delay)


def render(lines, attrs, show_colors, cols=COLS):
    """Print the screen, optionally marking cells tinted off the default ink."""
    if not show_colors:
        return "\n".join(lines)
    out = []
    for row, line in enumerate(lines):
        bg = [split_attr(a)[1] for a in attrs[row * cols : (row + 1) * cols]][: len(line)]
        out.append(
            "".join("#" if b not in (0, 8) else ch for ch, b in zip(line, bg)).rstrip()
        )
    return "\n".join(out)


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--socket", required=True, help="unix socket path of the QEMU QMP port")
    ap.add_argument("--keys", default="", help="keys to type before dumping, e.g. 'l s ret'")
    ap.add_argument("--wait", type=float, default=0.0, help="seconds to wait before dumping")
    ap.add_argument("--colors", action="store_true", help="mark tinted cells with #")
    ap.add_argument("--cols", type=int, default=COLS)
    ap.add_argument("--rows", type=int, default=ROWS)
    args = ap.parse_args(argv)

    mon = Qmp(args.socket)
    try:
        if args.keys:
            send_keys(mon, args.keys)
        if args.wait:
            time.sleep(args.wait)
        data = read_screen(mon, cols=args.cols, rows=args.rows)
    finally:
        mon.close()

    lines = decode_screen(data, args.cols, args.rows)
    print(render(lines, attr_map(data, args.cols, args.rows), args.colors, args.cols))
    over = [i for i, ln in enumerate(lines) if len(ln) > args.cols]
    if over:
        print(f"warning: rows {over} exceed {args.cols} columns", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
