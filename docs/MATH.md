<!--
SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
SPDX-License-Identifier: GPL-3.0-or-later

This file is part of Keptone. The SPDX lines name its copyright
holder and its license, in the machine-readable form of the REUSE
specification: the GNU General Public License, version 3 or any later version
(LICENSES/GPL-3.0-or-later.txt).
-->

# The mathematics of Keptone

This document is the only statement of the model and of the algorithms. The code follows it; it does not follow the code. A change in behaviour is made here first, in the same commit as the code and its tests. Each section names the module that implements it. Every value given here as a default is a setting (§17): the default applies when nothing else is chosen.

Keptone computes an **ideal DX7**: the continuous-time signal that the DX7's parameters describe, without the quantisation, noise and multiplexing of the 1983 hardware. It is computed in binary64 at the host's sample rate. Section 15 lists what is removed. Everything else (laws, tables, curves) is kept, because it is what a DX7 patch means.

## 0. Conventions

### 0.1 Units and symbols

| Quantity | Unit | Symbol |
| --- | --- | --- |
| time | seconds | $`t`$ |
| host sample rate | Hz | $`f_{\mathrm{s}}`$ |
| engine sample rate | Hz | $`f_{\mathrm{e}} = M f_{\mathrm{s}}`$ (§11; $`M`$ is `engine.oversampling`, 2 by default) |
| sample index at the engine rate | integer, `u64` | $`n`$, $`t_n = n / f_{\mathrm{e}}`$ |
| instant | an engine sample and a binary64 fraction of one | $`(n, \eta)`$, $`t = (n + \eta)/f_{\mathrm{e}}`$, $`0 \le \eta \lt 1`$ |
| frequency | Hz | $`f`$ |
| pitch | octaves (log₂ Hz) | $`p = \log_2\,f`$ |
| phase | **turns** (1 turn = 2π rad), binary64 | $`\theta`$ |
| level | **steps** of the DX7 log-gain scale, 1 step = $`20\,\log_{10}\,2/256 \approx 0.023\,52`$ dB | $`T`$ |
| linear gain | dimensionless | $`g = 2^{T/256 - 14}`$ (§3.4) |

"Turns" is the natural unit of the hardware phase and makes range reduction exact: $`x - \mathrm{round}(x)`$ is exact in binary64.

### 0.2 Arithmetic

- Every signal and control value is binary64. No binary32 anywhere in the audio path. Integers are used only for counts, indices, sample positions and identifiers. The one conversion out of binary64 is to the host's buffer type when the host asks for 32-bit audio (§14).
- Results are guaranteed at two levels:
  - **Accuracy contracts (kernels).** Every kernel (a function of §9, §10 or §12; an operator update; a filter) has a stated error bound against a high-precision reference: so many ulp, or an absolute bound derived from rounding analysis. The portable form and every vector form must each meet that bound. They are **not** required to agree with one another bit for bit. A vector form may reassociate sums, use Estrin's scheme instead of Horner's, fuse or split multiply–adds, or use another polynomial, as long as it meets the contract. This keeps optimisation free.
  - **Reproducibility (the engine).** With the same build, the same kernel selection (§17 `engine.kernels`, and the processor level in effect), the same settings and the same input, the output is bit-identical. That holds whatever the block splits (§0.3) and the thread timing. Across processors or builds the output may differ in its last bits. `engine.kernels = portable` gives the same output on every processor with FMA, because the portable form is plain Rust, which never contracts or reassociates floating-point operations on its own, and computes every elementary function with keptone-math's, never with the system's libm (§12).
- Bit identity across forms would require the *same sequence of rounded operations*: the same association, the same fusion decisions (`a*b + c` rounds twice, `fma(a,b,c)` once), the same polynomials, and the same reduction order across SIMD lanes. Moving one operation breaks it. FMA in hardware is necessary for that, not sufficient. That is why it is not required.
- Where the feedback operator is chaotic (β > 1, §9.3), any difference in the last bit grows exponentially. Two correct forms then produce different waveforms with the same statistics. The tests compare spectra and statistics there, not samples (§16).
- The portable form uses `f64::mul_add` where an FMA improves accuracy. The supported x86-64 baseline (v3) and every AArch64 have FMA in hardware (DESIGN §10.1), so this is fast everywhere.
- Within one form, sums over voices, operators and harmonics have a fixed, documented order, so reproducibility holds.
- Code never changes the floating-point environment (rounding mode, FTZ/DAZ, exception masks). In Rust that is undefined behaviour unless it is done inside a single `asm!` block that restores the state before it ends [RUST-SETCSR]. Subnormals are kept out structurally instead (§13.4).
- Constants are the binary64 nearest to their value, written with their derivation in a comment, for example `/* 49096 / 4096 */`.

### 0.3 Two independences the implementation guarantees

1. **Sample-rate independence.** Every law below is written in seconds and hertz. Rendering one input at 44.1, 48, 96 or 192 kHz gives signals that agree in the audible band to within the tolerances of §16. The control signals are functions of continuous time, between the samples as well (§2.5), and a segment starts where the one before it ends (item 2), so that at the instants two rates share they agree to within the rounding of their evaluation. The only exception is a setting documented as rate-dependent (for example `feedback.model = dx7-discrete`, §9.6).
2. **Block independence.** The output does not depend on how the host splits time into blocks. The engine's input is a stream of timed events, each at an engine sample and in an order: notes, controllers, the bend, the sustain pedal, per-note pitch, changes of parameters and settings, and a part's new compiled patch (DESIGN §5). The events of a sample act at its instant, in their order, before the sample there is computed: a law they change takes its new form from that instant on. No quantity is updated once per block, and nothing happens at a block's boundary: what the engine tests and resets of its own accord, a voice's end (§13.3), idleness and the flush of tiny states (§13.4), happens at engine samples that the input and the count of samples since the engine's first fix. Every law that runs in segments, the envelopes (§4, §5), the LFO's delay and phase (§6) and the portamento (§7.3), is evaluated in closed form from its segment's start, an instant (§0.1): a segment that ends between two samples starts the next at the instant it ends, and a sample at or after that instant takes the new segment. Rendering one input with random block sizes is bit-identical to rendering it in one block (§16).

### 0.4 Provenance and the duty to verify

Each constant or law carries one tag:

- **[DX7-MANUAL]**, **[DEXED-SYSEX]**: published specifications.
- **[LEVIEN-…]**, **[MRG-EG]**: measurements of real hardware.
- **[MSFA]**: values in Raph Levien's engine, partly measured, partly fitted.
- **verify**: provisional. Before a milestone that uses the value is closed, check it against an independent source ([AJXS-ROM], a published analysis of the hardware, or a measurement). Record the outcome in `docs/VERIFICATION.md`: the source consulted, what it says, whether the value changed. Where two sources disagree, the code keeps the value of the stronger source and the disagreement stays documented; a value that the stronger source refutes is dropped, not kept as a choice, and a setting that is a number keeps the verified value as its default. Where no independent source can settle a value, it stays provisional: its row says so and why, and a test pins it, so that it fails once a verified value is entered.

Read [AJXS-ROM] as documentation and write independent code. Never transcribe it. Where the firmware computes a law from a table of its ROM, Keptone writes the law as it behaves, in a form of its own, and copies no table of the ROM's (DESIGN §14).

### 0.5 The values of parameters

A parameter that is a number takes any finite binary64 (DESIGN §7.4): a value on its instrument's scale, such as 0–99, between the scale's steps, or beyond its ends, so that the user is free to take it where the instruments could not, whatever it means there. The scale is the range of the instrument's data, the steps its dumps hold, and the range that the editor and a host show; it is not a limit. A value is held as itself, in the unit of its scale: binary64 holds 53 significant bits at every magnitude, so that a value held scaled, larger or smaller, would be no finer. The laws of §2–§14 are written for the scales, and beyond them each continues by default so: a law written as a formula continues as written, a table continues linearly along its first and its last steps, and a limit of the instrument acts on what the parameters give on their scales and lets the rest pass (§0.7). A section that continues otherwise says so. Where a result would leave binary64, as a frequency beyond its largest number, a section may say what it is instead; where none does, an operation whose result would lie beyond binary64's largest finite number gives that number, with the result's sign, so that no value gives an infinity, and so no NaN, and every output stays finite. Enumerations, switches, counts and indices keep their ranges: they name things, and outside their ranges they name nothing. The settings of §17 keep their bounds, which the methods ask for.

### 0.6 Between the steps

The DX7 computes many of its laws from its parameters' steps alone, by tables and by integer arithmetic with floors. Keptone takes each such law at the steps as the DX7 computes it, its floors included, and between steps interpolates it linearly from its values at the steps around it: in each input that lies between integers, and multilinearly where several do. Where a law first computes one integer from several inputs, as the keyboard level scaling computes the distance of a key's group from the break point's (§3.2), it interpolates in that integer. So a patch of steps plays as the DX7's laws say, and a value between steps, of a parameter, a key or a velocity, moves each law continuously. Where a section interpolates otherwise, as the LFO's speed in $`\log\,f`$ (§6.1), it says so; a law that a section writes for real values, as $`\rho`$ (§4.1), takes its formula.

### 0.7 Limits of the instrument

A limit of the instrument is a bound at which one of its laws stops with its parameters on their scales: a sum that saturates, or a level held at its largest or its least. It belongs to the instrument's sound on the scales, and what a parameter's part beyond its scale changes passes it, continuously from the scale's end. Keptone evaluates a law that has limits twice: with its parameters as they are, and with each parameter that lies beyond its scale taken at the scale's nearer end; keys, velocities, the controllers' positions, the LFO and every other input that is not a parameter are the same in both. At each limit, the second evaluation takes the limit's value, as the instrument does, and the first takes that value plus the difference between the two evaluations at the limit's input. A bound that parameters set, as the bias sets the room of the amplitude modulation (§7.2), is the second evaluation's. So a law is the instrument's on the scales, continuous at their ends, and the law without its limits wherever its second evaluation reaches none of them. Each evaluation interpolates between steps as §0.6 says. The limits are these:

| Limit | Bound | § |
| --- | --- | --- |
| The depth $`D`$ of the pitch modulation | at most 255/256 | 2.4 |
| The keyboard scaling's curve $`C(u)`$ | at most 255 | 3.2 |
| The keyboard scaling $`c`$ | at most 127 | 3.2 |
| The operator's level $`\lambda`$ | 0 to 127 | 3.2 |
| The operator's offset $`O`$ | at most 4240 | 3.2 |
| The rate scaling's group of keys | 0 to 31 | 4.1 |
| The rate $`q_i`$ | at most 63 | 4.1 |
| The controllers' depth $`d_{\text{ctrl}}`$, of the pitch and of the amplitude | at most 255/256 | 7.1 |
| The sum of the EG bias's ranges | at most 255/256 | 7.1 |
| The EG bias $`b`$ | at least 0 | 7.1 |
| The depth $`a`$ of the amplitude modulation | at most 255/256 | 7.2 |
| The room that the bias leaves that depth | at most $`255/256 - b`$ | 7.2 |

Every other bound in §2–§14 belongs to its law and is no limit, as the attack's multiplier (§4.3), the LFO's least increment (§6.1), the bend's steps (§2.4) and the waits (§4.4), and each section says how its law continues; nor are the DX7's floor of the level (§4.6) and its largest attenuation (§3.2), which Keptone removes from the level.

## 1. Overview of a voice

A voice holds six operators $`k = 1 \dots 6`$, the DX7's OP1…OP6 (the number of operators is a setting for user graphs, §8.4). Operator $`k`$ outputs

```math
y_k(t) = g_k(t)\,\sin(2\pi[\theta_k(t) + \mu_k(t)]),\qquad \theta_k(t) = \theta_k(t_0) + \int_{t_0}^{t} f_k(\tau)\,\mathrm{d}\tau,\qquad \mu_k(t) = \sum_{j \to k} y_j(t)
```

- $`g_k`$ is the envelope gain (§3, §4).
- $`f_k`$ is the instantaneous frequency (§2).
- $`\mu_k`$ is the phase modulation, in turns, from the operators that modulate $`k`$ in the algorithm (§8). Modulator amplitude $`1`$ deviates the phase by $`1`$ turn. Full-scale gain is $`g = 2`$, so the full-scale index is 2 turns = $`4\pi`$ rad [MSFA `exp2.cc`, `fm_op_kernel.cc`] (VERIFICATION V1): the OPS turns the EGS's 12-bit level $`x`$, in 1/256 of an octave, into an amplitude of $`2^{x/256 - 2}`$ of its output's least step [SHIRRIFF-EXP, LEVIEN-HW], $`x`$ being $`T`$ within 16 steps where the EGS gives the top of its 12 bits for no attenuation (§3.3), and adds the modulation to the index of its sine table, $`2^{12}`$ steps a turn [SHIRRIFF-1, SHIRRIFF-SIN]. A modulator's amplitude in turns is then the gain $`g = 2^{T/256 - 14}`$ of §3.4, where the modulation's least step is the index's.
- Feedback, where the algorithm has it, is §9.
- The voice output is the sum of its carriers (§14).

## 2. Pitch and frequency (`keptone-engine::pitch`)

### 2.1 Key to frequency

The note number after transposition is

```math
k' = k + \tau(\text{TRANSPOSE} - 24) + \tau(s)
```

where TRANSPOSE (0–48 on its scale; 24 means none) is the voice's, which the DX7's firmware adds to the key before it takes the key's pitch [AJXS-ROM `VOICE_ADD`, DEXED-SYSEX], $`s`$ is the note shift of the part that plays the note (§13.5), and $`\tau`$ takes a shift in keys or in the tuning's octaves (below). $`k'`$ is a real number and is never clamped (§15). Its pitch is

```math
p_{\text{key}}(k') = \text{tuning}(k')
```

By default `tuning` is 12-TET with A4 = `tuning.a4_hz` = 440 Hz: $`p = \log_2\,440 + (k' - 69)/12`$. Alternatives: a Scala `.scl` + `.kbm` pair [SCALA-SCL, SCALA-KBM], or MTS-ESP [MTS-ESP] (optional, off by default). Under Scala, a key from the keyboard mapping's first key to its last takes the pitch of the scale's degree $`g(k)`$ that the mapping gives it, measured from the reference key $`k_{\mathrm{r}}`$, which sounds at the mapping's frequency $`f_{\mathrm{r}}`$ [SCALA-SCL, SCALA-KBM]:

```math
\text{tuning}(k) = \log_2\,f_{\mathrm{r}} + P(g(k)) - P(g(k_{\mathrm{r}})),\qquad P(g) = \lfloor g/N\rfloor P_N + P_{g \bmod N},\qquad g(k) = e_{(k - c) \bmod m} + d\lfloor (k - c)/m\rfloor
```

with $`P_j`$ the scale's $`j`$-th pitch in octaves, $`P_0 = 0`$ and $`P_N`$ its last, its period, $`N`$ its count; $`m`$ the mapping's size, $`c`$ its middle key, $`e_j`$ its entries and $`d`$ its formal octave, a degree, $`N`$ where the mapping gives 0; and $`g(k) = k - c`$ for a linear mapping, of size 0. A key whose entry is unmapped takes no note. A key outside the first and the last takes 12-TET's pitch, and a scale given without a mapping is mapped linearly on every key, degree 0 at key 60 and key 69 at `tuning.a4_hz`. Under MTS-ESP a key takes the pitch that the master gives it, and beyond keys 0 and 127 the pitch goes on from theirs in 12-TET's semitones. Between two keys, as a bend or a glide under `tuning.modulation = keys` makes a note value, the pitch is interpolated linearly between those of the nearest keys on either side that have one.

