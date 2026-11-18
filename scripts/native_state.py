# SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
# SPDX-License-Identifier: GPL-3.0-or-later
#
# This file is part of Keptone. The SPDX lines name its copyright holder and its
# license, in the machine-readable form of the REUSE specification: the GNU
# General Public License, version 3 or any later version
# (LICENSES/GPL-3.0-or-later.txt).
"""The native state as docs/STATE-FORMAT.md specifies it, written apart from keptone-model and its tests.

A part's members, their values and what they are as a part starts are read from the document's tables, and the
settings' defaults from the table of docs/MATH.md §17. Numbers are written as ECMAScript writes them [ECMA-262], from
the digits of Python's repr, the fewest that read back as the binary64 and, of two as near, the even, as ECMAScript
takes them; the patch's digest is the SHA-256 of its members in RFC 8785's canonical form [RFC8785]; and the text is
laid out as STATE-FORMAT.md §1 says that Keptone lays it out. scripts/fuzz_seeds.py writes states with it, which the
tests of keptone-model read and write back byte for byte.
"""

import hashlib
import json
import math
import re
from collections.abc import Iterator
from dataclasses import dataclass
from decimal import Decimal
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# The members of a part's patch begin so, its digest's own aside (STATE-FORMAT.md §7).
OF_PATCH = ("voice.", "supplement.", "fractional_scaling.", "patch.")
DIGEST = "patch.sha256"

# The minus sign that the documents write before a number.
MINUS = "\N{MINUS SIGN}"


@dataclass(frozen=True)
class Raw:
    """A number kept as the text it was written in, as a reader keeps a member it does not know."""

    text: str


@dataclass(frozen=True)
class Member:
    """A member of a part: its name, what values it takes, and what it is as a part starts, for the first part and
    for the others."""

    name: str
    kind: tuple
    first: object
    others: object


def ecmascript(value: float) -> str:
    """A finite binary64 as ECMAScript's Number::toString writes it [ECMA-262] §6.1.6.1.20."""
    if value == 0:
        return "0"
    sign = "-" if value < 0 else ""
    # repr gives the fewest digits that read back, of two as near the even; normalize drops the trailing zeros.
    shortest = Decimal(repr(abs(value))).normalize().as_tuple()
    digits = "".join(map(str, shortest.digits))
    k = len(digits)
    n = int(shortest.exponent) + k
    if k <= n <= 21:
        return sign + digits + "0" * (n - k)
    if 0 < n <= 21:
        return sign + digits[:n] + "." + digits[n:]
    if -6 < n <= 0:
        return sign + "0." + "0" * -n + digits
    exponent = n - 1
    mantissa = digits[0] + ("." + digits[1:] if k > 1 else "")
    return f"{sign}{mantissa}e{'+' if exponent >= 0 else '-'}{abs(exponent)}"


def string(text: str) -> str:
    """A string as JSON, a quotation mark, a backslash and each control character escaped and nothing else, as
    STATE-FORMAT.md §2 and RFC 8785 write it: json.dumps escapes the same, in small letters."""
    return json.dumps(text, ensure_ascii=False)


def scalar(value: object, *, canonical: bool) -> str:
    """A value that holds no other: in the canonical form, a number as its binary64 [RFC8785] §3.2.2.3, one beyond
    binary64's range as its text."""
    if value is None:
        return "null"
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, str):
        return string(value)
    if isinstance(value, Raw):
        number = float(value.text)
        return ecmascript(number) if canonical and math.isfinite(number) else value.text
    if isinstance(value, int):
        return ecmascript(float(value)) if canonical else str(value)
    if isinstance(value, float):
        return ecmascript(value)
    raise TypeError(f"no JSON for {value!r}")


def flat(value: object) -> bool:
    """Whether a value is written on one line within an array: one that holds no value within it."""
    return not (isinstance(value, (dict, list)) and value)


def layout(value: object, level: int = 0) -> str:
    """A value as Keptone writes it (STATE-FORMAT.md §1): each member of an object on a line of its own, indented two
    spaces a level, and an array of values that hold none on one line."""
    indent = "  " * (level + 1)
    if isinstance(value, dict) and value:
        lines = ",".join(f"\n{indent}{string(name)}: {layout(item, level + 1)}" for name, item in value.items())
        return "{" + lines + "\n" + "  " * level + "}"
    if isinstance(value, list) and value:
        if all(flat(item) for item in value):
            return "[" + ", ".join(layout(item, level + 1) for item in value) + "]"
        lines = ",".join(f"\n{indent}{layout(item, level + 1)}" for item in value)
        return "[" + lines + "\n" + "  " * level + "]"
    if isinstance(value, dict):
        return "{}"
    if isinstance(value, list):
        return "[]"
    return scalar(value, canonical=False)


