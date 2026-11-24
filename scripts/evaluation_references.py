#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
# SPDX-License-Identifier: GPL-3.0-or-later
#
# This file is part of Keptone. The SPDX lines name its copyright holder and its
# license, in the machine-readable form of the REUSE specification: the GNU
# General Public License, version 3 or any later version
# (LICENSES/GPL-3.0-or-later.txt).
"""Write the references of a voice's operators at engine samples for keptone-engine's tests.

    python3 scripts/evaluation_references.py

Evaluates the DX7's 32 algorithms, as crates/keptone-model/tests/data/algorithms.txt gives them, exactly at
binary64 inputs with mpmath at 400 bits (docs/MATH.md §1, §8.2, §9): each operator on no loop y = g sin 2 pi
(theta + mu); a self-loop's delay-free solution by Kepler's equation and a loop's by its fixed point y_b =
G(y_b); the DX7's recurrence, which feeds back the mean of y_b's two outputs before; and the relaxation, whose
trapezoidal substeps' equations it solves to 400 bits, theta and g given at the substeps' instants by the laws
of the sample. Each operator's theta and g follow lines in time, which may jump at a sample, where the
sample's events would act. The choices that the engine makes in binary64, the loop gains, the lag tau, the
substeps, a and the clamped kappa, are made here in the same binary64 operations, so that the exact values are
those of the engine's own choices. Its inputs are drawn from SplitMix64 with a fixed seed.

Beside each value it writes the bound of keptone-engine's error that the functions' documentation gives, first
order in u = 2^-53, which it follows through every operator and every sample. It writes
crates/keptone-engine/tests/data/evaluation.txt, every number as the 16 hexadecimal digits of its binary64, so
that the file is the same every time.
"""

import math
from collections.abc import Callable, Iterator
from dataclasses import dataclass, field
from pathlib import Path

import mpmath

from binary64 import splitmix64, to_bits, uniform

ROOT = Path(__file__).resolve().parent.parent
ALGORITHMS = ROOT / "crates" / "keptone-model" / "tests" / "data" / "algorithms.txt"
OUTPUT = ROOT / "crates" / "keptone-engine" / "tests" / "data" / "evaluation.txt"
SEED = 0x6576616C75617465
RELAXATION, DX7, CLAMP = 0, 1, 2
GENERAL, CASES = 288, 384

# The head of the file written, which names its copyright holder and license as
# every file of the repository does.
HEADER = """\
# SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Written by scripts/evaluation_references.py with mpmath; do not edit.
# A case's lines, every number the binary64 bits in hexadecimal:
#   case algorithm feedback model kappa tau onset clamp iterations substeps rate samples, then six
#     operators' laws, each theta0 speed gain0 slope jump theta0' speed' gain0' slope', theta in turns
#     theta0 + speed t and g gain0 + slope t at t samples, the primed ones in effect from the jump's
#     sample on, at its instant and over the samples after it (the jump 2^60 where there is none)
#   sample n, then each operator's output hi lo bound, and the carriers' sum hi lo bound
"""

mpmath.mp.prec = 400
U = mpmath.mpf(2) ** -53
TAU = math.tau
TWO_PI = 2 * mpmath.pi
NONE = 2.0**60


def read_algorithms() -> list[tuple[list[tuple[int, int]], tuple[int, int], list[int]]]:
    """The 32 algorithms of the file that keptone-model's tests read: each algorithm's modulations and
    feedback edge, from and to, and its carriers, by 0-based operator indices."""
    algorithms = []
    for line in ALGORITHMS.read_text(encoding="utf-8").splitlines():
        if not line.startswith("msfa "):
            continue
        _, rest = line.split(": ", 1)
        modulations, feedback, carriers = (part.strip() for part in rest.split("|"))

        def pairs(text: str) -> list[tuple[int, int]]:
            if text == "-":
                return []
            return [(int(edge.split(">")[0]) - 1, int(edge.split(">")[1]) - 1) for edge in text.split()]

        (edge,) = pairs(feedback)
        algorithms.append((pairs(modulations), edge, [int(c) - 1 for c in carriers.split()]))
    if len(algorithms) != 32:
        raise SystemExit(f"{len(algorithms)} algorithms in {ALGORITHMS}")
    return algorithms