A tuning is a function from real note values to pitch. `tuning.modulation` says where the bend, the portamento and the per-note pitch (DESIGN §7.3) move a note, each a number $`k_{\text{bend}}`$, $`k_{\text{porta}}`$ and $`k_{\text{note}}`$ of semitones or keys. With `pitch`, the default, they are intervals added to its pitch, $`p_{\text{bend}}`$, $`p_{\text{porta}}`$ and $`p_{\text{note}}`$ of §2.4, each its number over 12 in octaves, so that a bend's range is in semitones under any tuning and a glide runs in pitch between two notes' pitches (§7.3), as the DX7 adds its bend apart from the key's pitch [AJXS-ROM]. With `keys` they are keys added to the note before the tuning, $`p_{\text{key}} = \text{tuning}(k' + k_{\text{bend}} + k_{\text{porta}} + k_{\text{note}})`$, and their intervals in §2.4 are 0, so that a bend's range is in the tuning's keys and a glide moves through the note values between two keys. Under 12-TET the two are the same.

A shift is in keys. With `tuning.transpose_octave = keys`, the default, $`\tau(\sigma) = \sigma`$, as on the DX7. With `period`, each whole octave of a shift, twelve keys, moves by the tuning's period instead, so that an octave of transposition spans the tuning's period, as it does under 12-TET:

```math
\tau(\sigma) = \sigma + (P - 12)\,\mathrm{trunc}(\sigma/12)
```

where trunc takes the integer part, toward 0, and $`P`$ is the period in keys: 12 under 12-TET; under Scala, the keyboard mapping's size, or the scale's count of pitches where the mapping is linear [SCALA-KBM]; and under MTS-ESP, the size of its master's keyboard mapping, or 12 where the master gives none [MTS-ESP]. $`\tau`$ splits the transpose and the note shift each into its own octaves and keys.

### 2.2 Operator frequency, ratio mode (OSC MODE = 0)

```math
f_k = r\cdot 2^{p_{\text{key}} + \delta + p_{\text{mod}}(t)},\qquad r = \rho(\text{COARSE})\cdot(1 + \text{FINE}/100)
```

- $`\rho(n) = n`$ for $`n \ge 1`$ and $`(1 + n)/2`$ below, so that $`\rho(0) = 1/2`$: the DX7's ratios at its steps, 0.5 and 1 to 31 [MSFA `coarsemul`, DX7-MANUAL], linear in the ratio between them, the ratio being what a coarse frequency means, and beyond them on either side; FINE scales the ratio linearly, as $`1 + \text{FINE}/100`$ does at its steps. A ratio at or below 0 makes the frequency 0 or negative, the operator's phase standing still or running backwards (§0.5). Where the pitch $`p_{\text{key}} + \delta + p_{\text{mod}}`$ exceeds 1023 octaves it is taken at 1023, where a sample's increment is a whole number of turns and the phase stands still, and a frequency below binary64's least is 0.
- The detune law [MSFA `osc_freq`; verify]: $`\delta = 0.0209\,\mathrm{e}^{-0.396p}p\cdot(\text{DETUNE} - 7)/7`$ octaves at the key's pitch without its modulation, $`p = \text{tuning}(k')`$, which MSFA takes once a note and which keeps §2.1's two modes alike under 12-TET, and which is taken within the pitches of 12-TET's keys 0 to 127 at A4 = 440 Hz, where the law was measured, so that it stays finite beyond them; DETUNE 0–14, 7 the centre. The DX7's detune is a nearly constant offset in Hz at low pitches, not a constant number of cents; the law models that.
- $`p_{\text{mod}}(t)`$ is §2.4.

### 2.3 Operator frequency, fixed mode (OSC MODE = 1)

```math
p_k = \log_2\,10\cdot(\varphi(\text{COARSE}) + \text{FINE}/100) + \delta_{\text{fixed}} + p_{\text{bend}}(t) + p_{\text{LFO}}(t) + p_{\text{note}}(t)
```

So 1, 10, 100 and 1000 Hz, each times $`10^{\text{FINE}/100}`$ [MSFA, DX7-MANUAL]. $`\varphi(C)`$ is $`C \bmod 4`$ at an integer C, the remainder of Euclid's division, and between two integers it is interpolated between their values (§0.6), so that from 3 to 4 it falls from 3 to 0. Where $`p_k`$ exceeds 1023 octaves it is taken at 1023, as in §2.2.

- $`\delta_{\text{fixed}}`$ [MSFA; verify]: under `osc.fixed_detune = msfa` $`13457 \cdot 2^{-24}(\text{DETUNE} - 7)`$ octaves for DETUNE > 7 and 0 below, under `symmetric` the same on both sides, and under `none` 0. The DX7 writes the detune as a sign and a magnitude [AJXS-ROM `PATCH_ACTIVATE_OPERATOR_DETUNE`], so the asymmetry is the EGS's if it is anyone's (VERIFICATION V3).
- A fixed operator follows the bend, the LFO and the per-note pitch, and neither the key, the portamento, the pitch envelope nor the master tune. The firmware gives the EGS the bend and the LFO in one register and the key, the portamento, the pitch envelope and the master tune in another [AJXS-ROM `MOD_PITCH_LOAD_TO_EGS`, `VOICE_ADD_LOAD_FREQ_TO_EGS`], so a fixed operator of the DX7 follows all of a register or none of it; which the EGS gives it is yet to be measured, and this rule is provisional (VERIFICATION V3), where MSFA has a fixed operator follow the bend and the master tune. Under `tuning.modulation = keys`, $`p_{\text{bend}} + p_{\text{note}}`$ there is the interval that the bend and the per-note pitch give the voice's key, $`\text{tuning}(k' + k_{\text{bend}} + k_{\text{note}}) - \text{tuning}(k')`$ (§2.1).

### 2.4 Pitch modulation (ratio-mode operators)

```math
p_{\text{mod}}(t) = P_{\text{EG}}(t) + p_{\text{LFO}}(t) + p_{\text{bend}}(t) + p_{\text{note}}(t) + p_{\text{tune}} + p_{\text{porta}}(t)
```

- $`P_{\text{EG}}`$: the pitch EG (§5).
- $`p_{\text{LFO}}(t) = \sigma_{\text{PMS}}\cdot D(t)\cdot \ell(t)`$, with the depth $`D = \mathrm{min}(255/256,\ d_{\text{PMD}}w + d_{\text{ctrl}})`$: the firmware adds the controllers' depth to the LFO's and saturates the sum [AJXS-ROM `MOD_PITCH_LOAD_TO_EGS`], where MSFA takes the larger of the two:
  - $`\ell \in [-1, 1]`$ is the LFO waveform and $`w \in [0, 1]`$ its delay fade (§6).
  - $`d_{\text{PMD}} = \lfloor 165\,\text{PMD}/64\rfloor/256`$ [AJXS-ROM `PATCH_ACTIVATE_SCALE_VALUE`].
  - $`\sigma_{\text{PMS}} = \text{pmstab}[\text{PMS}]/256`$ octaves with pmstab = (0, 10, 20, 33, 55, 92, 153, 255) [MSFA `pitchmodsenstab`, `Dx7Note::compute`], the firmware's values as well [AJXS-ROM].
  - $`d_{\text{ctrl}}`$ is the depth of the controllers assigned to the pitch (§7.1).
  - The saturation is a limit of the instrument (§0.7), which a depth or a range above 99 passes.
- $`p_{\text{bend}}`$: the bend, $`k_{\text{bend}}/12`$ octaves, and 0 under `tuning.modulation = keys` (§2.1). With STEP (66) at 0, the default, $`k_{\text{bend}} = x\cdot\text{RANGE}`$ semitones (function parameter 65, 0–12), $`x`$ the bend's position from −1 to 1: a value $`v`$ of $`m`$ bits gives $`(v - 2^{m-1})/2^{m-1}`$ below its centre and $`(v - 2^{m-1})/(2^{m-1} - 1)`$ above, so that both ends reach the range, where the DX7 reads 7 bits with a dead zone at the centre [AJXS-ROM `PITCH_BEND_PARSE`] (§15). With STEP $`s \gt 0`$, whatever RANGE says, the bend takes $`n = \mathrm{min}(N, \lfloor\left|x\right|\cdot(N + 1/2)\rfloor)`$ whole steps of $`s`$ semitones, $`N = \lfloor 12/s\rfloor`$, toward the side x is on, as the DX7's does, whose steps begin within 1.5 % of the bend's travel from these [AJXS-ROM `PITCH_BEND_PARSE`]: a staircase by design, which §0.6 does not smooth. A STEP above 12 bends by nothing.
- $`p_{\text{note}}`$: the per-note pitch, $`k_{\text{note}}/12`$ (DESIGN §7.3), and 0 under `keys` (§2.1).
- $`p_{\text{porta}}`$: portamento (§7.3), and 0 under `keys` (§2.1).
- $`p_{\text{tune}}`$: the master tune, $`c/1200`$ octaves for $`c`$ cents, 0 by default: a value of Keptone's system set-up, the same for every part and every tuning. The DX7's master tune of ±75 cents [DX7-MANUAL], the master tune that the data of the DX7II, the TX7 and the TX802 carry, 0 to 127 [DX7II-MANUAL, TX7-MANUAL, TX802-SYSEX], and MIDI's tuning messages set it, as the system set-up writes with M5. It moves every operator in the ratio mode, and neither one in the fixed mode (§2.3) nor a scaling (§2.6).

### 2.5 Phase

Each operator keeps $`\theta_k \in [0, 1)`$ in binary64. Per engine sample: $`\theta \leftarrow \theta + \Delta`$, then $`\theta \leftarrow \theta - \lfloor\theta\rfloor`$. The increment is

```math
\Delta = \int_{t_{n-1}}^{t_n} f_k\,\mathrm{d}\tau
```

computed for $`p_{\text{mod}}`$ as the functions of continuous time that §2.4 sums, between the samples too. The integral is split at every instant inside the sample where one of them has a corner or a jump: a corner of the pitch envelope, a corner or an edge of the LFO's wave (§6.2), the end of a stage of the LFO's delay (§6.4) and the end of a glide. On a piece where the pitch is linear in time, as the pitch envelope makes it (§5), the integral of $`2^{p}`$ is in closed form, $`\int_0^{h} 2^{a + b\tau}\,\mathrm{d}\tau = 2^{a}h\cdot E(bh)`$ for a piece of length $`h`$ that starts at the pitch $`a`$ with the slope $`b`$, where $`E(z) = \mathrm{exp2m1}(z)/(z\ln\,2)`$ and $`E(0) = 1`$ (§12.5), which keeps its accuracy however slowly the pitch moves; on any other piece, as a sine LFO makes it, a 3-point Gauss–Legendre rule is used, on pieces short enough beside the LFO's period that its error stays far below binary64's resolution (§16): at the LFO's top speed on its scale and a depth of an octave, the rule errs over a whole sample by $`0.007u`$, $`u = 2^{-53}`$, at 44.1 kHz and by less at higher rates, by $`0.44u`$ at 22.05 kHz and by $`191u`$ at 8 kHz, where the pieces must be shorter. However many of the LFO's periods a sample holds, the integral stays as exact, and the work does not grow with them: whole periods, which differ from one another only by the rest of the pitch, linear on the piece, and by the depth that a fade moves (§6.4), are summed from one period's integral, as a geometric series and a series that converges fast; the sample and hold alone, whose every period has a value of its own, adds a term for each of its values in the sample. A signal that jumps takes its new value from the jump's instant on. The phase is also evaluated at **fractional** times $`t_n - s`$ for delay alignment (§10.5), from the same closed forms.

The phase is summed by two-sums, which keep each rounding of the sum beside the phase: within $`2^{-53}`$ turns of the exact sum of its increments but for $`N\cdot 2^{-105}`$ after $`N`$ samples, so that the phase runs at the frequency that the increments give, within their own rounding. A sum rounded once a sample would drift by up to $`N\cdot 2^{-53}`$ turns after $`N`$ samples of increments below a turn, $`8\cdot 10^{-8}`$ turns in an hour at 192 kHz, and the roundings of a steady increment add up rather than cancel, an error of the frequency: over 20 s, of $`7.3u`$ at 440 Hz, $`133u`$ at 55 Hz and $`843u`$ at 4.8 Hz at 48 kHz, and of $`71u`$ and $`9045u`$ at 440 Hz and 4.8 Hz at 192 kHz, where the increment's own rounding is below $`0.5u`$ (§16).

**Initial phase** (OSC KEY SYNC, parameter 136). On: $`\theta_k = 0`$ at key-on. Off: each operator's phase continues from the last note of the part's slot that the note takes (§13.2), run on since that note ended at the operator's last frequency, as the DX7's operators run theirs on, the key sync being the OPS's [AJXS-ROM `PATCH_ACTIVATE_ALG_MODE`]; a slot that has played no note starts at 0. `osc.free_phase_start = continue | zero | random`; `random` takes each operator's phase from the part's stream of phases (§6.3).

### 2.6 Keys of the keyboard scalings

The keyboard scaling of the level (§3.2) takes a key $`k_{\mathrm{s}}`$, a real number on the scale of 12-TET's keys, which `scaling.key` gives:

- `key`, the default: the key after transposition, $`k_{\mathrm{s}} = k'`$. The DX7's firmware takes the level scaling from the transposed key's pitch, once at key-on and before the pitch envelope and the master tune reach it [AJXS-ROM `VOICE_ADD`]. Neither the tuning nor the voice's pitch modulation moves a scaling.
- `onset-pitch`: the pitch that the tuning gives the key with its per-note pitch at key-on, measured in 12-TET's semitones from the tuning's reference key $`k_{\mathrm{r}}`$: $`k_{\mathrm{s}} = k_{\mathrm{r}} + 12(p_0 - \text{tuning}(k_{\mathrm{r}}))`$, where $`p_0`$ is $`\text{tuning}(k') + p_{\text{note}}`$ at key-on, or $`\text{tuning}(k' + k_{\text{note}})`$ under `tuning.modulation = keys` (§2.1). $`k_{\mathrm{r}}`$ is the key whose pitch the tuning fixes: 69 under 12-TET, the reference key of the keyboard mapping under Scala [SCALA-KBM], and under MTS-ESP its master's reference key, or 69 where the master gives none [MTS-ESP]. So neither `tuning.a4_hz` nor the master tune moves a scaling, and under 12-TET without a per-note pitch $`k_{\mathrm{s}} = k'`$.
- `pitch`: the voice's pitch at each sample, measured in the same way, $`k_{\mathrm{s}}(t) = k_{\mathrm{r}} + 12(p_{\text{key}}(t) + p_{\text{mod}}(t) - p_{\text{tune}} - \text{tuning}(k_{\mathrm{r}}))`$: the pitch that §2.2 gives a ratio-mode operator, without its frequency ratio, its detune and the master tune, $`p_{\text{key}}`$ and $`p_{\text{mod}}`$ as §2.1 and §2.4 give them. The level's scaling then follows the bend, the portamento, the pitch envelope, the LFO and the per-note pitch, and the offset $`O`$ moves with them, as §4.7 says of a live change.

The keyboard scaling of the rates (§4.1) takes a key $`k_{\mathrm{R}}`$ of its own, which `scaling.rate_key` gives:

- `frequency`, the default: the pitch of the key after transposition with its portamento and the pitch envelope, at each sample, measured as `pitch` measures it, $`k_{\mathrm{R}}(t) = k_{\mathrm{r}} + 12(p_{\mathrm{f}}(t) - \text{tuning}(k_{\mathrm{r}}))`$, where $`p_{\mathrm{f}} = \text{tuning}(k') + p_{\text{porta}}(t) + P_{\text{EG}}(t)`$, or $`\text{tuning}(k' + k_{\text{porta}}(t)) + P_{\text{EG}}(t)`$ under `tuning.modulation = keys` (§2.1, §2.4). The DX7's EGS determines the rates from its frequency buffer, which holds the key code, the pitch envelope and the transposition [DX7-SERVICE], and to which the firmware adds the portamento and the master tune, while the LFO and the bend go to another register [AJXS-ROM `VOICE_ADD_LOAD_FREQ_TO_EGS`, `PORTA_PROCESS`] (VERIFICATION V20). So the rates follow the pitch envelope and the portamento, and neither the master tune nor the tuning's reference moves them, as neither moves the level; the rates move with $`k_{\mathrm{R}}`$ as §4.7 says.
- `level`: the level's key, $`k_{\mathrm{R}} = k_{\mathrm{s}}`$.

$`k_{\mathrm{s}}`$ and $`k_{\mathrm{R}}`$ are the voice's, the same for each of its operators, those in the fixed mode included, as the DX7's firmware takes one key for all six and gives the EGS one frequency for each voice [AJXS-ROM `VOICE_ADD`, `VOICE_ADD_LOAD_FREQ_TO_EGS`].

## 3. Levels (`keptone-engine::level`)

### 3.1 The 0–99 to level-curve map

```math
s(L) = \begin{cases} [0, 5, 9, 13, 17, 20, 23, 25, 27, 29, 31, 33, 35, 37, 39, 41, 42, 43, 45, 46]_L & L \lt 20 \\ 28 + L & L \ge 20 \end{cases}
```

[MSFA `scaleoutlevel`, LEVIEN-EG], the firmware's values as well, whose table holds $`127 - s(L)`$ [AJXS-ROM `TABLE_LOG`]. Between integers, $`s`$ is interpolated linearly (§0.6), and an envelope's level, which takes $`\lfloor s/2\rfloor`$, is interpolated as a whole (§3.3).

### 3.2 Operator offset from output level, key scaling and velocity

```math
O = \mathrm{min}(4240,\ 32\,\mathrm{min}(127, \lambda) + V)\quad(\lambda \ge 1),\qquad \lambda = s(\text{OL}) \pm c
```

$`\lambda`$ is the operator's level before its velocity, its output level raised by the keyboard scaling's $`c`$ where a positive curve applies and lowered where a negative one does, 0 to 127 on the DX7 [AJXS-ROM `PATCH_ACTIVATE_OPERATOR_KBD_SCALING_LEVEL`]. The firmware gives the EGS the attenuation $`A = 2(127 - \lambda) + a_{\mathrm{V}}`$, in units of 16 steps, $`a_{\mathrm{V}}`$ being the velocity's part, and takes it from 4 to 255 [AJXS-ROM `VOICE_ADD_LOAD_OPERATOR_DATA_TO_EGS`]: $`O = 4304 - 16A`$, from 224 to 4240. The ceiling is the DX7's, by which a velocity raises a full level by 176 steps at most. At the other end, the attenuation 255, which an operator whose $`\lambda`$ is 0 reaches at any velocity and which the firmware gives one switched off, puts the operator below the envelope's floor $`T_{\mathrm{F}}`$ at every level (§3.3), where the DX7 holds it (§4.6): Keptone, which removes that floor from the level, takes $`O`$ on below 224 by the same law. It makes an operator silent where $`\lambda`$ is at or below 0, so that an output level of 0 is off where the scaling does not raise it and sounds where a positive curve does, and between $`\lambda = 0`$ and 1 interpolates the level in the gain (§3.4), from silence at 0 to the gain at 1: $`O = O_1 + 256\log_2\,\lambda`$, with $`O_1`$ the offset at $`\lambda = 1`$. The bounds of $`C`$, $`c`$, $`\lambda`$ and $`O`$ are limits of the instrument (§0.7): an output level above 99 takes $`\lambda`$ past 127, and a velocity sensitivity above 7 takes a loud note's $`O`$ past 4240. Then:

- **Keyboard level scaling** [AJXS-ROM `PATCH_ACTIVATE_OPERATOR_KBD_SCALING_LEVEL`, `VOICE_ADD_LOAD_OPERATOR_DATA_TO_EGS`] (VERIFICATION V5).
  - The firmware groups the keys by three on the DX7's key code, $`G(x) = \lfloor (x - 1)/3\rfloor`$ for the key $`x`$ [AJXS-ROM `TABLE_MIDI_KEY_TO_LOG_F`], groups fixed on the keyboard, and places the break point BP, 0 to 99, in the group of key BP + 20. With $`k_{\mathrm{s}}`$ the key of the level's scaling (§2.6) and $`\xi = G(k_{\mathrm{s}}) + 1 - G(\text{BP} + 20)`$, the right curve applies at $`u = \xi`$ where $`\xi \ge 1`$, with the right depth, and the left at $`u = -\xi`$ where $`\xi \le 0`$, with the left depth: the group below the break point's takes no scaling, and the break point's own takes the right curve's first step. BP 39, C3, lies in the group of keys 58 to 60, so that keys 55 to 57 take none.
  - $`c = \mathrm{min}(127, \lfloor C(u)\cdot\lfloor 165D/64\rfloor/256\rfloor)`$ for the curve $`C`$ and the depth $`D`$ of the side, the depth scaled as the LFO's depths are (§2.4) [AJXS-ROM `PATCH_ACTIVATE_SCALE_VALUE`]; $`\lambda = s(\text{OL}) + c`$ with +LIN and +EXP, and $`s(\text{OL}) - c`$ with −LIN and −EXP.
  - The linear curve is $`C(u) = \mathrm{min}(255, 8u)`$, and the exponential $`C(u) = u`$ up to $`u = 9`$ and $`\mathrm{min}(255,\ 2.094\cdot 2^{u/4})`$ from $`u = 10`$ on: Keptone's laws for the firmware's two tables, which the linear gives exactly but at $`u = 22`$, where its table holds 178, and the exponential within 1.4 at every step [AJXS-ROM `TABLE_KBD_SCALING_CURVE_LIN`, `TABLE_KBD_SCALING_CURVE_EXP`]. MSFA's exponential table agrees with the firmware's up to $`u = 14`$ and lies below it at most steps above, by up to 65 at $`u = 28`$ [MSFA `ScaleCurve`]. The laws hold at every $`u \ge 0`$: past $`u = 35`$, where the firmware's tables end and it reads the bytes after them, they stay at 255 (§15).
  - Between integers, $`G`$ is interpolated linearly in its key, and the scaling $`\pm c`$ in $`\xi`$ and in the depth (§0.6), so that it is continuous in the key, the break point and the depth, and at whole ones is the DX7's.
- **Velocity** [AJXS-ROM `MIDI_PROCESS_RECEIVED_DATA`, `PATCH_ACTIVATE_OPERATOR_KBD_VEL_SENS`, `VOICE_ADD_LOAD_OPERATOR_DATA_TO_EGS`] (VERIFICATION V6): $`V = 16(2\,\text{KVS} - \lfloor\text{KVS}\cdot B(v)/8\rfloor)`$, from the velocity's part of the attenuation, $`a_{\mathrm{V}} = 15 - 2\,\text{KVS} + \lfloor\text{KVS}\cdot B(v)/8\rfloor`$.
  - $`B(v)`$ is the firmware's velocity curve, which it composes from two tables of its ROM and takes in 32 levels, from 128 at the softest to 0 from $`v = 116`$ on [AJXS-ROM `TABLE_MIDI_VEL`, `TABLE_OP_VOLUME_VELOCITY_SCALE`]. Keptone's runs linearly between 128 at $`v = 0`$, 104 at 28, 60 at 56, 30 at 88 and 0 at 116, and is 0 above. At KVS 7, before rounding down, the level it gives lies within 1.3 dB of the firmware's at the middle of each of the firmware's 32 levels, and within 1.8 dB at every velocity.
  - So a velocity from 116 on raises the level by $`32\,\text{KVS}`$ steps, up to the ceiling, and the softest lowers it by up to 1568 at KVS 7, where MSFA's table lowered it by 3344 [MSFA `ScaleVelocity`]. At whole KVS and velocities $`V`$ rounds down as the firmware's does, and between them it is interpolated (§0.6).
  - Sources of $`v`$: MIDI's velocity 0–127, each of its 128 steps, where the DX7 takes 32 of them (§15); CLAP's real-valued velocity $`x`$, as $`v = 127x`$, and MIDI 2.0's 16-bit velocity $`w`$, as $`v = 127w/65535`$, values of $`v`$ between its steps (§0.6).

### 3.3 Total level of an EG level

```math
T(L) = E(L) + O - 4256,\qquad E(L) = 64\lfloor s(L)/2\rfloor
```

[MSFA `Env::advance`, LEVIEN-EG]. $`E`$ is the envelope's part of $`T`$, and $`\lfloor s/2\rfloor`$ the level that the firmware gives the EGS, as the attenuation $`63 - \lfloor s/2\rfloor`$, its table's $`127 - s`$ halved [AJXS-ROM `PATCH_ACTIVATE_OPERATOR_EG_LEVEL`]. At an integer L, $`E`$ is the DX7's, and between integers it is interpolated as a whole (§0.6), not through an interpolated $`s`$, whose floor would keep the steps. Full scale (L = OL = 99, no scaling or velocity) is $`T_{\text{full}} = 4032 + 4064 - 4256 = 3840`$. Velocity can push $`T`$ above $`T_{\text{full}}`$, by up to 176 steps with its sensitivity on its scale (§3.2). A level below 1 is interpolated in the gain, and one at or below 0 is silent (§4.6). The DX7 holds $`T`$ no lower than its floor $`T_{\mathrm{F}} = 16`$, 90 dB below full scale; Keptone keeps the floor's timing and removes it from the level (§4.6, §15).

### 3.4 Gain

```math
g = 2^{T/256 - 14}
```

[MSFA `exp2.cc`: `Exp2::lookup(level − 14·2²⁴)`; LEVIEN-HW: "linear gain is 2^(value/256)"]. At $`T_{\text{full}}`$, $`g = 2`$. Amplitude modulation and the EG bias (§7.2) subtract from $`T`$ before the exponential. A gain is taken at most $`2^{960}`$, where $`T`$ is $`256 \cdot 974`$, so that every sum of the engine, of fewer than $`2^{62}`$ terms, stays within binary64 (§0.5).

## 4. Operator envelope (`keptone-engine::eg`)

State: the segment index $`i \in \{0, 1, 2, 3\}`$ (targets L1…L4), the segment's start, an instant $`(n_0, \eta_0)`$ (§0.1), and the start level $`T_0`$. During a segment, $`T`$ at the engine sample $`n`$ is the closed form below evaluated at $`t = (n - n_0 - \eta_0)/f_{\mathrm{e}}`$, the time since the start. It is never an accumulated sum (§0.3). A segment ends where its level, as the DX7 takes it, has reached its target and its wait is over (§4.4, §4.6), and the next starts at that instant, between two samples where it falls there.

### 4.1 Rate

```math
q_i = \mathrm{min}(63, \lfloor 41 R_i / 64\rfloor + \text{RS}(k_{\mathrm{R}})),\qquad \text{RS}(k_{\mathrm{R}}) = \lfloor \text{KRS} \cdot \mathrm{clamp}(\lfloor k_{\mathrm{R}}/3\rfloor - 7, 0, 31)/8 \rfloor
```

[LEVIEN-EG, MSFA `ScaleRate`; verify]. Between integers of $`R_i`$, of the key and of KRS, $`q_i`$ is interpolated (§0.6), so that a rate between steps is continuous. $`k_{\mathrm{R}}`$ is the key of the rates (§2.6). The bounds of the group of keys and of $`q_i`$ are limits of the instrument (§0.7): a rate above 99 or a sensitivity above 7 speeds the envelope past $`q_i = 63`$.

**verify** (VERIFICATION V20). The DX7's firmware writes only the rate scaling's sensitivity to the EGS, which scales the rates itself [AJXS-ROM `PATCH_ACTIVATE_OPERATOR_KBD_SCALING_RATE`]: RS is the EGS's law, as [MSFA] gives it, with a variant that moves some of its steps by one (`SUPER_PRECISE`). The EGS determines the rates from its frequency and rate scaling buffers [DX7-SERVICE], whence §2.6's $`k_{\mathrm{R}}`$; when it reads them is not given, and Keptone follows $`k_{\mathrm{R}}`$ at each sample.

**Decay speed** (steps per second):

```math
\rho(q) = C\cdot 2^{\lfloor q/4\rfloor}\cdot(1 + (q - 4\lfloor q/4\rfloor)/4),\qquad C = \texttt{eg.clock\_hz}/4096
```

- $`\rho`$ is continuous in real $`q`$: it interpolates $`2^{q/4}`$ linearly between multiples of 4.
- `eg.clock_hz` is the default 49 096. At $`q = 0`$ the level then falls by one step every 4096 samples of the hardware clock, 0.2819 dB/s [LEVIEN-EG].
- The DX7's sample clock is 49 096 Hz, its OPS taking 96 operators every 20.368 μs [DX7-SERVICE]. MSFA's envelope applies the same increments at every sample of its own rate [MSFA `env.cc`], so that at 44.1 kHz its speeds are 44 100/49 096 ≈ 0.898 of these: an effect of its sample rate, not a measurement (VERIFICATION V7). `eg.clock_hz = 44100` gives MSFA's speeds.
- `eg.rate_law = dx7 | exponential`. `exponential` uses $`C\cdot 2^{q/4}`$.

### 4.2 Decay (target below current)

```math
T(t) = T_0 - \rho t,\quad\text{until } T = T_{\text{target}}
```

The DX7's decays are exponential in amplitude, so they are linear in $`T`$ [LEVIEN-EG, MRG-EG].

### 4.3 Attack (target above current)

**Jump.** Whenever a segment rises, at its start or where a change re-anchors it (§4.7), a level below $`T_{\text{jump}} = 1716`$, 1700 steps above the DX7's floor (§4.6), jumps to $`T_{\text{jump}}`$ [LEVIEN-EG; MSFA's `jumptarget`], as MSFA tests it at every sample of a rise; a target below $`T_{\text{jump}}`$ is then taken at once, as MSFA takes it [MSFA `Env::getsample`]. `eg.attack_jump = true` by default; with `false`, a rise follows the law below from where it starts.

**Law.**

```math
\frac{\mathrm{d}T}{\mathrm{d}t} = \rho\cdot\mathrm{max}\left(1,\ 2 + \frac{T_{\text{full}} - T}{256}\right)
```

Up to $`T_{\text{full}} + 256`$, where the multiplier comes down to 1, the level follows

```math
T(t) = T^{\ast} - (T^{\ast} - T_0)\mathrm{e}^{-\rho t/256},\qquad T^{\ast} = T_{\text{full}} + 512
```

and above it rises at the speed $`\rho`$, so that it reaches every target. A target up to $`T_{\text{full}} + 256`$ is reached at

```math
t^{\ast} = \frac{256}{\rho}\,\ln\,\frac{T^{\ast} - T_0}{T^{\ast} - T_{\text{target}}}
```

It is computed as $`(256\ln\,2/\rho)\,\mathrm{log2p1}((T_{\text{target}} - T_0)/(T^{\ast} - T_{\text{target}}))`$ (§12.4), which keeps its accuracy where the start is near the target.

- The hardware uses the integer multiplier $`2 + \lfloor (T_{\text{full}} - T)/256\rfloor`$ [LEVIEN-EG], which changes the attack's speed in bands of 256 steps; the law above is the continuous one that the bands step (§15).
- The DX7's targets stay below $`T_{\text{full}} + 256`$: the largest it reaches is $`4032 + 4240 - 4256 = 4016`$, in the band whose multiplier is 1. A level beyond the scale can lie above (§0.5), and there the speed stays that band's, where the formula's would fall to 0 at $`T^{\ast}`$ and never reach a target there.

### 4.4 Segments that do not move

A segment that does not move, its target being the level it starts at, both as the DX7 takes them (§4.6), waits before it ends; patches use this as a delay. It waits as long as it would take to move $`W = 480`$ steps at its speed, $`W/\rho(q_i)`$, and in segment 0 a twentieth of that [MSFA `ACCURATE_ENVELOPE`; verify]: MSFA's waits, measured on two TF1s for rates below 77, are 40 s at rate 0 and lie within −6 % and +10 % of this law (VERIFICATION V8). A segment that moves by $`m`$ steps, less than one, waits $`(1 - m)\,W/\rho(q_i)`$: the DX7's levels are whole steps at its parameters' steps, so that two that differ do so by a step at least, and between the steps the wait vanishes continuously (§0.6). A key-off ends a wait (§4.5). `eg.static_segments = dx7 | none`, default `dx7`; with `none`, no segment waits. A wait ends at its instant (§0.3).

### 4.5 Key events

- **Key-on:** segment 0 starts from the level the voice is at, as the DX7's firmware has the EGS start it, with a key-off and then a key-on [AJXS-ROM `VOICE_ADD_POLY`]: from where its envelope is, for a voice taken while it sounds (§13.2), and from $`T(L_4)`$, for a voice whose note has ended or that has had none. A mono part's legato note sends no key event, and the envelopes go on [AJXS-ROM `VOICE_ADD_MONO`] (§7.3).
- **Hold:** after segment 2 the level holds at L3 while the key is down or the sustain pedal holds the note [AJXS-ROM `VOICE_REMOVE`].
- **Key-off:** once neither the key nor the pedal holds the note, segment 3 (target L4) starts from the current $`T`$, by the rise or the fall law as its direction says; a key-off during a wait ends the wait.
- After segment 3 the level holds at L4. A non-zero L4 sustains after release (DX7 behaviour, kept).

### 4.6 The floor

The DX7 holds an operator's level no lower than its floor $`T_{\mathrm{F}} = 16`$, 90 dB below full scale [MSFA `Env::advance`, LEVIEN-EG]: a segment whose target lies below $`T_{\mathrm{F}}`$ ends where its level reaches $`T_{\mathrm{F}}`$, one that starts there toward a target there waits (§4.4), and a decayed operator rests there until its voice is stolen.

Keptone removes the floor from the level and keeps its timing. A segment starts, waits and ends where the DX7's would, with its level and its target each taken no lower than $`T_{\mathrm{F}}`$; the level itself goes on below $`T_{\mathrm{F}}`$ toward the target of the segment it is in, by that segment's law (§4.2, §4.3), and stays where it reaches it; and a segment that rises, as the DX7 takes it, starts its level from $`\mathrm{max}(T, T_{\mathrm{F}})`$. So $`\mathrm{max}(T, T_{\mathrm{F}})`$ is the DX7's level at every instant: a patch's segments keep the DX7's times, and below the floor the level falls on toward its target instead of resting (§15).

An EG level of 0 targets $`-\infty`$, toward which the level falls at its segment's speed. A level between 0 and 1 is interpolated in the gain $`g`$ (§3.4), from 0 at level 0 to the gain of level 1, so that its target falls continuously to $`-\infty`$ (§0.6), and a level at or below 0 is silent. An operator whose $`T`$ lies below $`T_{\text{end}}`$ (`eg.end_db`, default −200 dB relative to $`g = 1`$) has gain 0, until a segment takes $`T`$ above it.

### 4.7 Live parameter changes

A change to a rate or level during a segment re-anchors the closed form at the change's instant (§0.3), from the $`T`$ there, toward the new target at the new speed; a change to the pitch envelope's re-anchors it in the same way (§5). The envelope's own state is $`E`$, its part of $`T = E + O - 4256`$ (§3.3). A change to the output level, the scaling or the velocity sensitivity moves $`O`$, and with it $`T`$ and every target, and leaves the envelope's segments where they were; `param.level_glide_s`, default 0.002, ramps such a change linearly in $`T`$ over that time, so that host automation does not click, and 0 makes it at once. The laws that depend on $`T`$ within a segment, the attack's speed (§4.3), the floor (§4.6) and the wait (§4.4), take $`O`$ as it was at the segment's start. Under `scaling.key = pitch`, $`O`$ follows $`k_{\mathrm{s}}`$ at every instant (§2.6), without the glide. Wherever the rates' key $`k_{\mathrm{R}}`$ moves within a segment, as the pitch envelope and the portamento move it under `scaling.rate_key = frequency`, the default, a segment's laws take $`\int\rho\,\mathrm{d}t`$ in place of $`\rho t`$, evaluated as §2.5 evaluates the phase. A change to a pitch parameter (§2) acts at once, the phases going on (§2.5); one that moves $`k_{\mathrm{s}}`$ or $`k_{\mathrm{R}}`$, as the transpose does, moves $`O`$ as a change to the scaling does and the rates at once.

## 5. Pitch envelope (`keptone-engine::peg`)

```math
P_{\text{EG}} \in [-4, +4]\ \text{octaves},\qquad \text{level}(L) = \text{pt}[L]/32
```

- pt and pr are MSFA's `pitchtab` and `ratetab` [MSFA `pitchenv.cc`], which are the firmware's values as well [AJXS-ROM `PITCH_EG_PROCESS`]: pt = (−128, −116, −104, −95, −85, −76, −68, −61, −56, −52, −49, −46, −43, −41, −39, −37, −35, −33, −32, −31, −30, −29, −28, −27, −26, −25, −24, −23, −22, −21, −20, −19, −18, −17, −16, −15, −14, −13, −12, −11, −10, −9, −8, −7, −6, −5, −4, −3, −2, −1, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 38, 40, 43, 46, 49, 53, 58, 65, 73, 82, 92, 103, 115, 127), and pr = (1, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13, 14, 14, 15, 16, 16, 17, 18, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 30, 31, 33, 34, 36, 37, 38, 39, 41, 42, 44, 46, 47, 49, 51, 53, 54, 56, 58, 60, 62, 64, 66, 68, 70, 72, 74, 76, 79, 82, 85, 88, 91, 94, 98, 102, 106, 110, 115, 120, 125, 130, 135, 141, 147, 153, 159, 165, 171, 178, 185, 193, 202, 211, 232, 243, 254, 255).
- The speed is $`\text{pr}[R]/22.0`$ octaves per second. The firmware moves the level by pr[R]/4096 octave at every second interrupt of its timer, whose period is 3140 cycles of the CPU's clock, 9.4265 MHz/8 [VDX7, DX7-SERVICE], and the cycles until the interrupt's handler resets the counter, some 25 [AJXS-ROM `HANDLER_OCF`]: $`8192 \cdot 3165/1\,178\,312.5 \approx 22.0`$, where MSFA took 21.3.
- Linear in pitch, in both directions.
- Starts at L4 at every key-on of a voice, but for a mono part's legato key-on, which another held key precedes; moves to L1, L2 and L3, holds at L3 while the key or the sustain pedal holds the note, and goes to L4 at key-off [AJXS-ROM `VOICE_ADD_POLY`, `VOICE_ADD_MONO`, `PITCH_EG_PROCESS`]. A segment whose target is the level it starts at ends at once: the pitch envelope has no waits (§4.4 is the operators').
- Closed form per segment, like §4.
- Fractional L and R interpolate the tables. Beyond their scales pt goes on along its end steps, and pr along its last step above 99 and, below 0, halving at each step, as the LFO's speed does (§6.1), so that no speed falls to 0 or below (§0.5).

## 6. LFO (`keptone-engine::lfo`)

### 6.1 Frequency

The DX7's firmware advances the LFO's phase, 16 bits, at every interrupt of its timer, the one that runs the pitch envelope at every second (§5), $`f_{\text{tick}} = 9.4265\,\text{MHz}/(8\cdot 3165) \approx 372.3`$ Hz [AJXS-ROM `HANDLER_OCF`, `LFO_GET_AMPLITUDE`; VDX7, DX7-SERVICE], by an increment that it computes from the speed $`S`$ [AJXS-ROM `PATCH_ACTIVATE_SCALE_LFO_SPEED`]:

```math
f_{\text{LFO}} = \frac{f_{\text{tick}}}{65536}\cdot\begin{cases} 11\,\mathrm{max}(1, a) & a \lt 160 \\ a\cdot(11 + \lfloor (a - 160)/4\rfloor) & a \ge 160 \end{cases},\qquad a = \lfloor 165S/64\rfloor
```

so that speed 0 has a period of 16 s and speed 99 a frequency of 49.25 Hz. MSFA's measured table lies within 0.3 % of this law at every speed [MSFA `lfo.cc`] (VERIFICATION V10). A fractional speed interpolates in $`\log\,f`$ between the steps around it (§0.6). Below 0 the speed goes on along its first step, halving at each, and above 99 the law goes on as written, however many of the LFO's periods a sample then holds (§2.5): the engine's rate bounds only how well the sidebands that the LFO makes are sampled (§10, §11.1), not the LFO. Where a sample's advance of the phase, $`f_{\text{LFO}}/f_{\mathrm{e}}`$, reaches $`2^{53}`$ turns, beyond binary64's resolution of a turn, the phase stands still, as an operator's does (§2.2), and a frequency beyond binary64's largest number is taken at that number. The LFO's phase is in turns, $`\phi = \phi_0 + f_{\text{LFO}}\cdot(t - t_0)`$ in closed form from its last start $`t_0`$, an instant (§0.1): the engine's first sample, where $`\phi = 0`$, a key-on that resets it (§6.5), or a change of speed, which carries the phase over without a jump. The product $`f_{\text{LFO}}\cdot(t - t_0)`$ is formed exactly, as the sum of two binary64 numbers with a fused multiply–add (§0.2), and reduced to its fraction, so that the phase keeps its precision however long the LFO runs.

### 6.2 Waveforms, $`\ell \in [-1, 1]`$

| Wave | $`\ell`$ at a phase $`\phi`$ taken in [0, 1) |
| --- | --- |
| triangle | $`\left\|4\phi - 2\right\| - 1`$: +1 at 0, −1 at ½ |
| saw down | $`1 - 2\phi`$ |
| saw up | $`2\phi - 1`$ |
| square | −1 below ½, +1 from ½ on |
| sine | $`-\sin\,2\pi\phi`$ |
| sample & hold | a new value at each wrap of the phase and at each reset (§6.5), held until the next |

The phase is counted from where the firmware's key sync puts it, half a turn into the firmware's own [AJXS-ROM `LFO_GET_AMPLITUDE`, `VOICE_ADD_POLY`] (VERIFICATION V11): the firmware's triangle has its minimum at its phase 0, its saws jump at half a turn, where its square turns from +1 to −1 and its S&H draws a new value, and its key sync sets its phase just below half a turn, so that its waves start where these start at 0. Waveform edges are not band-limited: the LFO is a control signal, a function of continuous time whose corners and edges split the phase's integral (§2.5); at an edge it takes its new value from the edge's instant on.

### 6.3 Random source

SplitMix64, the generator KIRAL's tests use, read at counters, so that a value needs no history: a stream seeded with $`x`$ gives as its value of index $`i \ge 1`$ SplitMix64's $`i`$-th output from the state $`x`$, its finaliser applied to $`x + i\gamma`$ modulo $`2^{64}`$, $`\gamma`$ = 0x9E3779B97F4A7C15. A part $`j`$, 0 for the first, has three streams, seeded with the outputs $`3j + 1`$, $`3j + 2`$ and $`3j + 3`$ of SplitMix64 from `lfo.seed`: the S&H values of the part's LFO, its $`c`$-th value since the engine's first sample, from 0, taking index $`c + 1`$; those of its voices' LFOs under `lfo.scope = voice`, the $`c`$-th value of the LFO of the part's $`m`$-th note, both from 0, taking index $`2^{32}m + c + 1`$; and the random phases of §2.5, operator $`k`$ of the part's $`m`$-th note taking index $`8m + k`$. A part's values thus depend on its own input alone, and the notes of a chord draw values of their own. Of an output $`x`$, an S&H value is $`2\lfloor x/2^{11}\rfloor \cdot 2^{-53} - 1`$, in $`[-1, 1)`$, and a phase $`\lfloor x/2^{11}\rfloor \cdot 2^{-53}`$, in $`[0, 1)`$.

### 6.4 Delay fade $`w(t)`$

At every interrupt of §6.1 the firmware adds to a 16-bit counter the increment $`i_{\mathrm{d}} = (16 + (a \bmod 16))\cdot 2^{2 + \lfloor a/16\rfloor}`$, $`a = 99 - \text{DELAY}`$; once the counter overflows, it raises an 8-bit fade by $`\mathrm{max}(1, \lfloor i_{\mathrm{d}}/256\rfloor)`$ at every interrupt, up to 255, and scales the LFO's depths by it (§2.4, §7.2) [AJXS-ROM `PATCH_ACTIVATE_SCALE_LFO_DELAY`, `LFO_GET_AMPLITUDE`, `MOD_PITCH_LOAD_TO_EGS`, `MOD_AMP_LOAD_TO_EGS`]. So:

- Stage 1 lasts $`t_1 = 2^{16}/(i_{\mathrm{d}}f_{\text{tick}})`$ s, with $`w = 0`$.
- Stage 2 lasts $`t_2 = 256/(\mathrm{max}(1, \lfloor i_{\mathrm{d}}/256\rfloor)f_{\text{tick}})`$ s, with $`w`$ rising linearly from 0 to 1.
- DELAY 99 gives 2.75 s and 0.69 s, and DELAY 1 38 ms each. With DELAY 0 there is no delay, $`w \equiv 1`$: the firmware restarts no delay of a patch whose DELAY is 0 [AJXS-ROM `VOICE_ADD_POLY`].
- Between the steps of DELAY, $`t_1`$ and $`t_2`$ are interpolated (§0.6). At or below 0 there is no delay, and above 99 the law goes on as written, with Euclid's division for $`a`$ below 0, so that $`t_1`$ doubles every 16 steps and $`t_2`$ stays 0.69 s.

MSFA's constant 25 190 424 s⁻¹, which its comment takes from a period of 15.5 s at speed 0, is $`2^{16}f_{\text{tick}}`$ for a tick 3.2 % faster than the firmware's; the firmware's tick, which gives speed 0 the 16 s of MSFA's own table, replaces it (VERIFICATION V12).

### 6.5 Sync and scope

- Every key-on of a poly part, and every key-on of a mono part but a legato one, which another held key precedes, restarts the LFO's delay and, with LFO SYNC, resets its phase to 0, where the S&H draws a new value [AJXS-ROM `VOICE_ADD_POLY`, `VOICE_ADD_MONO`].
- A change of DELAY carries the delay's progress over, the part of its stage that has passed going on at the new duration, as a change of speed carries the phase over (§6.1); a change to DELAY 0 ends the delay at once.
- The DX7 has one LFO shared by all voices, and each part has one shared by its voices (§13.5). `lfo.scope = part | voice`, default `part`. With `part`, a key-on restarts the part's LFO for all its voices. With `voice`, each voice has an LFO of its own, whose delay starts at its key-on and whose phase, with LFO SYNC, starts at 0 there; without LFO SYNC, every voice takes the part's phase, which no key-on resets.

## 7. Controllers and amplitude modulation (`keptone-engine::control`)

### 7.1 Controller routing

Function parameters 70–77 set range and assign for four controllers: mod wheel, foot, breath, aftertouch. The assign bits route to pitch, amplitude and EG bias [DEXED-SYSEX, DX7-MANUAL]. MPE and CLAP note expressions (pressure, timbre, tuning) map to the same destinations per voice (DESIGN §7.3). A controller at position $`x`$, from 0 at rest to 1 at its far end, contributes the depth $`x\cdot r`$, its range scaled as the LFO's depths are, $`r = \lfloor 165\,\text{RANGE}/64\rfloor/256`$ [AJXS-ROM `MOD_CALCULATE_SOURCE_SCALED_INPUT`, `PATCH_ACTIVATE_SCALE_VALUE`]. The depths of the controllers assigned to a destination add, the sum saturating at 255/256 [AJXS-ROM `MOD_PITCH_SUM_MOD_SOURCE`, `MOD_AMP_SUM_MOD_SOURCE`]: $`d_{\text{ctrl}}`$ of the pitch (§2.4) and of the amplitude (§7.2), to which the LFO's depth is then added. The EG bias is $`b = \mathrm{max}(0,\ \mathrm{min}(255/256,\ \sum_c r_c) - \sum_c x_c r_c)`$ over the controllers assigned to it: a bias controller at rest attenuates by its whole range and at its far end not at all [AJXS-ROM `MOD_PROCESS_INPUT_SOURCES`]. The saturations and the bias's least, 0, are limits of the instrument (§0.7), which a range beyond its scale passes.

### 7.2 Amplitude modulation and EG bias

The DX7's firmware gives the EGS one value for the amplitude modulation and the EG bias together [AJXS-ROM `MOD_AMP_LOAD_TO_EGS`]. The depths of the LFO and of the controllers, $`a = \mathrm{min}(255/256,\ d_{\text{AMD}}w + d_{\text{ctrl}})`$ (§6.4, §7.1), are kept within the room that the bias $`b`$ of §7.1 leaves and scaled by the LFO's wave, and the bias is added, so that the operator keeps the part of its amplitude

```math
r(t) = 1 - b - \mathrm{min}(a,\ 255/256 - b)\cdot \frac{1 - \ell(t)}{2}
```

never less than 1/256 on the scales. The firmware gives the EGS $`r`$ as its logarithm, $`-32\log_2\,r`$ in units of 1/32 of an octave, through a table of its ROM; Keptone takes the logarithm itself (§0.4). The operator's level is lowered by

```math
\Delta T_{\text{AM}}(t) = \frac{A_{\mathrm{max}}}{8}\cdot \alpha_{\text{AMS}}\cdot \log_2\frac{1}{r(t)}
```

so that, with $`A_{\mathrm{max}}`$ at its default and $`\alpha_{\text{AMS}}`$ at 1, the operator keeps the amplitude $`r`$, the tremolo and the bias linear in amplitude; the deepest on the scales, $`r = 1/256`$, lowers the level by $`A_{\mathrm{max}}\alpha_{\text{AMS}}`$ steps. On the scales, for AMS above 0 they only attenuate, and an operator whose AMS is 0 takes neither. They lower the operator's level and not its envelope: they move neither the envelope's timing (§4.6) nor a voice's end (§13.3).

The saturation of $`a`$ and its room are limits of the instrument (§0.7), the room's bound being the second evaluation's $`255/256 - b`$. A depth or a range beyond its scale passes them, and can take $`r`$ to 0 and below, where the operator keeps none of its amplitude: there $`\Delta T_{\text{AM}}`$ takes the value it tends to as $`r`$ falls to 0, so that the operator is silent for AMS above 0, and at the largest gain (§3.4) for AMS below 0.

- $`d_{\text{AMD}} = \lfloor 165\,\text{AMD}/64\rfloor/256`$ [AJXS-ROM `PATCH_ACTIVATE_SCALE_VALUE`].
- $`\alpha_{\text{AMS}} = (0, 66, 109, 255)/255`$ [MSFA `ampmodsenstab`; verify]: the firmware writes the AMS to the EGS beside the rate scaling's sensitivity [AJXS-ROM `PATCH_ACTIVATE_OPERATOR_KBD_SCALING_RATE`], and the EGS scales the value by it. Between the steps $`\alpha`$ is interpolated (§0.6), and beyond them it goes on along its end steps (§0.5), so that below 0 the modulation and the bias raise the level instead.
- $`A_{\mathrm{max}}`$ = `am.max_steps`, provisional default 2048, the firmware's 8 octaves taken whole at AMS 3 [verify] (VERIFICATION V14), with a test that fails once a verified value replaces it.

### 7.3 Portamento and voice modes

The function parameters are mono or poly (64), the portamento mode (67), 0 for retain, which a mono part takes as fingered, and 1 for follow, full time in mono, the glissando (68) and the time (69) [DEXED-SYSEX]. The portamento's foot switch, MIDI's controller 65, is off until it is pressed, as on the DX7 with its pedal up [AJXS-ROM `MAIN_PORTA_SUS_PDL_STATE_UPDATE`].

**The glide.** At every second interrupt of its timer, as it moves the pitch envelope (§5), the DX7's firmware moves each voice's pitch toward its target by $`\text{pr}[99 - \text{TIME}]/4096`$ octave times $`1 + \lfloor 4d\rfloor`$, $`d`$ the distance left in octaves, and with glissando by three times that step without the factor [AJXS-ROM `PORTA_COMPUTE_RATE_VALUE`, `PORTA_PROCESS`]. Keptone moves it continuously, by the law that the bands of that factor step (§15):

```math
\frac{\mathrm{d}d}{\mathrm{d}t} = -\kappa\,(1 + 4d),\qquad \kappa = \text{pr}[99 - \text{TIME}]/22.0\ \text{octaves per second}
```

so that $`d(t) = ((1 + 4d_0)\,\mathrm{e}^{-4\kappa t} - 1)/4`$: the glide slows toward its target and reaches it at $`t^{\ast} = \ln(1 + 4d_0)/(4\kappa)`$, computed as $`(\ln\,2/(4\kappa))\,\mathrm{log2p1}(4d_0)`$ (§12.4). Under `tuning.modulation = keys` it moves the note value by the same law, 12 keys to an octave (§2.1).

- With glissando, the glide runs in keys at the constant speed $`36\,\text{pr}[99 - \text{TIME}]/22.0`$ keys per second, and the voice sounds the key that it has reached, its start moved by every whole key that the glide has passed, at that key's pitch in the tuning (§2.1), as the firmware steps its pitch a semitone each time the glide is a semitone past it [AJXS-ROM `PORTA_PROCESS`].
- pr is §5's table [AJXS-ROM `TABLE_PITCH_EG_RATE`], between and beyond its steps as §5 takes it. TIME 0 makes no glide, as the firmware takes its fastest rate [AJXS-ROM `VOICE_ADD_POLY`]; between 0 and 1 the glide's time scale $`1/\kappa`$ falls linearly to 0, and at or below 0 there is no glide.
- A change of TIME, of the glissando or of a target re-anchors a glide at its instant, from the pitch there (§0.3). The foot switch acts at the key events below alone: a glide that has begun goes on.

**Poly.** A key-on glides where the foot switch is on and TIME is not 0 [AJXS-ROM `VOICE_ADD_POLY`]:

- With retain, the new note glides from the pitch of the part's previous key-on, and the voices that the sustain pedal holds keep their pitches.
- With follow, every voice of the part whose key is up, whether the pedal holds it, it releases or it is silent, glides to the new note's pitch, and the new note glides from where its voice's glide has left it.
- Without a glide, the new note starts at its own pitch.

**Mono.** A mono part plays in one voice, its first slot (§13.2), and holds every key it is given, of every channel, where the DX7 holds 16 [AJXS-ROM `VOICE_ADD_MONO`]:

- A key pressed with none held sounds, and starts the voice's envelopes (§4.5, §5, §6.5). A second key sounds and sets the legato's direction, up where it lies above the first, and a further key sounds where it lies beyond the sounding one in that direction. When a key is released and others are held, the highest of them sounds where the direction is up, and the lowest where it is down. The last key's release, or the sustain pedal's after it, releases the voice [AJXS-ROM `VOICE_ADD_MONO`, `VOICE_REMOVE`].
- A change of the sounding key while keys are held sends no key event: the envelopes go on (§4.5).
- With fingered, such a change glides, whatever the foot switch, and a key pressed with none held does not; with full time, every change glides, the first key's from the voice's last pitch, while the foot switch is on [AJXS-ROM `VOICE_ADD_MONO`, `VOICE_REMOVE`].

### 7.4 Performances of the DX1 and the TX7

A performance of the DX1 or the TX7 gives each side, A and B, the function data of §7.1 and §7.3, but with each controller's sensitivity $`s`$ from 0 to 15 where the DX7's range $`r`$ runs from 0 to 99 [TX7-MANUAL] (DESIGN §11.2). With `syx.performance_sensitivity = linear`, the default, the range is in proportion:

```math
r = \frac{99s}{15}
```

so that 15 gives 99 and the steps between give values between the range's steps. With `firmware`, it is $`r = \lfloor 53s/8\rfloor`$, as the DX7's firmware computes it, the high byte of $`8 \cdot 212s`$: 0, 6, 13, 19, 26, 33, 39, 46, 53, 59, 66, 72, 79, 86, 92, 99 [AJXS-ROM]. A range written as a sensitivity is $`15r/99`$, which a dump rounds to a step as `syx.export_rounding` says; under either scale, a range given by a step comes back to that step when rounded to the nearest; rounded down, ten of the firmware's sixteen do not, those of 1, 2, 3, 4, 6, 7, 9, 11, 12 and 14.

## 8. Operators and algorithms (`keptone-model::graph`, `keptone-engine::graph`)

### 8.1 Algorithm data

The 32 algorithms are data: edges $`j \to k`$, the carrier set, and the feedback edges, each algorithm a graph of six operators like any other (§8.4). They are transcribed from the chart printed on the instruments' panels [DX7-PANEL] and checked against two independent encodings, [SHIRRIFF-ALG] and [MSFA]'s `FmCore::algorithms` (VERIFICATION V16). A test asserts that all three agree in one canonical form, each algorithm's edges, each of unit gain, its carriers and its feedback edges, into which `scripts/algorithm_sources.py` decodes MSFA's flags and the OPS's control bits that [SHIRRIFF-ALG] reads. Algorithms 4 and 6 have feedback loops that span several operators, closed by the edges from OP4 and from OP5 to OP6 [DX7-PANEL, SHIRRIFF-ALG, MSFA]. The rest have at most a self-loop. A feedback edge whose FB is 0 is not in the graph (§9.1).

### 8.2 Evaluation order

1. Decompose the graph into strongly connected components (SCCs). An SCC is one operator without feedback, one operator with a self-loop, or a multi-operator loop.
2. Topologically order the SCCs.
3. In each sample, evaluate the SCCs in that order, because phase modulation uses the *same-instant* output of the modulator.
4. Solve each SCC that contains feedback by §9. Every other operator uses §10 (or a point sample when `adaa.order = 0`).
5. An operator's $`\mu_k`$ adds its modulators' outputs in the order of their operators' numbers, OP1 first, and a voice adds its carriers in that order (§14), so that each sum's order is fixed (§0.2).

### 8.3 Per-voice algorithm

The OPS can run a different algorithm in each voice; the DX7 does not use this [SHIRRIFF-REG]. A voice plays its part's compiled patch (DESIGN §6.2), and a patch that changes while it sounds (§0.3) acts on it from the change's instant: its phases and its envelopes go on, re-anchored as §4.7 says, and an operator whose strongly connected component the change alters, as a new algorithm or a feedback to or from 0 does, starts its feedback state anew there, as a new voice's starts (§9.6). `graph.algorithm_scope = patch | voice`, default `patch`. With `voice`, a sounding voice keeps the graph that it started with, its algorithm and its feedback edges, and takes the rest of the new patch.

### 8.4 User graphs

Any directed graph over $`N`$ operators (`graph.operators`, default 6) is accepted: an edge joins any operator to any other, in either direction, or to itself, loops of any length and in any number are allowed, and each edge is a modulation or a feedback edge, as in §8.1. The rules above apply unchanged. A graph comes with its own feedback gains and carrier set (DESIGN §11.2).

### 8.5 Operator switches

An operator switched off (parameter 155 [DX7-MANUAL]) gives nothing, as a carrier or as a modulator, while its envelope and its phase go on, so that switched on again it goes on from where they are; a voice's end does not wait for a carrier switched off (§13.3). A switch acts at once: the operator's gain is multiplied by a factor that moves linearly between 0 and 1 over `param.level_glide_s` (§4.7), so that switching does not click. The DX7's firmware gives a switched-off operator the attenuation 0xFF at its voice's key-on [AJXS-ROM `VOICE_ADD_LOAD_OPERATOR_DATA_TO_EGS`], so that a switch reaches a sounding note at its next key-on; Keptone's reaches it at once.

## 9. Feedback (`keptone-engine::feedback`, `keptone-math::kepler`)

### 9.1 The DX7's recurrence

```math
y[n] = g\,\sin\left(2\pi\left(\theta[n] + \kappa\,\frac{y[n-1] + y[n-2]}{2}\right)\right),\qquad \kappa = 2^{\text{FB} - 8}\ \text{turns per unit},\ \text{FB} = 1\dots 7
```

FB = 0 means no feedback: the operator has no feedback edge (§8.1). Between the steps κ is interpolated (§0.6), from 0 at FB 0, so that a feedback grows from nothing; above 7 the steps go on as the formula gives them, and below 0 along the first step, so that κ falls below 0 and the feedback is inverted. The mean of the last two outputs is Tomisawa's anti-hunting filter, which the OPS forms in an adder of its own and scales by a power of two for each step of FB [SHIRRIFF-ALG, LEVIEN-HW, AJXS-ANALYSIS, MSFA `compute_fb`]; the scale of κ, MSFA's, is provisional (VERIFICATION V17). In radians the loop gain is

```math
\beta = 2\pi\kappa g
```

At full scale, $`g = 2`$:

| FB | 1 | 2 | 3 | 4 | 5 | 6 | 7 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| β | 0.098 | 0.196 | 0.393 | 0.785 | 1.571 | 3.142 | 6.283 |

β is proportional to the operator's *current* gain, so it moves with the envelope during a note. Where κ is below 0, so is β: the regimes and the models of §9.3 to §9.7 take β by its magnitude, and §9.4 solves the operator half a turn away.

### 9.2 The ideal operator

As the sample rate goes to infinity, the delay vanishes. The ideal feedback operator is the delay-free fixed point

```math
y = g\,\sin(2\pi(\theta + \kappa y))
```

Put $`\Theta = 2\pi(\theta + \kappa y)`$ and $`\Phi = 2\pi\theta`$. Then

```math
\Theta - \beta\,\sin\,\Theta = \Phi,\qquad y = g\,\sin\,\Theta
```

This is Kepler's equation with eccentricity β [KEPLER]. It is also exactly how Tomisawa's feedback patent states the operator: an address $`y = x + \beta\,\sin\,y`$ [TOMISAWA]. The DX7's two-sample delay and averaging are its hardware realisation.

### 9.3 Regimes

$`h(\Theta) = \Theta - \beta\,\sin\,\Theta`$ has $`h' = 1 - \beta\,\cos\,\Theta`$.

- **β < 1**: $`h`$ is strictly increasing, so the solution is unique. The recurrence of §9.1 is a fixed-point iteration with contraction factor $`\left|\beta\,\cos\,\Theta\right| \lt 1`$, and it converges to that solution as $`f_{\mathrm{e}} \to \infty`$. The ideal is the DX7's continuous limit.
- **β = 1**: the solution is unique, but there is a cusp at Θ ≡ 0 where $`h' = 0`$.
- **β > 1**: $`h`$ is not monotone, so the fixed point is multivalued. The DX7's delayed loop picks a branch, hunts or turns chaotic depending on the sample rate [LEVIEN-FM]. No delay-free ideal exists, so §9.6 makes it a setting.

### 9.4 Solving β < 1 (default whenever β < 1)

Safeguarded Newton on $`h(\Theta) - \Phi`$:

- Reduce Φ to $`[-\pi, \pi]`$. The solution keeps Φ's sign, by odd symmetry: solve for $`\left|\Phi\right|`$ and give the sign back.
- Bracket: for $`\Phi \ge 0`$, $`\Theta \in [\Phi, \mathrm{min}(\Phi + \beta, \pi)]`$, since $`0 \le \sin\,\Theta \le 1`$ there. On it $`h`$ is increasing and convex, so a Newton step from a point where $`h(\Theta) \gt \Phi`$ goes down to the root and no further.
- Start every sample from [MARKLEY]'s starter, his equations (5), (9), (10), (14), (15) and (20), within a relative 2.8·10⁻⁴ of the solution; the solver keeps no state. Continuation from the previous sample's Θ advanced by the change in Φ, specified here first, took up to 11 iterations near the cusp in measurements made while the solver was written, where the starter takes at most 4.
- If a Newton step leaves the bracket, take the bracket's top instead while it is still $`\mathrm{min}(\Phi + \beta, \pi)`$, from which Newton converges monotonically, and bisect otherwise [ODELL-GOODING]. Bisection alone, specified here first, crept up to the top linearly where $`\Theta \approx \pi/2`$, the root then lying within about 10⁻¹² of $`\Phi + \beta`$, and took up to 14 iterations on the grid of §16.
- Near the cusp $`h(\Theta) - \Phi`$ cancels. Evaluate it as $`(1 - \beta)\Theta + \beta\cdot(\Theta - \sin\,\Theta) - \Phi`$, with $`\Theta - \sin\,\Theta`$ from its own Taylor polynomial below $`\Theta = \pi/2`$, and $`h'`$ as $`1 - \beta + 2\beta\,\sin^2(\Theta/2)`$ [MARKLEY], so that the residual errs by less than $`u\left|\Phi\right|`$ and the stopping rule can be met.
- Stop when $`\left|\text{step}\right| \le 2u\,\mathrm{max}(1, \left|\Theta\right|)`$, with $`u = 2^{-53}`$, or after `feedback.max_iterations` (default 16).
- A negative β (§9.1) is solved as its magnitude half a turn away: $`\Theta = \Psi + \pi`$, with $`\Psi - \left|\beta\right|\sin\,\Psi = \Phi - \pi`$, so that $`y = -g\,\sin\,\Psi`$.
- `keptone-math::kepler` works in turns, $`\Theta = 2\pi\tau`$ and $`\Phi = 2\pi\varphi`$, where sin2pi and cos2pi reduce exactly; Newton's iteration and the stopping rule carry over as they are.
- The tests record the distribution of iteration counts and fail if any case in the test grid needs more than 8.

### 9.5 Band-limited form

For a feedback operator that nothing else modulates, $`\Phi`$ advances uniformly between pitch changes. Then [KAPTEYN]:

```math
y(t) = g\,\frac{2}{\beta}\sum_{n=1}^{\infty}\frac{J_n(n\beta)}{n}\,\sin(2\pi n\theta(t))
```

This is the operator's exact Fourier series. Truncating it at the Nyquist frequency gives the **alias-free** signal at the host rate.

- **Coefficients.** $`a_n(\beta) = 2J_n(n\beta)/(n\beta)`$ are the Fourier sine coefficients of $`\sin\,\Theta(\Phi)`$.
  - Compute them by a real DFT of $`\sin\,\Theta(\Phi_m)`$ on $`K`$ equispaced Φ, using §9.4's solver at each point. For a periodic analytic function this converges geometrically [TW-TRAP]. Bessel's integral at large $`n`$ is ill-conditioned [BOOST-TRAP]; this DFT is not, because every sample it sums is bounded by 1.
  - Take $`K \ge 4 N_{\mathrm{K}}`$ (below).
  - Tabulate $`a_n`$ on a grid of β in $`[0, \beta_{\text{series,max}}]`$ (`feedback.series_beta_max`, see Cost below) at activation, in a Chebyshev basis in β per $`n`$. Evaluate by Clenshaw.
  - Checked while writing this spec: with $`K = 4096`$ the DFT agrees with mpmath's $`J_n(n\beta)`$ to 10⁻¹⁶ for $`n \le 10`$ at β = 0.3, 0.785 and 0.95.
  - `keptone-math::kapteyn` takes $`K`$ the least power of two at least $`4N_{\mathrm{K}}`$, so that the phases $`m/K`$ and $`nm/K`$ are exact in turns, and sums each DFT in double-double. The grid is the $`M + 1`$ Chebyshev–Lobatto points of $`[0, \beta_{\text{series,max}}]`$, $`M`$ a power of two from 16, doubled, keeping every point, until the upper half of each $`a_n`$'s Chebyshev coefficients lies below $`\mathrm{max}(\varepsilon/M, \nu)`$, with ε = `feedback.series_epsilon` and ν four times the noise a coefficient takes from its values (the DFT's average of the solver's errors, and their rounding). Each series is then cut after its last coefficient above that floor. For the defaults the table holds $`N_{\mathrm{K}} = 366`$ series of 9891 coefficients in all, within 1.2·10⁻¹⁶ of mpmath's $`a_n`$ for $`n \le 2000`$ on $`[0, 0.8]`$; the DFT alone is within 1.4·10⁻¹⁷ for $`2 \le n \le 2000`$ up to β = 0.95, and within an ulp for $`a_1`$.
- **Truncation.** Kapteyn's inequality [DLMF-10.14 (10.14.8)] gives $`\left|a_n\right| \le 2\Omega(\beta)^n/(n\beta)`$ with $`\Omega(\beta) = \beta\mathrm{e}^{\sqrt{1-\beta^2}}/(1 + \sqrt{1-\beta^2}) \lt 1`$. Let $`N_{\mathrm{K}}`$ be the least $`n`$ at which the geometric tail falls below `feedback.series_epsilon` (default $`2^{-53}`$), and let $`N_{\text{Ny}} = \lfloor (f_{\mathrm{e}}/2)/f\rfloor`$.
- **Choosing the form.**
  - If the tail bound beyond $`N_{\text{Ny}}`$ is below `alias.epsilon` (default −180 dB), aliasing is negligible: use the point solution of §9.4.
  - Otherwise sum the series to $`N_{\text{Ny}}`$, by Clenshaw's recurrence for $`\sum a_n\,\sin(n x)`$ with Reinsch's modification near $`\cos\,x \approx \pm1`$.
  - **Cost.** The series is needed only while $`N_{\text{Ny}}`$ is below $`N_{\text{alias}}(\beta) = \ln(\text{alias.epsilon})/\ln\,\Omega(\beta)`$, so that is the worst case in terms per sample. Computed while writing this spec, for −180 dB:

    | β | 0.39 (FB 3) | 0.5 | 0.6 | 0.7 | 0.785 (FB 4) | 0.9 | 0.95 |
    | --- | --- | --- | --- | --- | --- | --- | --- |
    | Ω | 0.514 | 0.637 | 0.742 | 0.834 | 0.901 | 0.969 | 0.989 |
    | terms | 32 | 46 | 70 | 115 | 198 | 664 | 1922 |
    | series needed above, at 48 kHz | 772 Hz | 522 Hz | 346 Hz | 210 Hz | 121 Hz | 36 Hz | 12.5 Hz |

    The cost grows without bound as β → 1. Above `feedback.series_beta_max` (default **0.8**, revised from 0.98 by this table), use the point solution and leave the remaining alias to the oversampling of §11. Measure the choice in M4.
- β changes with $`g(t)`$, so coefficients are evaluated per sample from the table. The pitch can change too. The series is exact for uniform Φ and quasi-static otherwise; its error under modulation is measured against the reference renderer (§16).

### 9.6 `feedback.model`

Where β ≥ 1 no delay-free ideal exists (§9.3), and `feedback.model` says what the operator does. `dx7-discrete` and `clamp` act at every β, and `relaxation` lets its lag grow from below β = 1, so that nothing switches where β crosses 1.

| Model | Definition | Rate-independent |
| --- | --- | --- |
| `relaxation` (default) | A first-order lag in continuous time of the fed-back signal alone: $`\tau\dot Y = -Y + y`$, the operator's output $`y = g\,\sin(2\pi(\theta + \kappa Y))`$ taking the lagged $`Y`$ in place of itself. `feedback.tau_s` defaults to 1.5/49096 s, the hardware loop's effective delay (the mean of samples n−1 and n−2). The lag grows with β: τ is 0 up to β = `feedback.relaxation_onset`, default 0.9, where the operator is §9.4's solution, and rises linearly to `feedback.tau_s` at β = 1 and on, so that the output moves into the lag and out of it continuously. Each engine sample is integrated by the trapezoidal rule in equal substeps, the fewest for which a substep is shorter than $`2\tau/(\beta - 1)`$ at the sample's largest β, so that each substep's implicit equation has one root, and at most `feedback.max_substeps`, default 64, beyond which Newton from the previous value picks the root. θ and g are taken at the substeps' instants from their closed forms, and each equation is solved by safeguarded Newton, stopped as §9.4 stops. A new voice starts with Y = 0. | to within the integration's error, which §16 measures |
| `dx7-discrete` | §9.1 at the engine rate, at every β | **no**: the timbre changes with $`f_{\mathrm{e}}`$ (documented) |
| `clamp` | §9.4 with β replaced by min(β, `feedback.beta_clamp`), default 0.999, at every β | yes |

The default is provisional. Decide it by listening tests with patches that use FB 6–7 (DESIGN §15).

### 9.7 Multi-operator loops (algorithms 4 and 6)

The loop's ideal is the delay-free system $`y = G(y)`$, with $`G`$ the composed operators. Its gain is bounded by $`B = \prod_j 2\pi\kappa_j g_j`$ over the loop's operators, $`\kappa_j`$ the feedback edge's κ for the operator it enters and 1 for the others. While $`B \lt 1`$ the system is well posed: it contracts in the output $`y_{\mathrm{b}}`$ that the feedback edge takes, on $`[-g_{\mathrm{b}}, g_{\mathrm{b}}]`$, and is solved as one equation in $`y_{\mathrm{b}}`$ by Newton inside that bracket, bisecting where a step leaves it, from the previous sample's value, stopped as §9.4 stops and after `feedback.max_iterations`. Otherwise `feedback.model` acts on the feedback edge alone, B in place of β: `relaxation` lags the fed-back $`y_{\mathrm{b}}`$ as §9.6 lags y, `dx7-discrete` feeds back §9.1's mean of $`y_{\mathrm{b}}`$'s two previous samples, the loop's operators evaluated from the one the feedback enters to the one it leaves, and `clamp` scales κ so that B is at most `feedback.beta_clamp`. The series form is not used for loops.

## 10. Antialiasing of phase modulation (`keptone-math::adaa`, `keptone-engine::graph`)

### 10.1 Input

For operator $`k`$ at engine sample $`n`$:

```math
x_n = \theta_k(t_n - s_k) + \mu_k[n]
```

- $`s_k`$ is the evaluation offset of §10.5.
- The **unwrapped** increment $`\Delta_n = x_n - x_{n-1}`$ is computed directly as (phase increment) + (difference of $`\mu`$). It is never the difference of wrapped values.

### 10.2 First order (ADAA-1)

ADAA-1 [PARKER-ADAA] interpolates $`x`$ linearly between samples, applies $`\sin\,2\pi(\cdot)`$ in continuous time, and averages over one sample. For the sine this has a **closed form**:

```math
\hat y_n = \int_0^1 \sin\,2\pi(x_{n-1} + \tau\Delta_n)\,\mathrm{d}\tau = \frac{\cos\,2\pi x_{n-1} - \cos\,2\pi x_n}{2\pi\Delta_n} = \mathrm{sinc}(\Delta_n)\,\sin(2\pi(x_n - \Delta_n/2))
```

Here $`\mathrm{sinc}\,u = \sin(\pi u)/(\pi u)`$.

- The 0/0 of the general formula disappears: sinc is evaluated stably (§12.2).
- The delay is ½ sample. The frequency response is $`\mathrm{sinc}(f/f_{\mathrm{e}})`$.
- The kernel takes $`x_{n-1}`$ and $`\Delta_n`$, and reduces $`x_{n-1}`$ exactly to $`[-1/2, 1/2]`$ first (§12.1). Target: within $`9 \cdot 2^{-53}`$ absolute (§16).

### 10.3 Second order (ADAA-2, default)

ADAA-2 [BILBAO-ADAA] is twice the second divided difference of $`F_2(x) = -\sin(2\pi x)/(4\pi^2)`$:

```math
\hat y_n = 2\,\frac{F_2[x_n, x_{n-1}] - F_2[x_{n-1}, x_{n-2}]}{x_n - x_{n-2}},\qquad F_2[a,b] = -\frac{\cos\,\pi(a+b)\,\mathrm{sinc}(a-b)}{2\pi}
```

- The two first differences are well conditioned in closed form. Only the outer quotient can cancel, when $`h = x_n - x_{n-2}`$ is small (a stalled or reversing phase in through-zero FM).
- When $`\left|h\right| \lt`$ `adaa.tau2` (default $`2^{-10}`$ turns), use the symmetric expansion about $`a = (x_n + x_{n-2})/2`$, $`b = x_{n-1}`$:

  $`\displaystyle \hat y_n \approx 2\left(\partial_a F_2[a,b] + \frac{h^2}{24}\,\partial_a^3 F_2[a,b]\right),\qquad \partial_a F_2[a,b] = \frac{1}{2}\,\sin\,\pi(a+b)\,\mathrm{sinc}(a-b) - \frac{\cos\,\pi(a+b)\,\mathrm{sinc}'(a-b)}{2\pi}`$

  With $`d = a - b`$, the CAS of `scripts/adaa_derivatives.py` gives, and checks numerically:

  $`\displaystyle \partial_a^3 F_2[a,b] = \sin\,\pi(a+b)\cdot\left(\frac{3}{2}\,\mathrm{sinc}''(d) - \frac{\pi^2}{2}\,\mathrm{sinc}(d)\right) + \cos\,\pi(a+b)\cdot\left(\frac{3\pi}{2}\,\mathrm{sinc}'(d) - \frac{\mathrm{sinc}'''(d)}{2\pi}\right)`$

  sinc″ and sinc‴ come from their Taylor series below $`\left|d\right| = 1/4`$, and from $`u\,\mathrm{sinc}'' + 2\,\mathrm{sinc}' + \pi^2 u\,\mathrm{sinc} = 0`$ and its derivative above. The remainder of the expansion is at most $`(2\pi)^4 h^4/5760 \le 0.271h^4`$, which a still phase at a quarter turn reaches.
- Measured while writing this spec (`scripts/verify_spec_math.py`, in numpy, without the exact reduction of §12.1), against a 40-digit reference of the quotient:
  - The quotient alone errs 6·10⁻¹³ at $`\left|h\right| = 10^{-3}`$ and 7·10⁻⁸ at $`10^{-8}`$.
  - The expansion without the $`h^2`$ term errs 8·10⁻⁹ at $`10^{-4}`$.
  - With the $`h^2`$ term it errs 3·10⁻¹³ at $`10^{-3}`$. The crossover near $`2^{-10}`$ keeps the worst case about 3·10⁻¹³.
- Check: three equal points give $`\hat y = \sin\,2\pi x`$.
- The kernel takes $`x_{n-1}`$, $`\Delta_n`$ and $`\Delta_{n-1}`$, so that $`h = \Delta_n + \Delta_{n-1}`$, and reduces $`x_{n-1}`$ exactly first. Target: within $`\mathrm{max}(4.81u/\tau_2 + 4.35u,\ 0.271\tau_2^4 + 14u)`$ absolute, with $`u = 2^{-53}`$ and $`\tau_2`$ = `adaa.tau2`: $`5.5 \cdot 10^{-13}`$ for the default (§16). The first part bounds the quotient, whose rounding the division by $`\pi h`$ amplifies; the second, the expansion's remainder and rounding.
- The delay is 1 sample. For a steady partial (constant $`\Delta`$) the response is $`\mathrm{sinc}^2(f/f_{\mathrm{e}})`$.
- The formula is [BILBAO-ADAA]'s. It equals the triangular-kernel filtering of the piecewise-linear $`x(t)`$ **only when $`\Delta`$ is the same over both samples**. When the increment changes, it is a different (cheaper) approximation. This was checked numerically: it agrees with the triangular kernel to 10⁻¹⁴ for linear $`x`$ and differs for kinked $`x`$. `adaa.order2_kernel = bilbao | triangular`, default `bilbao`. `triangular` integrates $`\int_0^1 t\,\sin\,2\pi(a + t d)\,\mathrm{d}t`$ and its mirror exactly. These have closed forms with a small-$`\left|d\right|`$ series (cf. the exact piecewise formulae in [GABRIELLI-POLYADAA]'s background). Derive and test it in M4, then choose the default by the alias metric.

