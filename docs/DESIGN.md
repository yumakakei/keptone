<!--
SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
SPDX-License-Identifier: GPL-3.0-or-later

This file is part of Keptone. The SPDX lines name its copyright
holder and its license, in the machine-readable form of the REUSE
specification: the GNU General Public License, version 3 or any later version
(LICENSES/GPL-3.0-or-later.txt).
-->

# The design of Keptone

Keptone (the name: §14.1) is a six-operator FM synthesizer plugin. It asks what the DX series would be, made today as a plugin with none of its hardware's constraints: it imitates no hardware and is not limited to the DX7, yet it never strays from the clean sound of the DX series. It plays and edits Yamaha DX7 patches and computes them as an **ideal DX7**:

- The continuous-time signal the patch parameters describe, computed in binary64 at the host's sample rate.
- Antialiased.
- Free of the 1983 hardware's quantisation and noise.

It is to offer, in time, the features of the DX7's family as well: two voices layered or split, function data for each voice, supplements of voices, performances, pan, micro tunings, fractional scalings and more tone generators, from the DX1, the TX7, the DX7II, the DX7s and the TX802 (§6.4). It reproduces none of these instruments, the DX7 among them: it computes what their data means, and takes their features as its own.

`docs/MATH.md` defines that signal. This document defines the software around it. `docs/REFERENCES.md` lists the sources.

## 1. Goals and non-goals

### Goals

1. **Clean**: aliasing, zipper noise and quantisation noise are measured (MATH §16) and pushed below the measurement thresholds the project sets.
2. **Faithful in meaning**: a DX7 patch keeps its envelopes, scaling, modulation, feedback character and algorithm. Hardware *artefacts* go; hardware *laws* stay (MATH §15).
3. **Free**: every behaviour-changing choice is a setting. The defaults suit the main use, playing DX7 patches. Nothing is a hard-coded limit: polyphony, operator count, oversampling, tolerances and table sizes are all configurable, bounded only by memory.
4. **Safe**: hostile input (patch files, SysEx, host state, tuning files, events) can never cause memory unsafety, undefined behaviour, a panic on the audio thread, or a non-finite output.
5. **Real-time**: the audio thread never allocates, frees, locks, or makes system calls.
6. **Accurate and reproducible**:
   - Every kernel meets a stated accuracy contract, in every form.
   - One build, kernel selection, settings and input give the same output bit for bit, whatever the block sizes and the thread timing.
   - `engine.kernels = portable` extends that to every supported processor (MATH §0.2–0.3).
   - Vector forms are free to reorder within their contracts, so optimisation is not tied to the reference's operation order.
7. **Portable**: Linux, Windows and macOS. CLAP, VST3, AUv2 and standalone. Linux is the primary platform.
8. **Family-wide**: the features the DX7's family adds to the DX7 (§6.4) are Keptone's own, at the family's level, all of them together if the user wishes. An instrument's data becomes Keptone's, and Keptone's becomes the instrument's again as far as its data can hold it.

### Non-goals

- Reproducing an instrument. Keptone is never a copy of the DX7 or of any of its family: not of their chips (the YM21280/YM21290) and firmware bit for bit, which VDX7 and Plogue OPS7 emulate and which is a different product, nor of their limits, such as their polyphony and their outputs.
- Bundling Yamaha ROMs or factory voices (§14).
- A sampler, effects suite or arpeggiator. Effects belong to the host.
- More outputs than a stereo pair, such as the TX802's individual ones: the host's tracks give them, each with an instance of its own (§6.4).

## 2. What is designed out

The project's analysis of Dexed 1.1.0 and AV Dexed found these classes of defect. Each line names the structural rule that removes it.

| Defect class (example) | Rule here |
| --- | --- |
| Range checks only on some input paths. Dexed clamps cartridge voices, but single-voice SysEx and the legacy state reach the engine raw; an algorithm number ≥ 32 indexes past `algorithms[32]`. | **Parse, don't validate.** Every input becomes a typed `keptone-model` value, whose fields are range types, at exactly one boundary per format. The engine accepts only a `CompiledPatch` built from those types. An out-of-range value cannot be represented. |
| Buffer over-reads. A size check of 156 bytes before reading 162; a legacy blob copied with no length check. | Safe Rust slices in all parsers (`#![forbid(unsafe_code)]` in `keptone-model`). Fuzzing and mutation tests on every format (§12.3). |
| Integer truncation. The parameter-change group bits are truncated to `uint8`, so function parameters land on voice parameters. | `as` conversions are denied by lint (`clippy::as_conversions`); only checked conversions are used. Function parameters are a separate decoded type. |
| Work on the audio thread. Host SysEx triggers some 150 parameter notifications and allocations in `processBlock`. | All parsing and patch compilation run on the main thread. The audio thread receives finished objects through wait-free queues (§5). |
| Data race. The MIDI-input thread writes the active program while the audio thread reads it. | Exactly one writer per object. Ownership moves through queues; nothing is shared and mutable (§5). |
| Engine assumes sanitised data. | The `CompiledPatch` constructor is the only producer, and it is total: every typed input gives a valid compiled patch. |
| Block-rate control (64-sample EG/LFO/pitch steps), 1024-point sine table, Q24 arithmetic. | Per-sample closed forms in binary64 (MATH §0.3, §12). |

## 3. Repository layout

```
Cargo.toml  Cargo.lock  rust-toolchain.toml  rustfmt.toml  clippy.toml  ruff.toml  .cargo/config.toml
CMakeLists.txt  CMakePresets.json        # only the C++ wrappers and the C layout tests
REUSE.toml  LICENSE  LICENSES/  README.md  README.ja.md  .gitignore  .gitattributes  .gitmodules
.github/workflows/ci.yml  fuzz.yml       # the CI jobs of §13.2 (fuzz.yml the daily fuzzing), each running a script of ci/
crates/
  keptone-cpu/        processor features and the level in effect (as kiral-cpu)
  keptone-math/       reference elementary functions, ADAA, Kepler, Kapteyn tables
  keptone-kernels/    vector forms of keptone-math/engine loops, per architecture (§10)
  keptone-model/      patches, function data, operator graphs, DX7 SysEx/.syx, state format, tuning files
  keptone-engine/     voices, envelopes, LFO, graph, feedback, oversampling, mixing
  keptone-rt/         wait-free queues and mailboxes (the only lock-free code; Miri/loom-tested)
  keptone-host/       format-neutral processor: parameter table, event decoding, threads, state
  keptone-clap/       the CLAP ABI declared by hand, and the plugin entry (cdylib + staticlib)
  keptone-platform/   per-OS windows, input, DPI, surfaces, one file per system (§8.1)
  keptone-render/     scene renderers: cpu (reference), d3d11, metal, vulkan (§8.2)
  keptone-ui/         editor: state, layout, widgets, graph editor, text shaping (docs/UI.md)
  keptone-sys/        operating-system calls declared by hand (time, thread hints, dlopen for MTS-ESP)
  keptone-tools/      CLI: render, reference, compare, syx inspect/convert, bench, replay
wrappers/           C++26: clap-wrapper targets for VST3, AUv2 and standalone (CMake)
tests/
  layout/           C23 programs that check keptone-clap's struct layouts against the CLAP headers
  validators/       clap-validator, pluginval and auval invocations
conformance/        patches, settings and expected metrics (MATH §16)
fuzz/
  libfuzzer/        cargo-fuzz crate (nightly; outside the workspace)
  dict/  seeds/  regressions/<target>/
bench/baselines.toml
scripts/            Python (ruff-checked): mpmath references, Sollya drivers, filter design checks, fuzz seeds
ci/                 setup.sh  core-math.sh  rust.sh  targets.sh  emulated.sh  python.sh  wrappers.sh  fuzz.sh  sanitizers.sh
thirdparty/         git submodules, pinned commits (§12)
docs/               DESIGN.md  MATH.md  UI.md  REFERENCES.md  VERIFICATION.md  STATE-FORMAT.md  DEPENDENCIES.md
  mockups/          editor mockups (HTML, PNG) and their README
assets/themes/      the built-in themes (*.keptone-theme), embedded in keptone-ui
```