def write(state: dict) -> bytes:
    """A state's text, with a line feed at the end."""
    return (layout(state) + "\n").encode()


def utf16(name: str) -> bytes:
    """A name's UTF-16 code units, big-endian, which order as the code units do."""
    return name.encode("utf-16-be")


def canonical(value: object) -> str:
    """A value in the canonical form of RFC 8785 [RFC8785] §3.2."""
    if isinstance(value, dict):
        members = (f"{string(name)}:{canonical(value[name])}" for name in sorted(value, key=utf16))
        return "{" + ",".join(members) + "}"
    if isinstance(value, list):
        return "[" + ",".join(canonical(item) for item in value) + "]"
    return scalar(value, canonical=True)


def digest(part: dict) -> str:
    """The digest of a part's patch (STATE-FORMAT.md §7)."""
    patch = {name: value for name, value in part.items() if name != DIGEST and name.startswith(OF_PATCH)}
    return hashlib.sha256(canonical(patch).encode()).hexdigest()


def tables(path: Path) -> dict[str, list[list[str]]]:
    """The rows of each table of a document, by the heading of the section it stands in, header rows left out."""
    rows: dict[str, list[list[str]]] = {}
    heading = ""
    for line in path.read_text(encoding="utf-8").splitlines():
        if line.startswith("#"):
            heading = line.lstrip("#").strip()
        elif line.startswith("| ") and not line.startswith("| ---"):
            cells = [cell.strip() for cell in line.strip().strip("|").split(" | ")]
            if cells[0] not in {"Number", "Place", "Name", "Member", "Setting"}:
                rows.setdefault(heading, []).append(cells)
    return rows


def literal(cell: str) -> object:
    """The value of a cell that writes one: `true`, `false`, a name in quotes, or a number."""
    if cell in {"`true`", "`false`"}:
        return cell == "`true`"
    if cell.startswith('`"'):
        return cell.removeprefix('`"').removesuffix('"`')
    return float(cell.replace(MINUS, "-"))


def kind(cell: str) -> tuple:
    """What values a cell of a table's Values says a member takes."""
    if cell == "`true`, `false`":
        return ("switch",)
    if cell.startswith('`"'):
        return ("choice", tuple(re.findall(r'`"([^"`]*)"`', cell)))
    found = re.fullmatch(rf"(integers )?({MINUS}?\d+) to (\d+)(.*)", cell)
    if found is None:
        raise ValueError(f"values that say nothing: {cell}")
    least, most = (int(found[2].replace(MINUS, "-")), int(found[3]))
    return ("integer", least, most) if found[1] else ("number", float(least), float(most))


def starts(cell: str) -> tuple[object, object]:
    """What a member is as a part starts, for the first part and for the others, and OP1's beside the others'."""
    if " for the first part, " in cell:
        first, others = cell.removesuffix(" for the others").split(" for the first part, ")
        return literal(first), literal(others)
    return literal(cell), literal(cell)


def part_members() -> list[Member]:
    """Every parameter of a part as STATE-FORMAT.md §5 lists them, in the order of their IDs within each area: the
    voice's, the function data's, the supplement's and the part's own."""
    rows = tables(ROOT / "docs" / "STATE-FORMAT.md")
    voice = []
    for operator in range(6, 0, -1):
        for place, name, values, cell in rows["5.1 The voice"][:21]:
            start = cell
            if " for OP1, " in cell:
                op1, others = cell.removesuffix(" for the others").split(" for OP1, ")
                start = op1 if operator == 1 else others
            value = literal(start)
            voice.append(
                (
                    21 * (6 - operator) + int(place),
                    Member(f"voice.op{operator}.{name[1:-1]}", kind(values), value, value),
                )
            )
    voice += [
        (int(number), Member(name[1:-1], kind(values), *starts(start)))
        for number, name, values, start in rows["5.1 The voice"][21:]
    ]
    supplement = [
        (2 * (operator - 1) + int(place), Member(f"supplement.op{operator}.{name[1:-1]}", kind(values), None, None))
        for operator in range(1, 7)
        for place, name, values in rows["5.3 The supplement"][:2]
    ]
    supplement += [
        (int(number), Member(name[1:-1], kind(values), None, None))
        for number, name, values in rows["5.3 The supplement"][2:]
    ]
    function = [
        (int(number), Member(name[1:-1], kind(values), *starts(start)))
        for number, name, values, start in rows["5.2 The function data"]
    ]
    own = [
        (int(number), Member(name[1:-1], kind(values), *starts(start)))
        for number, name, values, start in rows["5.5 What a part has of its own"]
    ]
    return [member for area in (voice, function, supplement, own) for _, member in sorted(area, key=lambda row: row[0])]