### 10.4 Envelope gain

The output is $`\hat y_n \cdot g_k(t)`$, with $`g_k`$ evaluated at the kernel's centre: $`t_n - s_k - 1/(2f_{\mathrm{e}})`$ for ADAA-1, $`t_n - s_k - 1/f_{\mathrm{e}}`$ for ADAA-2. The EG's closed form evaluates at fractional times exactly (§4).

### 10.5 Delay alignment without delay lines

An operator with ADAA of order $`r`$ outputs a signal stamped at $`t_n - s_k - D_r`$, with $`D_1 = 1/(2f_{\mathrm{e}})`$ and $`D_2 = 1/f_{\mathrm{e}}`$ [WERNER-ADAA]. For an edge $`j \to k`$, the modulator's stamp must equal the carrier's evaluation time:

```math
s_j = s_k - D_{r_j}
```

Every carrier gets the same $`s`$, so all carriers stamp the same output time. Because phases and gains are closed-form functions of time, evaluating $`\theta_k`$ and $`g_k`$ at $`t_n - s_k`$ is exact, and no signal is delayed.

The constraints are solvable when every path from an operator to the output has the same total delay. Check the 32 algorithms in a test. Where a user graph violates this, insert integer delay lines on the offending edges (ADAA-2 delays are whole samples), or fractional-delay allpasses for ADAA-1. Report the added latency.