## 4. Crates

| Crate | Role | `unsafe` | Depends on |
| --- | --- | --- | --- |
| keptone-cpu | Features and level in effect; `KEPTONE_ISA=<level>` caps it and `KEPTONE_KERNELS=portable` forces the reference (kiral's `Request`). | forbid | std |
| keptone-math | Reference functions (MATH §9, §10, §12), all `#[inline]`, all scalar binary64 with `mul_add`. | forbid | — |
| keptone-kernels | Vector forms, one module per architecture (§10). Each call into assembly is one `unsafe` block whose comment says what was checked. | per-arch modules only | keptone-cpu, keptone-math |
| keptone-model | Typed patch model, every number any finite binary64 (MATH §0.5); the operator graph, of any number of operators, with the DX7's 32 algorithms as graphs (MATH §8); DX7 codecs (1-voice, 32-voice packed, the DX7II's supplements, performances, system set-up, micro tunings and fractional scalings, the TX802's performances and system set-up, the DX1's and the TX7's performances, parameter change incl. function group 2, the DX7's panel switches, the DX7II's switch remote and supplements, and the TX7's changes, request messages); parts and patches as Keptone plays them, the settings of MATH §17, and the parameters' stable IDs and names (§7.4); native state (docs/STATE-FORMAT.md); `.scl` and `.kbm`. | forbid | keptone-math (checksums need nothing else) |
| keptone-engine | MATH §1–§14, parts among them (§6.4). Owns no threads and allocates only in `prepare()`. | forbid | keptone-math, keptone-kernels, keptone-model |
| keptone-rt | SPSC ring, triple-buffer mailbox, drop-queue. | yes (atomics + `UnsafeCell`), each block one operation | std |
| keptone-host | Processor/main/worker split (§5), parameter table and smoothing, event decoding (CLAP events, MIDI 1.0, MIDI 2.0 UMP, SysEx), the parts' channels, the family's performances and system set-ups as parts (§6.4), state save/load. | forbid | engine, model, rt |
| keptone-clap | `#[repr(C)]` declarations of the CLAP structs and function tables, written by hand from `thirdparty/clap`; the entry, factory and extensions. | yes (FFI) | host |
| keptone-platform | `win32.rs`, `appkit.rs`, `x11.rs`, `wayland.rs`: window creation as a child of the host's window (or floating), input, DPI, surfaces, timers. System functions are declared by hand. | yes (FFI) | sys |
| keptone-render | The scene type and one backend per file: `cpu.rs` (reference rasteriser), `d3d11.rs`, `metal.rs`, `vulkan.rs`; shaders per backend; device sharing and fallback (§8.2–8.4). | yes, in backend files only | platform |
| keptone-ui | Editor state, layout, widgets, the algorithm graph editor, undo, text via FreeType + HarfBuzz (C, submodules). Builds scenes; never touches a GPU API. | forbid except a `text` FFI module | render (scene type), host (read-only views) |
| keptone-sys | OS calls the others need, declared by hand (no `libc`, `windows-sys` or `objc2` crates), as in kiral-sys. | yes | std |
| keptone-tools | Offline renderer (WAV/f64), reference renderer, alias metric, `.syx` tools, benchmarks, regression replay. | forbid | engine, model |

The lints follow kiral and JPEG-Unround exactly: `unsafe_op_in_unsafe_fn = forbid`, `missing_docs`, `missing_debug_implementations` and `unused_qualifications` denied, clippy `pedantic` denied, and `undocumented_unsafe_blocks`, `multiple_unsafe_ops_per_block`, `cast_lossless` and `as_conversions` denied. A crate that needs no `unsafe` forbids it in its root. In keptone-engine and keptone-tools, clippy's `disallowed-methods` refuses f64's elementary functions (`ln`, `log2`, `exp`, `exp2`, `powf`, `powi`, `sin`, `cos` and the rest), which call the system's libm: they take keptone-math's (MATH §12).

## 5. Threads and data flow

```
            main thread (CLAP main)              worker thread                audio thread (CLAP process)
  host ──► params/state/GUI ─ compile ─┐      heavy jobs: Kapteyn tables,     ┌─► events at sample offsets
           SysEx decode ◄──────────────┼──── decimator design, library scan  │   engine.render(..)
                                       └──► mailbox(CompiledPatch) ─────────►┤
                                            ring(Command)  ────────────────►┤
  GUI ◄── ring(Telemetry: voice states, meters, current EG levels) ◄────────┘
  main ◄── ring(Retired objects to drop) ◄──────────────────────────────────┘
```

