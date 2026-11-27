# Vesper Master To-Do List & Claimable Roadmap (`VSP-01` – `VSP-16`)

> **Goal:** Break past our current ~21–24 FPS settled plateau (`~44–48 ms/frame` across 720p–1440p) to a rock-solid **30–60 native FPS** on Windows x64 D3D12 with clean interiors, lower system RAM (`<20 GB`), and multi-vendor GPU support — **without** downgrading to unsafe barrier hacks.

---

## How Humans & AI Contributors Can Help Today

Whether you contribute directly or with an AI coding agent (`AGENTS.md` / `CLAUDE.md` / Cursor / Copilot), pick one of three high-impact lanes:

1. **Claim a Technical Task (`VSP-01` – `VSP-16`):** Pick an **Open** task below, open a **[Task Claim](.github/ISSUE_TEMPLATE/claim_task.yml)** issue first, stay strictly inside its `Primary Files` scope, and prove your change with tests + real benchmarks.
2. **Independent Peer Review (Unblocks Maintainer Review!):** Because high PR volume overwhelms a single maintainer, **every PR requires 3 independent `APPROVED` peer reviews before `@monte-alt` reviews it**. Open PRs need careful human + AI-assisted audits against our [6-Point Peer Review Rubric](CONTRIBUTING.md#3-the-3-independent-peer-review-gate).
3. **Reproducible Hardware Benchmarks:** Test `main` or open PR branches on NVIDIA RTX, Intel Arc, or AMD RDNA 3/4 rigs and file a **[Hardware Benchmark](.github/ISSUE_TEMPLATE/hardware_benchmark.yml)** report (especially `VESPER_GPU_TIMING=1` profiles).

### Claiming Rules (Prevents Merge Conflicts)
- **Claim first:** Open a `Task Claim` issue naming `VSP-XX` before writing code.
- **One active claim per contributor** at a time; claims release automatically after **72 hours** without a linked draft PR or measurable progress update.
- **Strict file-scope lock:** Do not touch files outside your task's `Primary Files` column so dozens of contributors can work in parallel safely.

---

## Milestone A (P0) — Break the ~44 ms Fixed GPU Floor → 30+ Native FPS

Hardware GPU timing (`VESPER_GPU_TIMING=1`) proved why `720p`, `1080p`, and `1440p` all plateau around ~21–24 FPS: fixed per-frame command-list splits (`266` lists = `~13 ms` sync floor), `~1,082` indirect clamps (`~1,600` barrier groups), and host/VRAM copy churn dominate GPU + submit time.

| ID | Priority | Track | Task | Primary Files (Scope Lock) | Acceptance Criteria | Status |
|---|---|---|---|---|---|---|
| **`VSP-01`** | **P0** | GPU / Lists (Hard) | **Collapse `266` command lists/frame down to `<100`** | `crates/vesper-gpu/src/list.rs`, `crates/vesper-gpu/src/host/place.rs` | Keep one host list per console list across safe work boundaries; cut the `~13 ms` per-list sync floor (`VESPER_GPU_TIMING=1`) with zero geometry culling or `VESPER_WAIT_RESIDENT=textures` violations. | Open |
| **`VSP-02`** | **P0** | GPU / Indirect (Hard) | **Batch or fuse `~1,082` indirect draw/dispatch argument clamps** | `crates/vesper-gpu/src/indirect.rs`, `crates/vesper-gpu/src/indirect/batch.rs`, `crates/vesper-gpu/src/draw.rs` | Replace per-draw sync splits with a batched compute pre-pass or amplification-shader validation guard; recover barrier/GPU frame time while preserving 100% clean character skin/shading (no corruption). | Open |
| **`VSP-03`** | **P0** | Memory / Porting (Hard) | **GPU-resident memory + expand `VESPER_INPLACE_HOT` past single-stretch sections** | `crates/vesper-gpu/src/port.rs`, `crates/vesper-gpu/src/shadow.rs`, `crates/vesper-hle/src/modules/xmem.rs` | Keep title graphics heaps resident on GPU with on-demand page migration and promote the ~62 currently refused multi-stretch candidate regions (`~585 MB` host copies left per frame) safely in place. | Open |
| **`VSP-04`** | **P0** | Loader / Porting (Hard) | **Static load-time allocator & descriptor patching ("Porting, not Emulating")** | `crates/vesper-loader/src/`, `crates/vesper-gpu/src/port_descriptors.rs` | Identify and patch title CPU-write vs GPU-only heap/descriptor allocators at load time (following `VESPER_PORT_CONSTANTS` / `VESPER_PORT_DESCRIPTORS`) to eliminate runtime shadow guesswork. | Open |

---

## Milestone B (P1) — Eliminate Stutter, Shaders & Upscaling Overhead

| ID | Priority | Track | Task | Primary Files (Scope Lock) | Acceptance Criteria | Status |
|---|---|---|---|---|---|---|
| **`VSP-05`** | **P1** | Pipelines (Medium) | **Async PSO/pipeline warmup & background pre-compilation** | `crates/vesper-pipeline/src/`, `apps/vesper/src/warm.rs` | Eliminate first-frame/late-frame PSO creation stutters cleanly without violating `VESPER_PIPELINE_WORKERS` memory limits on 32 GB RAM systems. | Open |
| **`VSP-06`** | **P1** | FidelityFX (Medium) | **FSR 4.1.2 zero-copy dispatch & Series S PSO pass warmup** | `crates/vesper-hle/src/modules/fidelityfx.rs`, `crates/vesper-gpu/src/host.rs` | Eliminate the extra frame-time penalty currently making `VESPER_FSR_4K=quality` slower than native NO-FSR on GPU-sync-bound scenes, and resolve `FFX_FSR4_PASS_*` fallback stalls on Series S. | Open |
| **`VSP-07`** | **P1** | Shaders (Hard) | **DXIL shader translator optimization for SM 6.6 / RDNA 4** | `crates/vesper-shader/src/emit/` | Trim redundant instructions, barrier waits, and register pressure in generated DXIL while passing every shader emitter unit test (`cube_lod`, `valu`, `wave64`, `raw_load`). | Open |
| **`VSP-08`** | **P1** | CPU Hot Path (Medium) | **Reduce remaining `~53 ms` per-frame CPU prep & submit overhead** | `crates/vesper-gpu/src/list/thin.rs`, `crates/vesper-gpu/src/descriptor/host.rs`, `crates/vesper-gpu/src/host/send.rs` | Profile lock contention / allocations in `vesper-list`, descriptor staged tables, and queue sends; prove `prep_ms_mean` reduction with capture/profile logs. | Open |

---

## Milestone C (P2) — Multi-Vendor GPUs, Hardware DXR & Backend Research

| ID | Priority | Track | Task | Primary Files (Scope Lock) | Acceptance Criteria | Status |
|---|---|---|---|---|---|---|
| **`VSP-09`** | **P2** | NVIDIA (Medium) | **RTX 32-lane wave emulation & barrier tuning** | `crates/vesper-shader/src/emit/wave.rs`, `crates/vesper-gpu/src/device.rs` | Resolve 64-lane console shader assumptions cleanly for NVIDIA 32-lane warps and measure before/after stability on RTX 30/40/50 GPUs. | Open |
| **`VSP-10`** | **P2** | Intel Arc (Medium) | **Intel Arc Xe2 TDR & device-removal triage** | `crates/vesper-gpu/src/device.rs`, `crates/vesper-gpu/src/tdr.rs`, `crates/vesper-gpu/src/host.rs` | Reproduce and fix slow menu/early-world `DEVICE_REMOVED` failures reported on Arc GPUs. | Open |
| **`VSP-11`** | **P2** | DXR / RT (Medium) | **RDNA 4 HW BVH4 fence-free RT scheduling (`VESPER_HW_BVH=1`)** | `crates/vesper-gpu/src/port_rt.rs`, `crates/vesper-gpu/src/memory.rs` | Profile RT pass bottlenecks and cut BVH upload/build synchronization overhead to improve optional HW ray tracing stability and frame rate. | Open |
| **`VSP-12`** | **P2** | Research RFC | **Linux / Mesa RADV exploration RFC** | `docs/` (RFC doc only) | Design analysis covering Vulkan memory mapping vs Windows WDDM D3D12 tradeoffs before any backend work starts. | Open |

---

## Milestone D (P2–P3) — 16 GB RAM Fit, Automated QA & Good First Tasks

| ID | Priority | Track | Task | Primary Files (Scope Lock) | Acceptance Criteria | Status |
|---|---|---|---|---|---|---|
| **`VSP-13`** | **P2** | Memory (Medium) | **Push Peak Private Commit below `20 GB` for 16 GB RAM hosts** | `crates/vesper-gpu/src/texture.rs`, `crates/vesper-gpu/src/residency.rs` | Safely evict idle/unmapped placed texture twins earlier under tight commit pressure without causing texture pop-in regressions. | Open |
| **`VSP-14`** | **P2** | QA Tooling (**Good First Task**) | **Automated visual regression & artifact detector** | `apps/vesper/src/replay.rs`, `crates/vesper-gpu/src/capture.rs` | Compare steady-state captured frames against baseline metrics to automatically flag culled geometry, black skin pixels, or confetti corruption. | Open |
| **`VSP-15`** | **P3** | UX / HUD (**Good First Task**) | **Lightweight runtime bottleneck telemetry HUD (`VESPER_PANEL=1`)** | `crates/vesper-gpu/src/host/panel.rs`, `crates/vesper-gpu/src/copystat.rs` | Display live FPS, `p95` frame time, active `VESPER_INPLACE_HOT` region count, VRAM pressure, and CPU-vs-GPU bottleneck indicator cleanly in window. | Open |
| **`VSP-16`** | **P3** | Input (**Good First Task**) | **Controller hotplug & `VESPER_KBM_PC` ergonomics** | `crates/vesper-hle/src/modules/gameinput.rs`, `crates/vesper-hle/src/modules/lifecycle.rs` | Improve multi-pad switching, mouse-look smoothing edge cases, and raw focus-loss cursor restore reliability with unit test coverage. | Open |
