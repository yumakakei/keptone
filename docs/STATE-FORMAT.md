<!--
SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
SPDX-License-Identifier: GPL-3.0-or-later
-->

# The native state

This document specifies the native state: what Keptone saves of itself, in a host's project or in a file, and reads back (DESIGN §11.1). `keptone_model::state` reads and writes it. It is Keptone's own format, binary64 throughout; the data of the DX7's family are formats of compatibility (DESIGN §11.2). The names of its members and the numbers of its parameters, which their IDs hold (§6), are fixed from Keptone's first release on: from then, none is given to another parameter or taken back, and what comes later takes names and numbers not yet given.

## 1. The text

A state is a JSON text [RFC8259] in UTF-8 without a byte order mark, of the subset that `keptone_model::json` reads: no name given twice in an object, and arrays and objects nested no deeper than `json.max_depth` (MATH §17), 64 by default; a state nests four deep. It is an object of five members:

| Member | Value |
| --- | --- |
| `format` | `"keptone-state"` |
| `version` | the version of the format, an integer above 0; this document's is 1 (§8) |
| `settings` | an object of every setting (§3) |
| `tuning` | an object of the tuning's Scala files (§4) |
| `parts` | an array of the parts, as many as `part.count` says (§5) |

Keptone writes the members of each object in the order this document gives them, each on a line of its own and indented two spaces a level; an array that holds nothing but numbers, strings, `true`, `false` and `null` on one line, a comma and a space between its values; and a line feed at the end. A reader takes the members in any order, with any white space that JSON allows.

## 2. Values

- A number is a binary64. Keptone writes it as ECMAScript writes it [ECMA-262] §6.1.6.1.20, which RFC 8785 takes for its canonical form [RFC8785] §3.2.2.3: the fewest significant digits that read back as the binary64, of those the nearest to it, and of two as near the even; in fixed notation from 10⁻⁶ to below 10²¹, as `0.000001` and `100000000000000000000`, and beyond with an exponent and its sign, as `1e-7` and `1e+21`. A reader takes any number that JSON allows, as the binary64 nearest to it, ties to even: one beyond binary64's range is infinite, which no member takes. −0 is taken as 0.
- A parameter's number is any finite binary64 where its values are given as a scale, as 0 to 99: on the scale, between its steps or beyond its ends, the scale being the range of the instrument's data and what the editor and a host show (MATH §0.5). It is an integer of its range where its values are given as integers (DESIGN §7.4).
- A setting's integer, a count, a power of two or a seed, is written in digits alone, and read exactly, up to 2⁶⁴ − 1.
- A choice is a string, one of its names, and a switch `true` or `false`.
- A string is written with a quotation mark, a backslash and each control character, U+0000 to U+001F, escaped, as `\"`, `\\`, `\b`, `\f`, `\n`, `\r`, `\t` or `\u00xx` in small letters, and every other character as it is.

## 3. Settings

`settings` holds every setting of MATH §17 by its name, in the order of MATH's table, which gives each one's default. A setting's number is its place in that table, and its ID's (§6). `adaa.droop_eq_hz` holds `"auto"` for its default, which MATH gives as a formula of the host's rate.

