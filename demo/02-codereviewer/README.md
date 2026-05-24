# 02 — Code Reviewer

> **One sentence.** Materialise a small fake project at runtime,
> then run a four-stage typed `agent_net` that reviews it for
> lint / security / performance issues and emits a Markdown
> verdict.

## What it shows

| Construct | Where |
|---|---|
| `agent` with `llm:` / `intent:` / `prompt:` / `accept:` / `produce:` | `lib/agents.aer` |
| `agent_net` with `flow A -> B -> C -> D` chaining (§ 14) | `lib/agents.aer` — `agent_net code_review { … }` |
| `model X@v1` with `where:` field invariants (§ 4.5) | `lib/models.aer` |
| Rolling state through the net (each agent enriches `ReviewDraft@v1`) | `lib/agents.aer` — three reviewer agents share `accept`/`produce` |
| `intent "…" { … }` annotation on write effects | `lib/sample.aer` — `materialise_sample_project` |
| Triple-quoted strings for prompts and sample code | every `const … = """ … """` |
| String interpolation `"{expr}"` | `lib/report.aer`, `main.aer` |
| `ai.usage()` for in-process LLM diagnostics | end of `main.aer` |
| `use "./lib/foo.aer"` local-file imports (§ 3.2) | `main.aer` and `lib/*.aer` |

## How it works

1. `main` calls `materialise_sample_project("./.tmp_sample")`
   (defined in `lib/sample.aer`), which writes `app.py`,
   `utils.py`, `config.py` — each carrying one deliberate issue
   (SQL injection, O(n²) dedup, shadowed builtin / unused
   imports). The same content is returned as a `Codebase@v1`
   value.
2. `code_review(codebase, cap)` runs the agent net (defined in
   `lib/agents.aer`). The runtime owns the routing protocol:
   every agent receives, in its system prompt, a JSON-fenced
   contract describing its inbox and the schema it must produce.
   Schema mismatches consume the agent's `retries:` budget.
3. `print_review(review)` (in `lib/report.aer`) renders the final
   `Review@v1` as a Markdown report on stdout.

## How to run

```bash
cd demo/02-codereviewer
aeris run ./main.aer
```

No external services. The `claude` CLI must be on `$PATH` (the
backend is configured via `aeris.toml [ai.backend]`).

The trace lands under `.aeris/traces/<trace_id>.jsonl`. Inspect
it with:

```bash
aeris trace tail
```

Each `agent_call` event records the agent name, the model, the
token count and the latency.

## Project layout

```
demo/02-codereviewer/
├── main.aer                # main + glue only; defers to lib/
├── lib/
│   ├── models.aer          # SourceFile@v1, Codebase@v1, Finding@v1,
│   │                       # ReviewDraft@v1, Review@v1
│   ├── agents.aer          # 4 agents + agent_net code_review
│   ├── sample.aer          # sample project constants + materialiser
│   └── report.aer          # print_review (Markdown rendering)
├── aeris.toml              # enforce = "off", Claude CLI backend
└── README.md
```

## Notes

- The temp directory `./.tmp_sample` is left in place after the
  run; remove it manually if you want a clean slate.
- All four agents target `claude-sonnet-4-6`. The model is a
  literal string in each agent's `llm:` field — change it there
  to repoint, and re-run.
- `enforce = "off"` means no `cap` annotations are required.
  `fn main(cap)` is still the signature — the runtime synthesises
  `cap[*]` and the call to `code_review(codebase, cap)` forwards
  it. Tightening the project (per `docs/language.md` § 8.4.1) is
  a one-line change in `aeris.toml`.
- **Multi-file loader.** Every `use "./lib/<file>.aer"` clause in
  `main.aer` and inside the `lib/*.aer` files is followed
  transitively by the runtime; diamond dependencies (e.g. each
  lib re-imports `lib/models.aer`) are loaded once.
