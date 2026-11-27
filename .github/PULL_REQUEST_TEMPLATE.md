## Summary

- **Claimed Task ID:** `VSP-__` (linked Task Claim issue: `#___`)
- **What this PR changes:**

## AI Assistance Disclosure

- [ ] No AI coding tools were used
- [ ] AI coding tools were used (specify model/tool and what you manually verified):

## Benchmark Comparison (Required for perf / GPU / memory changes)

Hardware (CPU / RAM / GPU / Driver):

| Metric | Before (`main`) | After (this PR) |
|---|---:|---:|
| Avg FPS | | |
| Med FPS | | |
| Max / Settled FPS | | |
| Med / p95 Frame Time (ms) | | |
| Peak Private RAM (MB) | | |

## Mandatory Verification Checklist

- [ ] Diff obeys `AGENTS.md` and is strictly scoped to the claimed `VSP-XX` files
- [ ] `cargo test -p xtask --locked` passes cleanly
- [ ] `cargo clippy --workspace --all-targets --locked -- -D warnings` passes with 0 warnings
- [ ] `cargo test --workspace --locked -- --test-threads=1` passes
- [ ] Safety gates remain enabled (`VESPER_WAIT_RESIDENT=textures`, indirect clamping intact) and steady-state visuals are verified clean
- [ ] Contains zero proprietary assets, keys, dumps, screenshots, or third-party studio/franchise names
- [ ] Ready for **3 independent peer reviews** (`peer-review-gate`) before maintainer review