@dataclass
class Law:
    """An operator's theta and g as lines in time, which may jump at a sample."""

    theta0: float
    speed: float
    gain0: float
    slope: float
    jump: float = NONE
    after: tuple[float, float, float, float] = (0.0, 0.0, 0.0, 0.0)

    def at(self, t: float, since: int) -> tuple[float, float]:
        """theta and g at t samples, in binary64 as the test computes them, by the law in effect from
        sample `since` on: the primed law where the jump lies at or before it."""
        theta0, speed, gain0, slope = (
            self.after if self.jump <= since else (self.theta0, self.speed, self.gain0, self.slope)
        )
        return theta0 + speed * t, gain0 + slope * t

    def numbers(self) -> list[float]:
        """Its numbers as the case line writes them."""
        return [self.theta0, self.speed, self.gain0, self.slope, self.jump, *self.after]


@dataclass
class Case:
    """A case: an algorithm, its feedback, the model and its settings, and the operators' laws."""

    algorithm: int
    feedback: bool
    model: int
    kappa: float
    tau: float
    onset: float
    clamp: float
    iterations: int
    substeps: int
    rate: float
    samples: int
    laws: list[Law] = field(default_factory=list)


def exact(x: float) -> mpmath.mpf:
    """A binary64 as an mpf, exactly."""
    return mpmath.mpf(x)


def sin2(x: mpmath.mpf) -> mpmath.mpf:
    """sin 2 pi x."""
    return mpmath.sin(TWO_PI * x)


def cos2(x: mpmath.mpf) -> mpmath.mpf:
    """cos 2 pi x."""
    return mpmath.cos(TWO_PI * x)


def finite(x: float) -> float:
    """binary64's largest finite number in place of an infinity, as keptone-engine's finite does."""
    if math.isinf(x):
        return math.copysign(1.7976931348623157e308, x)
    return x


def beta64(kappa: float, gain: float) -> float:
    """feedback::beta in binary64: (2 pi kappa) g."""
    return finite(TAU * kappa * gain)


def lag64(beta: float, onset: float, tau: float) -> float:
    """feedback::lag in binary64."""
    magnitude = abs(beta)
    if magnitude <= onset:
        return 0.0
    if magnitude >= 1.0:
        return tau
    return finite(tau * ((magnitude - onset) / (1.0 - onset)))


def substeps64(beta_max: float, tau: float, rate: float, cap: int) -> int:
    """feedback::substeps in binary64."""
    excess = abs(beta_max) - 1.0
    if excess <= 0.0 or tau <= 0.0:
        return 1
    needed = math.floor(excess / (2.0 * tau * rate)) + 1
    return cap if needed >= cap else int(needed)


@dataclass
class Graph:
    """An algorithm's graph as the evaluation takes it."""

    modulators: list[list[int]]
    feedback: tuple[int, int] | None
    carriers: list[int]
    stage: list[int]
    rest: list[int]

    @staticmethod
    def of(algorithm: tuple[list[tuple[int, int]], tuple[int, int], list[int]], *, feedback: bool) -> Graph:
        """The graph of an algorithm, with its feedback edge where it has one; the feedback stage's
        operators in their order, from the one the edge enters, and the others in an order in which
        each comes after its modulators, the highest operator first."""
        modulations, edge, carriers = algorithm
        modulators = [[] for _ in range(6)]
        for j, k in modulations:
            modulators[k].append(j)
        if feedback:
            modulators[edge[1]].append(edge[0])
        for m in modulators:
            m.sort()
        stage = []
        if feedback:
            frm, to = edge
            stage = [to]
            while stage[-1] != frm:
                stage.append(next(k for k in range(6) if stage[-1] in [j for j, kk in modulations if kk == k]))
        rest, placed = [], set(stage)
        while len(rest) + len(stage) < 6:
            k = max(
                k
                for k in range(6)
                if k not in placed and all(j in placed for j in modulators[k] if (j, k) != (edge if feedback else None))
            )
            placed.add(k)
            rest.append(k)
        return Graph(modulators, edge if feedback else None, carriers, stage, rest)


