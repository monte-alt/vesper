# Vesper Claimable Task Board (`VSP-01` – `VSP-10`)

To keep high-volume contributions and AI-assisted PRs conflict-free across hot modules (`crates/vesper-gpu/src/list.rs`, `draw.rs`, `shadow.rs`), **every feature or optimization PR must reference an active `VSP-XX` claim**.

## How to Claim a Task

1. Verify the task status below is **Open** and no open **Task Claim** issue holds an active lock.
2. Open a **[Task Claim](.github/ISSUE_TEMPLATE/claim_task.yml)** issue specifying the `VSP-XX` ID and your planned file scope.
3. **One active claim per contributor** at a time. Claims expire automatically after **72 hours** without a linked draft PR or measurable progress update.
4. Before marking a PR ready, run `cargo test -p xtask --locked`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, and attach benchmark + visual verification metrics matching the PR template.

---

## Active Queue Toward Stable 30 FPS

| ID | Priority | Task | Primary Files (Scope Lock) | Acceptance Criteria | Status |
|---|---|---|---|---|---|
| **`VSP-01`** | **P0** | **Cut command-list split count (`266 → <100` lists/frame)** | `crates/vesper-gpu/src/list.rs`, `crates/vesper-gpu/src/host/place.rs` | Coalesce safe sequential title/prelude lists without violating `VESPER_WAIT_RESIDENT=textures` ordering; measurable drop in `~13 ms` per-list sync overhead (`VESPER_GPU_TIMING=1`) and zero geometry culling regressions. | Open |
| **`VSP-02`** | **P0** | **Batch or fuse indirect draw/dispatch argument clamping** | `crates/vesper-gpu/src/indirect.rs`, `crates/vesper-gpu/src/indirect/batch.rs`, `crates/vesper-gpu/src/draw.rs` | Reduce the `~1,082` indirect clamps/frame barrier tax safely via GPU-side batch validation or compute compaction while keeping `VESPER_INDIRECT_CLAMP` visual integrity 100% clean (zero shader corruption). | Open |
| **`VSP-03`** | **P1** | **Expand `VESPER_INPLACE_HOT` eligibility past single-stretch sections** | `crates/vesper-gpu/src/port.rs`, `crates/vesper-hle/src/modules/xmem.rs` | Currently ~62 candidate regions are refused (`not one stretch of a section`); safely promote multi-stretch static/read-mostly heaps to cut remaining `~585 MB` host-copied buffers further. | Open |
| **`VSP-04`** | **P1** | **Async pipeline warmup & PSO cache pre-population** | `crates/vesper-pipeline/src/`, `apps/vesper/src/warm.rs` | Eliminate cold/late-frame PSO hitching spikes without regressing `VESPER_PIPELINE_WORKERS` stability on 32 GB hosts. | Open |
| **`VSP-05`** | **P1** | **FidelityFX FSR 4.1.2 ACO PSO pass-0..26 warmup / zero-copy dispatch** | `crates/vesper-hle/src/modules/fidelityfx.rs`, `crates/vesper-gpu/src/host.rs` | Reduce the extra pass cost currently slowing FSR Quality vs native NO-FSR on fixed-GPU-bound scenes, and eliminate Series S `FFX_FSR4_PASS_*` fallback overhead. | Open |
| **`VSP-06`** | **P2** | **Optimize RDNA 4 HW BVH4 RT fence-free pass scheduling** | `crates/vesper-gpu/src/port_rt.rs`, `crates/vesper-gpu/src/memory.rs` | Profile `VESPER_HW_BVH=1` frame breakdown and cut upload/build synchronization stalls toward playable RT frame times. | Open |
| **`VSP-07`** | **P2** | **Vulkan / Multi-vendor NVIDIA & Intel Arc validation profiles** | `crates/vesper-gpu/src/device.rs`, `crates/vesper-shader/src/emit/` | Benchmark and resolve GPU-specific shader/barrier bottlenecks on RTX 30/40/50 and Intel Arc Xe2 GPUs (including 32-lane wave emulation paths). | Open |
| **`VSP-08`** | **P2** | **Automated visual regression & headless steady-state diff harness** | `apps/vesper/src/replay.rs`, `crates/vesper-gpu/src/capture.rs` | Deterministic frame SSIM/PSNR + black-pixel/confetti anomaly checker so every PR proving perf gains also proves clean rendering automatically. | Open |
| **`VSP-09`** | **P3** | **Further reduce Peak Private Commit below `20 GB`** | `crates/vesper-gpu/src/texture.rs`, `crates/vesper-gpu/src/residency.rs` | Evict unreferenced placed texture twins earlier under tight commit budgets so 16 GB system RAM hosts avoid paging stalls. | Open |
| **`VSP-10`** | **P3** | **Interactive runtime telemetry overlay & live bottleneck HUD** | `crates/vesper-gpu/src/host/panel.rs`, `crates/vesper-gpu/src/copystat.rs` | Lightweight DX12 stats bar showing live FPS, `p95` ms, active `VESPER_INPLACE_HOT` region count, and GPU/CPU limit indicator (`VESPER_PANEL=1`). | Open |
