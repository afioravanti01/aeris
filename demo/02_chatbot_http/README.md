# 02 — Chatbot (HTTP server)

A web-facing chatbot. The Aeris documentation under `./docs` is the
knowledge base; an HTTP server on port 8080 exposes:

- `GET  /`          — the HTML frontend (`index.html`)
- `POST /api/chat`  — `{ "message": "..." }` → `{ "response": "..." }`
- `GET  /api/health` — `{ "status": "ok", "docs": N }`

Each incoming request runs in its own `spawn { … }` so the server
stays responsive.

## Prerequisites

The bundled `aeris.toml` uses the CLI Claude backend. Swap to
`kind = "mock"` to test the server flow without an LLM.

## Running

```sh
cd demo/02_chatbot_http
aeris run ./main.aer
```

Then open `http://localhost:8080` in a browser, or:

```sh
curl -s -X POST http://localhost:8080/api/chat \
     -H 'content-type: application/json' \
     -d '{"message":"what is enforce mode?"}'
```

## What it shows

- `net.http(port:)` + `server.accept()` + `spawn { … }` per request
- `req.path` / `req.method` / `req.body` field access on `HttpReq`
- `req.reply(status, body, content_type)` and `req.reply_json(...)`
- `ai.chat(system, dir)` knowledge base + `chat.ask(prompt)`
- `json.parse(raw) catch err { … }` and `json.encode({...})`