| Number | Name | Written as |
| --- | --- | --- |
| 0 | `tuning.a4_hz` | a number above 0 |
| 1 | `tuning.source` | `"12-tet"`, `"scala"`, `"mts-esp"` |
| 2 | `osc.fixed_detune` | `"msfa"`, `"symmetric"`, `"none"` |
| 3 | `osc.phase_accumulator` | `"plain"`, `"compensated"` |
| 4 | `osc.free_phase_start` | `"continue"`, `"zero"`, `"random"` |
| 5 | `eg.clock_hz` | a number above 0 |
| 6 | `eg.rate_law` | `"dx7"`, `"exponential"` |
| 7 | `eg.attack_jump` | `true`, `false` |
| 8 | `eg.static_segments` | `"dx7"`, `"none"` |
| 9 | `eg.end_db` | a number below 0 |
| 10 | `param.level_glide_s` | a number of at least 0 |
| 11 | `lfo.scope` | `"part"`, `"voice"` |
| 12 | `lfo.seed` | an integer from 0 to 18446744073709551615 |
| 13 | `am.max_steps` | a number of at least 0 |
| 14 | `graph.algorithm_scope` | `"patch"`, `"voice"` |
| 15 | `graph.operators` | an integer of at least 1 |
| 16 | `feedback.model` | `"relaxation"`, `"dx7-discrete"`, `"clamp"` |
| 17 | `feedback.tau_s` | a number above 0 |
| 18 | `feedback.beta_clamp` | a number above 0 and below 1 |
| 19 | `feedback.max_iterations` | an integer of at least 1 |
| 20 | `feedback.series_beta_max` | a number of at least 0 and below 1 |
| 21 | `feedback.series_epsilon` | a number above 0 |
| 22 | `alias.epsilon` | a number below 0 |
| 23 | `adaa.order` | `"0"`, `"1"`, `"2"` |
| 24 | `adaa.tau2` | a number above 0 |
| 25 | `adaa.order2_kernel` | `"bilbao"`, `"triangular"` |
| 26 | `adaa.modulators` | `true`, `false` |
| 27 | `adaa.interp` | `"linear"`, `"cubic"` |
| 28 | `adaa.droop_eq` | `"auto"`, `"on"`, `"off"` |
| 29 | `adaa.droop_eq_hz` | a number above 0, or `"auto"` |
| 30 | `engine.oversampling` | a power of two: 1, 2, 4 and on |
| 31 | `engine.kernels` | `"auto"`, `"portable"` |
| 32 | `resample.filter` | `"iir"`, `"fir"` |
| 33 | `resample.stopband_db` | a number above 0 |
| 34 | `voice.count` | an integer of at least 1 |
| 35 | `voice.steal` | `"dx7"`, `"oldest"`, `"quietest"`, `"released-first"` |
| 36 | `voice.retrigger` | `"same-voice"`, `"new-voice"` |
| 37 | `engine.idle_s` | a number of at least 0 |
| 38 | `part.count` | an integer of at least 1 |
| 39 | `part.function_source` | `"patch"`, `"part"` |
| 40 | `output.gain` | a number of at least 0 |
| 41 | `output.carrier_normalization` | `"none"`, `"count"`, `"sqrt-count"` |
| 42 | `output.channels` | `"stereo"`, `"mono"` |
| 43 | `syx.accept_bad_checksum` | `true`, `false` |
| 44 | `syx.out_of_range` | `"clamp"`, `"reject"` |
| 45 | `syx.export_rounding` | `"nearest"`, `"floor"` |
| 46 | `syx.performance_sensitivity` | `"linear"`, `"firmware"` |
| 47 | `json.max_depth` | an integer of at least 0 |
| 48 | `tuning.transpose_octave` | `"keys"`, `"period"` |
| 49 | `scaling.key` | `"key"`, `"onset-pitch"`, `"pitch"` |
| 50 | `tuning.modulation` | `"pitch"`, `"keys"` |
| 51 | `feedback.max_substeps` | an integer of at least 1 |
| 52 | `feedback.relaxation_onset` | a number of at least 0 and below 1 |

## 4. Tuning

`tuning` holds the Scala files that the tuning takes where `tuning.source` is `scala` (MATH §2.1): `scale`, the text of a `.scl` file [SCALA-SCL], and then `keyboard_mapping`, the text of a `.kbm` file [SCALA-KBM], each kept as it was given, or `null` for none. Each must be a file that `keptone_model::scala` reads, as DESIGN §11.4 says it reads them.

## 5. Parts

Each part (MATH §13.5) is an object of its members in this order: what the part has of its own (§5.5), its function data (§5.2), its patch's voice and the voice's name (§5.1), the patch's supplement (§5.3) and its fractional scaling (§5.4) where it has them, and the patch's digest (§7). A part has every member below, but the supplement's and the fractional scaling's where its patch has none. "As a part starts" gives the member's value in the state that Keptone starts in (MATH §13.5), where the first part is on and the others are off. A parameter's number is its number in its area, and its ID's (§6).

### 5.1 The voice

The voice's area is 1, and its numbers are the DX7's [DEXED-SYSEX]. An operator's 21 parameters are named `voice.opN.` and the name below, for N from 1 to 6, and numbered 21 (6 − N) and the place below, so that OP6's are 0 to 20 and OP1's 105 to 125:

