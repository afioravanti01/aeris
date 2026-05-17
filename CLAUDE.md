# CLAUDE.md

Rules for working on Aeris v0.3 in this repository.

## Sources of truth

- `docs/thesis.md` — rationale, non-negotiable
- `docs/language.md` — language specification, authoritative for surface
- `docs/project.md` — constraints

These three files are the **only** sources of truth for what the
language is and why. When in doubt about a design decision, consult
them in this order: thesis → language → project.

## Plan

- `docs/plan.md` is the implementation tracker. Follow it.
- At every iteration, update the **Status** column of any task that
  changed state (`pending` → `in progress` → `done`).
- When all tasks of a milestone are `done` and its acceptance suite
  passes, update the milestone's **Status** in `docs/plan.md § 3`.
- Do not skip or reorder milestones without an explicit instruction
  from the user.

## Documentation

- The documentation surface is **five files** in `docs/`:
  `thesis.md`, `language.md`, `project.md`, `plan.md`, `cheatsheet.md`.
- **Do not add further documentation files.** New conceptual material
  goes into the appropriate existing file.
- **Do not modify** `docs/thesis.md`, `docs/language.md`,
  `docs/project.md`, `docs/plan.md`, `docs/cheatsheet.md` unless the
  user explicitly asks. `cheatsheet.md` is a tabular index derived
  from `language.md`; when `language.md` gains a new construct or API,
  also add a row to `cheatsheet.md` so the two stay in sync.
- Inline comments only when the *why* is non-obvious (per the global
  rule). No multi-paragraph docstrings.

## Language of authoring

- **All documentation, all comments, and all source code MUST be in
  English.** This applies to:
  - the five `docs/` files, `README.md`, `RELEASE.md`, `CLAUDE.md`;
  - inline comments and doc strings in `.rs` and `.aer` files;
  - identifiers, error messages, commit messages, PR titles;
  - slides and any new artefacts produced in this repository.
- Conversations with the user may happen in any language the user
  prefers. The repository itself is monolingual: English.

## Code

- **Less is more.** No demo code, no extra examples, no speculative
  abstractions. Every file ships a load-bearing piece of `language.md`
  or supports one directly (tests, fixtures).
- **Modular structure**: one concern per crate, one responsibility
  per module. Cross-crate dependencies follow the layout in
  `docs/plan.md § 2`.
- **Simple but enterprise-ready**: no clever tricks, no hidden state.
  Errors are explicit, traces are deterministic, capabilities are
  values.
- **No `// TODO`, no half-finished code.** A task is incomplete until
  its acceptance check passes (`docs/plan.md § 7.1`).

## Workflow

1. Read the next `pending` task in `docs/plan.md § 5`.
2. Implement exactly that task — nothing else.
3. Run its acceptance check.
4. Update its Status to `done` in `docs/plan.md`.
5. Repeat.