class Evaluation:
    """An evaluation of the operators at one instant: each operator's exact output and the bound of the
    engine's error, given theta and g."""

    def __init__(self, graph: Graph, values: Callable[[int], tuple[float, float]], kappa: float) -> None:
        self.graph = graph
        self.values = values
        self.kappa = kappa
        self.outputs: dict[int, tuple[mpmath.mpf, mpmath.mpf]] = {}

    def operator(self, k: int, fed: tuple[mpmath.mpf, mpmath.mpf] = (mpmath.mpf(0), mpmath.mpf(0))) -> None:
        """Operator k's output and its bound: mu its modulators' outputs added in their order, the
        feedback edge's term kappa times the value fed back, and sin2pi of theta + mu times g."""
        theta, gain = self.values(k)
        terms = []
        for j in self.graph.modulators[k]:
            if self.graph.feedback == (j, k):
                value = exact(self.kappa) * fed[0]
                terms.append((value, abs(exact(self.kappa)) * fed[1] + U * abs(value)))
            else:
                terms.append(self.outputs[j])
        error, partial = mpmath.mpf(0), mpmath.mpf(0)
        for index, (value, term_error) in enumerate(terms):
            partial += value
            error += term_error
            if index > 0:
                error += U * abs(partial)
        turns = exact(theta) + partial
        g = exact(gain)
        y = g * sin2(turns)
        bound = abs(g) * TWO_PI * abs(cos2(turns)) * (error + U * abs(turns)) + mpmath.mpf("2.46") * U * abs(y)
        self.outputs[k] = (y, bound)

    def stage_with(self, fed: tuple[mpmath.mpf, mpmath.mpf]) -> None:
        """The feedback stage's operators with a value fed back."""
        for k in self.graph.stage:
            self.operator(k, fed)

    def derivative(self, fed: mpmath.mpf) -> mpmath.mpf:
        """G'(fed): the feedback stage's output's derivative in the value fed back."""
        slope, outputs = {}, {}
        for k in self.graph.stage:
            theta, gain = self.values(k)
            mu, d = mpmath.mpf(0), mpmath.mpf(0)
            for j in self.graph.modulators[k]:
                if self.graph.feedback == (j, k):
                    mu += exact(self.kappa) * fed
                    d += exact(self.kappa)
                elif j in outputs:
                    mu += outputs[j]
                    d += slope[j]
                else:
                    mu += self.outputs[j][0]
            turns = exact(theta) + mu
            outputs[k] = exact(gain) * sin2(turns)
            slope[k] = exact(gain) * TWO_PI * cos2(turns) * d
        return slope[self.graph.stage[-1]]

    def g_of(self, fed: mpmath.mpf) -> mpmath.mpf:
        """G(fed) exactly."""
        saved = dict(self.outputs)
        self.stage_with((fed, mpmath.mpf(0)))
        value = self.outputs[self.graph.stage[-1]][0]
        self.outputs = saved
        return value

    def entering(self, fed: mpmath.mpf) -> mpmath.mpf:
        """theta + mu of the operator that the feedback enters, reduced to a half turn."""
        to = self.graph.stage[0]
        theta, _ = self.values(to)
        mu = mpmath.mpf(0)
        for j in self.graph.modulators[to]:
            if self.graph.feedback == (j, to):
                mu += exact(self.kappa) * fed
            else:
                mu += self.outputs[j][0]
        turns = exact(theta) + mu
        return turns - mpmath.nint(turns)


def bracketed(f: Callable[[mpmath.mpf], mpmath.mpf], low: mpmath.mpf, high: mpmath.mpf) -> mpmath.mpf:
    """The root of an increasing function inside a bracket, bisected to 2^-100 of its width, then
    polished by the secant through its last bracket."""
    for _ in range(100):
        middle = (low + high) / 2
        if f(middle) > 0:
            high = middle
        else:
            low = middle
    flow, fhigh = f(low), f(high)
    if fhigh == flow:
        return (low + high) / 2
    for _ in range(6):
        value = high - fhigh * (high - low) / (fhigh - flow)
        low, flow, high, fhigh = high, fhigh, value, f(value)
        if fhigh == flow:
            break
    return high


def stop(evaluation: Evaluation, fed: mpmath.mpf, kappa: float) -> mpmath.mpf:
    """The stopping rule's step: 2u max(1/(2 pi), |theta + mu|) over |kappa|."""
    reduced = evaluation.entering(fed)
    return 2 * U * max(1 / TWO_PI, abs(reduced)) / max(abs(exact(kappa)), mpmath.mpf(2) ** -60)


