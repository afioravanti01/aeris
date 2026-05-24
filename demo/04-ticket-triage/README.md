# 04 — Ticket Triage

> **One sentence.** A two-process demo, both Aeris programs and
> both started from the same `main.aer`: a triage router that
> categorises inbound support tickets via `ai.decide(...)` and
> forwards each one to the matching backend, plus an in-process
> upstream that stands in for the four destination tools.

## Why this scenario

Support-ticket intake is the right shape for an AI-in-the-path
service:

- **Low cadence.** A medium SaaS sees tens to hundreds of tickets
  per hour, not thousands per second.
- **High per-decision value.** A misrouted ticket means a
  customer waiting longer, or a security report sitting in the
  product-feedback queue. Spending one second and a fraction of
  a cent on an LLM call is a bargain.
- **The LLM is replacing humans, not a regex.** Triage is the
  job operators do by reading; modelling it as a string-match
  pipeline never quite holds up.

Compare with the previous version of this demo (a generic
"webhook router"): web-scale traffic with an LLM in the hot path
would be inadmissible. Picking ticket triage keeps every
construct on display and removes the pretence.

## What it shows

| Construct | Where |
|---|---|
| `net.http(port:)` — integrated HTTP server (M20) | `main.aer` (router), `lib/upstream.aer` (upstream) |
| `ai.decide(prompt:, choices:, retries:)` — constrained choice over an LLM | `lib/routing.aer` |
| `cap.subset[…]` to narrow `main`'s cap before handing it to the request handler | `main.aer` |
| Explicit per-call cap allow-list (`http.post @ ["localhost"]`) | `lib/handler.aer`, `lib/routing.aer` |
| Two `policy` blocks gating the AI budget and the egress | `lib/policies.aer` |
| `enforce = "loose"` — cap-annotated functions checked statically, runtime allow-list still enforced | `aeris.toml` |
| `spawn { … }` per inbound request (inline in M31; the source shape stays forward-compatible) | both `main.aer` and `lib/upstream.aer` |
| `intent "…" { … }` on every write effect | `lib/routing.aer`, `lib/handler.aer` |
| `model X@v1` with `where:` invariants documenting the inbound shape | `lib/models.aer` |
| Live console logging via `io.println` carried through every `cap.subset` | both modules |

## How it works

Two processes, both `aeris run ./main.aer`, distinguished by an
argv flag:

| Process | Command | Port | Role |
|---|---|---|---|
| router | `aeris run ./main.aer` | `:8080` | Accepts tickets, asks the LLM which category they belong to, forwards or quarantines. |
| upstream | `aeris run ./main.aer upstream` | `:9090` | Pretends to be the four destination tools. |

### Router (port 8080)

```
POST /ticket          ← inbound JSON ticket
                      ↓
        ai.decide → "billing" | "bug" | "feature" | "spam"
                      ↓
"spam"     → audit.event("ticket.quarantined", …)         (no forward)
others     → POST http://localhost:9090/<category>        (forward)
                      ↓
HTTP 200 { "category": "…", "delivered_to": "…", "upstream": <status> }
```

### Upstream (port 9090)

```
POST /billing  → 202 Accepted     (Zendesk-style billing queue)
POST /bug      → 201 Created      (Linear-style bug tracker)
POST /feature  → 200 OK           (product roadmap board)
POST /spam     → 204 No Content   (abuse / shadow-quarantine)
GET  /health   → 200 OK
```

The point isn't realism — every backend is a few lines of Aeris
that send a JSON ack. The point is that the router and the
upstream are the **same kind of program**: same runtime, same
trace shape (`http_call` + `audit_event`), same cap discipline.

## How to run

In two separate shells:

```bash
# shell 1 — start the upstream
cd demo/04-ticket-triage
aeris run ./main.aer upstream

# shell 2 — start the router
cd demo/04-ticket-triage
aeris run ./main.aer
```

Then drive the router from anywhere:

```bash
curl -X POST http://localhost:8080/ticket \
     -H 'Content-Type: application/json' \
     -d '{"subject":"refund my last invoice",
          "body":"I was double-charged in March",
          "sender":"alice@acme.io"}'
```

The router will print something like:

```
[router] ← POST /ticket  (96 B)
[router]   payload: {"subject":"refund my last invoice", …}
[router]   asking AI to categorise …
[router]   ai.decide → calling claude-sonnet-4-6 (choices: billing|bug|feature|spam)
[router]   ai.decide ← billing
[router]   AI categorised → billing
[router]   http.post → http://localhost:9090/billing
[router]   http.post ← status 202
[router]   → 200  delivered to billing (upstream status 202)
```

`test.sh` runs a curl matrix (one per category + negatives) for
quick smoke-testing.

## Project layout

```
demo/04-ticket-triage/
├── main.aer              # router on :8080, or upstream on :9090
├── lib/
│   ├── models.aer        # Ticket@v1
│   ├── policies.aer      # ticket_ai_budget, ticket_egress
│   ├── routing.aer       # categorise (ai.decide) + forward
│   ├── handler.aer       # handle_request entry (per inbound ticket)
│   └── upstream.aer      # in-process backends server (port 9090)
├── aeris.toml            # enforce = "loose", http.allow = ["localhost"]
├── run-upstream.sh       # convenience wrapper for the upstream
├── run-webserver.sh      # convenience wrapper for the router
├── test.sh               # curl matrix
└── README.md
```

## Production patterns it deliberately doesn't show

For pedagogy this demo runs the LLM on **every** request. A real
ticket-triage service would layer two safety nets on top of it:

1. **Deterministic prefilter.** Cheap regexes / sender-domain
   checks for the obvious cases (auto-reply addresses → `spam`,
   the word "refund" in the subject → `billing`) before falling
   back to the LLM only for the ambiguous tail. In Aeris that's
   just an `if … else …` returning early before `categorise(...)`.
2. **Decision cache.** Hash the relevant fields of the payload
   and remember the category for a short TTL. Repeated mailings
   from the same template land in the same bucket without paying
   the LLM tax.

Neither pattern is necessary at the volumes ticket-triage runs
at — they become valuable when the same intake handles other
kinds of traffic too. The hooks are obvious where to add them
(`lib/routing.aer` for the prefilter, `cap.subset` keeping the
cache layer reachable).

## Notes

- The `claude` CLI must be on `$PATH` (see `aeris.toml [ai.backend]`).
- The single `http.allow = ["localhost"]` line in the manifest is
  the *only* place that grants outbound HTTP. Add a real
  ticket-tool host there to point the router at a production
  service — nothing in `lib/` needs to change.
- The router and the upstream share the same `audit.jsonl`
  (resolved against the project root by M41). Tail it with
  `tail -f .aeris/audit.jsonl` while the demo runs.