def settings() -> list[tuple[str, str, object]]:
    """Every setting, in the order of STATE-FORMAT.md §3: its name, what it is written as, and its default as MATH §17
    gives it."""
    defaults = {cells[0].strip("`"): cells[1] for cells in tables(ROOT / "docs" / "MATH.md")["17. Settings index"]}
    return [
        (name.strip("`"), written, default(written, defaults[name.strip("`")]))
        for _, name, written in tables(ROOT / "docs" / "STATE-FORMAT.md")["3. Settings"]
    ]


def default(written: str, cell: str) -> object:
    """A setting's default, of MATH's cell, as the state writes it."""
    cell = cell.removesuffix(" (provisional)").removesuffix(" dB").replace(MINUS, "-")
    if written.startswith("`true`"):
        return cell == "true"
    if written.startswith('`"'):
        return cell
    if written.endswith('`"auto"`') and cell.startswith("min("):
        return "auto"
    if written.startswith(("an integer", "a power")):
        return int(cell)
    powers = {"2⁻⁵³": 2.0**-53, "2⁻¹⁰": 2.0**-10}
    if cell in powers:
        return powers[cell]
    numerator, _, denominator = cell.partition("/")
    return float(numerator) / float(denominator) if denominator else float(cell)


def initial_part(members: list[Member], *, first: bool) -> dict:
    """A part as it starts: its own members, its function data, the VOICE INIT and its digest."""
    values = {member.name: member.first if first else member.others for member in members if member.first is not None}
    values["voice.name"] = "INIT VOICE"
    return part(values, {})


def part(values: dict, later: dict) -> dict:
    """A part of its members' values, in the order of STATE-FORMAT.md §5, with the digest of its patch and the members
    of a later version after it."""
    order = ("part.", "function.", "voice.", "supplement.", "fractional_scaling.")
    known = {name: value for beginning in order for name, value in values.items() if name.startswith(beginning)}
    known[DIGEST] = digest(known | later)
    return known | later


def initial_state() -> dict:
    """The state that Keptone starts in: every setting at its default, no Scala file, and two parts, the first on."""
    members = part_members()
    every = {name: value for name, _, value in settings()}
    return {
        "format": "keptone-state",
        "version": 1,
        "settings": every,
        "tuning": {"scale": None, "keyboard_mapping": None},
        "parts": [initial_part(members, first=index == 0) for index in range(every["part.count"])],
    }


def uniform(numbers: Iterator[int]) -> float:
    """A binary64 of [0, 1) with 53 random bits."""
    return (next(numbers) >> 11) / 2.0**53


def random_value(numbers: Iterator[int], member_kind: tuple) -> object:
    """A random value of a kind: a switch's, a choice's, an integer of its range, or a number (random_number)."""
    if member_kind[0] == "switch":
        return bool(next(numbers) & 1)
    if member_kind[0] == "choice":
        return member_kind[1][next(numbers) % len(member_kind[1])]
    least, most = member_kind[1], member_kind[2]
    if member_kind[0] == "integer":
        return float(least + next(numbers) % (most - least + 1))
    return random_number(numbers, least, most)


def random_number(numbers: Iterator[int], least: float, most: float) -> float:
    """A random number of a scale: an end now and then, an integer often, any binary64 on the scale, or a number beyond
    it, as far as 10^300 (MATH §0.5)."""
    choice = next(numbers) % 6
    if choice == 0:
        return (least, most)[next(numbers) & 1]
    if choice == 1:
        return float(math.floor(least + uniform(numbers) * (most - least + 1)))
    if choice in {2, 3}:
        return least + uniform(numbers) * (most - least)
    reach = 10.0 ** (uniform(numbers) * 300)
    return most + reach if choice == 4 else least - reach


def random_setting(numbers: Iterator[int], written: str) -> object:
    """A random value of a setting, of what it is written as."""
    if written.startswith(("`true`", '`"')):
        return random_value(numbers, kind(written))
    if written == "an integer from 0 to 18446744073709551615":
        return next(numbers)
    if written.startswith("an integer of at least "):
        return int(written.removeprefix("an integer of at least ")) + next(numbers) % 5
    if written.startswith("a power of two"):
        return 1 << next(numbers) % 7
    if written.endswith('`"auto"`') and next(numbers) & 1:
        return "auto"
    magnitude = 10.0 ** (uniform(numbers) * 12 - 6)
    bounded = {
        "a number above 0": magnitude,
        "a number of at least 0": magnitude if next(numbers) & 1 else 0.0,
        "a number below 0": -magnitude,
        "a number above 0 and below 1": uniform(numbers) or 0.5,
        "a number of at least 0 and below 1": uniform(numbers),
    }
    return bounded[written.removesuffix(', or `"auto"`')]