def kepler(case: Case, graph: Graph, evaluation: Evaluation, *, clamp: bool) -> tuple[mpmath.mpf, mpmath.mpf]:
    """A self-loop's delay-free solution y = g sin Theta, Theta - beta sin Theta = Phi, at the phase theta +
    mu of its other modulators, beta of either sign below 1 (clamped where clamp is true), and the bound
    that keptone-engine's point documents."""
    (to,) = graph.stage
    theta, gain = evaluation.values(to)
    terms = [evaluation.outputs[j] for j in graph.modulators[to] if (j, to) != graph.feedback]
    mu, mu_error = mpmath.mpf(0), mpmath.mpf(0)
    for index, (value, error) in enumerate(terms):
        mu += value
        mu_error += error + (U * abs(mu) if index > 0 else 0)
    phase = exact(theta) + mu
    phase_error = mu_error + U * abs(phase)
    beta = beta64(case.kappa, gain)
    beta_error = 3 * U * abs(exact(beta))
    if clamp and abs(beta) > case.clamp:
        beta = math.copysign(case.clamp, beta)
        beta_error = mpmath.mpf(0)
    b = exact(beta)
    shifted = b < 0
    target = phase - mpmath.mpf(1) / 2 if shifted else phase
    r = target - mpmath.nint(target)
    magnitude = abs(b)
    root = bracketed(
        lambda x: x - magnitude * mpmath.sin(x) - TWO_PI * r,
        TWO_PI * r - magnitude - mpmath.mpf(2) ** -300,
        TWO_PI * r + magnitude + mpmath.mpf(2) ** -300,
    )
    g = exact(gain)
    y = -g * mpmath.sin(root) if shifted else g * mpmath.sin(root)
    residual = 4 * U * max(1, abs(TWO_PI * r)) + (mpmath.pi * U if shifted else 0)
    slope = 1 - magnitude * mpmath.cos(root)
    theta_error = (residual + TWO_PI * phase_error + beta_error * abs(mpmath.sin(root))) / slope
    bound = abs(g) * (abs(mpmath.cos(root)) * theta_error + mpmath.mpf("1.46") * U * abs(mpmath.sin(root)))
    return y, bound + U * abs(y)


def loop_gain(case: Case, graph: Graph, gains: Callable[[int], float], kappa: float) -> float:
    """B of MATH §9.7 in binary64 as the engine forms it: 2 pi kappa g of the operator the edge enters
    times 2 pi g of each other, in the stage's order."""
    product = 1.0
    to = graph.stage[0]
    for k in graph.stage:
        factor = beta64(kappa, gains(k)) if k == to else finite(TAU * gains(k))
        product = finite(product * factor)
    _ = case
    return product


def fixed(evaluation: Evaluation, graph: Graph, kappa: float) -> tuple[mpmath.mpf, mpmath.mpf]:
    """A loop's delay-free solution y_b = G(y_b) at a binary64 kappa, unique where its gain lies below 1,
    and the bound of the engine's error: the error of G at y_b and the residual's rounding over 1 - G',
    and the stopping rule's step."""
    evaluation.kappa = kappa
    frm = graph.stage[-1]
    _, bound_gain = evaluation.values(frm)
    reach = abs(exact(bound_gain))
    root = bracketed(lambda y: y - evaluation.g_of(y), -reach, reach)
    saved = dict(evaluation.outputs)
    evaluation.stage_with((root, mpmath.mpf(0)))
    g_error = evaluation.outputs[frm][1]
    evaluation.outputs = saved
    slope = evaluation.derivative(root)
    error = (g_error + 2 * U * abs(root)) / abs(1 - slope) + stop(evaluation, root, kappa)
    return root, error


@dataclass
class State:
    """What the engine's memory holds for the feedback stage, exactly, with the bounds of its errors."""

    lagged: tuple[mpmath.mpf, mpmath.mpf] = (mpmath.mpf(0), mpmath.mpf(0))
    before: tuple[mpmath.mpf, mpmath.mpf] = (mpmath.mpf(0), mpmath.mpf(0))
    fresh: bool = True
    lagging: bool = False


