# 03 — Incident Postmortem

> **One sentence.** Read an Aeris JSONL trace from
> `.aeris/traces/<id>.jsonl` (or any trace fixture) and produce a
> Markdown postmortem using a four-stage typed `agent_net`.

This is a *meta* demo: Aeris ingests its **own** trace format as
input and emits a publishable incident report. The fixture under
`fixtures/trace.jsonl` describes a failed `deploy_release` saga
that rolls back when a `kube.apply` returns 502 after three
retries.

## What it shows

| Construct | Where |
|---|---|
| `agent_net` with `flow A -> B -> C -> D` chaining (§ 14) | `lib/agents.aer` — `agent_net postmortem_pipeline { … }` |
| Four `agent` declarations with `accept:` / `produce:` schemas | `lib/agents.aer` |
| Rolling-state pattern: the same draft model is the in/out type of three sequential agents | `lib/models.aer` — `PostmortemDraft@v1` |
| `model X@v1` with `where:` invariants on every public field | `lib/models.aer` |
| `fs.read_text` + `json.parse(line)?` for trace ingest | `lib/trace_loader.aer` — `load_trace` |
| `intent "…" { … }` on the write effect | `lib/persist.aer` — `persist_postmortem` |
| `??` to default an optional field (`parsed["trace_id"] ?? "unknown"`) | `lib/trace_loader.aer` |
| `fn main(cap, args)` — both synthesised cap and CLI argv (§ 25.5) | `main.aer` |
| `use "./lib/foo.aer"` local-file imports (§ 3.2) | `main.aer`, `lib/*.aer` |

## How it works

1. `main` resolves the trace path from `args[0]` or falls back to
   `./fixtures/trace.jsonl`.
2. `load_trace(path)` (in `lib/trace_loader.aer`) reads the file,
   splits on newlines, counts non-empty events, and pulls
   `trace_id` from the first event's JSON record. The result is
   a `TraceBundle@v1`.
3. `postmortem_pipeline(bundle, cap)` (in `lib/agents.aer`) runs
   the agent net. The runtime validates `accept`/`produce`
   against the schemas at every edge — a malformed agent reply
   consumes the agent's `retries:` budget.
4. `persist_postmortem(p)` (in `lib/persist.aer`) writes
   `./postmortems/<trace_id>.md` inside an `intent` block; the
   markdown is also echoed on stdout.

## How to run

Default — uses the bundled fixture:

```bash
cd demo/03-incident-postmortem
aeris run ./main.aer
```

Against a custom trace path:

```bash
aeris run ./main.aer ./fixtures/trace.jsonl
aeris run ./main.aer .aeris/traces/01JXR…jsonl
```

The `claude` CLI must be on `$PATH` (configured in
`aeris.toml [ai.backend]`).

## Project layout

```
demo/03-incident-postmortem/
├── main.aer                    # main + glue only; defers to lib/
├── lib/
│   ├── models.aer              # TraceBundle@v1, PostmortemDraft@v1,
│   │                           # Postmortem@v1
│   ├── agents.aer              # 4 agents + agent_net pipeline
│   ├── trace_loader.aer        # load_trace (JSONL → TraceBundle@v1)
│   └── persist.aer             # persist_postmortem (atomic write)
├── fixtures/
│   └── trace.jsonl             # sample failed-deploy saga trace
├── postmortems/                # created on first run
├── aeris.toml
└── README.md
```

## Sample fixture

`fixtures/trace.jsonl` simulates a typical Aeris saga that fails
mid-way:

| Event kind | Detail |
|---|---|
| `saga_enter` | `deploy_release` saga, intent `"ship v2.4.0 to prod-eu-1"` |
| `step_enter` / `step_exit` (`reserve`) | OK, idempotency key recorded |
| `step_enter` / `step_exit` (`build_image`) | two `shell_exec` (docker build / push), OK |
| `step_enter` (`apply_manifest`) | three `http_call` to `api.k8s.acme.com`, all `status=502` |
| `step_exit` (`apply_manifest`) | `outcome=err`, `err.kind=net` |
| `undo_enter` / `undo_exit` × 2 | reverse-order rollback of completed steps |
| `saga_exit` | `outcome=rolled_back` |

The four agents read this stream and produce — in order —
a timeline, a root-cause diagnosis, mitigation steps, and a
final publishable Markdown.

## Notes

- The agents read trace events as raw JSONL text rather than as
  typed `TraceEvent@v1` records. Aeris trace events are
  heterogeneous by `kind`; a single `model` cannot cover all of
  them without losing information. The runtime is comfortable
  handing structured text to an LLM — that's what they're for.
- The output file name uses the `trace_id` directly, so re-running
  on the same trace overwrites the prior postmortem. To preserve
  history, archive `./postmortems/` between runs.
- **Multi-file loader.** Every `use "./lib/<file>.aer"` clause is
  followed transitively by the runtime; diamond dependencies
  (every lib re-imports `lib/models.aer`) are loaded once.
