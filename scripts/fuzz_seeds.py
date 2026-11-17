#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
# SPDX-License-Identifier: GPL-3.0-or-later
#
# This file is part of Keptone. The SPDX lines name its copyright holder and its
# license, in the machine-readable form of the REUSE specification: the GNU
# General Public License, version 3 or any later version
# (LICENSES/GPL-3.0-or-later.txt).
"""Write the seed inputs of the fuzz targets: made up here, and taken from nowhere.

    python3 scripts/fuzz_seeds.py

libFuzzer starts from these inputs (ci/fuzz.sh), so that it need not find by itself the headers, byte counts
and checksums that a parser checks before anything else. fuzz/seeds/syx/ holds DX7 dumps of voices whose steps
are random numbers, each within the range of its parameter, laid out as [DEXED-SYSEX] lays them out (stated
again here, apart from keptone-model and its tests): a single-voice dump, a 32-voice dump, a file of several
messages among bytes that are none, and a dump whose checksum is wrong and some of whose bytes are out of range.
No voice of Yamaha's is among them (docs/DESIGN.md §14). fuzz/seeds/json/ holds JSON texts of the subset that the
native state is to be written in: an object of settings and values nested a few levels, with an integer of 64 bits;
strings with every escape, a surrogate pair and characters of several scripts; arrays and objects nested a few levels;
and numbers at the ends of binary64's range. The script writes the same bytes every time.
"""

import sys
from collections.abc import Iterator
from pathlib import Path

from binary64 import splitmix64

ROOT = Path(__file__).resolve().parent.parent
SEEDS = ROOT / "fuzz" / "seeds"

# The largest step of each of an operator's 21 parameters, in the order of the single-voice dump: R1-R4, L1-L4,
# the break point and depths; the curves; the rate scaling; the sensitivities; the output level; the mode; the
# coarse and fine frequency; the detune.
OPERATOR_LARGEST = (99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 3, 3, 7, 3, 7, 99, 1, 31, 99, 14)

# The largest step of each of the voice's own 19 parameters, 126 to 144: the pitch envelope; the algorithm; the
# feedback; the key sync; the LFO's speed, delay and depths, key sync and wave; the pitch sensitivity; the
# transposition.
VOICE_LARGEST = (99, 99, 99, 99, 99, 99, 99, 99, 31, 7, 1, 99, 99, 99, 99, 1, 5, 7, 48)

# Where each of an operator's parameters lies in its 17 bytes of a packed voice, and at which bit.
OPERATOR_PACKING = (
    *((byte, 0) for byte in range(11)),
    (11, 0),
    (11, 2),
    (12, 0),
    (13, 0),
    (13, 2),
    (14, 0),
    (15, 0),
    (15, 1),
    (16, 0),
    (12, 3),
)

# Where each of the voice's own parameters lies in a packed voice, and at which bit.
VOICE_PACKING = (
    *((byte, 0) for byte in range(102, 111)),
    (111, 0),
    (111, 3),
    *((byte, 0) for byte in range(112, 116)),
    (116, 0),
    (116, 1),
    (116, 4),
    (117, 0),
)


def largest(number: int) -> int:
    """The largest step of the parameter of a single-voice dump's byte of this number; a byte of the name takes
    any seven bits."""
    if number < 126:
        return OPERATOR_LARGEST[number % 21]
    if number < 145:
        return VOICE_LARGEST[number - 126]
    return 0x7F


def random_steps(numbers: Iterator[int]) -> list[int]:
    """The 155 bytes of a single-voice dump of random steps, each within its range."""
    return [next(numbers) % (largest(number) + 1) for number in range(155)]


