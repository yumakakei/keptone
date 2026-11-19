#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
# SPDX-License-Identifier: GPL-3.0-or-later
#
# This file is part of Keptone. The SPDX lines name its copyright holder and its
# license, in the machine-readable form of the REUSE specification: the GNU
# General Public License, version 3 or any later version
# (LICENSES/GPL-3.0-or-later.txt).
"""Write the DX7's algorithms as two independent encodings give them, for keptone-model's tests.

    python3 -I scripts/algorithm_sources.py <fm_core.cc> <post.html>

keptone-model's table of the 32 algorithms transcribes the chart printed on
the instruments' panels (docs/MATH.md §8.1). This script decodes the two
encodings that the table is checked against (docs/VERIFICATION.md V16), each
made apart from the chart:

- the table `algorithms` of [MSFA]'s app/src/main/jni/fm_core.cc (the file
  read for the repository's data is that of commit f67d41d313b7): six flag
  bytes an algorithm, OP6 first, each saying which bus an operator takes its
  modulation from, which bus it writes its output to or adds it to, bus 0 being
  the voice's output, and whether it gives or takes the feedback;
- the OPS's algorithm ROM as [SHIRRIFF-ALG] reads it from the chip's die, in
  the notes of his post (the page as served): six entries an algorithm,
  SEL / A,C,D / COM, one for each step of the OPS's cycle.

It writes crates/keptone-model/tests/data/algorithms.txt: for each source and
algorithm, its modulations, its feedback edges and its carriers in one
canonical form, with the SHA-256 of the files read. Neither MSFA's flags nor
the ROM's bits are written (DESIGN §14). Each decoding refuses what it cannot
read as the source describes itself, and the ROM's own compensation values
are checked against the carriers that its control bits give.
"""

import hashlib
import re
import sys
from dataclasses import dataclass
from html.parser import HTMLParser
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUTPUT = ROOT / "crates" / "keptone-model" / "tests" / "data" / "algorithms.txt"
OPERATORS = 6
ALGORITHMS = 32

# MSFA's flags (fm_core.cc's FmOperatorFlags).
OUT_BUS_ADD = 0x04
FB_IN = 0x40
FB_OUT = 0x80
KNOWN_FLAGS = 0xF7

# An entry of the ROM as [SHIRRIFF-ALG] prints it: SEL, then A, C and D, then
# COM.
ENTRY = re.compile(r"([0-7])/([01])([01])([01])/([0-7])")

# The head of the file written, which names its copyright holder and license as
# every file of the repository does.
HEADER = """\
# SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Written by scripts/algorithm_sources.py; do not edit.
#
# The DX7's 32 algorithms as two encodings give them, each decoded into the
# canonical form of docs/MATH.md §8.1:
#
#   msfa  the table `algorithms` of [MSFA]'s app/src/main/jni/fm_core.cc;
#   ops   the OPS's algorithm ROM as [SHIRRIFF-ALG] reads it from the die.
#
# Each line: the source, the algorithm, then its modulations j>k (operator j
# modulates operator k in the same sample), its feedback edges j>k (j's output
# modulates k a sample later on the DX7) and its carriers, the three lists
# separated by "|", "-" for an empty one.
"""

type Edge = tuple[int, int]
type Value = tuple[int, int]


@dataclass(frozen=True)
class Graph:
    """An algorithm in the canonical form: its edges, each (from, to), and its
    carriers."""

    modulations: frozenset[Edge]
    feedback: frozenset[Edge]
    carriers: frozenset[int]

    def line(self) -> str:
        """The three lists, each in ascending order of the operator modulated
        and then of the modulating one."""

        def edges(es: frozenset[Edge]) -> str:
            return " ".join(f"{j}>{k}" for j, k in sorted(es, key=lambda e: (e[1], e[0]))) or "-"

        return f"{edges(self.modulations)} | {edges(self.feedback)} | {' '.join(map(str, sorted(self.carriers)))}"


def msfa(text: str) -> dict[int, Graph]:
    """The algorithms of fm_core.cc's table, numbered by the comment that ends
    each of its rows, which must run from 1 to 32."""
    start = text.index("FmAlgorithm algorithms[32] = {")
    end = text.index("};", start)
    rows = re.findall(r"\{\s*\{([^{}]*)\}\s*\},\s*//\s*(\d+)", text[start:end])
    if [int(number) for _, number in rows] != list(range(1, ALGORITHMS + 1)):
        raise ValueError("fm_core.cc: the rows are not algorithms 1 to 32 in order")
    return {int(number): msfa_graph([int(flag, 16) for flag in flags.split(",")]) for flags, number in rows}