- **Ownership only moves; it is never shared.** A `CompiledPatch` is built on the main thread and published through a triple-buffer mailbox (latest wins), one for each part (§6.4). The audio thread takes it as an event (MATH §0.3): a host's change at its own sample, and one published from the main thread at the first sample of the block in which the audio thread finds it, which is the instant it enters the engine's input. It returns the previous one through the retire ring. Memory is freed on the main thread.
- **Commands** (note events from the GUI keyboard, settings that need no reallocation) go through a bounded SPSC ring, and the audio thread stamps each at the first sample of the block in which it takes it. A full ring is reported, never waited on.
- **Reallocation** (voice count, part count, the output's channels, oversampling, operator count) needs CLAP's `deactivate`/`activate`. Changing these settings asks the host to restart the plugin (`clap_host.request_restart`).
- **Host SysEx and MIDI** arrive on the audio thread inside `process`. They are copied raw into a bounded ring (no parsing), decoded on the main thread, and come back as a `CompiledPatch` or `Command`. Real-time notes and controllers are handled on the audio thread from fixed-size events.
- **ADLplug-Next's rule** carries over: if the audio thread ever has to touch something the main thread may be rebuilding, it *tries* the lock and plays silence for that block if it cannot get it. The design aims to need no lock at all. Any `try_lock` that remains carries a comment giving its reason (cf. ADLplug-Next `sources/utility/realtime.h`).

## 6. The engine

### 6.1 Compile step (main thread)

`CompiledPatch::new(&Patch, &Function, &Configuration) -> CompiledPatch` is total: a part's patch, its function data and the engine's configuration, the settings it takes typed, which the engine checks when it is prepared and refuses by name where M3 does not compute them (§16). A patch is what a part plays (§6.4): a DX7 voice, with the DX7II's supplement and fractional scaling where it has them. Each part's patch is compiled by itself, with the part's own parameters, so that a change to one part recompiles that part alone.

- What depends on the patch alone, a table's value or a law's, is resolved into binary64 coefficients; what depends on a note, its key and its velocity, at the note's key-on.
- The algorithm graph is reduced to SCCs, an evaluation order, ADAA offsets $`s_k`$ (MATH §10.5) and delay-line requirements.
- The Kapteyn tables are prepared if any operator might need them (MATH §9.5). Large tables are built on the worker thread before the patch that needs them is published, so that a patch reaches the audio thread complete and no output depends on when the worker finishes (MATH §0.2).

### 6.2 Render loop (audio thread)

For each engine sample, voices are processed in **structure-of-arrays** form, with the lanes holding voices, those of every part alike: L = 4 on AVX2, 8 on AVX-512, 2 on NEON.

1. Advance the control laws in closed form: EG levels, pitch, the LFO, and gains at the times MATH §10.4 asks for.
2. Evaluate the SCCs in order: feedback solves (MATH §9) and ADAA operators (MATH §10).
3. Sum the carriers.

Lanes are independent and each lane runs the same instruction sequence. A voice's result therefore does not depend on which lane it lands in or how many voices are active, which reproducibility needs: there must be no "scalar tail" path that rounds differently from the vector body. Sums across voices happen after the lanes, in the order of each part's slots (MATH §13.2). Events split the block at their sample offsets.

Each voice holds the compiled patch of its part, so voices of different parts share the lanes. A batch of lanes runs one instruction sequence, so it holds voices of one graph: voices are grouped by their graph, their part's algorithm or user graph (MATH §8.3, §8.4), when one starts or ends or a part's patch changes, at that event's sample. A voice's result does not depend on its lane, so the grouping changes nothing but the speed. Each part sums its voices in the order of its slots, and the output adds the parts in the order of their indices (MATH §14).

### 6.3 Determinism

Nothing in the engine reads time, thread identity or the address of an allocation, and everything it does happens at an engine sample that its input determines (MATH §0.3). Random values are SplitMix64's outputs read at counters, in streams of each part's own (MATH §6.3), so that a value needs no history and no part's values depend on another's.

### 6.4 Parts and the family

The DX7's family adds to the DX7 what a second voice, performances and more tone generators give. Keptone takes those of its features that suit a plugin, as its own: it never reproduces the instruments, and leaves to the host what a host does better, such as outputs beyond a stereo pair (§1). Its engine is built for them from the start: it plays **parts** (MATH §13.5).

- The family is the DX7's, of six operators: the DX7, the TX7, the DX1, the DX7II, the DX7s and the TX802. The four-operator instruments of the time are not of it. The eight waveforms of the TX81Z and the instruments after it would each need antiderivatives of their own (MATH §10), and would leave the feedback's equation, which is the sine's (MATH §9). The envelopes of all of them, those of the DX21, the DX27 and the DX100, which have the sine alone, among them, follow a law of attack, two decays and release, not MATH §4's rates and levels. A graph of four operators remains the user's to make (MATH §8.4).
- A part plays one patch on the notes it takes: those of its MIDI channel, or of every channel, within its range of keys and through its key filter, shifted by its note shift. It has its own detune, volume, tuning (§11.4), note limit, alternate group and function data, whose place a supplement in its patch takes as the DX7II's does (MATH §13.5), and its pan, its place in the stereo output (MATH §14).
- Every part takes its voices from one pool of `voice.count` (MATH §13.1), into slots of its own that keep what its notes leave (MATH §13.2), and the voices of every part share the lanes (§6.2). A note that two parts take starts a voice in each: the parts are layered.
- `part.count`, default 2, the A and B of the DX1 and the DX7II, is fixed at activation (§5). Two parts layer or split one performance within one instance, which parts alone can do; more instruments on more channels, as the TX802's, are another instance on another track, or a larger `part.count`. With the first part alone on, as it is by default, the engine is the DX7's as MATH writes it.
- Parts are Keptone's own, and the native state holds them, binary64 like every value (§11.1). An instrument's performance and system set-up are a way in and out, for compatibility, as `.syx` is for a voice (§11.2): they read into parts as far as Keptone's features take them, and what those do not take is kept with the parts as it came, so that the parts become the instrument's data again byte for byte; what parts hold that the instrument's data cannot is reported, as the writers of §11.2 report it. What each instrument's data gives the parts:

| Instrument | Parts | What its data gives them |
| --- | --- | --- |
| DX7 | one | the function data (MATH §7) |
| TX7 | one | the function data of each of its voices, and of those of a DX7 played with it, which its function memories hold [TX7-MANUAL] |
| DX1 | two, A and B | single, dual or split, the split point and the dual detune; each side's function data, key shift, output attenuation and sustain pedal; its outputs A, B and P, read as pans [DX1-MANUAL] [TX7-MANUAL] |
| DX7II | two, A and B | single, dual or split, the split point, the dual detune, the balance and the total volume; each side's channel, note shift, sustain, foot switch and sliders; the micro tuning, the key filter and the pan; each voice's supplement and fractional scaling [DX7II-MANUAL] |
| DX7s | one | the DX7II's, for one voice [DX7S-MANUAL] |
| TX802 | one to eight | each instrument's tone generators, two notes each, as a note limit; its channel, keys, note shift, detune, volume, outputs I and II read as a pan, micro tuning, EG forced damp and alternate assign; the tables of program and control changes [TX802-MANUAL] [TX802-SYSEX] |

- No setting ties Keptone to one instrument; what it does follows from the data. A message's format says whose data it holds, and a panel switch's number whose panel it presses. A performance of the DX1 or the TX7 becomes parts A and B as its key assign mode says, single playing the side its voice select names, the side one part alone takes, and selects the voice its first byte names (§11.2); a program change selects as the system set-up's tables and channels say. An instrument is named only where data leaves Keptone for it: a writer, or the librarian sending to an instrument connected to it (§11.2), is told which instrument's data to write. The one message whose meaning depends on its sender, the operator select, which the TX7 numbers from OP6 and the DX7II from OP1, is read only from an instrument the librarian knows to be connected, and is otherwise kept and passed over (§11.2).
- What the features do to the sound, a supplement's, a fractional scaling's and a pan's among them, is written in MATH as Keptone's own with the milestones that give it (§16), keeping what a voice's data means (§1). Until then a part plays its patch as a DX7 voice, its supplement and fractional scaling held but not heard.

## 7. Plugin formats

### 7.1 CLAP, natively in Rust

`keptone-clap` declares the CLAP ABI by hand (`#[repr(C)]`, `extern "C"`), following kiral's practice of declaring foreign functions itself.

- `tests/layout/` compiles C23 programs against `thirdparty/clap/include` that `static_assert` every offset, size and alignment. The expected values come from a table generated from the Rust definitions. The two can never drift.
- Extensions implemented:
  - params (with polyphonic modulation per note id), state, note-ports (CLAP + MIDI 1.0 + MIDI 2.0 dialects), whose channels, MIDI 2.0's groups among them, reach the parts that receive them (§6.4);
  - audio-ports with `CLAP_AUDIO_PORT_SUPPORTS_64BITS` and `CLAP_AUDIO_PORT_PREFERS_64BITS`: one output port, stereo, or mono as `output.channels` says (MATH §14);
  - latency, tail, voice-info, note-name, thread-check, render (offline/realtime hint), gui (§8), timer-support/posix-fd-support where the platform layer needs them, and preset-load later.

### 7.2 VST3, AUv2 and standalone via clap-wrapper

`wrappers/` builds them with [CLAP-WRAPPER]'s clap-first workflow, linking `keptone-clap`'s **staticlib**: one binary per format, each with the CLAP code inside. This is the only C++ in the project (C++26, Clang, LLD). The third-party sources are compiled with their own warnings and the project's own files with its strict warnings, as ADLplug-Next separates them.

- The VST3 SDK is MIT since 3.8 [VST3-MIT], so it can be shipped.
- AUv2 uses Apple's AudioUnitSDK (Apache-2.0).
- Validated with clap-validator, pluginval and `auval` in CI.

**Alternative (not taken):** implement VST3 directly in Rust over [VST3-C-API]. That would remove the C++ but duplicate clap-wrapper's mature translation. Reconsider if clap-wrapper blocks a feature.

LV2: not planned (CLAP and VST3 cover the Linux hosts in use). AAX: no. AUv3: only if MUTOS or iOS needs it, and then in Swift (MUTOS's decision).

### 7.3 Expression

- MPE (MIDI 1.0 zones), CLAP note expressions (tuning, pressure, brightness, volume, pan, vibrato) and MIDI 2.0 per-note controllers map to per-voice destinations (MATH §7.1).
- Per-note pitch is a real number of semitones added to a note's pitch, or of keys added to its note before the tuning, as `tuning.modulation` says (MATH §2.1).
- A part that receives an MPE zone takes the notes of all of its channels (§6.4).

### 7.4 Parameters

- Each part has one host parameter for each of its voice's parameters (151: the DX7's 145 and the six operators' switches), its function data's (22), its supplement's (50) and its own (10, MATH §13.5): 233 (`keptone_model::id::PartParameter`). The settings that suit automation (MATH §17) have one each, and so will the system set-up's parameters when they come (M5). A voice's name, a fractional scaling's 246 bytes and a micro tuning's 128 keys are data, edited in the editor and by their parameter changes, not host parameters.
- Every parameter that is a number takes any finite binary64 (`keptone_model::voice::Value`, MATH §0.5): a value on the instrument's scale, between its steps, or beyond its ends, held as itself. The enumerations (the algorithm, the curves of keyboard scaling, the oscillator mode, the LFO's wave) and the switches take only the DX7's steps. The display shows the DX7 grid, and the scale is the range that the editor's controls and the host's parameters show; how a value beyond it is shown and reaches a host is written with them (M5, M7). Exporting to `.syx` rounds by `syx.export_rounding = nearest | floor`, default `nearest` (a value halfway between two steps to the even one), a value beyond the scale as its nearer end, and the export warns when that loses information, as it does for an operator switched off, which no dump holds.
- Parameter IDs are stable from Keptone's first release on (`keptone_model::id::ParameterId`). An ID is 32 bits: an area in bits 31 to 28 (the settings 0, the voice 1, the function data 2, the supplement 3, the part's own 4, the system set-up's to come), a part's index in bits 27 to 12, 0 for a setting, and a number within the area in bits 11 to 0, so that a part's parameters keep their IDs whatever `part.count` is. The voice's numbers are the DX7's, 0 to 144 [DEXED-SYSEX], with the operators' switches at 155 to 160; the other areas number their parameters in the order of the model's fields, and the settings in the order of MATH §17. Each parameter has a name that never changes either, by which the native state writes it (§11.1): its area's and its place in the model, as `voice.op6.eg.rate.1`, `function.modulation_wheel.assign.pitch` and `part.lowest_key`, and a setting's its own. From that release on, a number or a name once given is never given to another parameter. The host sees those of `part.count` parts, and rescans them after the restart that changing the count asks for (§5). They are documented in `docs/STATE-FORMAT.md`.

## 8. Editor (GUI) and rendering

What the editor shows and how it behaves is specified in `docs/UI.md`. This section fixes the architecture underneath it.

### 8.1 Windows: one file per windowing system

`keptone-platform` creates the editor's window, takes input and DPI changes, and hands the renderer a surface:

| File | Window | Surfaces it can offer |
| --- | --- | --- |
| `win32.rs` | a child `HWND` of the host's window, used **only** by us | DXGI swap chain (flip model), GDI blit |
| `appkit.rs` | an `NSView` subclass, layer-backed, created through the Objective-C runtime declared by hand | `CAMetalLayer`, `CGImage` blit |
| `x11.rs` | a child of the host's XID via XCB, declared by hand | `VK_KHR_xcb_surface`, MIT-SHM blit |
| `wayland.rs` | CLAP floating window only (`xdg_toplevel`) | `VK_KHR_wayland_surface`, `wl_shm` |

Why no toolkit:

1. Plugins must embed into the host's window, and only Win32, Cocoa and X11 allow that. Native Wayland has no embedding; CLAP's Wayland API is for floating windows [CLAP-WAYLAND].
2. A desktop toolkit inside a host process conflicts with the host's copy of the same toolkit [QT-IN-PLUGINS]. WinUI 3 needs the Windows App SDK runtime and XAML islands in a foreign `HWND`, which a plugin cannot assume.
3. MUTOS (Wayland-only, plugins out of process) uses the floating Wayland window. Other Linux hosts use X11 through XWayland.

The child `HWND` is ours alone: flip-model presentation to a child window is supported as long as no blt-model presentation shares that `HWND` [MS-FLIP-CHILD].

### 8.2 Scene and renderers (`keptone-render`)

`keptone-ui` never draws. It builds a **scene**: a display list of filled and stroked paths (rectangles, rounded rectangles, circles, polylines, quadratic and cubic Béziers), glyph runs (glyph IDs and positions from HarfBuzz), images, clips, transforms and opacity layers. Geometry is binary32 in logical pixels, because GPUs work in binary32. This is the one sanctioned exception to binary64, since no audio passes through it. The renderer turns the scene into pixels.

One file per backend, behind one trait:

| Backend | Systems | Role |
| --- | --- | --- |
| `cpu.rs` | all | **Reference and fallback.** Analytic-coverage scanline rasteriser, premultiplied linear-light blending, sRGB output. Always available, and the oracle the GPU backends are tested against. |
| `d3d11.rs` | Windows | Direct3D 11 is enough for 2-D UI, is on every Windows 10/11 machine, and has the simplest device and swap-chain model. |
| `metal.rs` | macOS | Apple deprecated OpenGL on macOS [APPLE-GL-DEPRECATED], so Metal is the only first-class GPU API there. |
| `vulkan.rs` | Linux (X11 and Wayland) | No global "current context", so no conflict with a host that draws with OpenGL. |

Not planned:

- **OpenGL.** Machines that meet the x86-64-v3 baseline have Vulkan drivers in practice, and every other case falls back to `cpu.rs`. The one notable gap: Intel Haswell and Broadwell graphics on Linux have Vulkan only through Mesa's separate `intel_hasvk` driver, split out of ANV in Mesa 22.3, which a distribution may not build [MESA-HASVK]. Those machines get `cpu.rs`. Dropping GL also removes the save-and-restore of the host's current context that a GL plugin needs [GL-IN-PLUGINS].
- **Vulkan on Windows.** D3D11 covers every Windows machine. A second Windows backend doubles the testing for nothing.
- **Direct3D 12.** It adds explicit memory, barrier and descriptor management and gains nothing for a 2-D editor that draws a few thousand primitives. Revisit only if profiling ever shows D3D11 driver overhead to matter.

The API functions are declared by hand, as for the rest of the system interface:

- D3D11/DXGI COM vtables;
- Vulkan through `vkGetInstanceProcAddr`;
- Metal through `objc_msgSend`.

Only the subset used is declared. `unsafe` stays inside each backend file.

### 8.3 GPU drawing method

- **Shapes:** each primitive is a quad. Its fragment shader computes coverage analytically from a signed-distance function: rounded rectangles, circles, and line segments with round or butt caps. Béziers are flattened on the CPU to segments within ¼ device pixel. There is no MSAA. Edges match the CPU rasteriser's analytic coverage.
- **Text:** FreeType rasterises glyph coverage on the CPU into an R8 atlas, which the GPU samples with gamma-correct blending. Glyphs are positioned at subpixel offsets; there is no LCD subpixel antialiasing.
- **Plots** (EG, scaling, spectrum, scope): polylines from the UI's evaluation of the engine's closed forms. The scope and spectrum read the telemetry ring.
- **Shaders:** four programs (shape, glyph, image, composite), written once per backend and kept in that backend's directory:
  - HLSL SM 5.0, compiled offline with `fxc`, bytecode embedded;
  - MSL, compiled offline to a `.metallib`;
  - GLSL 4.50 compiled to SPIR-V with glslang (pinned).

  At this size three hand-written variants are cheaper and clearer than a cross-compiler dependency. Reconsider at about 10 programs.
- **Equivalence:** a test renders a fixed set of scenes with every available backend and compares each with `cpu.rs`. The tolerance is ≤ 2/255 per channel away from edges, and edge pixels within the coverage error of ¼ px. CI runs the CPU, Vulkan-on-lavapipe and (where the runner has a GPU) native backends.

### 8.4 Devices, threads and frames

- **One device per process per backend**, created lazily when the first editor opens, shared by every plugin instance in that process (reference-counted), released when the last editor closes. Each editor owns its own swap chain or surface.
- **All rendering runs on the host's UI thread.** It never runs on the audio thread, and the GPU never touches audio. GPU submission has millisecond-scale and unbounded latency and is not real-time. The offline reference renderer (MATH §16) may gain a GPU compute path later. It is not planned.
- **Frames are drawn only when something changed:** a parameter, telemetry, an animation or a resize. Damage rectangles limit the redraw. Animation frames come from the host's timer (CLAP `timer-support` / `posix-fd-support`) or the platform's display link, at most at the display's refresh rate. An idle editor uses no GPU time.
- **HiDPI:** scale sources per system, the device-pixel grid and the tests are specified in `docs/UI.md` §2.5.
- **Failure:** if device creation fails, the device is lost (`DXGI_ERROR_DEVICE_REMOVED`, `VK_ERROR_DEVICE_LOST`, a Metal command buffer error) or a backend is denylisted, the editor switches to `cpu.rs` without closing and says so in the status bar.
- **Choice:** `ui.renderer = auto | cpu | d3d11 | metal | vulkan`. `auto` tries d3d11 → cpu on Windows, metal → cpu on macOS, and vulkan → cpu on Linux. The setting is per user (a global settings file), not per project.

The editor is optional to the processor. The host's generic parameter UI works from milestone M5 on.

## 9. Real-time safety and numerics

### 9.1 The audio thread

The audio thread: `clap_plugin.process` and everything it calls.

- No allocation or deallocation.
- No locks except a documented `try_lock`.
- No system calls, no I/O, no unbounded loops.
- Every loop is bounded by the block length, the voice count, the part count, `feedback.max_iterations` or a table size fixed at activation.

### 9.2 Checking it

- `keptone-host` marks the process path for RealtimeSanitizer [RTSAN]. Until rustc's `#[sanitize(realtime = "nonblocking")]` is stable, use `__rtsan_realtime_enter`/`__rtsan_realtime_exit`, declared by hand behind a cargo feature `rtsan`, and linked against the `clang_rt.rtsan` runtime of the pinned LLVM (no third-party crate).
- A canary test must be reported, as ADLplug-Next's `tests/sanitizers/realtime_canary.cc` is, so the silence of the real run means something.

### 9.3 No panics on the audio thread

- Arithmetic is binary64, so it cannot overflow.
- Indexing uses `get` with explicit fallbacks, or iterators.
- `debug_assert!` documents invariants. Release builds do not unwind: `panic = "abort"` in the profiles of `keptone-clap` and the wrappers, so a bug shows as a crash, never as unwinding through C frames.

### 9.4 Subnormals

The floating-point environment is never changed (MATH §0.2). Subnormals are avoided structurally (MATH §13.4). A test feeds every recursive state adversarial small values and asserts that no subnormal is ever produced.

## 10. Assembly and vector code

### 10.1 Baselines

- **x86-64:** builds target x86-64-v3 (AVX2, FMA, BMI2), as ADLplug-Next's do. FMA in hardware makes the portable form's `mul_add` fast and lets `engine.kernels = portable` give identical bits on every machine (MATH §0.2). Every x86-64 machine a DAW runs on in 2026 has it. An x86-64-v4 build (AVX-512, all 512 bits) is offered as well. Runtime dispatch picks AVX-512 forms in the v3 build too when the processor has them. Wherever Rust code is compiled for AVX-512, it takes vectors of 512 bits: every x86-64 build turns LLVM's `prefer-256-bit` tuning off (`.cargo/config.toml`), which x86-64-v4 and Intel's processors otherwise imply, and rustc's warning that it does not know that feature is expected.
- **AArch64:** NEON, which has FMA. Apple M-series builds target macOS 26 (kiral's choice).

### 10.2 Forms

Every kernel has a **portable form** in plain Rust: the reference, written so the compiler can vectorise it (fixed chunks, lanes as independent sums). A form in assembly is added only with a measurement showing the portable form is slower, recorded in the kernel's comment (rates, processor, clock), as kiral does. It is tested against the reference at every level of the processor, and under Intel SDE for levels no runner has (`ci/emulated.sh`).

### 10.3 How assembly is written

kiral and ADLplug-Next keep assembly in `.S` files assembled by Clang through a preprocessor header that papers over ELF, Mach-O and COFF. Rust now offers a better tool for that job: **naked functions** (`#[unsafe(naked)]` with `naked_asm!`, stable since Rust 1.88) [RUST-ASM]. The compiler turns a naked function into global assembly and emits the symbol, section, visibility and mangling right for each object format. So:

- **Kernels:** naked functions, one Rust file per architecture and instruction level, for example `keptone-kernels/src/x86_64/avx2.rs` or `aarch64/neon.rs`. Each is declared `extern "sysv64"` on x86-64 on every system, as kiral's kernels are, so one ABI holds on Linux, macOS and Windows; and `extern "C"` on AArch64. AArch64 kernels never touch `x18`, the platform register on Apple and Windows. The architecture is still isolated by file: `#[cfg(target_arch)]` appears only on `mod` declarations, never inside a function. This removes `build.rs`, the external `clang`/`llvm-ar`/`llvm-nm` steps and the `common.h` macros. Where an instruction needs the assembler's permission (AArch64 extensions), the kernel states it with `.arch_extension` inside the template, not with `#[target_feature]` on the naked function. If indirect-branch tracking is ever enabled for x86-64 builds, kernels begin with `endbr64`; the constant that writes it lives in one place, as kiral's `KIRAL_BRANCH_TARGET` does.
- **`asm!` inside ordinary functions:** only for small things the compiler cannot express. Example: reading the timestamp counter in `keptone-tools bench`. It is **never** used to change MXCSR/FPCR outside a block that restores them, because that is undefined behaviour in Rust [RUST-SETCSR].
- **Intrinsics (`core::arch`)** are not used. That keeps the project's earlier line: the portable form leaves optimisation to the compiler, and the hand-written form says exactly what runs.
- **`.S` files** remain allowed where a naked function cannot do the job (none is known). They would follow kiral's `build.rs` pattern.

### 10.4 Candidate kernels

These are listed in expected order of benefit, and are to be measured first:

- The per-sample operator update across voice lanes: phase, ADAA-2, `sin2pi`, gain.
- The Kepler Newton step across lanes.
- The Clenshaw sum of the Kapteyn series.
- The halfband decimator.

## 11. Data formats

### 11.1 Native state

Specified in `docs/STATE-FORMAT.md`, and read and written by `keptone_model::state`.

- A text document in a **subset of JSON** [RFC8259], as planned for ADLplug-Next's successor: UTF-8, no duplicate keys, numbers as decimal binary64 written shortest-round-trip, as ECMAScript writes them [ECMA-262], no NaN or Inf, a nesting depth limit set by the parser (`json.max_depth`, default 64).
- A format name and version; every setting, by its name; the tuning's Scala files, kept as their texts; and every part, with what it has of its own, its function data and its patch (a voice with its supplement and fractional scaling), each parameter by its stable name, beside which `docs/STATE-FORMAT.md` gives its ID (§7.4). Each patch carries the SHA-256 [FIPS-180-4] of its members in the canonical form of RFC 8785 [RFC8785], which identifies it whatever order its members are in and however its numbers are written. The system set-up, the micro tunings that the parts play in, and the optional memories of voices, performances, micro tunings and fractional scalings, from which program changes select, enter the state with the milestones that give them their meaning (M5, M9a).
- The parser and writer are written by hand, in kiral's practice; there is no serde. The parser, `keptone_model::json`, is kiral's, made to keep the arrays and objects it is in on a stack of its own, so that no depth the limit allows takes the thread's stack, and to give the line and the column of an error; it keeps a number's text beside its binary64, so that a 64-bit integer such as `lfo.seed` reads exactly, and where an object and each of its members' names begin, so that what the state refuses can be placed. `keptone_model::sha256` is kiral's SHA-256.
- Unknown keys are kept at every level and written back after the known ones, so newer versions round-trip through older ones; a later version adds members and changes none.
- A state is read whole or refused whole, with the line, the column and the JSON Pointer [RFC6901] of what it refuses: a member that its version has and it lacks, or a value of another kind than its member's or one that its member does not take, as a number that is not finite (a number beyond its scale is taken, MATH §0.5). A host state that is refused leaves the plugin unchanged and is reported. A patch's digest that is not its members' is reported, and the state is written with theirs.

### 11.2 Voices, banks and other messages

- Keptone's own format is the native state (§11.1), binary64 throughout. `.syx` and the other data of the DX7's family are formats of compatibility: for exchanging data with the instruments, and for reading the patches found elsewhere.
- Keptone is upward compatible with the DX7 rather than a copy of it: it reads every message the DX7 reads, and those of its family that the DX7 does not, the DX1's, the TX7's and the DX7II's, and what the DX7's own manual documents where its firmware differs, wherever they mean nothing that conflicts with what the DX7 takes.
- DX7 `.syx` 1-voice and 32-voice dumps are read and written exactly [DEXED-SYSEX] (`keptone_model::syx`). Concatenated and garbage-prefixed files are scanned for every well-formed message, not only the first. A message cut short, by another status byte or by the end of the file, is reported, and the scan goes on from the byte that cut it.
- Checksum mismatches are reported. The data is loaded only if `syx.accept_bad_checksum` (default `true`) allows it.
- The DX7II's supplements of voices (`keptone_model::supplement`), the additional parameters of its ACED and AMEM formats, are read and written exactly [DX7II-MANUAL] [DX7S-MANUAL]: the dump of one, 49 bytes, under format 5, the dump of 32, packed into 35 bytes each, under 6, and the parameter changes of group 6 with the group byte's low bits 0. What each parameter does to the sound is for Keptone to say, keeping what the data means (§6.4). The pitch envelope's range, which the data give as one of eight, four, one and half an octave, is held as a binary64 in octaves, the span between the pitches of the envelope's lowest level and its highest (MATH §0.5): 0 holds the pitch still, and a range below 0 turns the envelope upside down. A range between the four or beyond them is written as the one that `syx.export_rounding` rounds it to: the nearest, of two as near the even step, or the widest at or below it, and half an octave for a range below that.
- The DX7II's universal bulk dumps (`keptone_model::dx7ii`) are read and written exactly [DX7II-MANUAL] [DX7S-MANUAL]: a group of a byte count, which counts the names, the classification name `LM  ` and a data format name, the data and the checksum of the names and the data, as many groups as a message holds. Read are a group of the performance edit buffer, `8973PE`, 51 bytes; of 32 performances, `8973PM`, one after another; of the system set-up, `8973S `, 102 bytes, of which the master tune is set by the DX performance parameter change's number 64, or the DX7s's 85, which has no place for the 17 bytes of the DX7II's that have no name and keeps the memory protect's byte as a reserved one; of a micro tuning, the edit buffer's, `MCRYE `, or one of the two in memory, `MCRYM` and the memory's number as a byte, two bytes for each of the 128 keys, a step from 0 to 10794 in its high seven bits and its low seven; of a fractional scaling, `FKSYE `, the offset and the levels of 40 groups of keys of each operator from OP6, each a byte sent as two, its high four bits and then its low four, each 0x30 above them [DX7S-MANUAL]; and of a cartridge's micro tunings, `MCRYC `, 64 from the DX7II and 63 from the DX7s, or fractional scalings, `FKSYC `, 32, a group each, as many as 64. The parameter changes of group 6 with the group byte's low bits 1 set a performance's parameters, numbers 0–50, and the system set-up's, 64–83; those with the low bits 0 and the numbers 126 and 127 set a key of the micro tuning, with three bytes of data, and a parameter of the fractional scaling, with four. The offset's byte holds the offset, −128 to +127, in two's complement, which [DX7II-MANUAL] and [DX7S-MANUAL] leave unsaid: dumps of DX7II banks hold 0x00 in the offset of every operator in fractional mode, and 0x00 to 0x74 or 0xFF in the others, 0x00 the most [DX7II-DUMPS], which two's complement reads as −1 to +116, mostly no offset, where 128 above the offset would read them as mostly −128, the far end. The offset is held as a binary64 on its scale of −128 to +127 or beyond it, as the levels are on theirs of 0 to 255 (MATH §0.5): a byte read gives an integer, written back as the same byte, and an offset between the integers or beyond the scale is written rounded as `syx.export_rounding` says. Where the DX7II's manual and the DX7s's give a parameter different ranges, both are taken. The DX7s's system reset, the switch remote's number 46, is read with its data, which the DX7s ignores.
- The performances of the DX1 and the TX7 (`keptone_model::performance`) are read and written exactly [TX7-MANUAL]: the dump of one, 94 bytes, under format 1, and under 2 as [DX7-MANUAL] prints it, which is reported and written under 1; and the dump of 64, packed into 64 bytes each, under 2. The DX7's firmware 1.8 takes format 1 and passes a dump under 2 over [AJXS-ROM]. A performance holds two sides, A and B, each with the function data, its controllers' sensitivities 0–15 where the DX7's ranges are 0–99 (MATH §7.4), and an attenuation; which side an instrument that plays one takes; the key assign mode; a name; and the bytes the TX7's manual leaves unnamed. Each dump holds all of it but one of the last two, and its writer reports what it could not hold. The TX7's condition acknowledge (format 125) is read and written as well. The DX7 takes the dump of one as the A side of a DX1's [DX7-MANUAL]: it selects the voice the first byte names and takes the function data, passing the rest over [AJXS-ROM]; the TX7 plays the side its voice select names [TX7-MANUAL].
- The other messages of the DX7 and its family are read and written by the same module: parameter changes of the voice group (0, parameters 0–155, the number's high bits in the group byte, and 156, the operator select of the TX7 and the DX7II, which number the operators in opposite orders and whose step is kept as it comes, to be read as the instrument that sent it numbers them, where that is known (§6.4)) and of the function group (2, parameters 64–77) [DEXED-SYSEX] [DX7-MANUAL] [TX7-MANUAL] [DX7II-MANUAL]; the DX7's panel switch messages, group 2 with numbers 0–41, a button pressed or released [AJXS-ROM]; the DX7II's switch remote, group byte 0x1B with switches 0–45 [DX7II-MANUAL]; the TX7's DX performance parameter change, group 1, and TX function parameter change, group 4 with the group byte's low bits 1 [TX7-MANUAL]; and dump requests of formats 0, 1, 2, 5, 6, 9 and 125, and of a universal bulk dump by its names [DX7II-MANUAL] [TX7-MANUAL]. Every other message is reported as one of a kind not read.
- Keptone answers a dump request of a voice, of 32 voices or of the DX7II's other data with that data [DX7II-MANUAL]; the DX7 itself answers none [AJXS-ROM]. Panel switch messages, the DX7's [AJXS-ROM] and the switch remotes of the DX7II [DX7II-MANUAL] and the TX802 [TX802-SYSEX], press the buttons of the front panels (UI.md §2.6). The librarian makes a hardware DX7 dump what it holds by sending it panel switch messages: with SYS INFO AVAIL, a voice button sends that voice, and FUNCTION, button 8 to MIDI transmit and YES send the bank [AJXS-ROM]. `keptone-model` decodes and encodes these messages (M2), and the host answers and acts on them (M5).
- Out-of-range bytes are reported: a step outside its parameter's range, a packed byte that sets bits its format leaves unused, and a switch's data that is neither 0 (released) nor 127 (pressed). They are mapped by an explicit, documented policy (`syx.out_of_range = clamp | reject`, default `clamp`), not by the ad-hoc scaling Dexed uses: `clamp` takes a step outside the range as the range's nearer end, ignores unused bits and takes a switch's data other than 0 as pressed, as the DX7's firmware does [AJXS-ROM]; `reject` refuses the message. A message is loaded or refused whole.
- The TX802's own data (`keptone_model::tx802`) is read and written exactly [TX802-SYSEX]: its performance edit buffer, `8952PE`, the twelve parameters of each of its eight tone generators and a name, 116 bytes, each sent as two hexadecimal digits, '0' to '9' and 'A' to 'F'; the 64 performances of its memory, `8952PM`, a group each of 84 packed bytes so sent, the detune in bits 6–3 as the TX802's dumps hold it [TX802-DUMPS], where Yamaha's table draws three bits; and its system set-up, `8952S `, 263 bytes of seven bits. A performance's group counts its data alone, the names not counted, unlike every other universal bulk dump's, and its checksum is of the names and the data. A byte that is no hexadecimal digit is reported, and taken as the digit it most likely stands for. The parameter changes of group 6 with the group byte's low bits 2 set a performance's parameters, numbers 0–115, the voice's and the micro tuning table's with two bytes of data; the switch remote's numbers 64–96 are the TX802's panel switches. Its voices, supplements, micro tunings and fractional scalings are the DX7II's.
- Later: user graphs in the native format.

### 11.3 Themes

`.keptone-theme` files: the JSON subset of §11.1, data only, specified in `docs/UI.md` §6b. The built-in themes are files of the same format embedded in the binary.

### 11.4 Tuning

`.scl` and `.kbm` parsers in `keptone-model` (`keptone_model::scala`; total, fuzzed); MTS-ESP behind a feature and a setting. Where [SCALA-SCL] and [SCALA-KBM] leave things open, the parsers:

- end a line at a line feed, a carriage return or both together, and take a line that begins with `!` as a comment;
- take a scale's description as its first line that is not a comment, whole and possibly empty, as UTF-8 where its bytes are UTF-8 and as ISO 8859-1 where they are not, and pass over blank lines after it, and throughout a mapping;
- take a value as the first run of characters on its line that are not spaces or tabs, and pass over the rest of the line, as the format says of a pitch;
- read a pitch that holds a period as cents, `-?[0-9]+\.[0-9]*`, and any other as a ratio, `[0-9]+(/[0-9]+)?`, both parts above 0 and within 64 bits, a ratio alone over 1;
- take a mapping's four keys as MIDI's, 0 to 127, its frequency as `[0-9]+(\.[0-9]*)?` above 0, its entries as integers or `x`, and the keys of its pattern after the last entry as unmapped, and refuse a mapping that leaves its reference key unmapped, as the format says;
- pass over what follows a scale's last pitch or the last entry that a mapping's size allows, and allocate only as they read, never because a count in the file says so.

A part plays in the tuning that `tuning.source` gives (MATH §2.1), or in a micro tuning of the DX7II's and the TX802's, a step from 0 to 10794 for each of the 128 keys (§11.2). Their eleven preset tunings, equal temperament, pure major and pure minor, mean tone, Pythagorean, Werckmeister, Kirnberger, Vallotti and Young, the equal temperament shifted by a quarter tone, the quarter tone and the eighth tone, the second to the fifth of them in any key [DX7II-MANUAL] [TX802-MANUAL], are computed from the ratios and temperaments they name and checked against the cents that [TX802-MANUAL] prints (its p. 28), never copied from an instrument's memory (§14).

## 12. Dependencies

### 12.1 Policy

Dependencies are kept to the minimum for their own sake, not banned. Each one is justified in `docs/DEPENDENCIES.md`: what it does, why writing it would cost more, its license and its pinned version or commit.

- crates.io crates are allowed when justified. `cargo --locked` always.
- Git submodules are pinned commits.

### 12.2 Current list

| Dependency | Kind | Where | License | Why |
| --- | --- | --- | --- | --- |
| CLAP headers | submodule | layout tests, clap-wrapper | MIT | the ABI's source of truth |
| clap-wrapper | submodule | wrappers/ | MIT | VST3/AUv2/standalone from the CLAP |
| VST 3 SDK ≥ 3.8 | submodule | wrappers/ | MIT | VST3 |
| AudioUnitSDK | submodule | wrappers/ (macOS) | Apache-2.0 | AUv2 |
| FreeType, HarfBuzz | submodules | keptone-ui | FreeType (FTL or GPLv2), HarfBuzz (Old MIT) | text. Record which FreeType license option is taken, and why, in DEPENDENCIES.md |
| CORE-MATH | submodule | tests only: the few files they compile, which `ci/core-math.sh` fetches | MIT | correctly rounded oracle |
| MTS-ESP client | submodule | optional feature | 0BSD | dynamic tuning (loads the user's `libMTS`) |
| libfuzzer-sys | crates.io | fuzz/libfuzzer only (nightly) | (MIT OR Apache-2.0) AND NCSA | coverage-guided fuzzing |
| loom | crates.io | keptone-rt dev-dependency | MIT | exhaustive interleaving tests of the queues |
| Python: mpmath, SymPy, numpy, scipy | pip | scripts/ only | BSD | high-precision references, derivations, filter checks |
| Sollya | tool | scripts/ only | CeCILL-C | coefficient design and error certificates |

### 12.3 Testing tools

These are not dependencies of the product: clap-validator, pluginval, auval, Intel SDE, Miri, and cargo-fuzz (nightly).

## 13. Build, toolchain and CI

### 13.1 Toolchain

As kiral and JPEG-Unround:

- Rust latest stable; edition 2024; `rust-version` set to the stable release at project start.
- `rustfmt` with 120 columns.
- LLD on every system: `lld-link` on Windows; Clang as the linker driver with `-fuse-ld=lld` elsewhere.
- LLVM pinned in `ci/setup.sh` (LLVM 23.1.x as in kiral, checked by SHA-256).
- C23 and C++26 with Clang, extensions off.
- Floating-point contraction is **on, for speed**: `-ffp-contract=fast-honor-pragmas`. Clang fuses `a*b + c` into an FMA within an expression by default, and `fast` extends that across statements. Plain `fast` makes Clang ignore the `FP_CONTRACT` pragmas [CLANG-FP-CONTRACT]; `fast-honor-pragmas` keeps them working. The exceptions are code whose correctness depends on separate rounding: error-free transformations (TwoSum, TwoProduct), compensated sums, and expressions such as `a*a - b*b` where a fused form can change the sign of a result that should be zero. Those functions carry `#pragma clang fp contract(off)` and a comment saying why. The kernel accuracy contracts (MATH §0.2) are tested on the build as shipped, so contraction is covered by the tests. GCC is not supported: it does not implement the pragma. `ci/setup.sh` checks on every runner that the pinned Clang driver accepts `-ffp-contract=fast-honor-pragmas` directly (older drivers needed `-Xclang -ffp-contract=fast-honor-pragmas`), that it then fuses a product and a sum written as separate statements, and that it leaves them unfused under `#pragma clang fp contract(off)`. Clang 22.1.8 was seen to do the same on x86-64 (ELF and COFF) and AArch64 while that check was written, and to fuse under the pragma as well with plain `fast`.
- Rust never contracts on its own. Rust code gets its FMAs from explicit `mul_add` and from the assembly kernels (§10).

### 13.2 CI jobs

GitHub Actions, plain git checkout, no third-party actions:

1. **rust** (Linux, Windows, macOS): `cargo fmt --check`; `cargo clippy --workspace --all-targets --locked -- -D warnings`; the documentation with every rustdoc warning an error; tests in dev and release, at every level the runner has, with the files of CORE-MATH that `ci/core-math.sh` fetches for them.
2. **targets**: the x86-64-v4 build, tested where the runner has AVX-512. The v3 build is the default one (§10.1), which the rust job builds and tests. **emulated**: SDE runs on a machine of one's own.
3. **wrappers**: CMake builds of VST3, AUv2 (macOS) and standalone; then clap-validator, pluginval at strictness 10, and auval.
4. **sanitizers**:
   - ASan/UBSan and TSan on the C++ wrappers and `tests/layout`;
   - RTSan on a host simulator that drives `process` with random events;
   - Miri on `keptone-rt`, `keptone-model` and `keptone-math`.
5. **fuzz**: on push, every libFuzzer target for a minute; daily, for thirty minutes each (`.github/workflows/fuzz.yml`), carrying on from the corpus the last daily run kept as an artifact. Earlier crashes are replayed first (ADLplug-Next's `ci/fuzz.sh` behaviour). A pinned nightly toolchain and cargo-fuzz build the targets with the address sanitizer, overflow checks and debug assertions; each target holds its parser to the same property as the parser's mutation tests.
6. **conformance**: the alias metric and the independences of MATH §16, against `bench/baselines.toml`.
7. **reuse**: `reuse lint`. **python**: `ruff`.

## 14. Legal hygiene

- GPL-3.0-or-later; REUSE with SPDX headers on every file.
- No Yamaha firmware, ROM tables or factory voices in the repository or the packages. The project's own original bank is made later.
- Tables taken from MSFA (Apache-2.0) are cited where they are used.
- Firmware behaviour is learned from [AJXS-ROM] and written independently. Never translate its code: algorithms are not copyrightable, but a line-by-line translation could be a derivative of Yamaha's firmware. A law that the firmware computes from a table of its ROM is written in a form of Keptone's own that behaves as it does, never as a copy of the table (MATH §0.4).
- [VDX7] needs a ROM the user owns. It may be used locally to measure timings, never in CI or the product.
- "DX7", "Yamaha" and the names of the other instruments appear only descriptively ("compatible with DX7 SysEx"), as text: never in the product name, and never as a logo, the front panels (UI.md §2.6) included. "VST" is used per Steinberg's rules if at all.

(This is engineering hygiene, not legal advice.)

### 14.1 The name

**Keptone**: Kepler + tone. The feedback operator is solved through Kepler's equation (MATH §9). Spelling and casing:

- "Keptone" in prose;
- `keptone` for the repository, crates (`keptone-*`), file extensions (`.keptone-theme`) and identifiers;
- `KEPTONE_` for environment variables.

Checks made on 2026-11-16, before choosing:

| Where | Result |
| --- | --- |
| crates.io | `keptone` not taken |
| GitHub | no repository named `keptone` |
| Web (products, synths, plugins) | nothing named Keptone |
| USPTO trademark search (wordmark) | no records |
| J-PlatPat (JPO), mark text KEPTONE / ケプトーン | no records |
| J-PlatPat, phonetic-similarity search ケプトーン, class 9 | 5 live registrations, none identical: CEPTON and §C∞CEPTON (セプトン; lidar company, classes 9/37/42), SEPTON (セプトン), REPTON (リプトン/レプトン), F-tone (エフトーン). The nearest differ in the first sound and the long vowel. |

Not checked: EUIPO, WIPO Madrid. A registration, if ever wanted, needs a proper clearance search by a professional first.

The earlier working name SIXOP was dropped: it is an anagram of Korg's *opsix*, a six-operator FM synthesizer, in the same class of goods.

## 15. Open decisions

These are recorded here and settled by the author (and GRØVeN where it is their use):

1. ~~The product name~~ — **decided: Keptone** (2026-11-16; §14.1).
2. **The default for β ≥ 1** (MATH §9.6): `relaxation` or `clamp`. Decide by listening tests with FB 6–7 patches.
3. **EG clock** 49 096 vs 44 100 (MATH §4.1), by verification.
4. **The default AA preset**: ADAA-2 + ×2 IIR oversampling + `droop_eq = auto`, to be confirmed against the CPU budget measured in M7.
5. **GRØVeN's priorities**: which DAW (Logic → AUv2 first?), and whether the librarian or the sound engine matters more to him first.
6. **GUI**: confirm §8 (renderer architecture) and `docs/UI.md` §9 (its open questions).
7. ~~Dump requests and panel switches~~ — **decided** (2026-11-17; §11.2, UI.md §2.6).
8. ~~The DX1's performance~~ — **decided** (2026-11-17; §6.4, §11.2).
9. ~~The DX7II's data~~ — **decided** (2026-11-17; §11.2, §16).
10. ~~The fractional scaling's offset~~ — **decided: two's complement** (2026-11-17; §11.2).
11. ~~How far the family reaches~~ — **decided: the DX7's six-operator family** (2026-11-17; §6.4).
12. ~~How many parts by default~~ — **decided: 2** (2026-11-17; §6.4).
13. ~~The function data or the supplement~~ — **decided: the supplement, as the DX7II** (2026-11-17; MATH §13.5).
14. ~~How each law continues beyond its scale~~ — **decided: its formula as written, its table along its end steps, a limit of the instrument acting on the parameters on their scales** (2026-11-19; MATH §0.5, §0.7).

## 16. Milestones

Each milestone ends with its tests green in CI and its docs updated. An item tagged **verify** in MATH that a milestone uses is resolved in `docs/VERIFICATION.md` before the milestone closes: decided, or provisional where no independent source can settle it (MATH §0.4).

| M | Content | Exit criteria |
| --- | --- | --- |
| M0 | Scaffold: workspace, lints, toolchain files, REUSE, CI skeleton, docs in place. | CI green on three OSes with empty crates. |
| M1 | `keptone-math`: sin2pi, cos2pi, sinc, sinc′, exp2; ADAA-1/2 closed forms; Kepler solver; Kapteyn DFT tables and bounds. `scripts/` reference vectors. | MATH §16 rows for these pass. |
| M2 | `keptone-model`: typed patch and function data, `.syx` codecs of the DX7's and its family's data (§11.2), parameter-change and request messages, native state format with docs/STATE-FORMAT.md, `.scl`/`.kbm`. Fuzz targets and mutation tests with regressions. | Round trips byte-identical; fuzzing clean for a CI day. |
| M3 | `keptone-engine` headless, point-sampled (`adaa.order = 0`, M = 1): pitch, levels, EG, PEG, LFO, controllers, graph, implicit feedback, voices, parts: the notes a part takes and its note limit (MATH §13.5). `keptone-tools render`. The engine refuses at activation, by name, a setting whose value M3 does not compute, so that no render differs silently from MATH: `engine.oversampling` above 1, `adaa.order` above 0, `feedback.series_beta_max` above 0, `tuning.source` other than `12-tet`, and `graph.operators` other than 6; `keptone-tools render` sets the first three to M3's values until M4 computes theirs. | Rate and block independence; EG tests; algorithm-data test of three sources; a part's output, with voices to spare, the same whatever the other parts play. |
| M4 | Antialiasing: ADAA-1/2 with delay alignment, droop EQ, the Kapteyn series, oversampling with IIR/FIR halfbands; the reference renderer and alias metric; baselines. | Alias metric baselines recorded; series ≡ point where both apply. |
| M5 | `keptone-host` + `keptone-clap`: params with polyphonic modulation, state, notes, MIDI 1.0/2.0, SysEx, expressions, tuning, 64-bit audio, latency/tail/voice-info; the parts' channels, and the family's performances and system set-ups as parts (§6.4); RTSan. | clap-validator clean; RTSan clean on random sessions; the family's performances and system set-ups become parts and come back byte for byte. |
| M6 | `wrappers/`: VST3, AUv2, standalone. | pluginval 10 and auval pass. |
| M7 | `keptone-kernels`: measured vector forms (§10.4), dispatch, SDE tests; CPU budget report. | Contracts met at all levels; engine-level agreement and reproducibility tests pass; measured speedups recorded. |
| M8a | `keptone-platform` + `keptone-render`: windows on four systems; `cpu.rs`, then `d3d11.rs`, `metal.rs`, `vulkan.rs`; device sharing, loss and fallback. | Scene-equivalence tests pass against `cpu.rs`; a test editor opens in Bitwig, REAPER, Logic and Ardour (X11) and as a Wayland floating window; an idle editor draws no frames. |
| M8b | `keptone-ui`: the editor of `docs/UI.md`, including the graph editor, user graphs (MATH §8.4) and the front panels of the family (UI.md §2.6). | UI.md §8 tests pass; event-sequence fuzzing clean; no RTSan reports with the editor open. |
| M9a | Voices of the family: what a voice's supplement does (amplitude modulation sensitivity 0–7, the pitch EG's range, velocity switch and rate scaling, the LFO's key trigger, unison and its detune, random pitch, the pitch bend's modes, portamento's steps, the controllers and their biases), fractional scaling and micro tuning, each a law of Keptone's own that keeps what a voice's data means (§6.4). | Every law in MATH with its source, its verify items resolved in `docs/VERIFICATION.md`. |
| M9b | Parts at the family's level: detune, volume, pan and what moves it, EG forced damp, alternate groups, the foot switch's functions, and the tables of program and control changes. | Every law in MATH; the family's performances and system set-ups play through parts as far as Keptone's features take them. |
| M10 | Extensions: MTS-ESP, per-voice algorithms, adaptive oversampling, cubic-interpolation ADAA. | Each behind its setting, with tests. |
