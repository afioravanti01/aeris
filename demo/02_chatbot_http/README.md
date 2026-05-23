# 02 — Chatbot (HTTP server)

A web-facing chatbot in a single `ai.chat(...)` call. The Aeris
documentation under `./docs` is the knowledge base; passing
`port:` to `ai.chat` binds an HTTP server on that port and serves:

- `GET  /`           — the HTML frontend (`index.html` next to the script)
- `POST /api/chat`   — `{ "message": "..." }` → `{ "response": "..." }`
- `GET  /api/health` — `{ "status": "ok", "docs": N }`
- `OPTIONS *`        — 204

The server is single-threaded: one LLM call blocks the loop until
it returns (same constraint as M31's `spawn` fallback).

## Prerequisites

The bundled `aeris.toml` uses the CLI Claude backend. Swap to
`kind = "mock"` to test the server flow without an LLM.

## Running

```sh
cd demo/02_chatbot_http
aeris run ./main.aer            # default port 8080
aeris run ./main.aer 3000       # custom port via main(args)
```

Then open `http://localhost:8080` in a browser, or:

```sh
curl -s -X POST http://localhost:8080/api/chat \
     -H 'content-type: application/json' \
     -d '{"message":"what is enforce mode?"}'
```

## What it shows

- `fn main(args)` receiving CLI arguments as a `list<string>` (M34.T2)
- `"""..."""` triple-quoted string for the system prompt
- `ai.chat(system:, dir:, port:)` — integrated KB + HTTP server (M35)
- `strings.parse_int(...) catch err { default }` for safe parsing
