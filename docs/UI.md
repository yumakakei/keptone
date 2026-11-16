<!--
SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
SPDX-License-Identifier: GPL-3.0-or-later
-->

# The editor

This document specifies what the editor shows and how it behaves. The architecture under it (windows, scene, renderers, GPU backends) is DESIGN §8. The values it displays come from `docs/MATH.md`. Where this document and MATH disagree about a value, MATH wins and this document is fixed.

## 0. Principles

1. **The algorithm is the centre.** An FM patch is a graph of operators, and most of what a parameter does depends on the operator's place in that graph. The graph is always visible, and it is the way into every operator.
2. **Show physical meaning next to every DX7 number.** The 0–99 values stay (they are what banks and players know), but each one is shown with what it does, computed from MATH. Examples:
   - output level 99 → 0 dB as a carrier, peak modulation index 4π rad as a modulator;
   - coarse 2, fine 5 → ratio 2.10, 924 Hz at A4;
   - EG rates → segment times in seconds;
   - FB 6 → β = 3.14, with the regime shown (MATH §9).

   This is the largest single gain in clarity over Dexed and AV Dexed, which show only the raw numbers.
3. **Compare, don't hide.** FM sound design is relational: a modulator's level matters relative to its carrier's ratio, and an operator's envelope relative to the others'. Nothing that edits one operator may hide all the others (see §2.3 on dialogs).
4. **Direct manipulation, with an exact alternative.** Every drag (a knob, an envelope point, an edge in the graph) has a precise keyboard or typed alternative and a single-click alternative ([WCAG22] SC 2.5.7).
5. **Nothing is lost.** Every edit is undoable, including graph edits, bank loads and randomisation. A destructive action (overwrite a bank file, delete a folder) asks first and names what it will change.
6. **Freedom by settings, defaults for the main use.** Anything that changes behaviour is a setting with a visible default and a reset. The editor warns about unusual choices (a silent operator, a chaotic feedback loop) but never forbids them.
7. **Live.** What the engine is doing is shown where it applies: operator activity in the graph nodes, the envelope position on the envelope plot, the output scope and spectrum. All of it comes from the telemetry ring (DESIGN §5). The editor never reads engine state directly.

## 1. What AV Dexed does well, and what this editor changes

AV Dexed's Play view puts the algorithm in the centre and lights each operator while it sounds, and clicking an operator opens it in the Advanced view. Its library is real folders on disk. This editor keeps all three ideas.

| AV Dexed | Here | Why |
| --- | --- | --- |
| Two views, Play and Advanced; editing means switching | One view. Graph and inspector are always on screen; the library is a drawer | No mode switch between hearing and editing |
| Advanced: six identical operator panels of knobs, 0–99 values only | Inspector for the selected operators + an operator table with one row each | Shows physical values; the table makes comparison a glance |
| Envelope shown as a small static shape | Envelope plot computed from MATH §4 closed forms, time axis in seconds, draggable points, live position | The DX7's rate/level pairs are notoriously hard to read |
| Algorithm chosen from 32 | 32 presets **and** a graph editable by drag and drop (MATH §8.4) | Requested; prior art: HALION-FMLAB, KORG-OPSIX |
| Feedback: one number | Number + β + regime badge on the loop edge | FB 5–7 are qualitatively different (MATH §9) |

## 2. Layout

### 2.1 Main view

Default size 1200 × 760 logical px. Minimum 880 × 560. Freely resizable; the layout reflows (§2.4).

