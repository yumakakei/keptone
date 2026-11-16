<!--
SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
SPDX-License-Identifier: GPL-3.0-or-later

This file is part of Keptone. The SPDX lines name its copyright
holder and its license, in the machine-readable form of the REUSE
specification: the GNU General Public License, version 3 or any later version
(LICENSES/GPL-3.0-or-later.txt).
-->

# The design of Keptone

Keptone (the name: §14.1) is a six-operator FM synthesizer plugin. It plays and edits Yamaha DX7 patches and computes them as an **ideal DX7**:

- The continuous-time signal the patch parameters describe, computed in binary64 at the host's sample rate.
- Antialiased.
- Free of the 1983 hardware's quantisation and noise.

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

### Non-goals

- Bit-accurate emulation of the YM21280/YM21290 and the firmware. That exists (VDX7, Plogue OPS7) and is a different product.
- Bundling Yamaha ROMs or factory voices (§14).
- A sampler, effects suite or arpeggiator. Effects belong to the host.

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
REUSE.toml  LICENSE  LICENSES/  README.md  README.ja.md  .gitignore  .gitattributes
.github/workflows/ci.yml                 # the CI jobs of §13.2, each running a script of ci/
crates/
  keptone-cpu/        processor features and the level in effect (as kiral-cpu)
  keptone-math/       reference elementary functions, ADAA, Kepler, Kapteyn tables
  keptone-kernels/    vector forms of keptone-math/engine loops, per architecture (§10)
  keptone-model/      patches, function data, DX7 SysEx/.syx, state format, tuning files
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
scripts/            Python (ruff-checked): mpmath references, Sollya drivers, filter design checks
ci/                 setup.sh  rust.sh  targets.sh  emulated.sh  python.sh  wrappers.sh  fuzz.sh  sanitizers.sh
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
| keptone-model | Typed patch model with range types; DX7 codecs (1-voice, 32-voice packed, parameter change incl. function group 2, request messages); native state (docs/STATE-FORMAT.md); `.scl` and `.kbm`. | forbid | keptone-math (checksums need nothing else) |
| keptone-engine | MATH §1–§14. Owns no threads and allocates only in `prepare()`. | forbid | keptone-math, keptone-kernels, keptone-model |
| keptone-rt | SPSC ring, triple-buffer mailbox, drop-queue. | yes (atomics + `UnsafeCell`), each block one operation | std |
| keptone-host | Processor/main/worker split (§5), parameter table and smoothing, event decoding (CLAP events, MIDI 1.0, MIDI 2.0 UMP, SysEx), state save/load. | forbid | engine, model, rt |
| keptone-clap | `#[repr(C)]` declarations of the CLAP structs and function tables, written by hand from `thirdparty/clap`; the entry, factory and extensions. | yes (FFI) | host |
| keptone-platform | `win32.rs`, `appkit.rs`, `x11.rs`, `wayland.rs`: window creation as a child of the host's window (or floating), input, DPI, surfaces, timers. System functions are declared by hand. | yes (FFI) | sys |
| keptone-render | The scene type and one backend per file: `cpu.rs` (reference rasteriser), `d3d11.rs`, `metal.rs`, `vulkan.rs`; shaders per backend; device sharing and fallback (§8.2–8.4). | yes, in backend files only | platform |
| keptone-ui | Editor state, layout, widgets, the algorithm graph editor, undo, text via FreeType + HarfBuzz (C, submodules). Builds scenes; never touches a GPU API. | forbid except a `text` FFI module | render (scene type), host (read-only views) |
| keptone-sys | OS calls the others need, declared by hand (no `libc`, `windows-sys` or `objc2` crates), as in kiral-sys. | yes | std |
| keptone-tools | Offline renderer (WAV/f64), reference renderer, alias metric, `.syx` tools, benchmarks, regression replay. | forbid | engine, model |

The lints follow kiral and JPEG-Unround exactly: `unsafe_op_in_unsafe_fn = forbid`, `missing_docs`, `missing_debug_implementations` and `unused_qualifications` denied, clippy `pedantic` denied, and `undocumented_unsafe_blocks`, `multiple_unsafe_ops_per_block`, `cast_lossless` and `as_conversions` denied. A crate that needs no `unsafe` forbids it in its root.

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

- **Ownership only moves; it is never shared.** A `CompiledPatch` is built on the main thread and published through a triple-buffer mailbox (latest wins). The audio thread takes it at a block boundary or at the event's sample, and returns the previous one through the retire ring. Memory is freed on the main thread.
- **Commands** (note events from the GUI keyboard, settings that need no reallocation) go through a bounded SPSC ring. A full ring is reported, never waited on.
- **Reallocation** (voice count, oversampling, operator count) needs CLAP's `deactivate`/`activate`. Changing these settings asks the host to restart the plugin (`clap_host.request_restart`).
- **Host SysEx and MIDI** arrive on the audio thread inside `process`. They are copied raw into a bounded ring (no parsing), decoded on the main thread, and come back as a `CompiledPatch` or `Command`. Real-time notes and controllers are handled on the audio thread from fixed-size events.
- **ADLplug-Next's rule** carries over: if the audio thread ever has to touch something the main thread may be rebuilding, it *tries* the lock and plays silence for that block if it cannot get it. The design aims to need no lock at all. Any `try_lock` that remains carries a comment giving its reason (cf. ADLplug-Next `sources/utility/realtime.h`).

