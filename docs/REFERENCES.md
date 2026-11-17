<!--
SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
SPDX-License-Identifier: GPL-3.0-or-later

This file is part of Keptone. The SPDX lines name its copyright
holder and its license, in the machine-readable form of the REUSE
specification: the GNU General Public License, version 3 or any later version
(LICENSES/GPL-3.0-or-later.txt).
-->

# References

Every algorithm, constant and format the project implements is traced to an entry here. `docs/MATH.md` and `docs/DESIGN.md` cite entries by their key in square brackets, such as [LEVIEN-EG]. Code cites the same keys in its comments. An entry says what the project takes from it. When a source turns out to be wrong, the entry says so and says what replaced it. Do not delete the entry.

Status tags:

- **spec**: defines a behaviour the project reproduces (the "ideal DX7").
- **method**: a numerical or DSP method the project implements.
- **verify**: a value the project uses provisionally until it is checked against an independent source (see `docs/MATH.md` §0.4).
- **context**: background only; nothing is taken from it.

## 1. The Yamaha DX7: behaviour and data

| Key | Source | Status | What is taken |
| --- | --- | --- | --- |
| DX7-MANUAL | Yamaha, *DX7 Digital Programmable Algorithm Synthesizer Operating Manual*. https://br.yamaha.com/files/download/other_assets/9/333979/DX7E1.pdf | spec | Parameter set and ranges, the 32 algorithm charts, function parameters, the MIDI implementation (its pp. 30–32): the bulk dumps of one voice and of 32, the parameter changes of the voice group (0, parameters 0–155) and the function group (2, parameters 64–77), when the DX7 receives them (its channel, SYS INFO AVAIL, memory protect off), and a 94-byte performance of the DX1's A side that it receives, whose format number it prints as 2, where [TX7-MANUAL] numbers it 1 and gives 2 to the dump of 64 performances, and firmware 1.8 takes 1 and passes a dump of 2 over [AJXS-ROM]; Keptone reads both (DESIGN §15). It documents no dump request and no panel switch message. Its table gives the LFO's wave as 0–4, a misprint: the waves are six, 0–5, as [DEXED-SYSEX] has them and [AJXS-ROM] names them. |
| DX7-SERVICE | Yamaha, *DX7/DX9 Service Manual*. https://archive.org/download/synthmanual-yamaha-dx-7-and-dx-9-service-manual/yamahadx-7anddx-9servicemanual.pdf | context | Block diagrams of the EGS/OPS, the DAC and output stage. These are the hardware artefacts the project removes. |
| DX7II-MANUAL | Yamaha, *DX7IIFD/D Digital Programmable Algorithm Synthesizer Owner's Manual*. https://data.yamaha.com/download/files/2075145/ (Yamaha serves it there as DX7IIE.PDF) | spec | Its appendix, the MIDI data format: the dump request, `F0 43 2n ff F7` with f = 0, 5, 6 or 9, and that of the universal bulk dump, which Keptone answers (DESIGN §15); the switch remote, parameter group 0x1B with switches 0–45, pressed (127) and released (0), the DX7II's counterpart of the DX7's panel switch messages; and, for the DX7II's later emulation (DESIGN §11.2), its parameter groups (voice, supplement, performance, system set-up, micro tuning, fractional scaling), its bulk formats (0, 5, 6, 9) and the universal bulk dump (7E). |
| DX7II-SERVICE | Yamaha, *DX7IIFD/D Service Manual* (1987). https://archive.org/download/synthmanual-dx-7ii-fd-service-notes/dx-7iifdservicenotes.pdf | context | Block diagram, memory configuration, LSI data table and a second copy of the MIDI data format, for the DX7II's emulation. |
| TX802-MANUAL | Yamaha, *TX802 FM Tone Generator Owner's Manual*. https://data.yamaha.com/download/files/2073639/ (Yamaha serves it there as TX802E.PDF) | spec | What the TX802 receives and sends over MIDI (its pp. 32–33): parameter changes and dump requests, under its device number and the protects of its memories, and its bulk data with their sizes: 32 voices as a VMEM and an AMEM message (4104 and 1128 bytes), 64 performances (PMEM, 11 598), the system setup (281), micro tunings (274; 16 952 for the 63 of a cartridge) and fractional scalings (16 165). It does not give the formats' bytes; those it shares with the DX7II are in [DX7II-MANUAL]. |
| TX802-SERVICE | Yamaha, *TX802 FM Tone Generator Service Manual* (1987). https://archive.org/download/synthmanual-tx-802-service-notes/tx-802servicenotes.pdf | context | Block diagram, memory configuration, the MIDI implementation chart and the LSI data table, for the TX802's emulation. |
| TX7-MANUAL | Yamaha, *TX7 Tone Generator Owner's Manual*. https://data.yamaha.com/download/files/2074429/ (Yamaha serves it there as TX7E1.PDF) | spec | Its MIDI data format (its pp. 33–45): the dumps of one voice, of one performance (format 1, 94 bytes, p. 42), of 64 performances (format 2, 4096 bytes, packed into 64 each, p. 43) and of 32 voices; the parameter changes of the voice group (0, with 156 the operator select, pp. 38–39), the DX performance parameter change (group 1, p. 40), the function group (2, whose sensitivities it keeps on a scale of 0–15, p. 41) and the TX function parameter change (group 4 with the group byte's low bits 1, p. 41); and the dump requests of formats 0, 1, 2, 9 and 125, the last answered by the condition acknowledge (p. 45). Keptone reads all of them (DESIGN §11.2, §15). It numbers the dump of one performance 1 and that of 64 2, where [DX7-MANUAL] prints 2 for one. |
| TX7-NOTES | Yamaha, *TX7 Tone Generator Performance Notes*. https://data.yamaha.com/download/files/2074422/ (Yamaha serves it there as TX7E2.PDF) | context | The TX7's factory performances, for a DX7 and a TX7 played together as their sides A and B: what a performance holds in use. Nothing is taken from it, and no factory data is kept (DESIGN §14). |
| DEXED-SYSEX | Dexed repository, `doc/sysex-format.txt`. https://github.com/asb2m10/dexed/blob/master/doc/sysex-format.txt | spec | Byte layout of the single-voice dump (155 bytes), the 32-voice packed dump (128 bytes per voice), parameter change messages (`0gggggpp` group byte) and function parameters 64–77. |
| AJXS-ROM | ajxs, annotated disassembly of DX7 firmware v1.8. https://github.com/ajxs/yamaha_dx7_rom_disassembly | spec / verify | The authority on firmware behaviour: LFO, pitch EG, operator EG updates, voice allocation, portamento, MIDI handling. Read it as documentation and write independent code from it. Never translate it line by line (see DESIGN §14). |
| AJXS-ANALYSIS | ajxs, *Yamaha DX7 Technical Analysis*. https://ajxs.me/blog/Yamaha_DX7_Technical_Analysis.html | spec / context | Sample rate 49 096 Hz; log-sine table structure; the anti-hunting filter in the feedback path; 12-bit log EG amplitude (Q8 log2); the DAC: 12-bit mantissa, ×1/2/4/8 scaling, 16 kHz output filter. |
| SHIRRIFF-1 | K. Shirriff, *Reverse-engineering the Yamaha DX7 synthesizer's sound chip from die photos*. https://www.righto.com/2021/11/reverse-engineering-yamaha-dx7.html | context | The OPS architecture. |
| SHIRRIFF-EXP | K. Shirriff, *The Yamaha DX7 synthesizer's clever exponential circuit*. https://www.righto.com/2021/11/reverse-engineering-yamaha-dx7_28.html | context / verify | The exponential ROM. It is used only to cross-check the level-to-gain law, never as a table to copy. |
| SHIRRIFF-SIN | K. Shirriff, *Inside the log-sine ROM*. https://www.righto.com/2021/12/yamaha-dx7-reverse-engineering-part-iii.html | context | The log-sine ROM (a hardware quantisation the project removes). |
| SHIRRIFF-ALG | K. Shirriff, *How algorithms are implemented*. https://www.righto.com/2021/12/yamaha-dx7-chip-reverse-engineering.html | spec | Second independent source for the 32 algorithm graphs. Also that the OPS can take an algorithm per voice, which the DX7 never uses (an option here). |
| SHIRRIFF-OUT | K. Shirriff, *The output circuitry*. https://www.righto.com/2022/02/yamaha-dx7-chip-reverse-engineering.html | context | Time multiplexing, sample-and-hold, the DAC scale factor (artefacts removed). |
| SHIRRIFF-REG | K. Shirriff, *The control registers*. https://www.righto.com/2022/02/yamaha-dx7-chip-reverse-engineering_7.html | context | Register interface; the per-voice algorithm capability. |
| LEVIEN-EG | R. Levien, *Dx7Envelope* (msfa wiki). https://github.com/google/music-synthesizer-for-android/blob/master/wiki/Dx7Envelope.wiki | spec / verify | The idealised envelope: the level mapping, total level, 0.0235 dB steps, the decay law 0.2819·2^(q/4)·(1+0.25·(q mod 4)) dB/s, the attack law (rate × (2 + ⌊(full − L)/256⌋)), the jump of 1700 steps at the start of an attack, the clip at 3824 steps below full scale, and qrate = ⌊41·R/64⌋. |
| LEVIEN-HW | R. Levien, *Dx7Hardware* (msfa wiki). https://github.com/google/music-synthesizer-for-android/blob/master/wiki/Dx7Hardware.wiki | spec / context | EG value: Q8 log2 gain. Frequency: Q10 log2 with 1/4096 linear steps within an octave (a quantisation removed here). Feedback: mean of the last two outputs, gain a power of two. |
| LEVIEN-FM | R. Levien, *FrequencyModulation* (msfa wiki). https://github.com/google/music-synthesizer-for-android/blob/master/wiki/FrequencyModulation.wiki | context | Phase vs frequency modulation; feedback FM turns chaotic at high loop gain. |
| MSFA | Google / R. Levien, *music-synthesizer-for-android* (Apache-2.0, archived 2023). https://github.com/google/music-synthesizer-for-android — and the derived copy in Dexed: https://github.com/asb2m10/dexed/tree/master/source/msfa | spec / verify | Measured tables and laws: `dx7note.cc` (coarse ratios, fine, the detune law, the velocity table, keyboard level and rate scaling, the pitch- and amp-modulation sensitivity tables), `env.cc` (output-level table, qrate, the attack/decay increments, the measured "static" segment timings), `lfo.cc` (the LFO frequency table, delay law, waveforms), `pitchenv.cc` (pitch-EG rate and level tables), `porta.cpp` (portamento law), `fm_core.cc` (algorithm table), `fm_op_kernel.cc` and `exp2.cc` (gain = 2^(T/256 − 14), full-scale gain 2, the feedback scaling). Apache-2.0 is compatible with GPL-3.0. The project still writes its own code and keeps only the numbers and laws, each cited. |
| MRG-EG | *The Yamaha DX7 Envelope Generator*, parts 1–4. https://mrg-website.onrender.com/notes/dx7eg1 (…/dx7eg2, …/dx7eg3, …/dx7eg4) | verify | Independent measurements of a real DX7: decays are exponential, rises are not, and the level scale is nonlinear. A cross-check for LEVIEN-EG. |
| KVR-DIFF | KVR thread listing audible DX7/Dexed differences. https://www.kvraudio.com/forum/viewtopic.php?p=6415112 | context | A checklist of behaviours to examine, such as the time to reach sustain and the beating of OP3/OP4. |
| VDX7 | chiaccona, *VDX7* (GPL-3.0; runs the user's own firmware ROM). https://github.com/chiaccona/VDX7 | verify | Optional local oracle for *behavioural* timing only (EG and LFO timings, voice allocation), never for audio. Using it needs a ROM the user owns (DESIGN §14). |

## 2. FM theory

| Key | Source | Status | What is taken |
| --- | --- | --- | --- |
| CHOWNING | J. M. Chowning, "The Synthesis of Complex Audio Spectra by Means of Frequency Modulation", *JAES* 21(7), 526–534, 1973. https://web.eecs.umich.edu/~fessler/course/100/misc/chowning-73-tso.pdf | method | Bessel sideband expansion sin(ωc t + I sin ωm t) = Σ Jn(I) sin((ωc + nωm)t). |
| TOMISAWA | N. Tomisawa (Nippon Gakki / Yamaha), US 4,249,447, *Tone production method for an electronic musical instrument*, granted 1981-02-10. https://patents.google.com/patent/US4249447A/en | spec | Feedback FM stated as the implicit address y = x + β sin y, and multi-unit loops. This grounds MATH §9.2's ideal (Kepler's equation) in the inventor's own formulation. |
| CARSON | Carson's bandwidth rule, B ≈ 2(Δf + fm), about 98 % of the power. https://en.wikipedia.org/wiki/Carson_bandwidth_rule (J. R. Carson, *Proc. IRE* 10(1), 1922) | method | A cheap per-voice bandwidth estimate for adaptive oversampling (MATH §11.3). |
| NIELSEN-FM | K. Nielsen, "Practical Linear and Exponential Frequency Modulation for Digital Music Synthesis", DAFx-20. https://dafx.de/paper-archive/details/IK3Kn6jQ6HB-lcUtx4BE4g | context | Linear vs through-zero FM vs PM, and a review of antialiasing options. |

## 3. Feedback operator: Kepler's equation, Bessel and Kapteyn series

| Key | Source | Status | What is taken |
| --- | --- | --- | --- |
| KEPLER | Kepler's equation and its Bessel series. https://en.wikipedia.org/wiki/Kepler%27s_equation | method | θ − β sin θ = φ is unique for β < 1. E − M = Σ (2/n) Jn(nβ) sin nM. |
| KAPTEYN | Kapteyn series. https://en.wikipedia.org/wiki/Kapteyn_series ; Watson, *A Treatise on the Theory of Bessel Functions*, ch. XVII | method | sin E = (2/e) Σ Jn(ne)/n · sin nM (also eq. B9 of https://arxiv.org/pdf/gr-qc/0005044). This is the band-limited form of the ideal feedback operator. |
| DLMF-10.14 | NIST DLMF §10.14, Kapteyn's inequality (10.14.8). https://dlmf.nist.gov/10.14 | method | Rigorous bound \|Jn(nz)\| ≤ zⁿ exp(n√(1−z²)) / (1+√(1−z²))ⁿ, used to truncate the series. |
| DLMF-10.9 | NIST DLMF §10.9, Bessel's integral Jn(x) = (1/π)∫₀^π cos(nτ − x sin τ) dτ. https://dlmf.nist.gov/10.9 | method | An independent route to the coefficients, used to cross-check MATH §9.5's DFT in tests (small n only). |
| TW-TRAP | L. N. Trefethen, J. A. C. Weideman, "The Exponentially Convergent Trapezoidal Rule", *SIAM Review* 56(3), 2014. https://people.maths.ox.ac.uk/trefethen/sirev56-3_385.pdf | method | The trapezoidal rule (equivalently the DFT) on periodic analytic integrands converges geometrically. This justifies computing the Kapteyn coefficients as a DFT of the Kepler solution (MATH §9.5). |
| BOOST-TRAP | Boost.Math, *Trapezoidal quadrature*. https://www.boost.org/doc/libs/1_67_0/libs/math/doc/html/math_toolkit/trapezoidal.html | context | Bessel's integral by trapezoidal sums becomes ill-conditioned for large n. This is why MATH §9.5 takes the DFT of sin Θ instead. |
| MARKLEY | F. L. Markley, "Kepler Equation Solver", *Celest. Mech. Dyn. Astr.* 63, 101–111, 1995. https://ntrs.nasa.gov/citations/19950021346 | method | Starting value and robust iteration for θ − β sin θ = φ. |
| ODELL-GOODING | A. W. Odell, R. H. Gooding, "Procedures for solving Kepler's equation", *Celest. Mech.* 38, 1986. https://doi.org/10.1007/BF01238923 | method | Safeguarded Newton/Halley, starting formulae, rounding analysis. |
| KEPLER-STARTERS | "Improved First Estimates to the Solution of Kepler's Equation". https://arxiv.org/pdf/2108.03215 | method | A comparison of starting guesses; reference for convergence counts. |

## 4. Antialiasing

| Key | Source | Status | What is taken |
| --- | --- | --- | --- |
| PARKER-ADAA | J. D. Parker, V. Zavalishin, E. Le Bivic, "Reducing the Aliasing of Nonlinear Waveshaping Using Continuous-Time Convolution", DAFx-16. https://dafx.de/paper-archive/details/vem_XXF5qBbfiWOH2RVVAA | method | First-order ADAA: piecewise-linear input, rectangular kernel. |
| BILBAO-ADAA | S. Bilbao, F. Esqueda, J. D. Parker, V. Välimäki, "Antiderivative Antialiasing for Memoryless Nonlinearities", *IEEE SPL* 24(7), 2017. https://doi.org/10.1109/LSP.2017.2675541 (open: https://aaltodoc.aalto.fi/items/470aab15-1702-4ccf-a148-24e6173079fb) | method | Higher orders; second order with a triangular kernel. |
| HOLTERS-ADAA | M. Holters, "Antiderivative Antialiasing for Stateful Systems", DAFx-19 / *Appl. Sci.* 10(1), 20, 2020. https://doi.org/10.3390/app10010020 | method / context | Why ADAA's delay breaks feedback loops, and how it can be fused. Here feedback loops use the implicit solver instead (MATH §9). |
| WERNER-ADAA | K. J. Werner, "An Equivalent Circuit Interpretation of Antiderivative Antialiasing", DAFx-20. https://dafx2020.mdw.ac.at/proceedings/papers/DAFx20in21_paper_42.pdf | method | The ADAA-1/ADAA-2 delay equivalents (1+z⁻¹)/2 and (1+z⁻¹+z⁻²)/3, used for delay alignment (MATH §10.4). |
| ZB-INTERP | V. Zheleznov, S. Bilbao, "Interpolation Filters for Antiderivative Antialiasing", DAFx-24. https://dafx.de/paper-archive/details/VEph91o4UTFBAN6Fha2z_A | method (later) | Cubic interpolation kernels for ADAA, a candidate for when the phase bends between samples (MATH §10.6). |
| GABRIELLI-POLYADAA | L. Gabrielli, S. Squartini, "PolyADAA", DAFx-26. https://www.dafx.de/paper-archive/2026/papers/DAFx26_paper_47.pdf | context | Higher-order Lagrange interpolation in ADAA. |
| VALENZUELA | R. A. Valenzuela, A. G. Constantinides, "Digital signal processing schemes for efficient interpolation and decimation", *IEE Proc. G* 130(6), 225–235, 1983 | method | Two-path polyphase allpass (elliptic) halfband filters for ×2 decimation. |
| HIIR | L. de Soras, *HIIR* (polyphase IIR halfband library; WTFPL). http://ldesoras.free.fr/prod.html#src_hiir | method | Reference implementation and coefficient design for VALENZUELA. Coefficients are computed independently and compared against it. |

## 5. Elementary functions and floating point

| Key | Source | Status | What is taken |
| --- | --- | --- | --- |
| CORE-MATH | A. Sibidanov, P. Zimmermann et al., *CORE-MATH* (MIT): correctly rounded binary64 functions incl. `sinpi`, `cospi`, `exp2`. https://core-math.gitlabpages.inria.fr/ | method (tests) | The oracle the project's polynomials are tested against, linked into tests only. |
| SOLLYA | *Sollya*: certified polynomial approximation and error bounds. https://www.sollya.org/ | method | Designing minimax coefficients for sinpi/cospi/exp2 and certifying their error. Scripts go in `scripts/`; the coefficients and their certified bounds go in code comments. |
| LEVIEN-SINE | R. Levien, *SinePoly* (msfa wiki). https://github.com/google/music-synthesizer-for-android/blob/master/wiki/SinePoly.wiki | context | The shape of polynomial error matters spectrally. Here the error is pushed below binary64 noise instead. |
| HIGHAM | N. J. Higham, *Accuracy and Stability of Numerical Algorithms*, 2nd ed., SIAM, 2002 | method | γₙ bounds for summation, used in test tolerances (as JPEG-Unround's `docs/math.md` does). |
| RUST-SETCSR | Rust `core::arch::x86_64::_mm_setcsr` documentation (deprecated). https://doc.rust-lang.org/core/arch/x86/fn._mm_setcsr.html | spec | Changing MXCSR (FTZ/DAZ/rounding) outside one asm block is immediate UB in Rust. The project avoids denormals structurally (DESIGN §9.4). |
| DENORMAL | Background on subnormal stalls in audio. https://www.kvraudio.com/forum/viewtopic.php?p=8314064 | context | Why subnormals must not reach hot loops. |

## 6. Plugin formats and hosting

| Key | Source | Status | What is taken |
| --- | --- | --- | --- |
| CLAP | free-audio, *CLAP* (MIT): headers and documentation. https://github.com/free-audio/clap | spec | The plugin ABI the Rust side implements by hand (entry, factory, plugin, params, state, note-ports, audio-ports with 64-bit support, gui, latency, tail, voice-info, note-name, thread-check). |
| CLAP-WRAPPER | free-audio, *clap-wrapper* (MIT): CLAP → VST3, AUv2, standalone. https://github.com/free-audio/clap-wrapper (wiki: https://github.com/free-audio/clap-wrapper/wiki) | method | Builds the VST3 and AUv2 plugins and the standalone program from the same CLAP code (its "clap-first" workflow). |
| VST3-MIT | Steinberg, VST 3 SDK 3.8 under the MIT license. https://steinbergmedia.github.io/vst3_dev_portal/pages/VST+3+Licensing/VST3+License | spec | Licensing: GPL-compatible; the trademark rules apply only if the logo or name is used. |
| VST3-C-API | Steinberg, *VST 3 C API*. https://github.com/steinbergmedia/vst3_c_api | context | Alternative to clap-wrapper: implement VST3 directly in Rust. Not taken for now (DESIGN §7). |
| AUSDK | Apple, *AudioUnitSDK* (Apache-2.0). https://github.com/apple/AudioUnitSDK | method | Needed by clap-wrapper's AUv2 target. |
| CLAP-WAYLAND | Discussion: no native Wayland embedding of plugin windows. https://www.kvraudio.com/forum/viewtopic.php?p=8333576 ; https://gitlab.freedesktop.org/wayland/wayland-protocols/-/issues/103 | context | Why Linux embedding is X11 and Wayland is floating-only (DESIGN §8). |
| QT-IN-PLUGINS | Qt forum and Ardour tracker on GUI toolkits inside plugins. https://forum.qt.io/post/356223 ; https://tracker.ardour.org/view.php?id=6949 | context | Why no desktop toolkit (Qt/GTK) runs inside the plugin process. |
| BASEVIEW | RustAudio, *baseview*. https://github.com/RustAudio/baseview | context | Prior art for per-OS plugin windows in Rust (not taken as a dependency). |
| RTSAN | LLVM, *RealtimeSanitizer*. https://clang.llvm.org/docs/RealtimeSanitizer.html ; Rust wrapper https://github.com/realtime-sanitizer/rtsan-standalone-rs ; upstream Rust work https://github.com/rust-lang/rust/pull/147935 | method | Checking that the audio thread never allocates, locks or makes system calls. |
| RUST-ASM | The Rust Reference, *Inline assembly*. https://doc.rust-lang.org/reference/inline-assembly.html ; *Stabilizing naked functions* (Rust 1.88). https://blog.rust-lang.org/2025/07/03/stabilizing-naked-functions | spec | The assembly policy (DESIGN §10). |
| CARGO-FUZZ | *Rust Fuzz Book* / cargo-fuzz (libFuzzer; nightly toolchain). https://rust-fuzz.github.io/book/ | method | Coverage-guided fuzzing of the parsers and the processor, in a crate outside the workspace. |

## 6a. Rendering, GPU and editor design (DESIGN §8, UI.md)

| Key | Source | Status | What is taken |
| --- | --- | --- | --- |
| MS-FLIP-CHILD | Microsoft, *Direct3D 9Ex improvements* (Flip Mode Present, child windows). https://learn.microsoft.com/en-us/windows/win32/direct3darticles/direct3d-9ex-improvements ; *IDXGIFactory2::CreateSwapChainForHwnd*. https://learn.microsoft.com/en-us/windows/win32/api/dxgi1_2/nf-dxgi1_2-idxgifactory2-createswapchainforhwnd ; *Swap chains* (D3D12 accepts only flip effects). https://learn.microsoft.com/windows/win32/direct3d12/swap-chains | method | Flip-model presentation to our own child `HWND`, never mixed with blt presentation in that window; one flip swap chain per `HWND` at a time, so the old one is released before resizing to a new one. |
| DXGI-SWAPCHAINS | C. Walbourn, *The Care and Feeding of Modern Swap Chains*. https://walbourn.github.io/care-and-feeding-of-modern-swapchains | method | `FLIP_DISCARD`, the back-buffer formats flip chains accept, and resize handling for `d3d11.rs`. |
| APPLE-GL-DEPRECATED | Apple, *OpenGL Programming Guide for Mac* (retired; "OpenGL was deprecated in macOS 10.14"). https://developer.apple.com/library/archive/documentation/GraphicsImaging/Conceptual/OpenGL-MacProgGuide/opengl_rev_hx/opengl_rev_hx.html | context | Why macOS uses Metal and has no GL backend. |
| GL-IN-PLUGINS | Reports of plugins that leave their GL context current or corrupt the host's: Native Instruments community thread on Kontakt 8. https://community.native-instruments.com/discussion/46831/kontakt-8-corrupts-the-opengl-context-upon-instantiation ; JUCE forum threads on GL editors in hosts. https://forum.juce.com/t/opengl-in-audio-plugins-current-state/17190 | context | Part of why there is no OpenGL backend: a GL plugin must save and restore the host's current context around every call. |
| MESA-HASVK | Mesa MR 18208, "intel: split vulkan driver between gfx7/8 and above". https://gitlab.freedesktop.org/mesa/mesa/-/merge_requests/18208 ; distribution reports of the split (Arch FS#76724). https://bugs.archlinux.org/task/76724 | context | Why some Haswell/Broadwell Linux machines have no Vulkan unless `intel_hasvk` is built; they fall back to `cpu.rs`. |
| CLANG-FP-CONTRACT | LLVM discourse, "fp-contract=fast and pragmas" (origin of `fast-honor-pragmas`). https://discourse.llvm.org/t/fp-contract-fast-and-pragmas/58529 ; Clang docs commit on `-ffp-contract` and standard compliance. https://lists.llvm.org/pipermail/cfe-commits/Week-of-Mon-20250217/677093.html ; musl discussion of Clang's default contraction. https://openwall.com/lists/musl/2025/08/17/4 | method | The C/C++ contraction policy (DESIGN §13.1). |
| YAMAHA-DX7-DESIGN | Yamaha Design Laboratory, *Design Insights: DX7* (Japanese). https://www.yamaha.com/ja/tech-design/design/insights/id_009/ | context | The design principle the editor follows (UI.md §6a): a square, knob-free form; flat switches; values shown as numbers on a display, presented as precise and reproducible controls; one vivid accent colour. No artwork or exact colour is taken. |
| VST3-SCALE | Steinberg forum, "IPlugView ContentScaleSupport and macOS". https://forums.steinberg.net/t/iplugview-contentscalesupport-and-macos/930318 | method | Plugin view sizes: logical points on macOS, physical pixels on Windows and Linux (UI.md §2.5). |
| HIDPI-BUGS | Vital forum: CLAP UI fills half the window on HiDPI macOS. https://forum.vital.audio/t/1-5-1-clap-synth-ui-only-fills-half-the-window-on-hidpi-in-bitwig-macos/10162 ; Dexed issue 413 (CLAP editor does not scale). https://github.com/asb2m10/dexed/issues/413 | context | The failure the platform layer's single logical/physical conversion prevents; test cases for M8a. |
| FREETYPE-CVE | CVE-2025-27363: FreeType ≤ 2.13.0, out-of-bounds write when parsing TrueType GX / variable font subglyphs; in CISA's Known Exploited Vulnerabilities catalogue. https://www.cisa.gov/known-exploited-vulnerabilities-catalog?field_cve=CVE-2025-27363 ; https://thehackernews.com/2025/03/meta-warns-of-freetype-vulnerability.html | context | Why external font files are not loaded (UI.md §6b.3); pin a patched FreeType. |
| WCAG22 | W3C, *Web Content Accessibility Guidelines 2.2*. https://www.w3.org/TR/WCAG22/ — SC 1.4.1 Use of Color, 1.4.3 Contrast (Minimum), 1.4.11 Non-text Contrast, 2.5.7 Dragging Movements, 2.5.8 Target Size (Minimum). | method | Applied to a desktop editor as design rules (UI.md §6): colour never the only cue; 4.5:1 text and 3:1 control contrast; every drag has a single-pointer alternative; targets ≥ 24 × 24 logical px. |
| SUGIYAMA | K. Sugiyama, S. Tagawa, M. Toda, "Methods for Visual Understanding of Hierarchical System Structures," *IEEE Trans. SMC* 11(2):109–125, 1981. https://doi.org/10.1109/TSMC.1981.4308636 | method | The layered layout of algorithm graphs: cycle removal, layering, crossing reduction, coordinates (UI.md §3.4). |
| EADES-FAS | P. Eades, X. Lin, W. F. Smyth, "A fast and effective heuristic for the feedback arc set problem," *Inf. Process. Lett.* 47(6):319–323, 1993. https://doi.org/10.1016/0020-0190(93)90079-O | method | Choosing which edges of a cyclic user graph to draw upward (as feedback). |
| BRANDES-KOEPF | U. Brandes, B. Köpf, "Fast and Simple Horizontal Coordinate Assignment," *Graph Drawing 2001*, LNCS 2265, pp. 31–44, 2002. https://doi.org/10.1007/3-540-45848-4_3 ; Erratum: U. Brandes, J. Walter, J. Zink, 2020. https://arxiv.org/abs/2008.01252 | method | Horizontal coordinates in the layered layout. Implement the corrected algorithm from the erratum. |
| HALION-FMLAB | Steinberg, *FM Lab — Algorithm Editor Page*. https://steinberg.help/fm_lab/v1/en/halion/topics/included_instruments/fm_lab_synth_page_algorithm_editor_page_r.html | context | Prior art: freely connected operators; click selects, double-click edits. |
| KORG-OPSIX | Korg, *opsix Owner's Manual* (USER ALG; the operator mixer of ratio knobs and level sliders). https://www.manualslib.com/manual/2053292/Korg-Opsix.html?page=67 | context | Prior art: user algorithms; six ratio/level controls always in reach; role shown by colour (carrier red, modulator blue). |
| AV-DEXED-UI | AV Vibes, AV Dexed README and screenshots (Play / Advanced views). https://github.com/AVVibesDSP/av-dexed | context | Prior art: algorithm in the centre with live operator activity; real-folder library; the Advanced view's six identical operator panels (UI.md §1). |

## 7. Tuning

| Key | Source | Status | What is taken |
| --- | --- | --- | --- |
| SCALA-SCL | Huygens-Fokker Foundation, *Scala scale file format*. https://www.huygens-fokker.org/scala/scl_format.html | spec | The `.scl` parser. |
| SCALA-KBM | Huygens-Fokker Foundation, *Keyboard mappings*. https://www.huygens-fokker.org/scala/help.htm#mappings | spec | The `.kbm` parser. |
| MTS-ESP | ODDSound, *MTS-ESP* (client: 0BSD; loads the closed `libMTS` at run time). https://github.com/ODDSound/MTS-ESP | method (optional) | Dynamic microtuning. Off by default. |

## 8. Prior art compared (DESIGN §2)

| Key | Source | Status |
| --- | --- | --- |
| DEXED | P. Gauthier et al., *Dexed* (GPL-3.0). https://github.com/asb2m10/dexed | context |
| AV-DEXED | AV Vibes, *AV Dexed* (GPL-3.0). https://github.com/AVVibesDSP/av-dexed | context |
| PLOGUE-OPS7 | Plogue, *chipsynth OPS7* (proprietary, bit-accurate). https://www.plogue.com/products/chipsynth-ops7.html | context |
| RETROMULATOR | discoDSP, *Retromulator* (GPL-3.0, wraps VDX7). https://www.discodsp.com/retromulator/ | context |

## 9. The author's own repositories this project follows

| Key | Source | What is taken |
| --- | --- | --- |
| KIRAL | https://github.com/yumasansansan/kiral | Workspace layout, lints, the `-cpu` / `-sys` / `-kernels` crate split, runtime dispatch with `*_ISA` / `*_KERNELS=portable` overrides, mutation tests with SplitMix64 and `fuzz/regressions/`, `ci/` scripts, pinned LLVM, REUSE. |
| JPEG-UNROUND | https://github.com/yumasansansan/JPEG-Unround | `docs/math.md` as "the one statement of the model"; binary64 policy, FMA allowed, fixed summation order, tolerances from rounding bounds; C23/C++26 settings; `conformance/`. |
| ADLPLUG-NEXT | https://github.com/yumasansansan/ADLplug-Next | Processor/worker/editor split with FIFOs; try-lock rule on the audio thread; the RealtimeSanitizer canaries; libFuzzer targets with dictionaries, seeds and regressions; sanitizer CI; plugin packaging. |
