# Commit Discipline

> Workflow-level rule for all work in this repository. Applies to every layer and every task.

---

## Commit often, commit small

- **Commit frequently in small, coherent units.** Each commit should represent one logical change: one fix, one feature slice, one refactor step, or one doc update.
- **Do NOT batch many unrelated changes into a single commit.** A 500-line commit mixing a bugfix, a refactor, and a new feature is hard to review, hard to revert, and hard to bisect.
- If a task produces several distinct changes, make several commits — one per change — even if they ship in the same PR/task.
- A good self-check: the commit message should describe exactly one thing. If you need "and" in the subject, split the commit.

---

## Run `just fmt` before every commit

- **Before staging/committing, run `just fmt`.** This is non-negotiable.
- `just fmt` runs:
  1. `cargo fmt` (Rust formatting),
  2. `pre-commit run --all-files` (hooks: lint, format, and repo-specific checks),
  3. `typstyle -i .` (paper formatting, under `paper/`).
- **Do not commit unformatted code.** Pre-commit hooks may reject the commit anyway, but running `just fmt` first keeps the working tree clean and avoids a failed-commit-reformat-recommit cycle.
- If `just fmt` reports issues it cannot auto-fix, resolve them manually, re-run `just fmt`, then commit.

---

## How to apply

1. Make a logical change (or finish one slice of work).
2. Run `just fmt`.
3. Review `git diff` and `git status` — confirm the change is coherent and minimal.
4. `git add <only the relevant files>` — avoid `git add -A` when the tree has unrelated WIP.
5. `git commit -m "<imperative subject, one thing>"` with a body if context helps.
6. Repeat for the next slice.

> **Reminder**: "Commit often" does NOT mean "commit broken code." Each commit should still build (`just check`) and ideally pass tests (`just test`) for the slice it touches. Use a WIP/stash only to switch context, not as a substitute for small commits.

---

**Language**: All documentation should be written in **English**.