```
┌──────────────────────────────────────────────────────────────────────────────┐
│ ☰ Library │ ◀ 21 BRS T12 ▶  ★  │ A│B │ ↶ ↷ │ Algorithm 18 ▾ │ ⚙ │ CPU · Voices │  header
├───────────┬──────────────────────────────────────────┬───────────────────────┤
│ (drawer,  │                                          │ INSPECTOR             │
│  closed   │            GRAPH CANVAS                  │ Operator 3   [⚙ pin]  │
│  by       │                                          │ ─ Frequency ───────── │
│  default) │        ┌───┐                             │  ratio 2.10  924 Hz   │
│           │        │ 6 │◯ FB 7  β 6.28 ⚠ chaotic     │  coarse 2  fine 5 …   │
│           │        └─┬─┘                             │ ─ Level ───────────── │
│           │        ┌─┴─┐                             │  99 → index 4π rad    │
│           │  ┌───┐ │ 5 │                             │ ─ Envelope ────────── │
│           │  │ 2 │ └─┬─┘ ┌───┐                       │  [plot, seconds]      │
│           │  └─┬─┘   │   │ 3 │                       │ ─ Keyboard scaling ── │
│           │    │   ┌─┴─┐ └─┬─┘                       │  [curve over keys]    │
│           │    │   │ 4 │   │                         │ ─ Sensitivity ─────── │
│           │    │   └─┬─┘   │                         │  velocity · AM · rate │
│           │  ══╧═══╤═╧═════╧══  OUT                  │                       │
├───────────┴────────┴─────────────────────────────────┴───────────────────────┤
│ OPERATORS  # on role   ratio   Hz@A4  level  dB/index  EG ▁▃▇▅▂  live         │  operator
│            1  ●  C     1.00    440   99     0 dB      ▁▇▆▅▁   ███            │  table
│            2  ●  C     2.10    924   85    −12 dB     ▁▇▇▇▁   ██             │
│            3  ●  M→1   1.00    440   79    1.9 rad    ...                    │
├──────────────────────────────────────────────────────────────────────────────┤
│ Pitch EG │ LFO │ Controllers │ Voice │ Tuning │ Output: scope · spectrum     │  tabbed
└──────────────────────────────────────────────────────────────────────────────┘  global strip
```

Regions:

- **Header:** patch name with previous/next and favourite; A/B compare slots; undo/redo; the algorithm picker (§3.6); settings; CPU and voice count.
- **Library drawer** (§5): closed by default, opens over the left edge, can be pinned open.
- **Graph canvas** (§3).
- **Inspector** (§4): docked on the right. It shows the selection.
- **Operator table** (§4.4): one row per operator, always visible.
- **Global strip:** tabs for Pitch EG, LFO, Controllers (wheel, foot, breath, aftertouch, their ranges and destinations), Voice (poly/mono, portamento, transpose, voices), Tuning, and the output scope and spectrum. The strip can be collapsed to its tab row.

A virtual keyboard is off by default (`ui.keyboard = off | bottom`). In a DAW the user has a controller or a piano roll. The standalone build turns it on.

### 2.2 Selection, the docked inspector and operator panels

- **Click a node** (or its row in the table): select it. The docked inspector shows it.
- **Shift/Ctrl-click:** multi-select. The inspector shows the shared parameters. A field whose values differ shows "—" and edits all selected operators. Dragging a knob edits relatively (each keeps its offset), and typing sets them all to the same value.
- **Double-click a node, or click its ⚙ button:** open that operator's **panel** (§2.3). This is the "settings button on each node" idea. Clicks on the node body stay free for selecting, dragging and connecting.
- **Click empty canvas:** select nothing; the inspector shows the patch's global parameters.

### 2.3 Operator panels are in-editor and non-modal

A panel is a floating box drawn by our renderer inside the editor window:

- it shows the same sections as the inspector for one operator;
- it is non-modal: the graph, the table and other panels stay usable;
- several can be open at once, so operators 5 and 6 can be edited side by side;
- it can be dragged, collapsed to its header, and closed with Esc or ×;
- its position is remembered per operator for the session.

Two things a dialog would usually be, a panel is not:

1. **Not an OS window.** A second top-level window from a plugin fights the host over focus, z-order and keyboard routing, and an embedded editor under Wayland cannot open one at all (DESIGN §8.1).
2. **Not modal.** A modal dialog hides the comparison between operators that FM editing depends on (§0.3), and modal dialogs in hosts often swallow or leak key events.

The docked inspector stays as well. It follows the selection, so it suits keyboard work and quick tweaks. Panels suit side-by-side comparison. Settings:

- `ui.inspector.docked = on | off` (default on);
- `ui.node_double_click = panel | inspector` (default panel; `inspector` only moves focus into the docked inspector).

### 2.4 Sizes and reflow

- Below 1100 px wide the inspector overlays the canvas from the right instead of sitting beside it. Closing it is one click or Esc.
- Below 680 px tall the operator table collapses to the selected rows only, and the global strip collapses to its tabs.
- `ui.scale` (50–400 %) multiplies all of the above on top of the system scale (DESIGN §8.4). The host is told the new size through CLAP `gui` and the wrappers.