Operators inside a feedback SCC, solved by §9, have $`D = 0`$.

### 10.6 Limits

ADAA assumes $`x`$ is linear between samples. A modulator well above $`f_{\mathrm{e}}/4`$ bends it, and the residual alias then grows with the modulation index. `adaa.interp = linear | cubic`; `cubic` follows [ZB-INTERP] (later milestone). ADAA also does not remove aliasing already present in $`\mu`$. ADAA on modulators is on by default (`adaa.modulators = true`).

### 10.7 Passband droop

The response is $`\mathrm{sinc}^r(f/f_{\mathrm{e}})`$: at 20 kHz, −0.6 dB per order at $`f_{\mathrm{e}}`$ = 96 kHz and −3.2 dB per order at 44.1 kHz. `adaa.droop_eq = auto | on | off`.

- `on`: a fixed minimum-phase FIR at $`f_{\mathrm{e}}`$ inverts $`\mathrm{sinc}^r`$ up to `adaa.droop_eq_hz` (default min(20 kHz, 0.45 $`f_{\mathrm{s}}`$)), with gain capped at +6 dB.
- `auto` (default): `on` when $`M = 1`$, `off` otherwise.
- With `adaa.order = 0` there is no droop, and no equaliser runs.

The droop depends on the instantaneous frequency, so the equaliser is exact only for steady partials. The tests measure the residual.