def msfa_graph(flags: list[int]) -> Graph:
    """An algorithm from its six flags, OP6's first, as FmCore::compute runs
    them: an operator reads the bus its IN_BUS bits name, 0 for none, and its
    output replaces what the bus its OUT_BUS bits name holds, or is added to it
    under OUT_BUS_ADD, bus 0 being the output; its feedback goes from the
    operator flagged FB_OUT to the one flagged FB_IN, one operator flagged both
    for a self-loop."""
    if len(flags) != OPERATORS:
        raise ValueError(f"fm_core.cc: {len(flags)} flags in a row")
    buses: dict[int, frozenset[int]] = {1: frozenset(), 2: frozenset()}
    modulations: set[Edge] = set()
    carriers: set[int] = set()
    takes: list[int] = []
    gives: list[int] = []
    for position, flag in enumerate(flags):
        operator = OPERATORS - position
        source, target = (flag >> 4) & 3, flag & 3
        if flag & ~KNOWN_FLAGS or source == 3 or target == 3:
            raise ValueError(f"fm_core.cc: flag {flag:#04x} of OP{operator}")
        if source:
            if not buses[source]:
                raise ValueError(f"fm_core.cc: OP{operator} reads an empty bus")
            modulations |= {(j, operator) for j in buses[source]}
        if target == 0:
            carriers.add(operator)
        else:
            buses[target] = buses[target] | {operator} if flag & OUT_BUS_ADD else frozenset({operator})
        if flag & FB_IN:
            takes.append(operator)
        if flag & FB_OUT:
            gives.append(operator)
    if len(takes) != 1 or len(gives) != 1:
        raise ValueError("fm_core.cc: not one operator giving the feedback and one taking it")
    return Graph(frozenset(modulations), frozenset({(gives[0], takes[0])}), frozenset(carriers))


class Tables(HTMLParser):
    """The rows of the page's tables whose id is "algorithm", each a list of
    its cells' texts."""

    def __init__(self) -> None:
        super().__init__()
        self.tables: list[list[list[str]]] = []
        self.depth = 0
        self.row: list[str] | None = None
        self.cell: list[str] | None = None

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        """Opens a table of the kind, or a row or a cell within one."""
        if tag == "table":
            if self.depth:
                self.depth += 1
            elif dict(attrs).get("id") == "algorithm":
                self.depth = 1
                self.tables.append([])
        elif self.depth == 1 and tag == "tr":
            self.row = []
        elif self.depth == 1 and tag in {"td", "th"} and self.row is not None:
            self.cell = []

    def handle_endtag(self, tag: str) -> None:
        """Closes what handle_starttag opened."""
        if tag == "table" and self.depth:
            self.depth -= 1
        elif tag in {"td", "th"} and self.cell is not None and self.row is not None:
            self.row.append("".join(self.cell).strip())
            self.cell = None
        elif tag == "tr" and self.row is not None:
            self.tables[-1].append(self.row)
            self.row = None

    def handle_data(self, data: str) -> None:
        """Keeps a cell's text."""
        if self.cell is not None:
            self.cell.append(data)


def ops(text: str) -> dict[int, Graph]:
    """The algorithms of the ROM's table in the post's notes, the one table of
    all 32; the post's excerpt of a few rows must agree with it."""
    parser = Tables()
    parser.feed(text)
    parser.close()
    tables: list[dict[int, dict[int, tuple[int, ...]]]] = []
    for rows in parser.tables:
        if ["Algorithm", "6", "5", "4", "3", "2", "1"] not in rows:
            raise ValueError("post: a table's columns are not operators 6 to 1")
        table = {}
        for row in rows:
            if row and row[0].isdigit():
                fields = [ENTRY.fullmatch(cell) for cell in row[1:]]
                if len(fields) != OPERATORS or not all(fields):
                    raise ValueError(f"post: row {row}")
                table[int(row[0])] = {
                    OPERATORS - i: tuple(int(g) for g in field.groups()) for i, field in enumerate(fields) if field
                }
        tables.append(table)
    full = [table for table in tables if sorted(table) == list(range(1, ALGORITHMS + 1))]
    if len(full) != 1:
        raise ValueError("post: not one table of the 32 algorithms")
    if any(row != full[0][number] for table in tables for number, row in table.items()):
        raise ValueError("post: the excerpt disagrees with the full table")
    return {number: ops_graph(number, full[0][number]) for number in range(1, ALGORITHMS + 1)}


