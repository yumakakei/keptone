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
| engine sample rate | Hz | $`f_{\mathrm{e}} = M f_{\mathrm{s}}`$ (§11; $`M = 1`$ by default) |
| sample index at the engine rate | integer, `u64` | $`n`$, $`t_n = n / f_{\mathrm{e}}`$ |
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
  - **Reproducibility (the engine).** With the same build, the same kernel selection (§17 `engine.kernels`, and the processor level in effect), the same settings and the same input, the output is bit-identical. That holds whatever the block splits (§0.3) and the thread timing. Across processors or builds the output may differ in its last bits. `engine.kernels = portable` gives the same output on every processor with FMA, because the portable form is plain Rust and Rust never contracts or reassociates floating-point operations on its own.
- Bit identity across forms would require the *same sequence of rounded operations*: the same association, the same fusion decisions (`a*b + c` rounds twice, `fma(a,b,c)` once), the same polynomials, and the same reduction order across SIMD lanes. Moving one operation breaks it. FMA in hardware is necessary for that, not sufficient. That is why it is not required.
- Where the feedback operator is chaotic (β > 1, §9.3), any difference in the last bit grows exponentially. Two correct forms then produce different waveforms with the same statistics. The tests compare spectra and statistics there, not samples (§16).
- The portable form uses `f64::mul_add` where an FMA improves accuracy. The supported x86-64 baseline (v3) and every AArch64 have FMA in hardware (DESIGN §10.1), so this is fast everywhere.
- Within one form, sums over voices, operators and harmonics have a fixed, documented order, so reproducibility holds.
- Code never changes the floating-point environment (rounding mode, FTZ/DAZ, exception masks). In Rust that is undefined behaviour unless it is done inside a single `asm!` block that restores the state before it ends [RUST-SETCSR]. Subnormals are kept out structurally instead (§13.4).
- Constants are the binary64 nearest to their value, written with their derivation in a comment, for example `/* 49096 / 4096 */`.

### 0.3 Two independences the implementation guarantees

1. **Sample-rate independence.** Every law below is written in seconds and hertz. Rendering one input at 44.1, 48, 96 or 192 kHz gives signals that agree in the audible band to within the tolerances of §16. The only exception is a setting documented as rate-dependent (for example `feedback.model = dx7-discrete`, §9.6).
2. **Block independence.** The output does not depend on how the host splits time into blocks. Events act at their sample position. No quantity is updated once per block. Every per-segment law (§4–§6) is evaluated in closed form from the segment's start sample, an integer. Rendering one input with random block sizes is bit-identical to rendering it in one block (§16).

### 0.4 Provenance and the duty to verify

Each constant or law carries one tag:

- **[DX7-MANUAL]**, **[DEXED-SYSEX]**: published specifications.
- **[LEVIEN-…]**, **[MRG-EG]**: measurements of real hardware.
- **[MSFA]**: values in Raph Levien's engine, partly measured, partly fitted.
- **verify**: provisional. Before a milestone that uses the value is closed, check it against an independent source ([AJXS-ROM] or measurement). Record the outcome in `docs/VERIFICATION.md`: the source consulted, what it says, whether the value changed. Where two sources disagree, the code keeps the value of the stronger source, the other becomes a setting, and the disagreement stays documented.

Read [AJXS-ROM] as documentation and write independent code. Never transcribe it.

## 1. Overview of a voice

A voice holds six operators $`k = 1 \dots 6`$, the DX7's OP1…OP6 (the number of operators is a setting for user graphs, §8.4). Operator $`k`$ outputs

```math
y_k(t) = g_k(t)\,\sin(2\pi[\theta_k(t) + \mu_k(t)]),\qquad \theta_k(t) = \theta_k(t_0) + \int_{t_0}^{t} f_k(\tau)\,\mathrm{d}\tau,\qquad \mu_k(t) = \sum_{j \to k} y_j(t)
```

- $`g_k`$ is the envelope gain (§3, §4).
- $`f_k`$ is the instantaneous frequency (§2).
- $`\mu_k`$ is the phase modulation, in turns, from the operators that modulate $`k`$ in the algorithm (§8). Modulator amplitude $`1`$ deviates the phase by $`1`$ turn. Full-scale gain is $`g = 2`$, so the full-scale index is 2 turns = $`4\pi`$ rad [MSFA `exp2.cc`, `fm_op_kernel.cc`; verify].
- Feedback, where the algorithm has it, is §9.
- The voice output is the sum of its carriers (§14).

## 2. Pitch and frequency (`keptone-engine::pitch`)

### 2.1 Key to frequency