def pack(steps: list[int]) -> list[int]:
    """The 128 bytes of a packed voice of a single-voice dump's 155."""
    packed = [0] * 128
    for number, step in enumerate(steps):
        if number < 126:
            byte, bit = OPERATOR_PACKING[number % 21]
            byte += 17 * (number // 21)
        elif number < 145:
            byte, bit = VOICE_PACKING[number - 126]
        else:
            byte, bit = number - 145 + 118, 0
        packed[byte] |= step << bit
    return packed


def dump(channel: int, format_number: int, data: list[int]) -> bytes:
    """A dump's message: its header with the channel, the format and the byte count, the data, their checksum
    and its end."""
    count = len(data)
    checksum = -sum(data) & 0x7F
    return bytes([0xF0, 0x43, channel, format_number, count >> 7, count & 0x7F, *data, checksum, 0xF7])


def main() -> int:
    """Write the seeds."""
    numbers = splitmix64(2026_11_17)
    voice = dump(0, 0, random_steps(numbers))
    bank = dump(1, 9, [byte for _ in range(32) for byte in pack(random_steps(numbers))])
    # A voice with a step beyond its range in seven parameters, each of a range of its own, and a wrong checksum.
    steps = random_steps(numbers)
    for number, step in ((0, 100), (11, 4), (17, 2), (20, 15), (134, 32), (142, 6), (144, 127)):
        steps[number] = step
    beyond = bytearray(dump(2, 0, steps))
    beyond[-2] ^= 0x01
    # A bank with unused bits set, a detune at 15 and a wave at 7.
    packed = [pack(random_steps(numbers)) for _ in range(32)]
    packed[0][11] |= 0x70
    packed[5][12] |= 0x78
    packed[31][116] |= 0x0E
    unused = dump(3, 9, [byte for voice_bytes in packed for byte in voice_bytes])
    # A file of several messages among bytes that are none: a voice, a note, a bank, a parameter change, another
    # maker's message, a message that the next one's F0 cuts short, and one that the end of the file does.
    mixed = b"".join(
        (
            b"bytes before\xf7",
            dump(4, 0, random_steps(numbers)),
            bytes([0x90, 0x3C, 0x40]),
            dump(5, 9, [byte for _ in range(32) for byte in pack(random_steps(numbers))]),
            bytes([0xF0, 0x43, 0x10, 0x01, 0x10, 0x05, 0xF7]),
            bytes([0xF0, 0x41, 0x10, 0x42, 0x12, 0xF7]),
            bytes([0xF0, 0x43, 0x00, 0x00, 0x01]),
            dump(6, 0, random_steps(numbers)),
            bytes([0xF0, 0x43, 0x00]),
        )
    )
    seeds = {
        "voice.syx": voice,
        "bank.syx": bank,
        "beyond.syx": bytes(beyond),
        "unused.syx": unused,
        "mixed.syx": mixed,
    }
    write(SEEDS / "syx", seeds)
    write(SEEDS / "json", json_seeds())
    return 0


def json_seeds() -> dict[str, bytes]:
    """The seeds of the json target."""
    state = (
        '{"format": 1, "voice": {"name": "SEED 1", "algorithm": 5, "operators": ['
        '{"rates": [99, 75.5, 30, 60], "levels": [99, 90, 0, 0], "output_level": 71.25}, '
        '{"rates": [80, 70, 60, 50], "levels": [99, 0, 0, 0], "output_level": 99}]}, '
        '"settings": {"lfo.seed": 18446744073709551615, "eg.clock_hz": 49096, "adaa.tau2": 0.0009765625, '
        '"output.gain": 0.5, "voice.count": 16, "eg.end_db": -200}, "bank": null, "unknown": {"kept": [true, false]}}'
    )
    escapes = '["\\"\\\\\\/\\b\\f\\n\\r\\t", "\\u00e9\\u3042\\ud83d\\ude00", "かな ア é 😀", ""]'
    nesting = '{"a": [[[{"b": [[], {}]}]]], "c": {"d": {"e": {"f": [1, [2, [3, [4]]]]}}}}'
    numbers = (
        "[0, -0, 1, -1, 0.1, 1e308, 1.7976931348623157e308, 2.2250738585072014e-308, 5e-324, 1E+2, -9007199254740993]"
    )
    return {
        "state.json": state.encode(),
        "escapes.json": escapes.encode(),
        "nesting.json": nesting.encode(),
        "numbers.json": numbers.encode(),
    }


def write(directory: Path, seeds: dict[str, bytes]) -> None:
    """Write seeds into a directory of their own."""
    directory.mkdir(parents=True, exist_ok=True)
    for name, content in seeds.items():
        (directory / name).write_bytes(content)
        print(f"wrote {directory.relative_to(ROOT) / name}, {len(content)} bytes")


if __name__ == "__main__":
    sys.exit(main())