## 11. Internal oversampling (`keptone-engine::resample`)

Optional; `engine.oversampling` defaults to 2.

### 11.1 Rate

$`f_{\mathrm{e}} = M f_{\mathrm{s}}`$, $`M = 2^m`$. Every law above is in seconds, so behaviour does not change with $`M`$ (§0.3). Oversampling refines *how well* the ideal signal is sampled; it does not change *what* the signal is.

### 11.2 Decimation

$`m`$ cascaded ×2 halfband stages. `resample.filter = iir | fir`.

- `iir` (default): two-path polyphase allpass elliptic halfband [VALENZUELA, HIIR]. Coefficients are designed in binary64 at activation from `resample.stopband_db` (default 150) and the transition band. The last stage passes to min(20 kHz, 0.45 $`f_{\mathrm{s}}`$). Latency is effectively zero, and the phase is non-linear.
- `fir`: linear-phase halfband FIR (Parks–McClellan). Its latency is reported through CLAP's latency extension.

Filters are applied to the summed voice signal: decimation is linear, so it commutes with the sum.

### 11.3 Adaptive oversampling (later milestone)

Estimate each carrier's bandwidth $`B = 2(\Delta f + f_{\text{mod}})`$ [CARSON], with $`\Delta f = 2\pi g_{\text{mod}} f_{\text{mod}}`$, and choose the smallest $`M`$ for which components folding past $`M f_{\mathrm{s}} - f_{\mathrm{s}}/2`$ fall in the stopband. Change $`M`$ only at note-on (`resample.adaptive_switch = note-on`), which needs per-voice decimators.