The note number after transposition is $`k' = k + (\text{TRANSPOSE} - 24)`$ (TRANSPOSE 0–48; 24 means none) [DEXED-SYSEX, MSFA]. Its pitch is

```math
p_{\text{key}}(k') = \text{tuning}(k')
```

By default `tuning` is 12-TET with A4 = `tuning.a4_hz` = 440 Hz: $`p = \log_2\,440 + (k' - 69)/12`$. Alternatives: a Scala `.scl` + `.kbm` pair [SCALA-SCL, SCALA-KBM], or MTS-ESP [MTS-ESP] (optional, off by default). A tuning is a function from real note values to pitch, so it applies to fractional notes from bends and glides too (§2.5).

### 2.2 Operator frequency, ratio mode (OSC MODE = 0)

```math
p_k = p_{\text{key}} + c(\text{COARSE}) + \log_2(1 + \text{FINE}/100) + \delta(\text{DETUNE}, p_{\text{key}}) + p_{\text{mod}}(t)
```

- $`c(0) = -1`$ (ratio 0.5). $`c(n) = \log_2\,n`$ for $`n = 1 \dots 31`$ [MSFA `coarsemul`, DX7-MANUAL].
- The detune law [MSFA `osc_freq`; verify]: $`\delta = 0.0209\,\mathrm{e}^{-0.396p_{\text{key}}}p_{\text{key}}\cdot(\text{DETUNE} - 7)/7`$ octaves, with DETUNE 0–14 and 7 the centre. The DX7's detune is a nearly constant offset in Hz at low pitches, not a constant number of cents; the law models that.
- $`p_{\text{mod}}(t)`$ is §2.4.

### 2.3 Operator frequency, fixed mode (OSC MODE = 1)

```math
p_k = \log_2\,10\cdot((\text{COARSE} \bmod 4) + \text{FINE}/100) + \delta_{\text{fixed}} + p_{\text{bend}}(t) + p_{\text{tune}}
```

So 1, 10, 100 and 1000 Hz, each times $`10^{\text{FINE}/100}`$ [MSFA, DX7-MANUAL].

- $`\delta_{\text{fixed}}`$ [MSFA; verify]: $`13457 \cdot 2^{-24}(\text{DETUNE} - 7)`$ octaves for DETUNE > 7, else 0. The asymmetry is suspect. Setting `osc.fixed_detune = msfa | symmetric | none`.
- Fixed operators follow pitch bend and master tune, but not the pitch EG or the LFO [MSFA; verify against AJXS-ROM].

### 2.4 Pitch modulation (ratio-mode operators)

```math
p_{\text{mod}}(t) = P_{\text{EG}}(t) + p_{\text{LFO}}(t) + p_{\text{bend}}(t) + p_{\text{tune}} + p_{\text{porta}}(t)
```

- $`P_{\text{EG}}`$: the pitch EG (§5).
- $`p_{\text{LFO}}(t) = \sigma_{\text{PMS}} \cdot \mathrm{max}(d_{\text{PMD}}w(t), d_{\text{ctrl}}(t))\cdot \ell(t)`$:
  - $`\ell \in [-1, 1]`$ is the LFO waveform and $`w \in [0, 1]`$ its delay fade (§6).
  - $`d_{\text{PMD}} = \lfloor 165\,\text{PMD}/64\rfloor/256`$.
  - $`\sigma_{\text{PMS}} = \text{pmstab}[\text{PMS}]/256`$ octaves with pmstab = (0, 10, 20, 33, 55, 92, 153, 255) [MSFA `pitchmodsenstab`, `Dx7Note::compute`; verify].
  - $`d_{\text{ctrl}}`$ is the controller pitch depth (§7).
- $`p_{\text{bend}}`$: the bend, as a fraction of the range (function parameter 65, 0–12 semitones; per-direction ranges are settings). STEP (66) quantises it when non-zero. A continuous bend is the default.
- $`p_{\text{porta}}`$: portamento (§7.3).

### 2.5 Phase

Each operator keeps $`\theta_k \in [0, 1)`$ in binary64. Per engine sample: $`\theta \leftarrow \theta + \Delta`$, then $`\theta \leftarrow \theta - \lfloor\theta\rfloor`$. The increment is

```math
\Delta = \int_{t_{n-1}}^{t_n} f_k\,\mathrm{d}\tau
```

computed exactly for the piecewise-defined $`p_{\text{mod}}`$. When the pitch is linear in time over the sample (the EG laws of §5), the integral of $`2^{p}`$ is closed-form. Otherwise a 3-point Gauss–Legendre rule is used. Its error is far below binary64 resolution for audio-rate pitch changes; this is checked in §16. The phase is also evaluated at **fractional** times $`t_n - s`$ for delay alignment (§10.5), from the same closed forms.