def integrate(case: Case, graph: Graph, state: State, n: int, before: dict) -> bool:
    """The relaxation's substeps over sample n, from n - 1 to n, by the laws that held over it; false
    where a substep's equation might have more than one root, which the references leave out."""
    if case.model != RELAXATION or not graph.stage or state.fresh:
        return True

    def within(samples: float) -> Callable[[int], tuple[float, float]]:
        t = float(n - 1) + samples
        return lambda k: case.laws[k].at(t, n - 1)

    ends = [abs(loop_gain(case, graph, lambda k, s=s: within(s)(k)[1], case.kappa)) for s in (0.0, 1.0)]
    largest = max(ends)
    tau = lag64(largest, case.onset, case.tau)
    state.lagging = tau > 0.0
    if tau <= 0.0:
        return True
    steps = max(substeps64(largest, tau, case.rate, case.substeps), 1)
    if largest - 1.0 > 0.0 and 1.0 / (steps * case.rate) >= 2.0 * tau / (largest - 1.0) * 0.999:
        return False
    a64 = finite(1.0 / (2.0 * tau * case.rate * float(steps)))
    a = exact(a64)

    def instant(j: int) -> float:
        return 1.0 if j >= steps else float(j) / float(steps)

    value, value_error = state.lagged
    frm = graph.stage[-1]
    for j in range(1, steps + 1):
        start, end = instant(j - 1), instant(j)
        first = Evaluation(graph, within(start), case.kappa)
        first.outputs = dict(before)
        first.stage_with((value, value_error))
        yk, yk_error = first.outputs[frm]
        last = Evaluation(graph, within(end), case.kappa)
        last.outputs = dict(before)
        _, g_bound = within(end)(frm)
        reach = a * abs(exact(g_bound)) / (1 + a) + mpmath.mpf(2) ** -300
        centre = ((1 - a) * value + a * yk) / (1 + a)

        def residual(
            y: mpmath.mpf, start_value: mpmath.mpf = value, yk: mpmath.mpf = yk, last: Evaluation = last
        ) -> mpmath.mpf:
            return (1 + a) * y - (1 - a) * start_value - a * yk - a * last.g_of(y)

        root = bracketed(residual, centre - reach, centre + reach)
        last.stage_with((root, mpmath.mpf(0)))
        g_value, g_error = last.outputs[frm]
        slope = last.derivative(root)
        derivative = 1 + a - a * slope
        rounding = U * (3 * abs((1 + a) * root) + 4 * abs((1 - a) * value) + 3 * a * abs(yk) + 2 * a * abs(g_value))
        rounding += a * g_error
        carried = abs(1 - a) * value_error + a * yk_error
        value_error = (rounding + carried) / abs(derivative) + stop(last, root, case.kappa)
        value = root
    state.lagged = (value, value_error)
    return True


def stage_output(case: Case, graph: Graph, state: State, evaluation: Evaluation) -> bool:
    """The feedback stage's outputs at the instant by the case's model, the state updated; false where a
    delay-free solution of a loop might not be unique."""
    stage, frm, to = graph.stage, graph.stage[-1], graph.stage[0]
    single = len(stage) == 1
    gain = loop_gain(case, graph, lambda k: evaluation.values(k)[1], case.kappa)
    if case.model == RELAXATION:
        if state.fresh:
            state.lagging = lag64(gain, case.onset, case.tau) > 0.0
            state.lagged = (mpmath.mpf(0), mpmath.mpf(0))
        if state.lagging:
            evaluation.stage_with(state.lagged)
        elif single:
            evaluation.outputs[to] = kepler(case, graph, evaluation, clamp=False)
            state.lagged = evaluation.outputs[to]
        else:
            if abs(gain) >= 1.0:
                return False
            state.lagged = fixed(evaluation, graph, case.kappa)
            evaluation.kappa = case.kappa
            evaluation.stage_with(state.lagged)
    elif case.model == DX7:
        mean = (state.lagged[0] + state.before[0]) / 2
        mean_error = (state.lagged[1] + state.before[1]) / 2 + U * abs(mean)
        evaluation.stage_with((mean, mean_error))
        state.before = state.lagged
        state.lagged = evaluation.outputs[frm]
    elif single:
        evaluation.outputs[to] = kepler(case, graph, evaluation, clamp=True)
        state.lagged = evaluation.outputs[to]
    else:
        magnitude = abs(gain)
        kappa = finite(case.kappa * (case.clamp / magnitude)) if magnitude > case.clamp else case.kappa
        if abs(loop_gain(case, graph, lambda k: evaluation.values(k)[1], kappa)) >= 1.0:
            return False
        state.lagged = fixed(evaluation, graph, kappa)
        evaluation.stage_with(state.lagged)
        evaluation.kappa = case.kappa
    state.fresh = False
    return True


def output(case: Case, graph: Graph, state: State, n: int) -> tuple[dict, tuple[mpmath.mpf, mpmath.mpf]] | None:
    """The operators' outputs at sample n's instant, after its events, and the carriers' sum; none where
    a delay-free solution of a loop might not be unique."""
    t = float(n)
    evaluation = Evaluation(graph, lambda k: case.laws[k].at(t, n), case.kappa)
    if graph.stage and not stage_output(case, graph, state, evaluation):
        return None
    for k in graph.rest:
        evaluation.operator(k)
    total, total_error = mpmath.mpf(0), mpmath.mpf(0)
    for index, c in enumerate(graph.carriers):
        total += evaluation.outputs[c][0]
        total_error += evaluation.outputs[c][1] + (U * abs(total) if index > 0 else 0)
    return evaluation.outputs, (total, total_error)


