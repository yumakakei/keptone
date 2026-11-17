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
No voice of Yamaha's is among them (docs/DESIGN.md §14). Beside them, the dumps of the performances of the DX1 and the
TX7, of one and of 64, laid out as [TX7-MANUAL] lays them out, of random steps; and a file of the DX7's other messages
and its family's: parameter changes of voice parameters, of the operators' switches and of every function parameter,
the presses of the DX7's panel buttons that make it send its bank [AJXS-ROM], switches of the DX7II's switch remote,
the TX7's changes of a performance, of its own settings and of its operator select, dump requests [DX7II-MANUAL]
[TX7-MANUAL], a performance under format number 2 and a condition acknowledge, with some out of range and some of no
kind read. fuzz/seeds/json/ holds JSON texts of the subset
that the native state is to be written in: an object of settings and values nested a few levels, with an integer of
64 bits; strings with every escape, a surrogate pair and characters of several scripts; arrays and objects nested a
few levels; and numbers at the ends of binary64's range. fuzz/seeds/scl/ and fuzz/seeds/kbm/ hold scales and
keyboard mappings of Scala's: equal steps in cents, just intonation in ratios, the three ends of a line, ISO 8859-1,
a linear mapping and keys left unmapped. The script writes the same bytes every time.
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
    others = messages(numbers)
    # One performance, and 64 with random key assign modes.
    performance = dump(10, 1, random_performance(numbers))
    packed = [pack_performance(random_performance(numbers), next(numbers) % 3) for _ in range(64)]
    performances = dump(11, 2, [byte for performance_bytes in packed for byte in performance_bytes])
    seeds = {
        "voice.syx": voice,
        "bank.syx": bank,
        "beyond.syx": bytes(beyond),
        "unused.syx": unused,
        "mixed.syx": mixed,
        "messages.syx": others,
        "performance.syx": performance,
        "performances.syx": performances,
    }
    write(SEEDS / "syx", seeds)
    write(SEEDS / "json", json_seeds())
    write(SEEDS / "scl", scl_seeds())
    write(SEEDS / "kbm", kbm_seeds())
    return 0


# The largest step of each byte of a side's thirty in the dump of one performance [TX7-MANUAL]: mono/poly at 2; the
# pitch bend's range and step at 3 and 4; the portamento's time, glissando and mode at 5 to 7; the sensitivity and the
# assignment of the modulation wheel, the foot controller, the aftertouch and the breath controller at 9 to 16; the
# attenuation at 26. The bytes the manual leaves unnamed take any seven bits.
SIDE_LARGEST = (0x7F, 0x7F, 1, 12, 12, 99, 1, 1, 0x7F, 15, 7, 15, 7, 15, 7, 15, 7, *(0x7F,) * 9, 7, 0x7F, 0x7F, 0x7F)

# Where each of a side's fields lies in its sixteen bytes of a packed performance, by its byte in the dump of one: some
# of its bits' byte, the bit there of the lowest of them, that bit's place in the field, and how many they are.
SIDE_PACKING = (
    (2, ((0, 6, 0, 1),)),
    (3, ((1, 0, 0, 4),)),
    (4, ((1, 4, 0, 3), (15, 6, 3, 1))),
    (5, ((2, 0, 0, 7),)),
    (6, ((3, 0, 0, 1),)),
    (7, ((3, 1, 0, 1),)),
    (9, ((4, 0, 0, 4),)),
    (10, ((4, 4, 0, 3),)),
    (11, ((5, 0, 0, 4),)),
    (12, ((5, 4, 0, 3),)),
    (13, ((6, 0, 0, 4),)),
    (14, ((6, 4, 0, 3),)),
    (15, ((7, 0, 0, 4),)),
    (16, ((7, 4, 0, 3),)),
    (26, ((14, 0, 0, 3),)),
)


def performance_largest(number: int) -> int:
    """The largest step of the byte of this number of the dump of one performance: the sides', the voice memory
    select at 61, and the name, which takes any seven bits."""
    if number < 60:
        return SIDE_LARGEST[number % 30]
    return 1 if number == 61 else 0x7F


def random_performance(numbers: Iterator[int]) -> list[int]:
    """The 94 bytes of the dump of one performance of random steps, each within its range."""
    return [next(numbers) % (performance_largest(number) + 1) for number in range(94)]


def pack_performance(steps: list[int], key_assign: int) -> list[int]:
    """The 64 bytes of a packed performance of the dump of one's 94 and a key assign mode: side A in bytes 0 to 15,
    side B in 16 to 31, the voice memory select and the key assign mode in byte 32, the name in 34 to 63."""
    packed = [0] * 64
    for side in range(2):
        for offset, places in SIDE_PACKING:
            step = steps[30 * side + offset]
            for byte, bit, low, width in places:
                packed[16 * side + byte] |= (step >> low & ((1 << width) - 1)) << bit
    packed[32] = steps[61] << 2 | key_assign
    packed[34:] = steps[64:]
    return packed


# The largest step of each function parameter, 64 to 77: mono/poly; the pitch bend's range and step; the
# portamento's mode, glissando and time; the range and the assignment of each of the four controllers.
FUNCTION_LARGEST = (1, 12, 12, 1, 1, 99, 99, 7, 99, 7, 99, 7, 99, 7)


