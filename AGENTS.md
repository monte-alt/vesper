# Vesper — Mandatory Rules for AI Coding Agents & Contributors

> Read this entire file **before** planning, creating, editing, or reviewing any code in this repository. These rules apply automatically even if the user did not repeat them in their prompt.

---

## 1. Mandatory Pre-Flight Gate (STOP Before Editing)

Before touching any file to implement a feature, optimization, or bugfix:

1. Read [`ROADMAP.md`](ROADMAP.md) and [`CONTRIBUTING.md`](CONTRIBUTING.md).
2. Confirm the user provided:
   - An explicit **`VSP-XX`** task ID from `ROADMAP.md` (or an approved scoped issue), **AND**
   - An open **`Task Claim`** GitHub issue (`#NNN`) assigning them that task.
3. **If either is missing, STOP immediately. Do NOT edit any files.** Instead:
   - Show the user the relevant **Open** `VSP-XX` tasks from `ROADMAP.md` matching their goal.
   - Ask them to open a `Task Claim` issue on GitHub first and paste the `VSP-XX` ID + issue `#NNN` before you write code.

*(Exception: you may review someone else's PR read-only without claiming a task — see Section 6.)*

---

## 2. Strict File-Scope Lock

- Modify **only** the files listed under your claimed `VSP-XX` task's **`Primary Files (Scope Lock)`** column in `ROADMAP.md` (plus narrow unit tests in the same crate if needed).
- Do **not** refactor unrelated modules, rename unrelated switches, reformat untouched files, or bump dependencies in `Cargo.toml` / `Cargo.lock`.
- If a fix requires touching files outside the claimed scope, **stop and explain** so the claim scope can be updated first.

---

## 3. Non-Negotiable Safety & Clean-Room Invariants

Never merge, generate, or suggest changes that violate any of these invariants:

1. **Zero Asset / Key / Dump Leakage:** Never add executables, DLLs, decrypted assets, keys, memory dumps, shader dumps, extracted tables, or screenshots to the repo or PR descriptions. Follow [`docs/asset-policy.md`](docs/asset-policy.md).
2. **Zero Banned Third-Party / Franchise Names:** Never mention third-party game studios, commercial franchises/titles, character/location names, or leaked executable filenames anywhere in source code, comments, commit messages, PR titles, or issues. Keep terminology strictly technical (Scarlett / Xbox Series / DirectX 12).
3. **Never Disable Safety Barriers to Fake Higher FPS:**
   - Do **not** disable `VESPER_WAIT_RESIDENT=textures` (causes skin/texture corruption).
   - Do **not** bypass indirect draw/dispatch clamping (`VESPER_INDIRECT_CLAMP`) without mathematically equivalent validation proving identical safety.
   - Do **not** drop draws or cull valid geometry under batching/barrier changes.
4. **xtask Switch Integrity:** Any `VESPER_*` variable name written in a `.md` file MUST appear as a quoted string literal (`"VESPER_..."`) in a `.rs` file outside `xtask/`. Never invent placeholder `VESPER_*` names in Markdown docs.

---

## 4. Mandatory Local Verification Before Opening a PR

Run every check locally and report exact observed counts (never summarize from memory or skip failing tests):

```powershell
cargo test -p xtask --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked -- --test-threads=1
```

For GPU, barrier, memory, shader, or scheduling changes (`VSP-01` – `VSP-13`), you must **also**:
- Measure **Before vs. After** (`Avg FPS`, `Med FPS`, `Max / Settled FPS`, `Med / p95 ms`, `GPU Busy %`, `Peak Priv RAM`) on identical hardware and duration.
- Visually verify steady-state interiors render with **zero missing walls, zero black character skin, and zero shader corruption**.

---

## 5. How PR Approval Works (3-Independent-Review Gate)

Every pull request must pass **all four gates** before maintainer `@monte-alt` will review or merge it:

1. **PR Intake (`pr-intake`):** Valid `VSP-XX` claim + issue `#NNN`, completed AI disclosure, zero banned strings, tight file scope.
2. **CI (`check`):** Clean Clippy (`-D warnings`), default tests, and diagnostics tests (`--test-threads=1`).
3. **3 Independent Peer Reviews (`peer-review-gate`):** At least **3 distinct reviewers** (excluding the PR author and bots, with zero unresolved `CHANGES_REQUESTED`) must review and `APPROVE` the PR after verifying all items in Section 6.
4. **Maintainer Sign-Off (`@monte-alt`):** Final review and merge only after gates 1–3 are green.

---

## 6. AI Peer Review Mode (When Asked to Review a PR)

When reviewing another contributor's PR, act as a strict independent reviewer — do **not** rubber-stamp an approval. Check all 6 items before submitting `APPROVE` vs `REQUEST_CHANGES`:

1. **Claim & Scope:** Does the PR reference a real `VSP-XX` task + `#NNN` issue, and are all changed files inside its `Primary Files` scope?
2. **Safety Barriers:** Are `VESPER_WAIT_RESIDENT=textures`, indirect clamping, and barrier ordering preserved with zero geometry/shading regressions?
3. **Evidence:** Did the author provide real `cargo test -p xtask`, `clippy`, `--test-threads=1`, and reproducible Before/After benchmark + visual proof?
4. **Clean-Room Policy:** Zero game data/screenshots and zero banned studio/franchise/executable names in code or diff?
5. **Code Quality:** No duplicated logic, shallow nesting, accurate error handling, and idiomatic Rust (`unsafe` blocks justified with `// SAFETY:` comments)?
6. **xtask Integrity:** No unquoted/invented `VESPER_*` names in `.md` files and all changed files pass `xtask` allowlists?