| Place | Name | Values | As a part starts |
| --- | --- | --- | --- |
| 0 | `eg.rate.1` | 0 to 99 | 99 |
| 1 | `eg.rate.2` | 0 to 99 | 99 |
| 2 | `eg.rate.3` | 0 to 99 | 99 |
| 3 | `eg.rate.4` | 0 to 99 | 99 |
| 4 | `eg.level.1` | 0 to 99 | 99 |
| 5 | `eg.level.2` | 0 to 99 | 99 |
| 6 | `eg.level.3` | 0 to 99 | 99 |
| 7 | `eg.level.4` | 0 to 99 | 0 |
| 8 | `break_point` | 0 to 99 | 0 |
| 9 | `left_depth` | 0 to 99 | 0 |
| 10 | `right_depth` | 0 to 99 | 0 |
| 11 | `left_curve` | `"-lin"`, `"-exp"`, `"+exp"`, `"+lin"` | `"-lin"` |
| 12 | `right_curve` | `"-lin"`, `"-exp"`, `"+exp"`, `"+lin"` | `"-lin"` |
| 13 | `rate_scaling` | 0 to 7 | 0 |
| 14 | `amplitude_sensitivity` | 0 to 3 | 0 |
| 15 | `velocity_sensitivity` | 0 to 7 | 0 |
| 16 | `output_level` | 0 to 99 | 99 for OP1, 0 for the others |
| 17 | `mode` | `"ratio"`, `"fixed"` | `"ratio"` |
| 18 | `coarse` | 0 to 31 | 1 |
| 19 | `fine` | 0 to 99 | 0 |
| 20 | `detune` | 0 to 14 | 7 |

The voice's own follow, the algorithm by its number rather than the DX7's step, and then each operator's switch:

| Number | Name | Values | As a part starts |
| --- | --- | --- | --- |
| 126 | `voice.pitch_eg.rate.1` | 0 to 99 | 99 |
| 127 | `voice.pitch_eg.rate.2` | 0 to 99 | 99 |
| 128 | `voice.pitch_eg.rate.3` | 0 to 99 | 99 |
| 129 | `voice.pitch_eg.rate.4` | 0 to 99 | 99 |
| 130 | `voice.pitch_eg.level.1` | 0 to 99 | 50 |
| 131 | `voice.pitch_eg.level.2` | 0 to 99 | 50 |
| 132 | `voice.pitch_eg.level.3` | 0 to 99 | 50 |
| 133 | `voice.pitch_eg.level.4` | 0 to 99 | 50 |
| 134 | `voice.algorithm` | integers 1 to 32 | 1 |
| 135 | `voice.feedback` | 0 to 7 | 0 |
| 136 | `voice.oscillator_sync` | `true`, `false` | `true` |
| 137 | `voice.lfo.speed` | 0 to 99 | 35 |
| 138 | `voice.lfo.delay` | 0 to 99 | 0 |
| 139 | `voice.lfo.pitch_depth` | 0 to 99 | 0 |
| 140 | `voice.lfo.amplitude_depth` | 0 to 99 | 0 |
| 141 | `voice.lfo.sync` | `true`, `false` | `true` |
| 142 | `voice.lfo.wave` | `"triangle"`, `"saw-down"`, `"saw-up"`, `"square"`, `"sine"`, `"sample-hold"` | `"triangle"` |
| 143 | `voice.pitch_sensitivity` | 0 to 7 | 3 |
| 144 | `voice.transpose` | 0 to 48 | 24 |
| 155 | `voice.op1.on` | `true`, `false` | `true` |
| 156 | `voice.op2.on` | `true`, `false` | `true` |
| 157 | `voice.op3.on` | `true`, `false` | `true` |
| 158 | `voice.op4.on` | `true`, `false` | `true` |
| 159 | `voice.op5.on` | `true`, `false` | `true` |
| 160 | `voice.op6.on` | `true`, `false` | `true` |

`voice.name` is the voice's name, which has no number (DESIGN §7.4): a string of ten characters, each below U+0080, the ten bytes of the name as the dumps hold them, each the character of its code. As a part starts it is `"INIT VOICE"`.

### 5.2 The function data

The function data's area is 2, and its numbers follow the order of the model's fields (`keptone_model::function`):

