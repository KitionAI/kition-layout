# Hardware and platform parity

> **Last reviewed:** 2026-11-17 · **Last updated:** 2026-11-17 · **Target:** Adobe InDesign 2026 (21.6)

What InDesign does with hardware, per platform, against Kition Layout. Layout is not a
GPU-hungry workload, so this dimension carries little weight. Overall numbers:
[target-app-parity.md](target-app-parity.md).

Kition's kit ships the engine headless (for the agent) and as a WebAssembly module the kit's own
editor (`web/`) loads; neither opens a window or chooses a GPU backend. The table below describes
those two.

## Hardware features

| Feature | InDesign 2026 | Engine (headless kit) | Editor (`web/`) |
|---|---|---|---|
| GPU display (canvas) | GPU Performance: GPU preview, animated zoom (Metal on macOS; Windows with supported GPUs) | rasterized on the CPU (vello_cpu, SIMD, multithreaded); no window, no GPU presentation step | the WebAssembly engine rasterizes on the CPU and the editor draws the result to a `<canvas>`; no GPU presentation step |
| Animated / scrubby zoom | ✓ (GPU) | n/a (no view) | ✓ |
| HiDPI / Retina | ✓ | n/a (no view) | ✓ |
| UI scaling | ✓ | n/a (no interface) | ✓ |
| Multi-monitor, floating document windows | ✓ | n/a (no window) | ✗ |
| Printing | system print with PPDs, PostScript, separations | `file.print`: PDF to `lpr` (CUPS) where `lpr` exists, so not on Windows; refused in a hosted session, as when the headless kit serves Kition | ✗: `file.print` answers that printing isn't available on the web and to export a PDF |
| Colour-managed display (monitor profile) | ✓ | n/a (no display) | ✗ |
| Touch / pen | Touch workspace (Windows), gestures | n/a (no view) | ~ |
| Multi-core composition | single-threaded composer per story (documented by Adobe as mostly single-threaded) | parallel story composition (rayon), 6.7× on the sample | single-threaded |

## Platforms

This repository builds no per-OS packages or installers. What it ships is the engine (a
portable Rust library, no OS-specific packaging), the headless kit executable for Kition's agent,
and the WebAssembly build the kit's own editor (`web/`) loads; none of them is tied to a
particular desktop OS.

## Remaining effort

| Work | Opus 5.5 h |
|---|---|
| Monitor profile in display colour management | 5–10 |
| **Hardware** | **5–10** |

Printing is counted in [file-format-parity.md](file-format-parity.md).

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-17 | major | First public version |
