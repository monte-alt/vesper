# Contributing to Vesper

Vesper is engineered toward stable 30–60 FPS native execution on Windows x64. Because many contributors (human and AI-assisted) work concurrently across `vesper-gpu`, `vesper-hle`, and `vesper-shader`, **every contribution goes through a structured 4-stage pipeline so maintainers only review proven, peer-vetted work.**

---

## 1. The 4-Stage PR Approval Pipeline

1. **Claim (`VSP-XX`):** Pick an **Open** task in [`ROADMAP.md`](ROADMAP.md) and open a **`Task Claim`** issue first. One active claim per contributor; claims release after **72 hours** without progress.
2. **Scoped Implementation & Automated Gate (`CI` + `pr-intake`):** Implement only inside your claimed task's `Primary Files` scope, obey [`AGENTS.md`](AGENTS.md), and pass `xtask`, `clippy`, and local tests.
3. **3 Independent Peer Reviews (`peer-review-gate`):** Every PR must earn **`APPROVED` reviews from at least 3 distinct independent reviewers** (excluding the PR author and bots, with zero unresolved `CHANGES_REQUESTED`) before maintainer `@monte-alt` will look at it.
4. **Maintainer Final Review & Merge (`@monte-alt`):** Once `check`, `pr-intake`, and `peer-review-gate` (3/3 approvals) are all green, maintainer review happens cleanly.

---

## 2. Policy on AI Coding Agents

AI coding tools are welcome — **unverified AI output and unclaimed drive-by PRs are closed automatically.**

All major AI agents auto-load Vesper's rules via [`AGENTS.md`](AGENTS.md), [`CLAUDE.md`](CLAUDE.md), [`.cursor/rules/vesper.mdc`](.cursor/rules/vesper.mdc), and [`.github/copilot-instructions.md`](.github/copilot-instructions.md):
- **Explicit disclosure** in the PR template stating which tool/model assisted and what you personally verified.
- **Zero unclaimed PRs / zero drive-by refactors** outside the claimed `VSP-XX` file scope.
- **Local command proof required** — never claim tests passed without running them.
- **Zero fake-FPS hacks:** Never disable `VESPER_WAIT_RESIDENT=textures`, bypass `VESPER_INDIRECT_CLAMP`, or cull valid geometry.

---

## 3. The 3-Independent-Peer-Review Rubric

Want to help Vesper move faster right now — **including by auditing PRs alongside your AI reviewer**? Follow [`AGENTS.md`](AGENTS.md) Section 6 and verify all 6 items before submitting an **`APPROVE`** review:

1. **Claim & File Scope:** Does the PR link an active `VSP-XX` claim `#NNN` and modify only the allowed files?
2. **Safety & Visual Integrity:** Are `VESPER_WAIT_RESIDENT=textures`, indirect argument clamping, and barrier dependencies intact? Do captured screenshots/frames remain clean?
3. **Reproducible Benchmarks:** For GPU/CPU/RAM PRs, are Before vs. After metrics measured on identical hardware and duration?
4. **Build & Test Proof:** Are `cargo test -p xtask --locked`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, and `cargo test --workspace --locked -- --test-threads=1` clean?
5. **Clean-Room Compliance:** Zero proprietary assets, screenshots, keys, or third-party studio/franchise/executable names?
6. **Code Correctness:** No duplicated logic, sound `unsafe` blocks (`// SAFETY:` invariants documented), and no new hidden bottlenecks?

*Rubber-stamp "+1 / LGTM" approvals without substantive review comments will be dismissed.*

---

## 4. Mandatory Local Commands & Zero-Tolerance Asset Policy

```powershell
cargo test -p xtask --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked -- --test-threads=1
```

Never commit executables, decrypted assets, keys, dumps, screenshots, or third-party franchise/studio names ([Asset Policy](docs/asset-policy.md)).

<!-- test PR check registration -->