The drift over a note is bounded by the rounding of the additions: under $`\sqrt{N}\cdot 2^{-53}`$ turns after $`N`$ samples, about $`10^{-11}`$ turns after an hour at 192 kHz. A compensated (two-sum) accumulator is a setting `osc.phase_accumulator = plain | compensated`, default `plain`.

**Initial phase** (OSC KEY SYNC, parameter 136). On: $`\theta_k = 0`$ at key-on. Off: the phase continues from the voice slot's previous note (DX7). `osc.free_phase_start = continue | zero | random`; `random` draws from §6.3's generator.

## 3. Levels (`keptone-engine::level`)

### 3.1 The 0–99 to level-curve map

```math
s(L) = \begin{cases} [0, 5, 9, 13, 17, 20, 23, 25, 27, 29, 31, 33, 35, 37, 39, 41, 42, 43, 45, 46]_L & L \lt 20 \\ 28 + L & L \ge 20 \end{cases}
```

[MSFA `scaleoutlevel`, LEVIEN-EG]. For fractional $`L`$ (allowed, §17) $`s`$ is interpolated linearly between integers. `level.curve_interpolation = linear | step`.

### 3.2 Operator offset from output level, key scaling and velocity

```math
O = 32\,\mathrm{min}(127, s(\text{OL}) + \text{KLS}(k)) + V(v)
```

$`O`$ is clamped at 0 [MSFA `Dx7Note::init`]. Then:

- **Keyboard level scaling** [MSFA `ScaleLevel`, `ScaleCurve`; DX7-MANUAL; verify].
  - Let $`o = k - \text{BP} - 17`$, where BP 0–99 is the break point (BP 39 is C3).
  - If $`o \ge 0`$: $`\text{KLS} = C_{\mathrm{R}}(\lfloor (o+1)/3\rfloor, \text{RD})`$. Otherwise: $`\text{KLS} = C_{\mathrm{L}}(\lfloor (1-o)/3\rfloor, \text{LD})`$.
  - For curve $`c \in \{-\text{LIN}, -\text{EXP}, +\text{EXP}, +\text{LIN}\}`$, group $`u`$, depth $`D`$:
    - linear: $`\lfloor 329uD / 2^{12}\rfloor`$
    - exponential: $`\lfloor 329\,e_uD / 2^{15}\rfloor`$ with $`e = (0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 11, 14, 16, 19, 23, 27, 33, 39, 47, 56, 66, 80, 94, 110, 126, 142, 158, 174, 190, 206, 222, 238, 250)`$, $`u`$ clamped to the table.
    - The sign is negative for −LIN and −EXP.
  - Setting `scaling.exact = false` keeps the floors. `true` drops them and interpolates $`e`$ for fractional groups. Notes still map to the same groups, but the result is continuous in BP and depth.
- **Velocity** [MSFA `ScaleVelocity`; verify]: $`V(v) = 16\lfloor (\text{KVS}\cdot(\text{vt}[\lfloor v/2\rfloor] - 239) + 7)/8 \rfloor`$, where vt is MSFA's 64-entry `velocity_data`. Sources of $`v`$:
  - MIDI velocity 0–127. A 7-bit MIDI velocity is the default, not the DX7's 32-level quantisation. `velocity.quantize_32 = false` [VDX7 FAQ].
  - CLAP's real-valued velocity, interpolated in vt.
  - MIDI 2.0's 16-bit velocity.

### 3.3 Total level of an EG level

```math
T(L_i) = 64\lfloor s(L_i)/2\rfloor + O - 4256,\qquad T \ge T_{\mathrm{min}} = 16
```

[MSFA `Env::advance`, LEVIEN-EG]. Full scale (L = OL = 99, no scaling or velocity) is $`T_{\text{full}} = 4032 + 4064 - 4256 = 3840`$. Velocity can push $`T`$ above $`T_{\text{full}}`$ (by up to 224 steps) [LEVIEN-EG]. The clip at $`T_{\mathrm{min}}`$ is the DX7's floor, about 84 dB below full scale. See §4.6 for what replaces it.

### 3.4 Gain

```math
g = 2^{T/256 - 14}
```

[MSFA `exp2.cc`: `Exp2::lookup(level − 14·2²⁴)`; LEVIEN-HW: "linear gain is 2^(value/256)"]. At $`T_{\text{full}}`$, $`g = 2`$. Amplitude modulation and the EG bias (§7.2) subtract from $`T`$ before the exponential.

## 4. Operator envelope (`keptone-engine::eg`)