def double_double(value: mpmath.mpf) -> list[float]:
    """value as hi + lo."""
    hi = float(value)
    return [hi, float(value - mpmath.mpf(hi))]


def draw_case(randoms: Iterator[int], algorithms: list, number: int) -> Case:
    """A case drawn at random: an algorithm, its feedback, a model and the operators' laws; past the
    first GENERAL cases, which take the 32 algorithms in turn, the loops of algorithms 4 and 6."""
    algorithm = number % 32 if number < GENERAL else (3, 5)[number % 2]
    model = (number // 32) % 3 if number < GENERAL else (number // 2) % 3
    feedback = next(randoms) % 5 != 0
    fb = uniform(randoms, -1.0, 7.5) if next(randoms) % 4 else float(next(randoms) % 8)
    kappa = 0.0
    if feedback:
        if fb < 0:
            kappa = fb / 128
        else:
            whole = math.floor(fb)
            low = 0.0 if whole <= 0 else 2.0 ** (whole - 8)
            high = 2.0 ** (whole + 1 - 8)
            kappa = low + (fb - whole) * (high - low)
        if kappa == 0.0:
            feedback = False
    looped = algorithm in (3, 5)
    size = next(randoms) % 3 if looped else 2
    samples = 1 + next(randoms) % (16 if model == RELAXATION and feedback else 6)
    case = Case(
        algorithm=algorithm + 1,
        feedback=feedback,
        model=model,
        kappa=kappa,
        tau=1.5 / 49096 if next(randoms) % 2 else uniform(randoms, 1e-5, 6e-5),
        onset=0.9 if next(randoms) % 3 else uniform(randoms, 0.5, 0.95),
        clamp=0.999 if next(randoms) % 3 else uniform(randoms, 0.3, 0.99),
        iterations=16,
        substeps=64,
        rate=[48000.0, 44100.0, 96000.0][next(randoms) % 3],
        samples=samples,
    )
    for _ in range(6):
        top = (0.35, 0.8, 2.0 if next(randoms) % 4 else 6.0)[size]
        law = Law(
            theta0=uniform(randoms, 0.0, 1.0),
            speed=uniform(randoms, 0.0, 0.2),
            gain0=uniform(randoms, 0.0, top),
            slope=uniform(randoms, -top / 400, top / 400),
        )
        if next(randoms) % 6 == 0:
            law.jump = float(1 + next(randoms) % samples)
            law.after = (
                uniform(randoms, 0.0, 1.0),
                uniform(randoms, 0.0, 0.2),
                uniform(randoms, 0.0, top / 2),
                uniform(randoms, -top / 800, top / 800),
            )
        case.laws.append(law)
    _ = algorithms
    return case


def run(case: Case, algorithms: list) -> list[list[float]] | None:
    """A case's samples, or none where one leaves the references."""
    graph = Graph.of(algorithms[case.algorithm - 1], feedback=case.feedback)
    state = State()
    rows = []
    before = {}
    for n in range(case.samples + 1):
        if n > 0 and not integrate(case, graph, state, n, before):
            return None
        result = output(case, graph, state, n)
        if result is None:
            return None
        outputs, total = result
        row = [float(n)]
        for k in range(6):
            value, bound = outputs[k]
            row += [*double_double(value), float(bound)]
        row += [*double_double(total[0]), float(total[1])]
        rows.append(row)
        before = outputs
    return rows


def main() -> None:
    """Writes the file."""
    algorithms = read_algorithms()
    randoms = splitmix64(SEED)
    lines = [HEADER.rstrip("\n")]
    written = 0
    number = 0
    while written < CASES:
        case = draw_case(randoms, algorithms, number)
        number += 1
        rows = run(case, algorithms)
        if rows is None:
            continue
        head = [
            float(case.algorithm),
            1.0 if case.feedback else 0.0,
            float(case.model),
            case.kappa,
            case.tau,
            case.onset,
            case.clamp,
            float(case.iterations),
            float(case.substeps),
            case.rate,
            float(case.samples),
        ]
        for law in case.laws:
            head += law.numbers()
        lines.append("case " + " ".join(f"{to_bits(x):016x}" for x in head))
        lines += ["sample " + " ".join(f"{to_bits(x):016x}" for x in row) for row in rows]
        written += 1
    OUTPUT.write_text("\n".join(lines) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
