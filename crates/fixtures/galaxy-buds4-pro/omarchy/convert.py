#!/usr/bin/env python3
"""Converts the omarchy-buds Buds4 Pro captures in this directory into
headphone-bridge fixtures (see crates/fixtures/README.md).

Usage: python3 convert.py   (writes ../*.frames)

Decoded `.log` captures are re-encoded into wire frames. Header bits 10-11 and
14-15 are lost in that format; the parser ignores them anyway. Every `link up`
starts a new fixture file. The raw `.hex` capture keeps its wire bytes exactly.

The captures were taken on 2026-09-13 in an unknown time zone; their wall-clock
times are written as UTC. capture-1 has no times, so its frames are given
synthetic ones, 1 ms apart.
"""

import re
from pathlib import Path

HERE = Path(__file__).resolve().parent
OUT = HERE.parent
DATE = "2026-09-13"
MODEL = "galaxy-buds4-pro"
LINE = re.compile(
    r"^(?:(\d\d:\d\d:\d\d\.\d+) )?(?:(link up|link closed)|(<<|>>) id=(\d+) (?:resp=(\d)|flags=0x([0-9a-f]+)) payload=(.*))$"
)


def crc16(data: bytes) -> int:
    crc = 0
    for b in data:
        crc ^= b << 8
        for _ in range(8):
            crc = ((crc << 1) ^ 0x1021) if crc & 0x8000 else (crc << 1)
        crc &= 0xFFFF
    return crc


def encode(msg_id: int, payload: bytes, flags: int) -> bytes:
    size = 1 + len(payload) + 2
    header = (size & 0x3FF) | (flags & 0x3000)
    body = bytes([msg_id]) + payload
    return b"\xfd" + header.to_bytes(2, "little") + body + crc16(body).to_bytes(2, "little") + b"\xdd"


def record(time: str, direction: str, frame: bytes) -> str:
    return f"{DATE}T{time[:12]}Z {MODEL} {direction} {frame.hex(' ')}"


def synthetic_time(n: int) -> str:
    return f"00:00:{n // 1000:02d}.{n % 1000:03d}"


def convert_log(path: Path) -> None:
    sessions: list[list[str]] = []
    header = [l for l in path.read_text().splitlines() if l.startswith("#")][:1]
    count = 0
    for line in path.read_text().splitlines():
        if not line.strip() or line.startswith("#"):
            continue
        m = LINE.match(line)
        if not m:
            raise ValueError(f"{path.name}: unrecognised line {line!r}")
        time, link, direction, msg_id, resp, flags, payload = m.groups()
        if link == "link up":
            sessions.append([])
            continue
        if link:
            continue
        if time is None:
            time = synthetic_time(count)
        count += 1
        flag_bits = int(resp) << 12 if resp is not None else int(flags, 16)
        frame = encode(int(msg_id), bytes.fromhex(payload), flag_bits)
        sessions[-1].append(record(time, direction, frame))

    stem = path.stem.removeprefix("buds4pro-")
    for i, frames in enumerate(sessions):
        name = stem if len(sessions) == 1 else f"{stem}{chr(ord('a') + i)}"
        lines = [
            f"# Converted by omarchy/convert.py from omarchy/{path.name}"
            + ("" if len(sessions) == 1 else f", link session {i + 1} of {len(sessions)}")
            + ".",
            *header,
            *frames,
        ]
        (OUT / f"{name}.frames").write_text("\n".join(lines) + "\n")


def split_frames(data: bytes) -> list[bytes]:
    frames = []
    while data:
        assert data[0] == 0xFD, "raw capture is not frame-aligned"
        size = int.from_bytes(data[1:3], "little") & 0x3FF
        frames.append(data[: size + 4])
        data = data[size + 4 :]
    return frames


def convert_hex(path: Path) -> None:
    text = path.read_text().splitlines()
    header = [l for l in text if l.startswith("#")]
    data = bytes.fromhex("".join(l.strip() for l in text if l.strip() and not l.startswith("#")))
    frames = [record(synthetic_time(i), "<<", f) for i, f in enumerate(split_frames(data))]
    lines = [f"# Converted by omarchy/convert.py from omarchy/{path.name}; times are synthetic.", *header, *frames]
    (OUT / f"{path.stem.removeprefix('buds4pro-')}.frames").write_text("\n".join(lines) + "\n")


if __name__ == "__main__":
    for log in sorted(HERE.glob("*.log")):
        convert_log(log)
    for raw in sorted(HERE.glob("*.hex")):
        convert_hex(raw)