### 2.5 Scale and the device-pixel grid

High-DPI is the harder sizing problem: Full HD at 100 %, 4K at 150 % or 200 % on Windows and Linux, and a Mac with a 4K display in a scaled mode. The editor is laid out in logical pixels and drawn at

$$s = s_{\text{system}} \times \texttt{ui.scale}$$

device pixels per logical pixel. `ui.scale` is 50–400 %, default 100 %.

**Where $s_{\text{system}}$ comes from:**

| System | Source | Notes |
| --- | --- | --- |
| Windows | per-monitor DPI awareness v2; `WM_DPICHANGED`; the host's CLAP `gui.set_scale` or VST3 content-scale call | Sizes exchanged with the host are in physical pixels [VST3-SCALE] |
| macOS | `backingScaleFactor` of the view's window; `viewDidChangeBackingProperties` | Sizes exchanged with the host are in logical points [VST3-SCALE]. The scale is 2 on any HiDPI mode. In a "looks like 2560 × 1440" mode on a 4K display, the system renders at 5120 × 2880 and downsamples to 3840 × 2160. The editor cannot see that step, so it just draws sharply at 2. |
| X11 | `Xft/DPI` from XSETTINGS, else `Xft.dpi` from the resource database, else 96 dpi | Many desktops leave this at 96 on 4K. Then a one-time notice offers 150 % or 200 % from the monitor's RandR physical size. The notice never applies a scale by itself. |
| Wayland | `wp_fractional_scale_v1` + `wp_viewporter` (floating window only) | Integer `wl_surface` scale as the fallback |

Mixing up logical and physical sizes between systems is the classic HiDPI plugin bug: a CLAP editor drawn at half its window on HiDPI macOS, or one that ignores Windows' 200 %. Several shipped synths have had it [HIDPI-BUGS]. The platform layer converts once, at the boundary, and nothing above it sees physical sizes.

**Drawing on the device grid.** Fractional scales (125 %, 150 %, 175 %) blur naively scaled UIs. The rules:

- Layout happens in logical units. Before drawing, every rectangle edge is rounded to a device pixel: $x_{\text{dev}} = \operatorname{round}(x \cdot s)$, for both edges independently. A 1 px gap never becomes 0 or 2 px apart from rounding.
- A stroke of logical width $w$ is $\max(1, \operatorname{round}(w s))$ device pixels wide. It is placed so that it covers whole pixels: centred on a pixel centre when that width is odd, on a pixel edge when it is even. Hairlines are exactly one device pixel at every scale.
- Text is rasterised by FreeType at $\text{size} \times s$ device pixels, with light (vertical-only) hinting, and the glyph atlas is rebuilt per scale. Glyph advances stay unhinted, so a line of text keeps the same length across scales.
- Curves (envelopes, edges in the graph, plots) are not snapped. They are antialiased by analytic coverage (DESIGN §8.3).
- There are no bitmaps. Icons are paths, so they are exact at every scale.

**Changing scale live.** When the window moves between a Full HD and a 4K monitor:

- the editor re-lays out at the new $s$;
- it rebuilds the glyph atlas;
- it asks the host for the new physical size (CLAP `gui.request_resize`; the wrappers' equivalents) so the logical size stays the same.

Nothing is cached at the old scale.

**Windows hosts that are not DPI-aware.** Windows then stretches the host's windows, ours included, as a bitmap. A child window can differ from its parent only if the host opted into mixed-mode DPI hosting (`SetThreadDpiHostingBehavior`). The editor detects the thread's awareness context. If the host is unaware, it renders at the host's DPI, and `ui.scale` is the user's way to compensate. The editor states this in its about box rather than guessing.

**Sizes:**

- At 4K and 150 %, 1200 × 760 logical is 1800 × 1140 physical.
- At 4K and 100 % (unscaled Linux), 1200 × 760 is small. That is what the X11 notice above is for.
- Text is never below 11 logical px.
- Line weights are 1 or 2 logical px.
- Targets are at least 24 logical px.

## 3. The graph canvas

### 3.1 What is drawn

- **Nodes.** A node is a rounded rectangle with:
  - the operator number, large;
  - the frequency: the ratio (e.g. `×2.10`) or `fixed 924 Hz`;
  - a level bar;
  - an activity glow driven by telemetry (the operator's current gain in the most recent voice; `ui.activity = latest | max | off`);
  - a ⚙ button;
  - an output port at its bottom edge and an input port at its top edge.
- **Roles.** Carriers sit on the output bus; modulators sit above what they modulate. Role is shown by position and by an outline style: solid for carriers, dashed for modulators. Colour is only a third cue ([WCAG22] SC 1.4.1).
- **Operator colours.** Each operator has one hue used everywhere: node, table row, inspector header, envelope traces. Six hues chosen for colour-vision deficiency safety, distinct in both themes. The number is always printed as well.
- **Edges.** Phase-modulation edges are arrows from a modulator's output port to the target's input port. The carrier connection is a short drop to the **output bus**, a horizontal bar at the bottom.
- **Feedback.** A self-loop is drawn as a loop on the node's side, labelled `FB n`, β (at full level and live, MATH §9.2), and a regime badge:
  - none for β < 1 (unique solution);
  - ⚠ for β ≥ 1, with the active `feedback.model`.

  A multi-operator loop (algorithms 4 and 6, or user graphs) is drawn as an edge routed back up the side, with the same label on that edge.
- **Not heard.** An operator with no path to the bus is dimmed and labelled "not heard". It costs no CPU (the compile step drops it) but keeps its parameters.
- **Disabled.** An operator switched off is drawn hollow.

### 3.2 Editing the graph by drag and drop

All of this applies to any graph. Editing one of the 32 presets turns the patch's graph into a user graph (§3.5).

| Action | Pointer | Single-click alternative | Keyboard |
| --- | --- | --- | --- |
| Connect A → B (A modulates B) | drag from A's output port, drop on B | click A's output port, then click B | select A, `C`, then `1`–`6` |
| Make A a carrier | drag from A's output port to the bus | click port, click bus | select A, `O` |
| Add feedback A → A | drag from A's output port back onto A | click port, click A | select A, `F` |
| Remove an edge | drag its head away and drop on empty canvas | click the edge, then 🗑 | select the edge, `Del` |
| Move a node | drag the node body | — (layout only) | arrow keys move by one grid step |
| Swap two operators' places in the graph | drag node A onto node B with `Alt` | context menu "Swap with…" | `S`, then a number |
| Swap two operators' parameters | — | context menu "Swap parameters with…" | `Shift+S`, then a number |
| Copy operator parameters | drag A onto B with `Ctrl` | context menu "Copy parameters to…" | `Ctrl+C` / `Ctrl+V` on a selection |
| Add an operator (user graphs) | `+` button on the canvas | same | `N` |

Rules while dragging:

- Valid targets highlight. Invalid drops (onto the node itself without the feedback gesture, onto empty space) cancel without a change.
- A drop that would add an edge that already exists does nothing.
- Every completed gesture is one undo step.
- The engine receives the new graph only when a gesture completes, never during a drag: compile on the main thread, then hand over through the ownership-moving queue (DESIGN §5).

### 3.3 What the engine does with a new graph mid-note

Sounding voices keep their compiled graph until they end unless `graph.switch = immediate | next-note` (default `next-note`). `immediate` swaps at the next sample with the operators' phases and envelope states kept. It clicks when the change is large, which is the user's choice.

### 3.4 Layout

The 32 preset algorithms are drawn as in the DX7 manual's chart, with node positions stored with the algorithm data (MATH §8.1). A user graph is laid out automatically when it is created or when the user presses "Tidy":

1. Choose the edges to draw upward with Eades–Lin–Smyth's greedy heuristic for a small feedback arc set [EADES-FAS].
2. Assign layers by longest path from the bus, so carriers sit at the bottom.
3. Reduce crossings by the barycentre method, layer by layer, a few sweeps [SUGIYAMA].
4. Assign horizontal coordinates with Brandes–Köpf, using the erratum's corrected algorithm [BRANDES-KOEPF].

At N ≤ 16 nodes this takes microseconds. After that the user can move nodes freely. Positions are stored in the patch's editor data (cosmetic, ignored by the engine and by `.syx` export).

### 3.5 Presets, user graphs and DX7 compatibility

- The header shows **"Algorithm 18"** while the graph equals preset 18 edge for edge, and **"User graph"** otherwise.
- After every graph edit the editor checks the user graph against the 32 presets:
  - exactly, by labelled edges;
  - up to a renumbering of the operators (a graph isomorphism, at most 6! = 720 permutations).

  If a renumbering matches, the header offers **"Matches algorithm 7 if operators are renumbered — renumber?"**. Renumbering moves the parameters with the operators, so the sound is unchanged.
- `.syx` export of a user graph that matches no preset: the export dialog says so, offers the nearest preset (fewest edge edits), and exports nothing until the user picks one or cancels. The native state format always keeps the user graph (DESIGN §11).

### 3.6 The algorithm picker

A grid of the 32 presets as thumbnails, drawn from the same data, with the current one highlighted. Hovering a thumbnail previews it on the canvas (the sound changes only on click; `ui.algorithm_preview_sound = off | on`). The arrow keys and the header's ◀ ▶ step through them. "Keep sound when switching" (default on) keeps each operator's parameters; the alternative is to load a matching factory-style starting point, which needs factory-style content the project may not have (DESIGN §14), so it is off until there is some.

## 4. The inspector

### 4.1 Value controls (everywhere)

Every value is one control type, the **value field**: a label, the DX7 value, the physical value, and a small arc or bar.

| Gesture | Effect |
| --- | --- |
| press and drag vertically | relative change, no popover; `Shift` = fine (×1/10); `Ctrl` = coarse (×10) |
| click (press and release within 3 px) | open the value's **editor popover** (§4.1.1), focus on its main control; typing a digit goes straight to its text field |
| double-click | open the popover with its text field focused and selected |
| `Enter` / `Space` on a focused field | open the popover (keyboard path) |
| wheel | one step; `Shift` = fine |
| right-click | menu: reset to default, reset to the value at load, copy, paste, MIDI learn, show automation in host (CLAP) |
| `Ctrl`-click | reset to default |

#### 4.1.1 The editor popover

The DX7's reputation for unintuitive programming has several causes:

- one shared data-entry slider and paged parameters;
- 0–99 values with no stated meaning;
- an envelope given as rates and levels instead of times.

The inspector already removes the paging and draws the envelope. The popover deals with the rest wherever a number is edited. It is drawn by the editor (no OS window), anchored to the field, and non-modal: the sound keeps playing and every change is heard at once.

Contents, top to bottom:

1. **Main control, chosen by the kind of value:**
   - **Continuous or many-step values** (levels, rates, depths, fine, detune, LFO speed): a wide slider, at least 280 logical px for 0–99.
     - The DX value scale is drawn on one side and the physical scale on the other (dB, rad, Hz, cents, seconds), so nonlinear maps are visible. Example: output levels below 20 are compressed (MATH §3.1), and the dB ticks bunch up there.
     - Snap marks at meaningful points: the default, the detune centre, integer ratios.
     - The slider snaps to them unless `Alt` is held.
   - **A small set of choices** (curve types, LFO waveform, ratio/fixed mode): segmented buttons with icons. A slider over four choices is a worse control.
   - **Coarse ratio:** a grid of the 32 ratios (0.5, 1, 2 … 31). For operators in a modulation chain it shows the resulting carrier/modulator ratio, which is what determines the harmonic or inharmonic character.
2. **Text field** in either unit, as before. The converted DX value is shown before it is applied.
3. **− / +** step buttons, 24 px or larger, auto-repeating while held.
4. **One line on what the value does here**, depending on the operator's role. For example: "Modulator of 1: raises the brightness of operator 1; peak index 1.44 rad."
5. **Before/after:** the value at open, the value now, and the change in physical units.

Behaviour:

- `Enter` or clicking outside commits (one undo step for the whole popover session). `Esc` reverts to the value at open.
- The arrow keys step. `PageUp` and `PageDown` step by 10.
- Host automation gestures begin when the popover opens and end when it closes, so a DAW records one gesture.
- Fractional values (settings that allow them, MATH §17) use a continuous slider. Integers snap.
- With several operators selected, the popover edits all of them (§2.2) and shows each operator's value as a small marker on the slider.

A value changed from the patch's loaded state shows a dot. A host automation gesture is bracketed by begin and end (CLAP `param_gesture_begin/end`).

Where the DX7 value has an exact physical meaning, the physical value is computed by the same `keptone-math` and `keptone-engine` functions the engine uses. The UI never has its own copy of a formula.

### 4.2 Operator sections

1. **Frequency.**
   - Mode (ratio | fixed), coarse, fine, detune.
   - The resulting ratio, and Hz at A4 and at the last note played.
   - Detune in cents (MATH §2).
   - Fixed mode: the frequency in Hz with the coarse/fine decomposition.
2. **Level.**
   - Output level 0–99.
   - The physical value by role:
     - as a carrier: dB at full EG;
     - as a modulator: the peak modulation index in rad;
     - both, if it is both.
3. **Envelope.**
   - A plot of the EG computed from MATH §4 closed forms, with a time axis in seconds. A key-off marker at a chosen hold time (`ui.eg_plot.hold`, default 1 s) or "until sustain".
   - Four rate and four level fields under the plot.
   - Dragging a breakpoint vertically sets the level. Dragging it horizontally sets the rate of the segment that ends there, which moves the following points.
   - Rate scaling is shown as a shaded band (the plot at the lowest and highest key).
   - The live position of the EG of the most recent voice is a moving dot.
4. **Keyboard level scaling.**
   - A curve over a drawn keyboard: break point (draggable on the keyboard), left and right depth and curve type.
   - The y axis is in dB.
5. **Sensitivities.** Velocity (curve of level vs velocity), amplitude modulation, rate scaling, each with its plot.
6. **Extensions** (only when their settings are on): per-edge gain in user graphs, per-operator waveform if MATH ever gains one. Hidden otherwise, so the default editor shows exactly the DX7's parameters.

The EG and scaling plots use the engine's formulas at the current sample rate and settings, so changing, say, `eg.clock_hz` moves them.

### 4.3 Global parameters (nothing selected, or the global strip)

- **Pitch EG:** the same plot and fields as an operator envelope, y in cents.
- **LFO:** wave, speed (Hz), delay (s), PMD, AMD, key sync, with a plotted period.
- **Controllers:** for each of wheel, foot, breath and aftertouch: range and destinations (pitch, amplitude, EG bias).
- **Voice:** poly or mono, voices, portamento mode and time (s), transpose, pitch-bend range and step.
- **Feedback:** the FB value is edited on its loop edge or here, with β and the regime.

### 4.4 The operator table

One row per operator, always visible:

- number and colour;
- on/off;
- role (`C`, `M→3`, or both);
- mode and ratio or fixed Hz;
- Hz at A4;
- level, with dB or rad;
- an EG sparkline;
- a live meter.

The table is a spreadsheet:

- arrow keys move between cells; typing edits;
- `Ctrl+D` fills the selection's value down;
- a column header click selects that parameter on every operator.

The six level and ratio cells are the "operator mixer" of KORG-OPSIX: quick sound shaping without opening anything.

## 5. Library

- A drawer over the left edge:
  - folders mirror folders on disk (as in AV Dexed);
  - search by name, folder and tag;
  - favourites;
  - "same voice" de-duplication by the SHA-256 of the canonical encoding (DESIGN §11.2);
  - a "duplicates" view.
- **Drop** `.syx`, native patches or folders onto the editor to import, with a preview listing what will be added and where. Nothing is overwritten without a confirmation naming the file.
- **Compare:**
  - A/B slots in the header;
  - "diff with…" shows a table of every parameter that differs between two voices, with both values and a button to take either side per row.
- **Randomise and mutate** (KORG-OPSIX prior art): a per-parameter lock (padlock in each value field's menu), and an amount. Both are undoable. Default off in the header (`ui.show_randomise`).

## 6. Accessibility, text and theme

Rules taken from [WCAG22] and applied to the editor:

- Text contrast ≥ 4.5:1, and ≥ 3:1 for controls and graph edges, in both themes. A test checks the theme tokens.
- Targets at least 24 × 24 logical px (SC 2.5.8). The node ports are drawn smaller but their hit areas are 24 px.
- Every drag has a single-pointer alternative (§3.2, SC 2.5.7) and a keyboard path. The whole editor is usable without a pointer: `Tab` cycles regions, arrows move within one, `Enter` edits, `Esc` leaves.
- Colour is never the only cue (§3.1).
- `ui.reduce_motion`: activity glows become static indicators; no animated transitions.
- Themes: dark (default) and light. `ui.theme = dark | light | system`.
- Languages: English and Japanese at first (`ui.language`), strings in one table per language. Text is shaped with HarfBuzz, so Japanese renders correctly. A Noto Sans CJK subset (OFL) is bundled; the license is recorded in DEPENDENCIES.md.
- No platform accessibility tree at first (DESIGN §8). This is a known gap with an issue: AccessKit-style integration per OS later.

## 6a. Visual direction

The DX7's product design ([YAMAHA-DX7-DESIGN]) set out to show at a glance that it was a new, digital instrument, and it did so with three choices:

1. **A square, simple form.** It was meant as a clear contrast with the knob-covered analogue synthesizers of 1983.
2. **No three-dimensional controls except the keys.** Flat membrane switches made the controls all-digital.
3. **One vivid colour** on a dark brown body, picked for visibility. That green became a colour for "digital".

The editor takes the principle, not the look. The 2026 equivalent of "no knobs" is to drop skeuomorphic knobs:

- values are flat fields with exact numbers (§4.1) and a thin fill line;
- the graph and the plots are the only pictures;
- one high-contrast accent on a dark, low-chroma ground;
- square geometry;
- numerals in a monospaced face with tabular figures.

No DX7 artwork, logo, panel layout or exact colour is reproduced (DESIGN §14).

Three directions are mocked up at full size with the same layout and the same patch, in `docs/mockups/` (HTML and PNG; its README says what is binding). The patch's levels, dB, modulation indices and β come from MATH §3 and §9. The scope and spectrum come from a render of that patch.

- **A — Membrane.** The closest descendant of the DX7:
  - a warm brown-black ground;
  - one mint accent;
  - 2 px corners, uppercase labels;
  - IBM Plex Sans and IBM Plex Mono.
- **B — City Night.** City-pop at night:
  - a deep indigo ground;
  - a sodium-amber accent, with mint for the output bus;
  - rounded nodes;
  - a skyline-like trace under the graph;
  - Manrope and DM Mono.
- **C — Ideal Compute.** The "cyber" reading of "computed ideally":
  - a near-black ground with a dot grid;
  - a phosphor accent;
  - square corners;
  - full binary64 precision shown where it is cheap (β to 15 digits);
  - the engine's settings in the header;
  - Space Grotesk and JetBrains Mono.

The chosen direction becomes the theme tokens:

- ground, panels, rules, two text levels, accent and accent ink;
- six operator hues;
- radii;
- the two typefaces.

The tokens are tested for contrast (§8). Every theme also gets a light variant, derived from the same tokens.

## 6b. Themes

The three directions of §6a ship as **built-in themes**:

- `membrane` (default);
- `city-night`;
- `ideal-compute`.

Each has a dark variant and a light one. Users can also load their own themes from files.

### 6b.1 What a theme is

A theme is data only. It contains:

- **Colours:** ground, panels, field, canvas, rules, two text levels, accent, accent ink, edges, output bus, node fills and borders (carrier and modulator), the six operator hues, plot fills.
- **Geometry:** corner radius of controls and of nodes; stroke weights (1 or 2 px).
- **Type:** the UI face and the numeral face. Each is chosen **by name from the bundled families** (§6b.3).
- **Style switches:** a fixed, enumerated set.
  - `labels = upper | as-written`;
  - `canvas = plain | dots | skyline`;
  - `numbers = compact | full` (`full` shows β and similar values to binary64 precision);
  - `meters = bar | dot`.

  New switches are added to the enum. A theme can never contain code, layout or images.

The UI reads every colour and size from the theme. A test fails if a colour literal appears in `keptone-ui` outside the built-in theme files.

### 6b.2 Built-in and external themes take the same path

The built-in themes are theme **files**, `assets/themes/*.keptone-theme`, embedded with `include_bytes!`. Those three files are the reference for the key names and value forms. They are parsed by the same parser that loads a user's file. The external path is therefore exercised on every start, and supporting it costs:

- the file format and its parser;
- an "Import theme…" command and drag-and-drop of a theme file;
- a user theme folder;
- an "Export current theme" command, so a user starts from a built-in.

That is a small addition if the UI is built on themes from the start. It would be a large one later, because retrofitting means hunting for hard-coded colours. So external themes are in M8b.

**The format** (`.keptone-theme`):

- The JSON subset of the native state (DESIGN §11.1), parsed by hand, total.
- Size cap 64 KiB.
- `"base": "<built-in name>"`: keys a file leaves out are inherited from that base. A three-line theme that only changes the accent is valid.
- Colours as `#RRGGBB` or `#RRGGBBAA`. Numbers are range-checked. Unknown keys are kept and reported. A file that fails to parse changes nothing and is reported with the line and column.
- A fuzz target, mutation tests, and `fuzz/regressions/theme/`.

**Freedom with warnings:**

- The theme manager computes the contrast of every text, control and edge pair (§6) and lists the failures.
- It does not refuse the theme. A low-contrast theme is the user's choice.
- `ui.theme` is a per-user setting, not saved in projects. A project opened on another machine therefore never forces a theme.

### 6b.3 Fonts

- **Now:** only the bundled families (the six faces of §6a and Noto Sans CJK JP), by name.
- **Not now, external font files.** A font file is complex untrusted input to FreeType, which has had memory-safety bugs exploited in the wild: an out-of-bounds write in versions up to 2.13.0, on TrueType GX and variable fonts [FREETYPE-CVE].
- **Later, possibly:** fonts installed in the operating system, chosen by family name. They are already trusted by the system, and the editor would not open the files itself. Recorded as an issue.

## 7. How `keptone-ui` is built

- **State and view.** The editor state (selection, pins, scroll, drawer, drags in progress) is a plain struct. A frame is a pure function `view(&EditorState, &ModelView, &Telemetry) -> Scene` plus hit regions. Input is turned into `Action`s, and `update(&mut EditorState, Action) -> Vec<Command>` produces commands for the host side (parameter changes, graph changes, file operations). Widgets keep interaction state by stable IDs (immediate-mode style), so there is no retained widget tree to synchronise.
- **Undo.** Every `Command` that changes the patch is recorded with its inverse. A gesture groups its commands into one step. Undo history is per editor session; the host's own undo sees parameter gestures.
- **Text measurement** is cached by (string, font, size).
- **Damage.** The scene is diffed by region against the previous frame; only changed regions are redrawn (DESIGN §8.4).
- **No unsafe** outside the text FFI module. Platform and GPU code lives in `keptone-platform` and `keptone-render`.

## 8. Tests

| What | How |
| --- | --- |
| Value formatting and parsing | every DX7 value → text → value round trip; typed physical values map to the nearest DX7 value; property tests with SplitMix64 |
| Graph editing | for random action sequences: the graph stays well-formed, every SCC is solvable by MATH §9, undo of every step restores the previous graph exactly |
| Preset matching | each of the 32 presets matches itself; each matches under all 720 relabellings when renumbering is allowed; random user graphs never falsely match |
| Layout | no node overlaps; every non-feedback edge points downward; deterministic output for one input |
| Event fuzzing | a cargo-fuzz target feeds arbitrary input event sequences (pointer, keys, resize, scale changes, drops of arbitrary bytes as files) to the editor with a mock host: no panic, no unbounded growth, invariants hold |
| Rendering | golden images of fixed scenes rendered with `cpu.rs`; GPU backends compared with `cpu.rs` (DESIGN §8.3) |
| Scale | every screen rendered by `cpu.rs` at s = 1, 1.25, 1.5, 1.75, 2, 2.5, 3: every snapped edge lies on a device pixel, hairlines are exactly 1 px, no text below 11 logical px; a simulated monitor change re-lays out with nothing left at the old scale; host size requests are logical on macOS and physical elsewhere |
| Themes | each built-in theme parses, round-trips through export, and passes contrast; theme parser fuzzed; a missing key falls back to the base |
| Popover | open/commit/revert sequences: Esc restores the exact value at open; one undo step and one host gesture per popover session |
| Contrast and target sizes | computed from the theme tokens and the layout of every screen at 100 % and 50 % scale |
| Real time | RTSan runs with the editor open and being driven by the fuzzer's event streams: no audio-thread reports |

## 9. Open questions

1. Which built-in theme is the default (§6b; `membrane` for now).
2. Languages beyond English and Japanese.
3. Whether the operator table or the global strip should be visible by default on small screens.