## 12. Elementary functions (`keptone-math`)

All are portable Rust (the reference) with optional vector forms (DESIGN §10). The engine and the tools compute every elementary function with these, never with the system's libm, whose results differ between systems and versions (§0.2). Their tests compare against [CORE-MATH]'s correctly rounded functions, and sinc and sinc′, which CORE-MATH does not have, against references computed with mpmath (§16).

### 12.1 sin and cos of turns

$`\mathrm{sin2pi}(x) = \sin\,2\pi x`$.

1. $`r = x - \mathrm{round}(x)`$ (exact), giving $`r \in [-1/2, 1/2]`$.
2. Reduce by octant symmetry to $`[0, 1/8]`$.
3. Evaluate minimax polynomials for $`\sin\,2\pi r`$ and $`\cos\,2\pi r`$, designed with [SOLLYA], with certified bounds in the comments.

Target: ≤ 1 ulp against `cr_sinpi(2r)` (2r is exact). `cos2pi` is analogous.

### 12.2 sinc and its derivative

- For $`\left|u\right| \ge 2^{-6}`$: $`\mathrm{sinc}(u) = \mathrm{sin2pi}(u/2)/(\pi u)`$, with $`\pi u`$ as a double-double.
- Below: an even Taylor polynomial to degree 12, exact to < 1 ulp on that range.
- For $`\left|u\right| \ge 1/2`$: $`\mathrm{sinc}'(u) = (\cos\,\pi u - \mathrm{sinc}\,u)/u`$. Below: an odd minimax polynomial. The formula cancels as $`u \to 0`$, and near the zeros of sinc′ ($`u \approx 1.4303, 2.4590, \ldots`$) no error bound relative to the result can hold, so from 1/2 on its bound is absolute, which is what ADAA-2 needs of it (§10.3).