| Number | Name | Values | As a part starts |
| --- | --- | --- | --- |
| 0 | `function.polyphony` | `"poly"`, `"mono"` | `"poly"` |
| 1 | `function.pitch_bend_range` | 0 to 12 | 2 |
| 2 | `function.pitch_bend_step` | 0 to 12 | 0 |
| 3 | `function.portamento_mode` | `"retain"`, `"follow"` | `"retain"` |
| 4 | `function.glissando` | `true`, `false` | `false` |
| 5 | `function.portamento_time` | 0 to 99 | 0 |
| 6 | `function.modulation_wheel.range` | 0 to 99 | 99 |
| 7 | `function.modulation_wheel.assign.pitch` | `true`, `false` | `true` |
| 8 | `function.modulation_wheel.assign.amplitude` | `true`, `false` | `false` |
| 9 | `function.modulation_wheel.assign.eg_bias` | `true`, `false` | `false` |
| 10 | `function.foot_controller.range` | 0 to 99 | 0 |
| 11 | `function.foot_controller.assign.pitch` | `true`, `false` | `false` |
| 12 | `function.foot_controller.assign.amplitude` | `true`, `false` | `false` |
| 13 | `function.foot_controller.assign.eg_bias` | `true`, `false` | `false` |
| 14 | `function.breath_controller.range` | 0 to 99 | 0 |
| 15 | `function.breath_controller.assign.pitch` | `true`, `false` | `false` |
| 16 | `function.breath_controller.assign.amplitude` | `true`, `false` | `false` |
| 17 | `function.breath_controller.assign.eg_bias` | `true`, `false` | `false` |
| 18 | `function.aftertouch.range` | 0 to 99 | 0 |
| 19 | `function.aftertouch.assign.pitch` | `true`, `false` | `false` |
| 20 | `function.aftertouch.assign.amplitude` | `true`, `false` | `false` |
| 21 | `function.aftertouch.assign.eg_bias` | `true`, `false` | `false` |

### 5.3 The supplement

The supplement's area is 3. Its members are there where the patch has a supplement, the DX7II's (DESIGN §11.2), and not as a part starts. An operator's two are named `supplement.opN.` and the name below, for N from 1 to 6, and numbered 2 (N − 1) and the place below:

| Place | Name | Values |
| --- | --- | --- |
| 0 | `scaling_mode` | `"normal"`, `"fractional"` |
| 1 | `amplitude_sensitivity` | 0 to 7 |

The supplement's own follow. `supplement.pitch_eg_range` is the range of the pitch envelope in octaves, the span between the pitches of its lowest level and its highest: the DX7II's ranges are 8, 4, 1 and 0.5, 0 holds the pitch still, and a range below 0 turns the envelope upside down (DESIGN §11.2):

| Number | Name | Values |
| --- | --- | --- |
| 12 | `supplement.pitch_eg_range` | 0 to 8 |
| 13 | `supplement.lfo_trigger` | `"single"`, `"multi"` |
| 14 | `supplement.pitch_eg_velocity` | `true`, `false` |
| 15 | `supplement.polyphony` | `"poly"`, `"mono"` |
| 16 | `supplement.unison` | `true`, `false` |
| 17 | `supplement.pitch_bend_range` | 0 to 12 |
| 18 | `supplement.pitch_bend_step` | 0 to 12 |
| 19 | `supplement.pitch_bend_mode` | `"normal"`, `"lowest"`, `"highest"`, `"key-on"` |
| 20 | `supplement.random_pitch` | 0 to 7 |
| 21 | `supplement.portamento_mode` | `"retain"`, `"follow"` |
| 22 | `supplement.portamento_step` | 0 to 12 |
| 23 | `supplement.portamento_time` | 0 to 99 |
| 24 | `supplement.modulation_wheel.pitch` | 0 to 99 |
| 25 | `supplement.modulation_wheel.amplitude` | 0 to 99 |
| 26 | `supplement.modulation_wheel.eg_bias` | 0 to 99 |
| 27 | `supplement.foot_controller_1.pitch` | 0 to 99 |
| 28 | `supplement.foot_controller_1.amplitude` | 0 to 99 |
| 29 | `supplement.foot_controller_1.eg_bias` | 0 to 99 |
| 30 | `supplement.foot_controller_1_volume` | 0 to 99 |
| 31 | `supplement.breath_controller.pitch` | 0 to 99 |
| 32 | `supplement.breath_controller.amplitude` | 0 to 99 |
| 33 | `supplement.breath_controller.eg_bias` | 0 to 99 |
| 34 | `supplement.breath_controller_pitch_bias` | 0 to 100 |
| 35 | `supplement.aftertouch.pitch` | 0 to 99 |
| 36 | `supplement.aftertouch.amplitude` | 0 to 99 |
| 37 | `supplement.aftertouch.eg_bias` | 0 to 99 |
| 38 | `supplement.aftertouch_pitch_bias` | 0 to 100 |
| 39 | `supplement.pitch_eg_rate_scaling` | 0 to 7 |
| 40 | `supplement.foot_controller_2.pitch` | 0 to 99 |
| 41 | `supplement.foot_controller_2.amplitude` | 0 to 99 |
| 42 | `supplement.foot_controller_2.eg_bias` | 0 to 99 |
| 43 | `supplement.foot_controller_2_volume` | 0 to 99 |
| 44 | `supplement.midi_control.pitch` | 0 to 99 |
| 45 | `supplement.midi_control.amplitude` | 0 to 99 |
| 46 | `supplement.midi_control.eg_bias` | 0 to 99 |
| 47 | `supplement.midi_control_volume` | 0 to 99 |
| 48 | `supplement.unison_detune` | 0 to 7 |
| 49 | `supplement.foot_controller_1_as_slider` | `true`, `false` |

