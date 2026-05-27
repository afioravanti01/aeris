# Aeris v0.3 — Demo set

Self-contained Aeris projects, each split into a small `main.aer`
plus a `lib/` of focused modules. Every project ships its own
`aeris.toml`, a `README.md`, and (where relevant) fixtures or
input data.

| # | Scenario | Headline construct |
|---|---|---|
| 01 | [`01-chatbot`](./01-chatbot/) | `ai.chat(system, dir, port)` — KB-loaded HTTP chatbot in one call |
| 02 | [`02-codereviewer`](./02-codereviewer/) | `agent_net` with 4 typed agents over a fake project generated at runtime |
| 03 | [`03-incident-postmortem`](./03-incident-postmortem/) | `agent_net` reading **Aeris's own JSONL trace** as input |
| 04 | [`04-ticket-triage`](./04-ticket-triage/) | `net.http(port)` + `ai.decide` + `policy` + explicit `cap.subset[…]`; LLM categorises inbound tickets and forwards to a sibling in-process upstream |
| 05 | [`05-test-suite`](./05-test-suite/) | `assert_status` + `assert_json` + `assert_semantic` over a live HTTP endpoint |
| 06 | [`06-deploy-rollback`](./06-deploy-rollback/) | `saga` with paired `do`/`undo` + automatic rollback over `kube.*` |
| 07 | [`07-docker-deploy`](./07-docker-deploy/) | `saga` mirroring a **real** docker deploy (build → rm → run → gate) over typed `docker.*` ops, with compensating stop/rmi + `ai.complete` postmortem of a hard-to-spot mis-built image |
| 08 | [`08-pipeline-deploy`](./08-pipeline-deploy/) | `pipeline` — the forward-only sibling of `saga`: build → push → roll out → audit as ordered, traced steps with `on_step` / `on_failure`, no rollback (roll forward + re-run) |

## Running

From the repo root, after building the runtime:

```bash
cargo build --release
cd demo/<scenario>
aeris run ./main.aer            # optional CLI args per scenario
```

Each demo's `README.md` lists the exact CLI surface and any
external requirements.

## Multi-file projects

The runtime resolves `use "./lib/<file>.aer"` and
`use alias from "./lib/<file>.aer"` transitively, inlining each
referenced module's items into the entry module. Diamond
dependencies (multiple libs each importing `lib/models.aer`) are
loaded once; true cycles are rejected with exit code 64.

Each demo's `main.aer` is just **the entry point plus glue**:
declaratives (`model`, `agent`, `agent_net`, `saga`, `policy`)
and reusable helpers all live under `lib/`, grouped by concern:

```
demo/<scenario>/
├── main.aer
├── lib/
│   ├── models.aer       # all `model X@v1` declarations
│   ├── agents.aer       # agents + agent_net (or saga / policy)
│   ├── <feature>.aer    # one file per concern
│   └── …
├── aeris.toml
└── README.md
```

## What each demo covers, side by side

| Feature | 01 | 02 | 03 | 04 | 05 | 06 | 07 |
|---|---|---|---|---|---|---|---|
| String interpolation `{x}` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Triple-quoted strings `"""…"""` | ✓ | ✓ | ✓ | ✓ |   | ✓ |   |
| Raw strings `r"""…"""` |   |   |   |   |   |   | ✓ |
| `loop`, `??`, `catch`, `defer` | ✓ |   | ✓ | ✓ |   | ✓ | ✓ |
| `ai.chat(dir:)` | ✓ |   |   |   |   |   |   |
| `ai.decide` |   |   |   | ✓ |   |   |   |
| `ai.complete` |   |   |   |   |   |   | ✓ |
| `agent` + `agent_net` |   | ✓ | ✓ |   |   |   |   |
| `saga` / `step` / `do` / `undo` |   |   |   |   |   | ✓ | ✓ |
| `net.http(port:)` server |   |   |   | ✓ |   |   |   |
| `test "…" { … }` |   |   |   |   | ✓ |   |   |
| `model X@vN` with `where:` |   | ✓ | ✓ | ✓ |   | ✓ | ✓ |
| `policy` |   |   |   | ✓ |   |   |   |
| Explicit `cap[…]` + `cap.subset[…]` |   |   |   | ✓ |   |   |   |
| `intent "…" { … }` |   | ✓ | ✓ | ✓ |   | ✓ | ✓ |
| `assert_status` / `assert_json` |   |   |   |   | ✓ |   |   |
| `assert_semantic` |   |   |   |   | ✓ |   |   |
| `kube.*` |   |   |   |   |   | ✓ |   |
| `docker.*` (build/run/inspect/logs/stop/rm/rmi) |   |   |   |   |   |   | ✓ |
| `audit.event` |   |   |   |   |   | ✓ | ✓ |
| `http.get` / `http.post` |   |   |   | ✓ | ✓ |   |   |
| `spawn { … }` |   |   |   | ✓ |   |   |   |
| Multi-file (`use "./lib/…"`) | — | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |

## Enforcement modes used

| Demo | `enforce` | Why |
|---|---|---|
| 01 chatbot | `off` | script-mode docs assistant |
| 02 codereviewer | `off` | LLM-driven analysis, no external writes |
| 03 incident-postmortem | `off` | reads trace fixtures, writes markdown only |
| 04 ticket-triage | `loose` | runtime allow-list + `intent` mandatory on writes |
| 05 test-suite | `off` | tests need ad-hoc HTTP + AI access without ceremony |
| 06 deploy-rollback | `off` | the saga is the story; cap discipline is voluntary here |
| 07 docker-deploy | `off` | the saga + AI postmortem are the story; cap discipline is voluntary here |

Flipping `enforce` from `off` → `loose` → `strict` is a one-line
change in `aeris.toml`. The body of each `main.aer` does not
change.

## v0.3 surface notes

These constraints affect demo authoring; the surface is still
maturing:

1. **No tuple destructuring**: `let (a, b) = expr` is rejected.
   Workaround: `let r = expr; let a = r[0]; let b = r[1]`.
2. **No `let var`**: `var x = …` (without leading `let`) is the
   only form admitted by the parser, despite some doc snippets
   suggesting otherwise.
3. **`intent "..."` is a string literal**: interpolation
   `{var}` inside the intent string is rejected. Keep intent
   strings as plain text.
4. **Policy DSL**: only `match:`, `audit:`, and `limit:` parse
   cleanly in v0.3. The richer `deny:` / `require:` predicates
   from the spec are not yet implemented — see
   `04-ticket-triage` for the workaround.
5. **`spawn` runs inline (M31)**: the tree-walk runtime degrades
   `spawn { … }` to inline execution; the source shape stays
   forward-compatible.

## Where the multi-file loader lives

The CLI runs an entry file through `src/loader.rs`, which walks
every `use "./…"` / `use alias from "./…"` clause depth-first
and inlines each referenced file's items into the entry module.
Diamond dependencies are deduplicated; cycles are rejected. The
helper is exposed as `crate::loader::load_with_imports(entry)`
and `inline_local_imports(module, base_dir)`.