## 6. The engine

### 6.1 Compile step (main thread)

`CompiledPatch::new(&Patch, &Settings) -> CompiledPatch` is total.

- Every table lookup of MATH §2–§7 is resolved into binary64 coefficients.
- The algorithm graph is reduced to SCCs, an evaluation order, ADAA offsets $s_k$ (MATH §10.5) and delay-line requirements.
- The Kapteyn tables are prepared if any operator might need them (MATH §9.5). Large tables are built on the worker thread and swapped in when ready; until then the point solver is used.

### 6.2 Render loop (audio thread)

For each engine sample, voices are processed in **structure-of-arrays** form, with the lanes holding voices: L = 4 on AVX2, 8 on AVX-512, 2 on NEON.

1. Advance the control laws in closed form: EG levels, pitch, the LFO, and gains at the times MATH §10.4 asks for.
2. Evaluate the SCCs in order: feedback solves (MATH §9) and ADAA operators (MATH §10).
3. Sum the carriers.

Lanes are independent and each lane runs the same instruction sequence. A voice's result therefore does not depend on which lane it lands in or how many voices are active, which reproducibility needs: there must be no "scalar tail" path that rounds differently from the vector body. Sums across voices happen after the lanes, in voice order. Events split the block at their sample offsets.

### 6.3 Determinism

Nothing in the engine reads time, thread identity or the address of an allocation. Random sources are SplitMix64 streams seeded from settings and the voice's start sample.

## 7. Plugin formats

### 7.1 CLAP, natively in Rust

`keptone-clap` declares the CLAP ABI by hand (`#[repr(C)]`, `extern "C"`), following kiral's practice of declaring foreign functions itself.

- `tests/layout/` compiles C23 programs against `thirdparty/clap/include` that `static_assert` every offset, size and alignment. The expected values come from a table generated from the Rust definitions. The two can never drift.
- Extensions implemented:
  - params (with polyphonic modulation per note id), state, note-ports (CLAP + MIDI 1.0 + MIDI 2.0 dialects);
  - audio-ports with `CLAP_AUDIO_PORT_SUPPORTS_64BITS` and `CLAP_AUDIO_PORT_PREFERS_64BITS`;
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
- Per-note pitch is a real-valued note into the tuning (MATH §2.1).

### 7.4 Parameters

- One host parameter per voice parameter (155) and per function parameter (14). Settings that suit automation (MATH §17) get one too.
- Every parameter is continuous (binary64). The display shows the DX7 grid. Exporting to `.syx` rounds by `syx.export_rounding = nearest | floor`, default `nearest`, and the export warns when that loses information.
- Parameter IDs are stable forever. They are documented in `docs/STATE-FORMAT.md`.

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
- Every loop is bounded by the block length, the voice count, `feedback.max_iterations` or a table size fixed at activation.

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

Specified in `docs/STATE-FORMAT.md`, which is written in milestone M2.

- A text document in a **subset of JSON**, as planned for ADLplug-Next's successor: UTF-8, no duplicate keys, numbers as decimal binary64 written shortest-round-trip, no NaN or Inf, a nesting depth limit set by the parser (a setting, default 64).
- A format version, every parameter by stable ID, every setting, the current voice, an optional bank, the tuning, and a SHA-256 of the canonical encoding of the voice.
- The parser and writer are written by hand, in kiral's practice; there is no serde.
- Unknown keys are kept and written back, so newer versions round-trip through older ones.
- A host state that fails to parse leaves the plugin unchanged and is reported.

### 11.2 Voices and banks

- DX7 `.syx` 1-voice and 32-voice dumps are read and written exactly [DEXED-SYSEX]. Concatenated and garbage-prefixed files are scanned for every well-formed message, not only the first.
- Checksum mismatches are reported. The data is loaded only if `syx.accept_bad_checksum` (default `true`) allows it.
- Out-of-range bytes are reported. They are mapped by an explicit, documented policy (`syx.out_of_range = clamp | reject`, default `clamp`), not by the ad-hoc scaling Dexed uses.
- Later: DX7II/TX802 supplements (AMEM/ACED, the TX802 performance) and user graphs in the native format.

### 11.3 Themes

`.keptone-theme` files: the JSON subset of §11.1, data only, specified in `docs/UI.md` §6b. The built-in themes are files of the same format embedded in the binary.

### 11.4 Tuning

