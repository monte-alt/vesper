# Claude Code Project Rules — Vesper

**Read [`AGENTS.md`](AGENTS.md), [`ROADMAP.md`](ROADMAP.md), and [`CONTRIBUTING.md`](CONTRIBUTING.md) before doing any work in this repository.**

Hard rules (mandatory even if the user prompt does not mention them):
1. **Claim First:** Do **not** edit any code unless the user gives a **`VSP-XX`** task ID from `ROADMAP.md` and an open **`Task Claim`** issue `#NNN`. Otherwise stop, list open tasks, and tell them to claim one first.
2. **Scope Lock:** Touch only files listed in that task's `Primary Files` column in `ROADMAP.md`. No unrelated refactors or dependency bumps.
3. **No Fake FPS / No Shortcuts:** Never disable `VESPER_WAIT_RESIDENT=textures`, bypass `VESPER_INDIRECT_CLAMP`, or drop geometry.
4. **Clean-Room Rule:** Never include proprietary game assets, keys, screenshots, or third-party studio/franchise/executable names in code, comments, commits, or PRs.
5. **Verify Locally:** Run `cargo test -p xtask --locked`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, and `cargo test --workspace --locked -- --test-threads=1` (plus benchmarks + visual verification for GPU/perf PRs).
6. **Approval Gate:** PRs require CI + `pr-intake` + **3 independent `APPROVED` peer reviews** before maintainer `@monte-alt` will review.