### 5.4 The fractional scaling

A fractional scaling's members are there where the patch has one, the DX7II's [DX7II-MANUAL] p. Add-15, and not as a part starts. They are data, not parameters, and have no numbers (DESIGN §7.4). For each operator, N from 1 to 6:

| Name | Values |
| --- | --- |
| `fractional_scaling.opN.offset` | −128 to 127: the offset, which may lie between the integers and beyond the scale, as the levels may; a dump's byte holds an integer of it in two's complement (DESIGN §11.2) |
| `fractional_scaling.opN.levels` | an array of 40 numbers, each 0 to 255: the levels of the groups of keys, the lowest group's first |

### 5.5 What a part has of its own

The area of what a part has of its own is 4. `part.note_limit` is a count of voices, 0 for no limit.

| Number | Name | Values | As a part starts |
| --- | --- | --- | --- |
| 0 | `part.on` | `true`, `false` | `true` for the first part, `false` for the others |
| 1 | `part.omni` | `true`, `false` | `true` |
| 2 | `part.group` | integers 0 to 15 | 0 |
| 3 | `part.channel` | integers 0 to 15 | 0 |
| 4 | `part.lowest_key` | integers 0 to 127 | 0 |
| 5 | `part.highest_key` | integers 0 to 127 | 127 |
| 6 | `part.key_filter` | `"all"`, `"odd"`, `"even"` | `"all"` |
| 7 | `part.note_shift` | −24 to 24 | 0 |
| 8 | `part.note_limit` | integers 0 to 4294967295 | 0 |
| 9 | `part.alternate` | `true`, `false` | `false` |

## 6. IDs

A host knows each parameter by an ID of 32 bits (DESIGN §7.4, `keptone_model::id::ParameterId`): its area in bits 31 to 28, the index of its part in bits 27 to 12, 0 for the first part and for a setting, and its number in bits 11 to 0. The settings' area is 0, the voice's 1, the function data's 2, the supplement's 3 and that of what a part has of its own 4. So `tuning.a4_hz` is `0x00000000`, and `voice.op6.eg.rate.1` of the second part `0x10001000`.

## 7. The patch's digest

`patch.sha256` is the SHA-256 [FIPS-180-4] of the patch, in 64 hexadecimal digits in small letters: of an object of the part's members whose names begin `voice.`, `supplement.`, `fractional_scaling.` or `patch.`, but `patch.sha256` itself, those of a later version among them (§8), written in the canonical form of RFC 8785 [RFC8785]: without white space, the members in the order of their names' UTF-16 code units, and each number and string as §2 writes them. Every member that a later version adds to the patch begins so, and no other member does. The digest identifies the patch whatever order its members are in and however its numbers are written: the VOICE INIT's, the patch a part starts with, is `a4c71bc06d4cc4139834b78878df821a6e32fac8073b233bfe9ec2336892efaf`. A reader that finds another digest than its patch's members have reports it and reads the state all the same, and Keptone writes the state with theirs.

## 8. Versions

A later version of the format adds members, and changes no member's name, number, kind or meaning. A reader takes a state of any version: it requires the members of the earlier of the two versions, its own and the state's, and takes those of a later version that it knows wherever they are there. A member that a reader does not know, at any level, is kept as it was read, the text of its numbers and all, and written back after the members that it knows, in the order read, so that a state passes through an older Keptone whole; a part that a smaller `part.count` takes away goes with what it kept. Keptone writes the version that it knows, 1.

## 9. What is refused

A state is read whole or not at all; where it is refused, the plugin is left unchanged and the refusal reported (DESIGN §11.1). A text is refused where it is not JSON of §1's subset, or where:

- `format` is not `"keptone-state"`, or `version` is not an integer above 0;
- a member that the state's version has is not there, among them a member of a supplement or a fractional scaling where another of its members is;
- a value is of another kind than its member's, or one that its member does not take: a number that is not finite, a setting's value beyond its bounds, or an integer beyond its range or with a fraction;
- `parts` holds another count of parts than `part.count` says;
- a Scala file is not one that `keptone_model::scala` reads.

The refusal gives the line and the column of the name of the member that it refuses, or of the opening brace of the object that lacks one, and the member's place as a JSON Pointer [RFC6901], as `/settings/lfo.seed` and `/parts/0/voice.op6.eg.rate.1`.
