<!--
SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
SPDX-License-Identifier: GPL-3.0-or-later
-->

# Editor mockups

These files are the editor mockups exported from the design canvas (2026-11-16). They show `docs/UI.md`; they do not replace it.

| File | What it shows |
| --- | --- |
| `editor-membrane.html` / `.png` | The main view (UI.md §2.1) in the default built-in theme `membrane` |
| `editor-city-night.html` / `.png` | The same view and patch in `city-night` |
| `editor-ideal-compute.html` / `.png` | The same view and patch in `ideal-compute` |
| `value-editor.html` / `.png` | The value-editor popovers (UI.md §4.1.1). It is **interactive**: drag the slider, type `79`, `-15dB` or `2.2rad`, pick a ratio, switch themes. |
| `source/*.dc.html`, `source/canvas.json` | The canvas sources, kept for provenance. They need the design tool's runtime and are not used by the build. |

The `.png` files are rendered at 2× by headless Chrome. The `.html` files are self-contained apart from their Google Fonts link.

## What is binding

These parts are **binding** and must match the implementation:

- **The layout regions and their order:** header, graph canvas, inspector, operator table, global strip.
- **The interaction model shown:**
  - ⚙ on every node;
  - double-click opens a panel;
  - "Tidy" and "+ Operator";
  - "Panel" in the inspector header;
  - a popover per value, with two scales, typed input in either unit, − / + and Revert / Done;
  - a ratio grid for Coarse.
- **The numbers.** The example patch's levels, dB, modulation indices and feedback β come from MATH §3 and §9:
  - output level 74 → T = 3040 → 1.440 rad, −18.81 dB;
  - FB 5 at that level → β = 0.180, and 1.571 at full level.

  The scope and spectrum are a real render of that patch (A3, 96 kHz).

  The implementation must show the same numbers for the same patch. Use them as test vectors for the value formatting (UI.md §8).
- **The theme tokens.** They are in `assets/themes/*.keptone-theme`. The mockups use exactly these values.

These parts are **illustrative**:

- the envelope shapes (they are not computed from the MATH §4 closed forms);
- the live meter levels, the CPU figure and the voice count;
- the patch name and bank position.

The implementation draws these from the engine and telemetry.

## The example patch

| Op | Role | Coarse | Output level | As carrier | As modulator |
| --- | --- | --- | --- | --- | --- |
| 1 | carrier | 1 | 99 | 0.00 dB | — |
| 2 | → 1 | 14 | 58 | — | 0.360 rad |
| 3 | → 1, feedback 5 | 1 | 74 | — | 1.440 rad |
| 4 | → 1 | 1 | 79 | — | 2.221 rad |
| 5 | → 4 | 3 | 70 | — | 1.019 rad |
| 6 | → 5 | 1 | 62 | — | 0.509 rad |

The algorithm is 18. Fine 0, detune 7 and no scaling on every operator.