State: the segment index $`i \in \{0, 1, 2, 3\}`$ (targets L1…L4), the start sample $`n_0`$, and the start level $`T_0`$. During a segment, $`T(t)`$ is the closed form below evaluated at $`t = (n - n_0)/f_{\mathrm{e}}`$. It is never an accumulated sum (§0.3).

### 4.1 Rate

```math
q_i = \mathrm{min}(63, \lfloor 41 R_i / 64\rfloor + \text{RS}(k)),\qquad \text{RS}(k) = \lfloor \text{KRS} \cdot \mathrm{clamp}(\lfloor k/3\rfloor - 7, 0, 31)/8 \rfloor
```

[LEVIEN-EG, MSFA `ScaleRate`]. With `eg.rate_quantize = false` the floors are dropped, so a fractional $`R`$ is continuous.

**Decay speed** (steps per second):

```math
\rho(q) = C\cdot 2^{\lfloor q/4\rfloor}\cdot(1 + (q - 4\lfloor q/4\rfloor)/4),\qquad C = \texttt{eg.clock\_hz}/4096
```

- $`\rho`$ is continuous in real $`q`$: it interpolates $`2^{q/4}`$ linearly between multiples of 4.
- `eg.clock_hz` is the default 49 096. At $`q = 0`$ the level then falls by one step every 4096 samples of the hardware clock, 0.2819 dB/s [LEVIEN-EG].
- **verify.** MSFA uses the same per-sample increments normalised to 44 100 Hz, so its envelopes run at 44100/49096 ≈ 0.898 of this speed. One of the two is wrong. Settle it with [AJXS-ROM] and with [MRG-EG]'s measured rate-50 decay (time constant 961 samples at 44.1 kHz). The other value stays available as `eg.clock_hz = 44100`.
- `eg.rate_law = dx7 | exponential`. `exponential` uses $`C\cdot 2^{q/4}`$.

### 4.2 Decay (target below current)

```math
T(t) = T_0 - \rho t,\quad\text{until } T = T_{\text{target}}
```

The DX7's decays are exponential in amplitude, so they are linear in $`T`$ [LEVIEN-EG, MRG-EG].

### 4.3 Attack (target above current)