Targets: sinc ≤ 2 ulp; sinc′ ≤ 2 ulp for $`\left|u\right| \lt 1/2`$, and within $`9 \cdot 2^{-53}`$ absolute from 1/2 on.

### 12.3 Exponentials

$`\exp_2(z)`$: split $`z = k + r`$ with $`r \in [-1/2, 1/2]`$, evaluate a minimax polynomial for $`2^r`$, and scale exactly by building $`2^k`$ from exponent bits. Gains are bounded away from subnormals (§13.4). $`\mathrm{e}^{-x} = \exp_2(-x\,\log_2\,\mathrm{e})`$. Target ≤ 1 ulp against `cr_exp2`.

### 12.4 Logarithms

$`\log_2\,x`$: split $`x = 2^{k} m`$ from the exponent bits, exactly, with $`m \in [1/\sqrt{2}, \sqrt{2})`$, and evaluate $`\log_2\,m`$ from $`t = m - 1`$, which Sterbenz's lemma makes exact, by a minimax approximation of $`\log_2(1 + t)/t`$ designed with [SOLLYA], with certified bounds in the comments, so that the result keeps its relative accuracy where x is near 1. $`\mathrm{log2p1}(x) = \log_2(1 + x)`$ takes x itself where it is small, so that a ratio near 1, as §4.3's, written as 1 plus its small part, loses nothing to its rounding. $`\ln\,x = \log_2\,x\cdot\ln\,2`$. Targets: ≤ 1 ulp against `cr_log2` and `cr_log2p1`.

### 12.5 exp2m1

$`\mathrm{exp2m1}(z) = 2^{z} - 1`$, computed without the cancellation of $`\exp_2(z) - 1`$ near 0, from a minimax approximation of $`(2^{z} - 1)/z`$ there, designed with [SOLLYA], and with extra precision where the subtraction would round the result more than once. It gives §2.5's closed form. Target ≤ 1 ulp against `cr_exp2m1`.

## 13. Voices (`keptone-engine::voice`)

### 13.1 Polyphony

`voice.count`, default 16, from 1 up to what memory allows. Voices are allocated at activation and never on the audio thread. Every part takes its voices from this one pool, into slots of its own (§13.2, §13.5).

### 13.2 Allocation and stealing

A part keeps `voice.count` slots of its own, in an order, and a note it takes goes into one of them; the pool of §13.1 only counts the voices that sound. A slot keeps what its last note leaves, its phases (§2.5), and a part's voices are summed in the order of its slots (§14). So, while the pool has a voice to spare, what a part plays depends on its own input alone.

A note goes to the part's first slot, after the one it gave last, whose voice has ended (§13.3), while the pool has a voice to spare. Otherwise it takes a voice as `voice.steal = dx7 | oldest | quietest | released-first` says, of the part's own first and of every part's after:

- `dx7`, the default: the first voice whose key is up, sounding or not, after the one last given, as the DX7 takes its voices; a voice held by the sustain pedal alone counts as one whose key is up, and where every key is held the note takes none [AJXS-ROM `VOICE_ADD_POLY`].
- `oldest`: the voice whose note started first, the input's order breaking a tie.
- `quietest`: the voice whose loudest carrier has the lowest level $`T`$ at the note's instant, with $`O`$ and without amplitude modulation (§4, §7.2), the oldest breaking a tie.
- `released-first`: of the voices whose key is up, the one released first; where every key is held, the oldest.

The parts that take a note (§13.5) are served in the order of their indices. A note is known by its part, its channel and its key before the note shift, or by its identifier where the host gives one, and its note-off goes to the voices that its note-on started. A voice taken while it sounds starts its envelopes from where they are (§4.5). A key pressed again while its voice sounds is as `voice.retrigger = same-voice | new-voice` says: with `new-voice`, the default, the note takes a voice as above, and the voice already there, released if its key was still held, goes on to its end, as on the DX7 [AJXS-ROM `VOICE_ADD_POLY`]; with `same-voice`, the note restarts that voice.

### 13.3 End of a voice

A voice ends when no key or sustain holds it and every carrier of its graph has ended: switched off, its factor at 0 (§8.5), or in segment 3 or the hold after it, with its envelope's level, $`O`$ included and the amplitude modulation not (§4.7, §7.2), below $`T_{\text{end}}`$. Its end is reported to the host (CLAP note-end). It is tested at every engine sample, so that a voice ends at the same sample whatever the blocks (§0.3).

### 13.4 Subnormals and stopping

No recursive state is allowed to decay into subnormals:

- A gain is 0 where its envelope's level lies below $`T_{\text{end}}`$ (§4.6), and at least $`10^{-10}`$ above it at the default, before amplitude modulation lowers it by a bounded amount (§7.2).
- The relaxation state of §9.6 and every decimator state are set to exact zero when their voice, or the whole engine, has been silent for $`\lceil\texttt{engine.idle\_s}\cdot f_{\mathrm{e}}\rceil`$ engine samples, `engine.idle_s` 0.5 s by default, counted from the first at which it is silent: a voice is silent where every operator's gain is 0 (§4.6, §8.5), and the engine where every voice is silent or has ended.
- In addition, at every engine sample $`n`$ that is a multiple of 64, counted from the engine's first, every IIR state whose magnitude is below $`2^{-500}`$ is set to zero. The rule is a function of the state and of $`n`$ alone, so block independence (§0.3) holds.

### 13.5 Parts

A part plays one patch on the notes it takes (DESIGN §6.4). There are `part.count` of them, default 2, the A and B of the DX1 and the DX7II, fixed at activation. With the first part alone on, as it is by default, this document reads as it is written for the DX7.

- **The notes a part takes.** A note event goes to every part that takes it: the part receives the event's channel, or every channel; the key lies between the part's lowest key and its highest, both included; and the part's key filter, every key, the odd ones or the even ones [DX7II-MANUAL], lets it through. The part plays the key shifted by its note shift, −24 to +24, which adds to the patch's transpose (§2.1) [TX802-MANUAL]. A part that is off takes no note, as a tone generator of the TX802 switched off takes none [TX802-MANUAL]; the first part is on by default, and every other off. The DX7II's split is two parts on one channel whose ranges meet at the split point, and its dual two parts on every key.
- **Alternate groups.** The parts of an alternate group, adjacent and on one channel, take its notes in turn, each new note the next part's, within the range of the group's first part [TX802-MANUAL].
- **Voices.** Every part takes its voices from the one pool of §13.1, into slots of its own (§13.2). A part's note limit, where it has one (by default it has none), caps how many of them it holds; a note beyond it takes the voice of the part's own that §13.2 picks. A note that several parts take starts a voice in each.
- **What a part's voices share**: the LFO (§6.5), the portamento and the mono or poly state (§7.3), and the controllers (§7.1). Parts on one channel hear the same controller messages.
- **The function data and the supplement.** A supplement holds for its voice what the function data holds for the instrument: the poly or mono, the pitch bend, the portamento and the controllers [DX7II-MANUAL]. Where a part's patch has one, the part plays the supplement's, as the DX7II does; with `part.function_source = part`, it plays its own function data. `part.function_source = patch | part`, default `patch`. Until the supplement's laws are written (M9a, DESIGN §16), a part plays its own function data, whatever `part.function_source` says.
- **Output.** A part sums its voices as §14 sums them, scales the sum by its volume, 1 by default, and its pan, the centre by default, places it in the output (§14).
- **What a part starts with.** Its patch is the DX7's VOICE INIT, the voice its firmware 1.8 sets up for one to be made [AJXS-ROM]: every operator's envelope at rates 99, 99, 99, 99 and levels 99, 99, 99, 0, its break point, depths, curves, rate scaling, sensitivities and fine frequency 0, its detune 7, which is none, its coarse frequency 1 in the ratio mode, and its output level 0 but OP1's, 99; the pitch envelope at rates 99 and levels 50; algorithm 1, no feedback, and the oscillators' key sync on; the LFO at speed 35, with no delay and no depths, a triangle, its key sync on; a pitch sensitivity of 3, and the transpose at 24, which is none; the name INIT VOICE; and no supplement and no fractional scaling. Its function data is Keptone's own, the DX7 having none but what its battery keeps, which its firmware checks at power-up and does not set [AJXS-ROM]: poly, a pitch bend of 2 semitones without steps, the portamento retaining with no time and no glissando, and the modulation wheel at its range of 99 to the pitch, no other controller assigned. It receives every channel, takes every key without a filter, a note shift or a note limit, and alternates with none.
- The rest of a part's laws, its detune, volume and pan, its alternate group, EG forced damp, and what a supplement, unison and micro tunings do, are written here as Keptone's own, with the milestones that give them sound (DESIGN §16). Until then a part plays its patch as a DX7 voice, at a volume of 1 and at the centre, and alternates with none.

## 14. Output (`keptone-engine::mix`)

```math
\text{out}[n] = \texttt{output.gain}\cdot\sum_{\text{voices}}\sum_{c\in\text{carriers}} y_c[n]
```

