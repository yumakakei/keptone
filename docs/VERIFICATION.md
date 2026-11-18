<!--
SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
SPDX-License-Identifier: GPL-3.0-or-later

This file is part of Keptone. The SPDX lines name its copyright
holder and its license, in the machine-readable form of the REUSE
specification: the GNU General Public License, version 3 or any later version
(LICENSES/GPL-3.0-or-later.txt).
-->

# Verification log

Every value that `docs/MATH.md` tags **verify** has a row here. A row records the provisional value, its source, the independent source consulted, what that source says, and the decision. A row is open while it waits for its sources; decided once the decision is made, until the code and the tests follow it; provisional where no independent source can settle the value, which then stays, pinned by a test that fails once a verified value is entered; and closed once the code and the tests follow the decision. The milestone that first uses a value cannot close while its row is open (DESIGN §16).

| # | MATH § | Item | Provisional (source) | Check against | Status | Decision |
| --- | --- | --- | --- | --- | --- | --- |
| V1 | 1, 3.4 | Full-scale gain 2 → modulation index 2 turns (4π rad) | MSFA `exp2.cc`, `fm_op_kernel.cc` | SHIRRIFF-EXP; AJXS-ROM operator output scaling | open | |
| V2 | 2.2 | Detune law $`0.0209\,\mathrm{e}^{-0.396p}p\cdot(d - 7)/7`$ | MSFA `osc_freq` (measured on one DX7) | AJXS-ROM pitch computation (`VOICE_CONVERT_NOTE_TO_PITCH` and detune) | open | |
| V3 | 2.3 | Fixed-mode detune (only for d > 7) and fixed operators ignore PEG/LFO | MSFA | AJXS-ROM | open | |
| V4 | 2.4 | Pitch-modulation sensitivity table and depth scaling (/256 octaves) | MSFA `pitchmodsenstab`, `Dx7Note::compute` | AJXS-ROM LFO pitch modulation; DX7-MANUAL ranges | open | |
| V5 | 3.2 | Keyboard level scaling constants (329, exp table, offset 17) | MSFA `ScaleLevel`/`ScaleCurve` | AJXS-ROM keyboard scaling; DX7-MANUAL | open | |
| V6 | 3.2 | Velocity table and scaling | MSFA `velocity_data`, `ScaleVelocity` | AJXS-ROM velocity handling | open | |
| V7 | 4.1 | EG clock: 49 096 (LEVIEN-EG) vs 44 100-normalised (MSFA) | LEVIEN-EG | AJXS-ROM EG update rate; MRG-EG rate-50 decay (τ = 961 samples at 44.1 kHz) | open | |
| V8 | 4.4 | Static-segment delays (full table) | MSFA `statics` (partial, rates < 77) | AJXS-ROM; measurement | open | |
| V9 | 5 | Pitch-EG rate table, level table, constant 21.3 | MSFA `pitchenv.cc` | AJXS-ROM pitch EG | open | |
| V10 | 6.1 | LFO frequency table | MSFA `lfoSource` (measured) | AJXS-ROM LFO speed table | open | |
| V11 | 6.2 | LFO waveform phase conventions (triangle start, saw directions) | MSFA `Lfo::getsample` | AJXS-ROM | open | |
| V12 | 6.4 | LFO delay law and unit constant 25 190 424 s⁻¹ | MSFA `Lfo::reset`/`getdelay` | AJXS-ROM | open | |
| V13 | 7.1 | Controller depths, assign routing, max(LFO, controller) rule | MSFA | AJXS-ROM; DX7-MANUAL function parameters | open | |
| V14 | 7.2 | Amplitude-modulation depth law, A_max, EG bias | MSFA (ad hoc fit) | AJXS-ROM | open | |
| V15 | 7.3 | Portamento law, modes, glissando | MSFA `porta.cpp` (fit) | AJXS-ROM; DX7-MANUAL | open | |
| V16 | 8.1 | 32 algorithm graphs incl. multi-operator loops in 4 and 6 | DX7-MANUAL chart | SHIRRIFF-ALG; MSFA `FmCore::algorithms` | open | |
| V17 | 9.1 | Feedback scaling κ = 2^(FB−8) turns per unit, the two-sample mean | MSFA `compute_fb`; LEVIEN-HW | AJXS-ANALYSIS; Shirriff OPS feedback path | open | |
| V18 | 13.2 | Voice allocation and stealing (`voice.steal = dx7`) | — | AJXS-ROM voice allocation | decided | `VOICE_ADD_POLY` gives, from a pointer it moves past each voice given, the first of the 16 voices whose key is up, sounding or not, those held by the sustain pedal among them, and none where every key is held [AJXS-ROM]; a key pressed again takes a voice so, and its old voice goes on. Keptone's `dx7` takes a voice that has ended first and this order after (§13.2). |
| V19 | 2.1 | TRANSPOSE centre 24 | MSFA / DEXED-SYSEX | DX7-MANUAL; AJXS-ROM `VOICE_ADD` | decided | 24 is none: the firmware adds TRANSPOSE to the key and subtracts 24 before it takes the key's pitch [AJXS-ROM], as Dexed does; MSFA applies no transpose. |
| V20 | 4.1 | The key of the rate scaling: $`k_{\mathrm{s}}`$ of §2.6, the transposed key by default, as the level scaling's | AJXS-ROM `VOICE_ADD` (the level scaling's key) | AJXS-ANALYSIS and DX7-SERVICE on the EGS; a DX7's decay times under its pitch envelope and its portamento | open | |