`.scl` and `.kbm` parsers in `keptone-model` (total, fuzzed); MTS-ESP behind a feature and a setting.

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
| CORE-MATH | submodule | tests only | MIT | correctly rounded oracle |
| MTS-ESP client | submodule | optional feature | 0BSD | dynamic tuning (loads the user's `libMTS`) |
| libfuzzer-sys | crates.io | fuzz/libfuzzer only (nightly) | MIT/Apache-2.0 | coverage-guided fuzzing |
| loom | crates.io | keptone-rt dev-dependency | MIT | exhaustive interleaving tests of the queues |
| Python: mpmath, numpy, scipy | pip | scripts/ only | BSD | high-precision references, filter checks |
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

1. **rust** (Linux, Windows, macOS): `cargo fmt --check`; `cargo clippy --workspace --all-targets --locked -- -D warnings`; the documentation with every rustdoc warning an error; tests in dev and release, at every level the runner has.
2. **targets**: the x86-64-v4 build, tested where the runner has AVX-512. The v3 build is the default one (§10.1), which the rust job builds and tests. **emulated**: SDE runs on a machine of one's own.
3. **wrappers**: CMake builds of VST3, AUv2 (macOS) and standalone; then clap-validator, pluginval at strictness 10, and auval.
4. **sanitizers**:
   - ASan/UBSan and TSan on the C++ wrappers and `tests/layout`;
   - RTSan on a host simulator that drives `process` with random events;
   - Miri on `keptone-rt`, `keptone-model` and `keptone-math`.
5. **fuzz**: on push, every libFuzzer target for a short time; daily, for long. Earlier crashes are replayed first (ADLplug-Next's `ci/fuzz.sh` behaviour).
6. **conformance**: the alias metric and the independences of MATH §16, against `bench/baselines.toml`.
7. **reuse**: `reuse lint`. **python**: `ruff`.

## 14. Legal hygiene

- GPL-3.0-or-later; REUSE with SPDX headers on every file.
- No Yamaha firmware, ROM tables or factory voices in the repository or the packages. The project's own original bank is made later.
- Tables taken from MSFA (Apache-2.0) are cited where they are used.
- Firmware behaviour is learned from [AJXS-ROM] and written independently. Never translate its code: algorithms are not copyrightable, but a line-by-line translation could be a derivative of Yamaha's firmware.
- [VDX7] needs a ROM the user owns. It may be used locally to measure timings, never in CI or the product.
- "DX7" and "Yamaha" appear only descriptively ("compatible with DX7 SysEx"), never in the product name. "VST" is used per Steinberg's rules if at all.

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

## 16. Milestones

Each milestone ends with its tests green in CI and its docs updated. An item tagged **verify** in MATH that a milestone uses is resolved in `docs/VERIFICATION.md` before the milestone closes.

| M | Content | Exit criteria |
| --- | --- | --- |
| M0 | Scaffold: workspace, lints, toolchain files, REUSE, CI skeleton, docs in place. | CI green on three OSes with empty crates. |
| M1 | `keptone-math`: sin2pi, cos2pi, sinc, sinc′, exp2; ADAA-1/2 closed forms; Kepler solver; Kapteyn DFT tables and bounds. `scripts/` reference vectors. | MATH §16 rows for these pass. |
| M2 | `keptone-model`: typed patch and function data, `.syx` codecs, parameter-change and request messages, native state format with docs/STATE-FORMAT.md, `.scl`/`.kbm`. Fuzz targets and mutation tests with regressions. | Round trips byte-identical; fuzzing clean for a CI day. |
| M3 | `keptone-engine` headless, point-sampled (`adaa.order = 0`, M = 1): pitch, levels, EG, PEG, LFO, controllers, graph, implicit feedback, voices. `keptone-tools render`. | Rate and block independence; EG tests; algorithm-data test of three sources. |
| M4 | Antialiasing: ADAA-1/2 with delay alignment, droop EQ, the Kapteyn series, oversampling with IIR/FIR halfbands; the reference renderer and alias metric; baselines. | Alias metric baselines recorded; series ≡ point where both apply. |
| M5 | `keptone-host` + `keptone-clap`: params with polyphonic modulation, state, notes, MIDI 1.0/2.0, SysEx, expressions, tuning, 64-bit audio, latency/tail/voice-info; RTSan. | clap-validator clean; RTSan clean on random sessions. |
| M6 | `wrappers/`: VST3, AUv2, standalone. | pluginval 10 and auval pass. |
| M7 | `keptone-kernels`: measured vector forms (§10.4), dispatch, SDE tests; CPU budget report. | Contracts met at all levels; engine-level agreement and reproducibility tests pass; measured speedups recorded. |
| M8a | `keptone-platform` + `keptone-render`: windows on four systems; `cpu.rs`, then `d3d11.rs`, `metal.rs`, `vulkan.rs`; device sharing, loss and fallback. | Scene-equivalence tests pass against `cpu.rs`; a test editor opens in Bitwig, REAPER, Logic and Ardour (X11) and as a Wayland floating window; an idle editor draws no frames. |
| M8b | `keptone-ui`: the editor of `docs/UI.md`, including the graph editor and user graphs (MATH §8.4). | UI.md §8 tests pass; event-sequence fuzzing clean; no RTSan reports with the editor open. |
| M9 | Extensions: MTS-ESP, per-voice algorithms, DX7II/TX802 data, adaptive oversampling, cubic-interpolation ADAA. | Each behind its setting, with tests. |