- `output.gain` defaults to 0.5, which puts one full-scale carrier at 0 dBFS.
- `output.carrier_normalization = none | count | sqrt-count`, default `none` (DX7): `count` divides a voice's sum by the number of its graph's carriers, switched off or not, and `sqrt-count` by that number's square root.
- The sum above is one part's, and mono (§13.5), its voices taken in the order of the part's slots (§13.2). The output is one port. With `output.channels = stereo`, the default, it has two channels, and each part's sum, scaled by its volume, goes to both, placed by the part's pan, as $`((\texttt{output.gain}\cdot S)\cdot v)\cdot q`$ for the part's sum $`S`$, its volume $`v`$ and the pan's gain $`q`$ for the channel: at the centre, the pan's default, $`q = 1`$ on both. With `mono` it has one channel, which takes every part's sum, scaled by its volume, without its pan. The port adds the parts in the order of their indices. The pan's law and per-operator pan are later settings; outputs beyond these are the host's, another instance on another track (DESIGN §1).
- When the host offers 64-bit ports, the output stays binary64. Otherwise it is rounded to nearest binary32 with no dither: the signal's own floor is far below binary32's resolution. Each product and sum of the output is taken no further than the format's largest finite number, so that a gain, a volume or a pan beyond its scale gives no infinity, and so no NaN.
- There is no limiter or soft clip; that is the host's business.

## 15. What is removed, and what is kept

**Removed** (sources: DX7-SERVICE, AJXS-ANALYSIS, LEVIEN-HW, SHIRRIFF-*):

- The 12-bit DAC with its ×1/2/4/8 exponent scaling, the sample-and-hold, the 16 kHz output filter, and the 16-voice time multiplexing.
- The log-sine and exponential ROM quantisation.
- The 12-bit EG and its clocked increment patterns.
- The Q10 log frequency and its 1/4096 linear steps within an octave.
- The finite phase accumulator.
- The residual output that some algorithms (16–18) never let fall to zero [PLOGUE-OPS7 discussion].
- The 49 096 Hz sampling and its aliasing.
- The key after transposition held in a byte: the firmware clamps a key above 127 to 127, and a key below 0, which wraps around, to 127 as well [AJXS-ROM `VOICE_ADD`].
- The steps of the parameters, between which every law moves continuously (§0.6).
- The 8 bits in which the firmware combines the amplitude modulation and the EG bias, and the steps of the table of its ROM that takes their logarithm [AJXS-ROM `MOD_AMP_LOAD_TO_EGS`].
- The 8 bits in which the firmware multiplies the LFO's depth, its wave and its sensitivity, with their floors and a wave from −128 to 127 [AJXS-ROM `MOD_PITCH_LOAD_TO_EGS`].
- The LFO's steps at the firmware's 372 Hz, its 16-bit phase and 8-bit wave with the 64 steps of its sine's quarter, the 8-bit linear congruential generator of its sample and hold, and the 8-bit fade of its delay, which stops at 255/256 [AJXS-ROM `LFO_GET_AMPLITUDE`].
- The pitch envelope's steps at the firmware's 186 Hz, and the runs of it and of the portamento that the firmware skips while MIDI data wait to be read [AJXS-ROM `HANDLER_OCF`].
- The portamento's steps at 186 Hz, which take half of the voices at each interrupt, its speed in bands of a quarter octave of its distance, rounded one way for a glide up and another for one down, and the mono part's 16 keys [AJXS-ROM `PORTA_PROCESS`, `VOICE_ADD_MONO`].
- The DX7's 32 levels of velocity [VDX7 FAQ; AJXS-ROM `MIDI_PROCESS_RECEIVED_DATA`].
- The irregularities of the scaling's tables: key 0 in key 1's group, by the key code's floor; the linear curve's 178 at $`u = 22`$, where the laws of §3.2 give 176; and the curves' reading past their last step, $`u = 35`$, into the bytes after them, which break points 0 to 4 reach at keys above 123 and 92 to 99 at keys below 10 [AJXS-ROM `TABLE_MIDI_KEY_TO_LOG_F`, `TABLE_KBD_SCALING_CURVE_LIN`, `PATCH_ACTIVATE_OPERATOR_KBD_SCALING_LEVEL`].
- The attack's integer multiplier, which changes its speed in bands of 256 steps [LEVIEN-EG].
- The floor at which the DX7 holds an operator's level, 90 dB below full scale, until its voice is stolen, and the least offset, 224, that the firmware's largest attenuation gives, which lies beneath it: the level falls on below both, and the envelope keeps the floor's timing (§3.2, §4.6).
- The bend's 7 bits and the dead zone at its centre [AJXS-ROM `PITCH_BEND_PARSE`].
- The operator switches' reaching a sounding note at its next key-on alone [AJXS-ROM `VOICE_ADD_LOAD_OPERATOR_DATA_TO_EGS`] (§8.5).
- The block-rate control and the 1024-point sine table of MSFA-based emulators.

**Kept** (they define patches): every law and table in §2–§9, the limits of the instrument on the scales (§0.7), the attack jump, the static segment delays, the envelope's timing at the floor, sustain after release with L4 > 0, the shared LFO, and the DX7's keyboard scaling, velocity and rate scaling curves. The attack jump, the static segments and the shared LFO each have a setting that lets the user step away from them.

## 16. Tests and tolerances

An error in ulp is measured from the correctly rounded result, in units in the last place of that result, as against CORE-MATH; an absolute error, from the exact value.

| What | Check | Tolerance |
| --- | --- | --- |
| sin2pi, cos2pi, exp2, log2, log2p1, exp2m1 | against CORE-MATH, ≥ 10⁸ random inputs and special points | ≤ 1 ulp |
| sinc, sinc' | against MPFR-precision references (`scripts/` with mpmath) | ≤ 2 ulp; sinc' for $`\left\|u\right\| \ge 1/2`$: ≤ $`9 \cdot 2^{-53}`$ absolute |
| ADAA-1, ADAA-2 | against mpmath references of the closed forms and of the divided difference, the inputs exact (`scripts/adaa_references.py`): special points (still, steady and reversing phases, $`h`$ on either side of $`\tau_2`$) and random inputs; ADAA-2 at $`\tau_2 = 2^{-12}, 2^{-10}, 2^{-8}`$; three equal points give $`\sin\,2\pi x`$ | ≤ $`9 \cdot 2^{-53}`$ absolute; ≤ $`\mathrm{max}(4.81u/\tau_2 + 4.35u,\ 0.271\tau_2^4 + 14u)`$ absolute |
| Kepler solver | residual over a β × Φ grid (β ∈ [0, 0.999], 10⁶ points), and iteration counts; the same nearer the cusp, β up to 1 − 2⁻⁵² and Φ down to 10⁻³⁰⁰, the residual in double-double | $`\left\| h(\Theta) - \Phi\right\| \le 4u\,\mathrm{max}(1,\left\|\Phi\right\|)`$; ≤ 8 iterations |
| Kapteyn coefficients | against mpmath for n ≤ 2000, β on a grid (`scripts/kapteyn_references.py`): the DFT up to β = 0.95, the table of the defaults inside its domain; Kapteyn's inequality and the tail bound on the same references | ≤ 1e-15 absolute |
| Series vs point solution | where both apply and the tail is negligible | ≤ 1e-14 |
| Pitch | §2.1–§2.3's transposition, 12-TET's pitch and its inverse, the ratio, the detunes, the fixed mode's pitch and the frequency against mpmath's values of their laws at their binary64 inputs (`scripts/pitch_references.py`), on the parameters' scales and beyond them | within a bound from rounding analysis, beside each function |
| Phase increment | §2.5's closed form against its exact value and its Gauss–Legendre rule against mpmath's quadrature (`scripts/phase_references.py`), over pitches that move linearly, barely, steeply and across 1023 octaves, and as a sine, the LFO's, at its speeds on its scale and beyond, a sample long at every rate from 8 kHz to 768 kHz and in parts of a sample; and the rule's instants | within the bound that each function documents from rounding analysis, and for the rule with the bound of its truncation, beside the test |
| Phase accumulator | the phase against the exact sum of its increments, every one a multiple of $`2^{-80}`$: steady tones of 4.8 Hz to 3520 Hz for 20 s at 48 kHz and 192 kHz, and random increments of either sign; beside it, a sum rounded once a sample, whose error of the frequency is reported | the phase within $`2^{-53}`$ and $`N\cdot 2^{-105}`$ turns after $`N`$ samples, the rounded sum within $`N\cdot 2^{-53}`$ |
| Relaxation | renders with each sample's substeps doubled, twice, on patches with β from below `feedback.relaxation_onset` to beyond `feedback.max_substeps`'s reach | observed order 2; the error within a bound beside the test |
| Algorithm data | the table against [MSFA]'s flags and the OPS's ROM as [SHIRRIFF-ALG] reads it, both decoded by `scripts/algorithm_sources.py` (§8.1) | all 32 identical in the canonical form |
| Evaluation order | §8.2's components and their order for the 32 algorithms, with their feedback edges and without them; for random graphs, against the reachability that Warshall's closure of their edges gives; for a chain and a loop of 200 000 operators | exact |
| Feedback loops | §9.7's equation's residual over a grid of gains and phases, and iteration counts | as the Kepler solver's |
| EG | closed form vs high-precision integration of the ODE; segment times vs §4.3's $`t^{\ast}`$ | ≤ 1e-12 relative |
| Sample-rate independence | the same input at 44.1, 48, 96 and 192 kHz, its events on the grid of 1/300 s that the four rates share; each voice's control values ($`T_k`$, $`p_k`$, $`P_{\text{EG}}`$, $`\ell`$, $`w`$ and its segments' instants), read through a probe of the tests, against mpmath references at each rate's own samples, and the rates against one another at the instants they share; the audio at those instants where every law is in closed form, and its spectrum below 20 kHz where one is integrated step by step (§9.6) | each control value within a bound from rounding analysis, absolute in its unit, its derivation beside the test; the audio within the bound of its phases' rounding; the spectrum: see the alias metric |
| Block independence | random block splits, with events and new compiled patches at random samples, vs one block | **bit-identical** |
| Kernel contracts | every form (portable and vector) at every ISA level (with Intel SDE where the runner lacks it) vs the high-precision reference | each kernel's stated bound (§0.2), e.g. ≤ 1 ulp for sin2pi |
| Reproducibility | same build, same kernels, same settings and input, rendered twice in different processes with different thread timing | **bit-identical** |
| Forms agree on the engine | a full render with `engine.kernels = auto` vs `portable`, non-chaotic patches | ≤ the error budget the contracts imply (derived per patch class; start at −200 dBFS) |
| Chaotic regimes | as above, patches with β > 1 | long-term spectrum (Welch) within 0.1 dB below 20 kHz; level statistics within 0.1 dB |
| Alias metric | each patch of `conformance/patches/` rendered at $`f_{\mathrm{s}}`$ and by the reference renderer (`engine.oversampling = 64`, FIR decimation, `adaa.order = 0`); the residual's energy below min(20 kHz, 0.45 $`f_{\mathrm{s}}`$), in dB relative to the signal | regression baselines in `bench/baselines.toml`; may only improve |
| .syx round trip | every valid dump, parameter change and dump request → model → message; a performance under format 2 is written under 1 | byte-identical |
| Parsers | random mutations of the seeds (SplitMix64) plus `fuzz/regressions/` on every test run; libFuzzer in CI | never a panic; every input is accepted or rejected |
| Finiteness | random patches, their numbers beyond their scales up to binary64's largest finite number, × random event streams × random settings | output always finite; the audio thread never allocates (RTSan) |
| Limits of the instrument | each law of §0.7's table at random parameters on their scales, at their ends and beyond them, against the law with its limits and the law without them | on the scales, the law with its limits; beyond them, the law without its limits wherever the second evaluation reaches none; continuous at the scales' ends |
| Subnormals | inputs that drive the relaxation's lag, dx7-discrete's earlier outputs, the decimators and a decay to level 0 toward the subnormals | no subnormal reached; the states zeroed at the samples §13.4 names |

## 17. Settings index

Every setting is stored in the plugin state and has a host parameter where continuous automation makes sense. Defaults are chosen for the main use (playing DX7 patches cleanly). Anything else is the user's choice. A setting's row is its number, which its ID holds (DESIGN §7.4): from Keptone's first release on, a setting added later takes the next row, whatever its section, and none is taken away.

| Setting | Default | Values | § |
| --- | --- | --- | --- |
| `tuning.a4_hz` | 440 | > 0 | 2.1 |
| `tuning.source` | 12-tet | 12-tet / scala / mts-esp | 2.1 |
| `osc.fixed_detune` | msfa | msfa / symmetric / none | 2.3 |
| `osc.free_phase_start` | continue | continue / zero / random | 2.5 |
| `eg.clock_hz` | 49096 | > 0 | 4.1 |
| `eg.rate_law` | dx7 | dx7 / exponential | 4.1 |
| `eg.attack_jump` | true | bool | 4.3 |
| `eg.static_segments` | dx7 | dx7 / none | 4.4 |
| `eg.end_db` | −200 | < 0 | 4.6 |
| `param.level_glide_s` | 0.002 | ≥ 0 | 4.7 |
| `lfo.scope` | part | part / voice | 6.5 |
| `lfo.seed` | 0 | u64 | 6.3 |
| `am.max_steps` | 2048 (provisional) | ≥ 0 | 7.2 |
| `graph.algorithm_scope` | patch | patch / voice | 8.3 |
| `graph.operators` | 6 | ≥ 1 | 8.4 |
| `feedback.model` | relaxation (provisional) | relaxation / dx7-discrete / clamp | 9.6 |
| `feedback.tau_s` | 1.5/49096 | > 0 | 9.6 |
| `feedback.beta_clamp` | 0.999 | (0, 1) | 9.6 |
| `feedback.max_iterations` | 16 | ≥ 1 | 9.4 |
| `feedback.series_beta_max` | 0.8 | [0, 1) | 9.5 |
| `feedback.series_epsilon` | 2⁻⁵³ | > 0 | 9.5 |
| `alias.epsilon` | −180 dB | < 0 | 9.5 |
| `adaa.order` | 2 | 0 / 1 / 2 | 10 |
| `adaa.tau2` | 2⁻¹⁰ | > 0 | 10.3 |
| `adaa.order2_kernel` | bilbao | bilbao / triangular | 10.3 |
| `adaa.modulators` | true | bool | 10.6 |
| `adaa.interp` | linear | linear / cubic | 10.6 |
| `adaa.droop_eq` | auto | auto / on / off | 10.7 |
| `adaa.droop_eq_hz` | min(20000, 0.45 $`f_{\mathrm{s}}`$) | > 0 | 10.7 |
| `engine.oversampling` | 2 | 1, 2, 4, … | 11 |
| `engine.kernels` | auto | auto / portable | 0.2 |
| `resample.filter` | iir | iir / fir | 11.2 |
| `resample.stopband_db` | 150 | > 0 | 11.2 |
| `voice.count` | 16 | ≥ 1 | 13.1 |
| `voice.steal` | dx7 | dx7 / oldest / quietest / released-first | 13.2 |
| `voice.retrigger` | new-voice | same-voice / new-voice | 13.2 |
| `engine.idle_s` | 0.5 | ≥ 0 | 13.4 |
| `part.count` | 2 | ≥ 1 | 13.5 |
| `part.function_source` | patch | patch / part | 13.5 |
| `output.gain` | 0.5 | ≥ 0 | 14 |
| `output.carrier_normalization` | none | none / count / sqrt-count | 14 |
| `output.channels` | stereo | stereo / mono | 14 |
| `syx.accept_bad_checksum` | true | bool | DESIGN §11.2 |
| `syx.out_of_range` | clamp | clamp / reject | DESIGN §11.2 |
| `syx.export_rounding` | nearest | nearest / floor | DESIGN §7.4 |
| `syx.performance_sensitivity` | linear | linear / firmware | 7.4 |
| `json.max_depth` | 64 | ≥ 0 | DESIGN §11.1 |
| `tuning.transpose_octave` | keys | keys / period | 2.1 |
| `scaling.key` | key | key / onset-pitch / pitch | 2.6 |
| `tuning.modulation` | pitch | pitch / keys | 2.1 |
| `feedback.max_substeps` | 64 | ≥ 1 | 9.6 |
| `feedback.relaxation_onset` | 0.9 | [0, 1) | 9.6 |
| `scaling.rate_key` | frequency | frequency / level | 2.6 |