**Jump.** At the start of a rising segment, if $`T \lt T_{\text{jump}} = T_{\mathrm{min}} + 1700`$, set $`T \leftarrow T_{\text{jump}}`$ [LEVIEN-EG; MSFA's `jumptarget` 1716]. `eg.attack_jump = true`.

**Law.**

```math
\frac{\mathrm{d}T}{\mathrm{d}t} = \rho\left(2 + \frac{T_{\text{full}} - T}{256}\right)\quad\Longrightarrow\quad T(t) = T^{\ast} - (T^{\ast} - T_0)\mathrm{e}^{-\rho t/256},\qquad T^{\ast} = T_{\text{full}} + 512
```

The segment reaches its target at

```math
t^{\ast} = \frac{256}{\rho}\,\ln\,\frac{T^{\ast} - T_0}{T^{\ast} - T_{\text{target}}}
```

- The hardware uses the integer multiplier $`2 + \lfloor (T_{\text{full}} - T)/256\rfloor`$ [LEVIEN-EG]. `eg.attack_law = continuous | stepped`, default `continuous`. `stepped` is piecewise linear in 256-step bands and also has a closed form, band by band.
- Targets stay below $`T^{\ast}`$, because the largest reachable $`T`$ is $`4032 + 32\cdot127 + 224 - 4256 = 4064 \lt 4352`$.

### 4.4 Segments that do not move

When a segment's target equals the current level, or segment 0 has L1 = 0, the DX7 waits a rate-dependent time before advancing. Patches use this as a delay. MSFA has a partial measured table (`statics`, rates < 77, in samples at 44.1 kHz; 20·(99 − R) above that; 1/20 of the time for the first segment) [MSFA `ACCURATE_ENVELOPE`; verify, complete from AJXS-ROM]. `eg.static_segments = dx7 | none`, default `dx7`. Durations are converted to seconds.

### 4.5 Key events

- **Key-on:** go to segment 0 from the current $`T`$ (for a retriggered voice) or from $`T(L_4)`$ (for a new voice).
- **Key-off:** go to segment 3 (target L4) from the current $`T`$, using the rise or fall law by direction.
- After segment 3 the level holds at L4. A non-zero L4 sustains after release (DX7 behaviour, kept).

### 4.6 The floor

The DX7 holds a decayed operator at $`T_{\mathrm{min}}`$, about −84 dB, until the voice is stolen. Cutting a voice from there is a small step.

`eg.zero_level = silence` (default): an EG level of 0 targets $`-\infty`$, so the decay continues past $`T_{\mathrm{min}}`$ at the same speed. The operator is inactive once $`T \lt T_{\text{end}}`$ (`eg.end_db`, default −200 dB relative to $`g = 1`$).

`eg.zero_level = dx7` reproduces the clip at $`T_{\mathrm{min}}`$.

### 4.7 Live parameter changes

A change to a rate or level during a segment re-anchors the closed form at the current sample, from the current $`T`$, toward the new target at the new speed. A change to output level, scaling or velocity sensitivity moves $`O`$, and so every target. `param.level_glide_s`, default 0.002, ramps $`O`$ linearly in $`T`$ over that time, so host automation does not click. Set it to 0 to switch instantly.

## 5. Pitch envelope (`keptone-engine::peg`)

```math
P_{\text{EG}} \in [-4, +4]\ \text{octaves},\qquad \text{level}(L) = \text{pt}[L]/32
```

- pt is MSFA's `pitchenv_tab` (−128…127; pt[50] = 0). The speed is $`\text{pr}[R]/21.3`$ octaves per second, with pr MSFA's `pitchenv_rate` [MSFA `pitchenv.cc`; verify, including the 21.3].
- Linear in pitch, in both directions.
- Starts at L4 at key-on, moves to L1, L2, L3, and goes to L4 at key-off.
- Closed form per segment, like §4.
- Fractional L and R interpolate the tables.

## 6. LFO (`keptone-engine::lfo`)

### 6.1 Frequency

$`f_{\text{LFO}} = \text{lf}[\text{SPEED}]`$ Hz, with lf MSFA's measured `lfoSource` table (0.0625 Hz … 49.26 Hz) [MSFA `lfo.cc`; verify]. A fractional speed interpolates in $`\log\,f`$. The LFO phase is in turns, like §2.5.

### 6.2 Waveforms, $`\ell \in [-1, 1]`$

| Wave | Definition |
| --- | --- |
| triangle | ±1 peaks at phase 0 and ½; the phase of the peaks follows MSFA, verify |
| saw down / saw up | linear ramps |
| square | ±1 |
| sine | $`\sin\,2\pi\phi`$ |
| sample & hold | a new value at each wrap of the phase, held for one cycle |

Waveform edges are not band-limited: the LFO is a control signal, evaluated per engine sample.

### 6.3 Random source

SplitMix64, the generator KIRAL's tests use, seeded by `lfo.seed`. S&H values are uniform in [−1, 1]. `lfo.sh_source = splitmix | dx7-lcg`. The `dx7-lcg` option reproduces MSFA's `(r·179 + 17) mod 256` sequence [MSFA; verify].

### 6.4 Delay fade $`w(t)`$

Let $`a = 99 - \text{DELAY}`$. If $`a = 99`$, then $`w \equiv 1`$. Otherwise [MSFA `Lfo::reset`, `Lfo::getdelay`; verify the unit constant 25 190 424 s⁻¹]:

- $`u_1 = (16 + (a \bmod 16))\cdot 2^{1 + \lfloor a/16\rfloor}`$.
- $`u_2 = \mathrm{max}(128, 128\lfloor u_1/128\rfloor)`$.
- Stage 1 lasts $`t_1 = 2^{31}/(25190424u_1)`$ s, with $`w = 0`$.
- Stage 2 lasts $`t_2 = 2^{31}/(25190424u_2)`$ s, with $`w`$ rising linearly from 0 to 1.

### 6.5 Sync and scope

- LFO SYNC = 1 resets the phase at key-on.
- The DX7 has one LFO shared by all voices. `lfo.scope = global | voice`, default `global`. With `global`, the key-on sync resets the shared LFO.

## 7. Controllers and amplitude modulation (`keptone-engine::control`)

### 7.1 Controller routing

Function parameters 70–77 set range and assign for four controllers: mod wheel, foot, breath, aftertouch. The assign bits route to pitch, amplitude and EG bias [DEXED-SYSEX, DX7-MANUAL]. MPE and CLAP note expressions (pressure, timbre, tuning) map to the same destinations per voice (DESIGN §7.3). Each controller contributes a depth in [0, 1]. Per destination, the DX7 takes the larger of the LFO depth and the controller depth (§2.4) [MSFA; verify].

### 7.2 Amplitude modulation and EG bias

Amplitude modulation lowers $`T`$ by

```math
\Delta T_{\text{AM}}(t) = A_{\mathrm{max}}\cdot \alpha_{\text{AMS}}\cdot \mathrm{max}(d_{\text{AMD}}w(t), d_{\text{ctrl}})\cdot \frac{1 - \ell(t)}{2}
```

It is unipolar: it only attenuates.

- $`\alpha_{\text{AMS}} = (0, 66, 109, 255)/255`$ [MSFA `ampmodsenstab`].
- $`d_{\text{AMD}} = \lfloor 165\,\text{AMD}/64\rfloor/256`$.
- EG bias lowers $`T`$ by a controller amount on operators with AMS > 0.
- **verify:** MSFA applies AM through an ad-hoc exponential fit (its own comment says so). Derive $`A_{\mathrm{max}}`$, the depth law and the EG-bias law from [AJXS-ROM]. Until then $`A_{\mathrm{max}}`$ = `am.max_steps`, provisional default 2048 (48 dB), with a test that fails once a verified value replaces it.

### 7.3 Portamento and voice modes

- Function parameters: mono/poly (64), portamento mode (67: retain/follow; fingered in mono), glissando (68), time (69) [DEXED-SYSEX].
- Glide speed is $`v(\text{TIME})`$ semitones per second, linear in pitch. MSFA fits $`2100\cdot 2^{-0.062c}`$ over a 0–127 control $`c`$ (glissando $`1300\cdot2^{-0.062c}`$). Map TIME 0–99 onto it with $`c = 127\,\text{TIME}/99`$ [MSFA `porta.cpp`; verify against AJXS-ROM].
- Glissando quantises the glide to the tuning's notes.

## 8. Operators and algorithms (`keptone-engine::graph`)

### 8.1 Algorithm data

The 32 algorithms are data: edges $`j \to k`$, the carrier set, and the feedback edges. Derive them from [DX7-MANUAL]'s chart and check them against two independent encodings, [SHIRRIFF-ALG] and [MSFA]'s `FmCore::algorithms`. A test asserts that all three agree. Algorithms 4 and 6 have feedback loops that span several operators [MSFA flags; DX7-MANUAL]. The rest have at most a self-loop.

### 8.2 Evaluation order

1. Decompose the graph into strongly connected components (SCCs). An SCC is one operator without feedback, one operator with a self-loop, or a multi-operator loop.
2. Topologically order the SCCs.
3. In each sample, evaluate the SCCs in that order, because phase modulation uses the *same-instant* output of the modulator.
4. Solve each SCC that contains feedback by §9. Every other operator uses §10 (or a point sample when `adaa.order = 0`).

### 8.3 Per-voice algorithm

The OPS can run a different algorithm in each voice; the DX7 does not use this [SHIRRIFF-REG]. `graph.algorithm_scope = patch | voice`. With `voice`, a voice keeps the algorithm it started with when the patch changes mid-note.

### 8.4 User graphs

Any directed graph over $`N`$ operators (`graph.operators`, default 6) is accepted. The rules above apply unchanged. A graph comes with its own feedback gains and carrier set (DESIGN §11.2).

## 9. Feedback (`keptone-engine::feedback`, `keptone-math::kepler`)

### 9.1 The DX7's recurrence

```math
y[n] = g\,\sin\left(2\pi\left(\theta[n] + \kappa\,\frac{y[n-1] + y[n-2]}{2}\right)\right),\qquad \kappa = 2^{\text{FB} - 8}\ \text{turns per unit},\ \text{FB} = 1\dots 7
```

FB = 0 means no feedback. The mean of the last two outputs is Tomisawa's anti-hunting filter [LEVIEN-HW, AJXS-ANALYSIS, MSFA `compute_fb`; verify κ]. In radians the loop gain is

```math
\beta = 2\pi\kappa g
```

At full scale, $`g = 2`$:

| FB | 1 | 2 | 3 | 4 | 5 | 6 | 7 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| β | 0.098 | 0.196 | 0.393 | 0.785 | 1.571 | 3.142 | 6.283 |

β is proportional to the operator's *current* gain, so it moves with the envelope during a note.

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

### 9.6 β ≥ 1: `feedback.model`

| Model | Definition | Rate-independent |
| --- | --- | --- |
| `relaxation` (default) | First-order lag in continuous time: $`\tau\dot Y = -Y + g\,\sin(2\pi(\theta + \kappa Y))`$, $`y = Y`$. `feedback.tau_s` defaults to 1.5/49096 s, the hardware loop's effective delay (the mean of samples n−1 and n−2). Integrate per engine sample with the trapezoidal rule, solving its implicit equation by safeguarded Newton. For β < 1 and τ → 0 it tends to §9.2. | in the limit $`f_{\mathrm{e}} \gg 1/\tau`$; the tests measure convergence |
| `dx7-discrete` | §9.1 at the engine rate | **no**: the timbre changes with $`f_{\mathrm{e}}`$ (documented) |
| `clamp` | §9.4 with β replaced by min(β, `feedback.beta_clamp`), default 0.999 | yes |

The default is provisional. Decide it by listening tests with patches that use FB 6–7 (DESIGN §15).

### 9.7 Multi-operator loops (algorithms 4 and 6)

The loop's ideal is the delay-free system $`y = G(y)`$, with $`G`$ the composed operators. Solve it by vector Newton (2–3 unknowns, analytic Jacobian), safeguarded by damping. It is well posed when the loop gain $`\prod_j 2\pi\kappa_j g_j \left|\cos(\cdot)\right| \lt 1`$. Otherwise apply `feedback.model` to the loop as a whole. The series form is not used for loops.

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

All are portable Rust (the reference) with optional vector forms (DESIGN §10). Their tests compare against [CORE-MATH]'s correctly rounded functions, and sinc and sinc′, which CORE-MATH does not have, against references computed with mpmath (§16).

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

## 13. Voices (`keptone-engine::voice`)

### 13.1 Polyphony

`voice.count`, default 16, from 1 up to what memory allows. Voices are allocated at activation and never on the audio thread.

### 13.2 Allocation and stealing

`voice.steal = dx7 | oldest | quietest | released-first`.

- Default `dx7`: [AJXS-ROM]'s algorithm, documented in `docs/VERIFICATION.md` before it is implemented.
- Re-pressing a sounding key retriggers that voice: `voice.retrigger = same-voice | new-voice`.

### 13.3 End of a voice

A voice ends when every carrier is in segment 3 with $`T \lt T_{\text{end}}`$ and no key or sustain holds it. Its end is reported to the host (CLAP note-end).

### 13.4 Subnormals and stopping

No recursive state is allowed to decay into subnormals:

- Gains are bounded below by $`T_{\text{end}}`$, so $`g \ge 2^{-33}`$ at the default.
- The relaxation state of §9.6 and every decimator state are set to exact zero when their voice or the whole engine has been silent for `engine.idle_s` (default 0.5 s).
- In addition, at each block boundary every IIR state whose magnitude is below $`2^{-500}`$ is set to zero. The rule is a deterministic function of the state, so block independence (§0.3) still holds: the check runs at fixed sample multiples of 64 at the engine rate, not at host block boundaries.

## 14. Output (`keptone-engine::mix`)

```math
\text{out}[n] = \texttt{output.gain}\cdot\sum_{\text{voices}}\sum_{c\in\text{carriers}} y_c[n]
```

- `output.gain` defaults to 0.5, which puts one full-scale carrier at 0 dBFS.
- `output.carrier_normalization = none | count | sqrt-count`, default `none` (DX7).
- The output is mono, duplicated to stereo. Per-voice and per-operator pan (DX7II) is a later setting.
- When the host offers 64-bit ports, the output stays binary64. Otherwise it is rounded to nearest binary32 with no dither: the signal's own floor is far below binary32's resolution.
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
- The block-rate control and the 1024-point sine table of MSFA-based emulators.

**Kept** (they define patches): every law and table in §2–§9, the attack jump, the static segment delays, sustain after release with L4 > 0, the shared LFO, and the DX7's keyboard scaling, velocity and rate scaling curves. Each has a setting that lets the user step away from it.

## 16. Tests and tolerances

An error in ulp is measured from the correctly rounded result, in units in the last place of that result, as against CORE-MATH; an absolute error, from the exact value.

| What | Check | Tolerance |
| --- | --- | --- |
| sin2pi, cos2pi, exp2 | against CORE-MATH, ≥ 10⁸ random inputs and special points | ≤ 1 ulp |
| sinc, sinc' | against MPFR-precision references (`scripts/` with mpmath) | ≤ 2 ulp; sinc' for $`\left\|u\right\| \ge 1/2`$: ≤ $`9 \cdot 2^{-53}`$ absolute |
| ADAA-1, ADAA-2 | against mpmath references of the closed forms and of the divided difference, the inputs exact (`scripts/adaa_references.py`): special points (still, steady and reversing phases, $`h`$ on either side of $`\tau_2`$) and random inputs; ADAA-2 at $`\tau_2 = 2^{-12}, 2^{-10}, 2^{-8}`$; three equal points give $`\sin\,2\pi x`$ | ≤ $`9 \cdot 2^{-53}`$ absolute; ≤ $`\mathrm{max}(4.81u/\tau_2 + 4.35u,\ 0.271\tau_2^4 + 14u)`$ absolute |
| Kepler solver | residual over a β × Φ grid (β ∈ [0, 0.999], 10⁶ points), and iteration counts; the same nearer the cusp, β up to 1 − 2⁻⁵² and Φ down to 10⁻³⁰⁰, the residual in double-double | $`\left\| h(\Theta) - \Phi\right\| \le 4u\,\mathrm{max}(1,\left\|\Phi\right\|)`$; ≤ 8 iterations |
| Kapteyn coefficients | against mpmath for n ≤ 2000, β on a grid (`scripts/kapteyn_references.py`): the DFT up to β = 0.95, the table of the defaults inside its domain; Kapteyn's inequality and the tail bound on the same references | ≤ 1e-15 absolute |
| Series vs point solution | where both apply and the tail is negligible | ≤ 1e-14 |
| EG | closed form vs high-precision integration of the ODE; segment times vs §4.3's $`t^{\ast}`$ | ≤ 1e-12 relative |
| Sample-rate independence | the same input at 44.1, 48, 96 and 192 kHz; envelopes compared at common times; spectra compared below 20 kHz | EG/LFO/pitch ≤ 1e-12; audio band: see the alias metric |
| Block independence | random block splits vs one block | **bit-identical** |
| Kernel contracts | every form (portable and vector) at every ISA level (with Intel SDE where the runner lacks it) vs the high-precision reference | each kernel's stated bound (§0.2), e.g. ≤ 1 ulp for sin2pi |
| Reproducibility | same build, same kernels, same settings and input, rendered twice in different processes with different thread timing | **bit-identical** |
| Forms agree on the engine | a full render with `engine.kernels = auto` vs `portable`, non-chaotic patches | ≤ the error budget the contracts imply (derived per patch class; start at −200 dBFS) |
| Chaotic regimes | as above, patches with β > 1 | long-term spectrum (Welch) within 0.1 dB below 20 kHz; level statistics within 0.1 dB |
| Alias metric | each patch of `conformance/patches/` rendered at $`f_{\mathrm{s}}`$ and by the reference renderer (`engine.oversampling = 64`, FIR decimation, `adaa.order = 0`); the residual's energy below min(20 kHz, 0.45 $`f_{\mathrm{s}}`$), in dB relative to the signal | regression baselines in `bench/baselines.toml`; may only improve |
| .syx round trip | every valid dump → model → dump | byte-identical |
| Parsers | random mutations of the seeds (SplitMix64) plus `fuzz/regressions/` on every test run; libFuzzer in CI | never a panic; every input is accepted or rejected |
| Finiteness | random patches × random event streams × random settings | output always finite; the audio thread never allocates (RTSan) |

## 17. Settings index

Every setting is stored in the plugin state and has a host parameter where continuous automation makes sense. Defaults are chosen for the main use (playing DX7 patches cleanly). Anything else is the user's choice.

| Setting | Default | Values | § |
| --- | --- | --- | --- |
| `tuning.a4_hz` | 440 | > 0 | 2.1 |
| `tuning.source` | 12-tet | 12-tet / scala / mts-esp | 2.1 |
| `osc.fixed_detune` | msfa | msfa / symmetric / none | 2.3 |
| `osc.phase_accumulator` | plain | plain / compensated | 2.5 |
| `osc.free_phase_start` | continue | continue / zero / random | 2.5 |
| `level.curve_interpolation` | linear | linear / step | 3.1 |
| `scaling.exact` | false | bool | 3.2 |
| `velocity.quantize_32` | false | bool | 3.2 |
| `eg.clock_hz` | 49096 | > 0 | 4.1 |
| `eg.rate_law` | dx7 | dx7 / exponential | 4.1 |
| `eg.rate_quantize` | true | bool | 4.1 |
| `eg.attack_jump` | true | bool | 4.3 |
| `eg.attack_law` | continuous | continuous / stepped | 4.3 |
| `eg.static_segments` | dx7 | dx7 / none | 4.4 |
| `eg.zero_level` | silence | silence / dx7 | 4.6 |
| `eg.end_db` | −200 | < 0 | 4.6 |
| `param.level_glide_s` | 0.002 | ≥ 0 | 4.7 |
| `lfo.scope` | global | global / voice | 6.5 |
| `lfo.seed` | 0 | u64 | 6.3 |
| `lfo.sh_source` | splitmix | splitmix / dx7-lcg | 6.3 |
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
| `voice.retrigger` | same-voice | same-voice / new-voice | 13.2 |
| `engine.idle_s` | 0.5 | ≥ 0 | 13.4 |
| `output.gain` | 0.5 | ≥ 0 | 14 |
| `output.carrier_normalization` | none | none / count / sqrt-count | 14 |
