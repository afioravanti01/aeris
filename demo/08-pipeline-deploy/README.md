# 08 — Pipeline deploy (forward-only automation)

A four-step deploy expressed as a `pipeline`: build → push → roll out →
audit. It is the forward-only sibling of the saga in
[`07-docker-deploy`](../07-docker-deploy/).

```
aeris run ./main.aer
```

Runs offline — the `docker` and `kube` steps use the trace-only mock
backends (`aeris.toml`), so no daemon or cluster is required.

## `saga` vs `pipeline`

Both are ordered, `intent`-gated, `cap`-checked and fully traced. They
differ in one thing: **what happens on failure.**

| | `saga` (07) | `pipeline` (08) |
|---|---|---|
| step shape | `do` / `undo` pair | single forward expression |
| on failure | reverse-order `undo`, else `PartialFailure` | `on_failure` hook, then stop or `continue` |
| recovery model | roll **back** | roll **forward** + re-run |
| `intent` · `cap` · trace · idempotency key | yes | yes |
| compensation guarantee | **yes** | **no** (by design) |

Reach for a `saga` when a failed run must be undone; reach for a
`pipeline` when the recovery is *fix and re-run* (deploys, ops
automation). A `do`/`undo` block inside a pipeline step is a compile
error that points you back at `saga`.

## What to look at

- The single mandatory `intent` wraps every step — the *why* is in the
  grammar, not a comment.
- `on_step` logs each completed stage; `on_failure` reports the stop
  with the `last_step` / `last_error` implicits.
- Each step is taped: `pipeline_enter`, `step_enter` / `step_exit`,
  `pipeline_exit`, plus a per-step idempotency key. See the JSONL trace
  printed at startup (`[aeris] trace_id = …`).
- `Deploy.run(version: "1.4.2")` — the cap is supplied by the ambient
  scope; pass `on_error: "continue"` to roll past a failed step instead
  of stopping.