def ops_graph(number: int, entries: dict[int, tuple[int, ...]]) -> Graph:
    """An algorithm from its six entries, as [SHIRRIFF-ALG] describes the OPS.

    The OPS computes a voice's operators from 6 down to 1. In the step where
    operator N's output is at hand, the modulation of operator N - 1 is formed,
    of operator 6 of the next sample where N is 1, as the step's entry says:
    SEL picks it, 0 none, 1 operator N's output, 2 the adder's output, which
    the register takes as its new value, 3 the register's value before the
    step, 4 the feedback register's latest value and 5 the mean of its last
    two; A loads operator N's output into the feedback register; C keeps the
    register's value and D adds operator N's output to it, so that (C, D) =
    (1, 0) holds it, (0, 1) loads N, (1, 1) adds N and (0, 0) clears it; and
    COM is the count of carriers less one where the operator modulated is a
    carrier, else 0. After operator 1's step the register holds the voice's
    output, the sum of the carriers.

    The registers are followed as the sets of values, each an operator's
    output of a sample, that they hold, through three samples; the middle
    sample's operators give the edges, a value of their own sample a
    modulation and one of the sample before a feedback edge, and the register
    after its operator 1 the carriers.
    """
    taken, compensations, output = run(number, entries)
    modulations: set[Edge] = set()
    feedback: set[Edge] = set()
    for modulated, source in taken.items():
        for j, s in source:
            (modulations if s == 1 else feedback).add((j, modulated))
    if any(s != 1 for _, s in output):
        raise ValueError(f"post: algorithm {number}: an output of another sample")
    carriers = frozenset(j for j, _ in output)
    for operator, com in compensations.items():
        if com != (len(carriers) - 1 if operator in carriers else 0):
            raise ValueError(f"post: algorithm {number}: COM of OP{operator} against the carriers")
    return Graph(frozenset(modulations), frozenset(feedback), carriers)


def run(
    number: int, entries: dict[int, tuple[int, ...]]
) -> tuple[dict[int, frozenset[Value]], dict[int, int], frozenset[Value]]:
    """The entries run through three samples, as ops_graph says: the values
    that modulate each operator of the middle sample, its COM, and the
    register after the middle sample's operator 1. The feedback register must
    be loaded once a sample, and SEL 5 must find in it a value of the sample
    before."""
    register: frozenset[Value] = frozenset()
    loads: list[Value] = []
    taken: dict[int, frozenset[Value]] = {}
    compensations: dict[int, int] = {}
    output: frozenset[Value] = frozenset()
    for sample in range(3):
        for n in range(OPERATORS, 0, -1):
            sel, a, c, d, com = entries[n]
            modulated, at = (n - 1, sample) if n > 1 else (OPERATORS, sample + 1)
            value = (n, sample)
            new = (register if c else frozenset()) | (frozenset({value}) if d else frozenset())
            if sel > 5:
                raise ValueError(f"post: algorithm {number}: SEL {sel}")
            latest = frozenset(loads[-1:])
            if at == 1:
                if sel >= 4 and (not loads or (sel == 5 and loads[-1][1] != 0)):
                    raise ValueError(f"post: algorithm {number}: the feedback of OP{modulated}")
                taken[modulated] = (frozenset(), frozenset({value}), new, register, latest, latest)[sel]
                compensations[modulated] = com
            register = new
            if a:
                loads.append(value)
        if sum(1 for _, s in loads if s == sample) != 1:
            raise ValueError(f"post: algorithm {number}: not one load of the feedback register a sample")
        if sample == 1:
            output = register
    return taken, compensations, output


def main() -> None:
    """Reads the two files named, decodes them and writes the data."""
    if len(sys.argv) != 3:
        raise SystemExit("usage: python3 -I scripts/algorithm_sources.py <fm_core.cc> <post.html>")
    msfa_bytes, ops_bytes = (Path(argument).read_bytes() for argument in sys.argv[1:])
    lines = [
        HEADER.rstrip("\n"),
        "#",
        f"# SHA-256 of fm_core.cc read: {hashlib.sha256(msfa_bytes).hexdigest()}",
        f"# SHA-256 of the post read:   {hashlib.sha256(ops_bytes).hexdigest()}",
    ]
    for name, graphs in (("msfa", msfa(msfa_bytes.decode())), ("ops", ops(ops_bytes.decode()))):
        lines += [f"{name} {number}: {graph.line()}" for number, graph in sorted(graphs.items())]
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    OUTPUT.write_text("\n".join(lines) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