def change(channel: int, group: int, number: int, data: int) -> bytes:
    """A parameter change: its header with the channel, the group byte, the number's low seven bits and the
    data."""
    return bytes([0xF0, 0x43, 0x10 | channel, group, number, data, 0xF7])


def request(channel: int, *body: int) -> bytes:
    """A dump request: its header with the channel, the format and, for a universal bulk dump, its names."""
    return bytes([0xF0, 0x43, 0x20 | channel, *body, 0xF7])


def messages(numbers: Iterator[int]) -> bytes:
    """A file of parameter changes and dump requests."""
    # The voice group, 0, holds the number's high bits in the group byte's low two.
    voice = [change(0, number >> 7, number & 0x7F, next(numbers) % (largest(number) + 1)) for number in (0, 121, 144)]
    switches = change(1, 0x01, 155 & 0x7F, 0b101101)
    function = [
        change(2, 0x08, number, next(numbers) % (largest + 1)) for number, largest in enumerate(FUNCTION_LARGEST, 64)
    ]
    # FUNCTION; button 8 three times, which steps from the receive channel to SYS INFO AVAIL and then to MIDI
    # TRANSMIT; and YES, each pressed and released: by these a DX7 sends its 32 voices [AJXS-ROM]. Then a switch of
    # the DX7II's at each end of their numbers.
    panel = [change(3, 0x08, button, data) for button in (39, 7, 7, 7, 41) for data in (0x7F, 0x00)]
    remote = [change(4, 0x1B, switch, data) for switch in (0, 45) for data in (0x7F, 0x00)]
    requests = [request(5, 0x00), request(6, 0x05), request(7, 0x06), request(8, 0x09)]
    requests.append(request(9, 0x7E, *b"LM  8973PE"))
    # Out of range: a rate at 100, the pitch bend's range at 13, a button's data neither 0 nor 127; and of no
    # kind read: function parameter 50, a request of format 1.
    beyond = [change(10, 0x00, 0, 100), change(11, 0x08, 65, 13), change(12, 0x08, 32, 0x40)]
    unread = [change(13, 0x08, 50, 0), request(14, 0x03)]
    # The TX7's: the source select, the aftertouch's sensitivity and the master tuning of a performance; its lowest
    # note and its memory protect; its operator select; its requests; a performance under format number 2, as
    # [DX7-MANUAL] prints it; and a condition acknowledge. Then some out of range, and some of no kind read.
    tx7 = [
        change(5, 0x04, 1, 11),
        change(5, 0x04, 13, 9),
        change(5, 0x04, 64, 64),
        change(6, 0x11, 5, 21),
        change(6, 0x11, 7, 0x7F),
        change(7, 0x01, 0x1C, 3),
        request(8, 0x01),
        request(8, 0x02),
        request(8, 0x7D),
        dump(9, 2, random_performance(numbers)),
        dump(0, 0x7D, [*b"LM  8950  ", 1, 2, 0x58, 5, 31, 0]),
        change(10, 0x04, 1, 0),
        change(11, 0x11, 0, 2),
        change(12, 0x01, 0x1C, 6),
        change(13, 0x04, 8, 0),
        change(13, 0x11, 8, 0),
    ]
    return b"".join((*voice, switches, *function, *panel, *remote, *requests, *beyond, *unread, *tx7))


def scl_seeds() -> dict[str, bytes]:
    """The seeds of the scl target: twelve equal steps in cents, five-limit just intonation in ratios with CR and
    LF, a scale of ISO 8859-1 ending its lines with CR alone, and a scale of no notes."""
    equal = "! equal.scl\n!\nTwelve equal steps of the octave\n 12\n!\n"
    equal += "".join(f"{100 * step}.0\n" for step in range(1, 12)) + "2/1\n"
    ratios = ("16/15", "9/8", "6/5", "5/4 E", "4/3", "45/32", "3/2", "8/5", "5/3", "9/5", "15/8", "2/1")
    just = "Five-limit just intonation\r\n12\r\n" + "".join(f"{ratio}\r\n" for ratio in ratios)
    return {
        "equal.scl": equal.encode(),
        "just.scl": just.encode(),
        "latin.scl": b"Caf\xe9\r3\r-5.0\r10/20\r 3 cents\r",
        "empty.scl": b"\n0\n",
    }


def kbm_seeds() -> dict[str, bytes]:
    """The seeds of the kbm target: twelve keys from middle C with A at 440 Hz, a linear mapping with CR and LF,
    and a mapping of seven keys with one left unmapped and degrees below 0."""
    twelve = "! a mapping of twelve keys\n12\n0\n127\n60\n69\n440.0\n12\n! the entries\n"
    twelve += "".join(f"{degree}\n" for degree in range(12))
    return {
        "twelve.kbm": twelve.encode(),
        "linear.kbm": b"0\r\n0\r\n127\r\n60\r\n60\r\n261.6255653\r\n0\r\n",
        "seven.kbm": b"7\n21\n108\n60\n62\n293.66\n-7\n0\nx\n2\n-1\n",
    }


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
