# Contributing to Vesper

Vesper moves quickly toward stable 30+ FPS compatibility on Windows x64. To avoid conflicting rewrites across critical paths (`vesper-gpu`, `vesper-hle`, `vesper-shader`), all contributions follow a lightweight **Claim → Scope → Verify** workflow.

---

## 1. Claim Before Coding

1. Check [`ROADMAP.md`](ROADMAP.md) and open issues first.
2. Open a **`Task Claim`** issue referencing a `VSP-XX` task ID (or propose a scoped optimization issue first if no task fits).
3. **Keep PRs focused** to the claimed task's file scope. Unclaimed mega-PRs that bundle unrelated formatting, crate moves, or drive-by refactors are closed automatically.

---

## 2. Policy on AI-Assisted Contributions

AI coding assistants are welcome — **unverified AI output is not**. Every PR generated or assisted by an AI model must satisfy all of the following:

- **Explicit disclosure** in the PR template stating which tool/model assisted and what you personally reviewed/tested.
- **No drive-by diffs:** Do not rewrite unrelated comments, functions, or switches outside your claimed scope.
- **Proof of local verification** pasted directly in the PR description (command output, not assumptions).
- **Never bypass safety gates to inflate FPS:** Disabling texture residency waits (`VESPER_WAIT_RESIDENT=0`), indirect argument clamping (`VESPER_INDIRECT_CLAMP=0`), or culling geometry to fake higher FPS is grounds for immediate closure.

---

## 3. Mandatory Verification Before Opening a PR

Run these checks locally from a Windows MSVC shell:

```powershell
# 1. Asset & documented-switch policy check (must pass 9/9)
cargo test -p xtask --locked

# 2. Strict Clippy across all workspace crates and targets
cargo clippy --workspace --all-targets --locked -- -D warnings

# 3. Unit & integration tests (sequential thread execution for D3D12/GPU tests)
cargo test --workspace --locked -- --test-threads=1
```

For performance or rendering PRs, also include:
- A **Before vs. After** comparison table (`Avg FPS`, `Med FPS`, `p95 ms`, `GPU Busy %`, `Peak Priv RAM`) measured on identical scene duration and hardware.
- Confirmation of **clean steady-state visual verification** (no missing interior walls, no black character skin, no rainbow confetti).

---

## 4. Zero-Tolerance Asset, Key & Naming Policy

1. **No proprietary data:** Never commit game files, dumps, keys, extracted tables, shaders, screenshots, or binary fixtures (`cargo xtask policy` enforces strict extension and size allowlists).
2. **Keep branding neutral:** Keep code comments, commit messages, PR titles, and benchmark logs strictly focused on Scarlett / Xbox Series / D3D12 technical behavior — never reference third-party studios, commercial franchises, or leaked build/executable names.
