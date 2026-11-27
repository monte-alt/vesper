# GitHub Copilot Instructions — Vesper

Before proposing, generating, or reviewing code in Vesper, strictly follow [`AGENTS.md`](../AGENTS.md), [`ROADMAP.md`](../ROADMAP.md), and [`CONTRIBUTING.md`](../CONTRIBUTING.md):

- **Claim Required (`VSP-XX`):** Refuse to start implementation changes without a claimed `VSP-XX` task ID from `ROADMAP.md` and an open `Task Claim` issue `#NNN`. Keep edits strictly scoped to that task's `Primary Files`.
- **Safety & Visual Invariants:** Never disable `VESPER_WAIT_RESIDENT=textures`, bypass `VESPER_INDIRECT_CLAMP`, or cull valid geometry to inflate FPS.
- **Clean-Room Policy:** Zero game dumps/assets/screenshots and zero references to third-party studios, commercial franchises/titles, or proprietary executable names anywhere in code, comments, or PRs.
- **Required Checks:** All PRs require `cargo test -p xtask --locked`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo test --workspace --locked -- --test-threads=1`, real Before/After benchmark metrics for perf changes, and **3 independent `APPROVED` peer reviews** before maintainer review.
